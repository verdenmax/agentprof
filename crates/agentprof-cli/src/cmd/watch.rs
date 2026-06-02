//! `agentprof watch` subcommand (M1.6.3).
//!
//! Live-refresh TUI for a single Copilot session (default) or a
//! cross-session aggregate (`watch aggregate ...`). Uses `notify` +
//! `notify-debouncer-mini` for kernel-level filesystem events;
//! debounce window defaults to 250 ms.
//!
//! Watcher target:
//! - Single mode: `<root>/<session-id>/events.jsonl` (`NonRecursive`).
//! - Cross mode: `<root>/` (`Recursive`) — any session change refreshes.
//!
//! Per spec D-5 (single mode): `--session latest` locks to the initial
//! latest session; later sessions are NOT auto-followed.

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::{Adapter, AgentKind, SessionRef};
use agentprof_core::analyzer::analyze;
use agentprof_core::episode::derive_episodes;
use agentprof_tui::watch::{RefreshKind, ReloadError, WatchData, WatchRunner};

use crate::cmd::aggregate::{compute_aggregate, AggExportFormat, AggregateCmd};
use crate::cmd::analyze::{resolve_session, ExitKind, SessionSelector};

/// Arguments for `agentprof watch`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
#[command(
    after_help = "Note: global flags like --debounce-ms, --agent, --root, --session \
                  must appear BEFORE the `aggregate` subcommand. \
                  Example: `agentprof watch --debounce-ms 500 aggregate --by tool`."
)]
pub struct WatchCmd {
    /// Aggregate sub-mode (cross-session). Omit for single-session watch.
    #[command(subcommand)]
    pub sub: Option<WatchSub>,

    /// Agent whose session to watch. M1.6.3 supports `copilot` only.
    #[arg(long, value_enum, default_value_t = AgentKind::Copilot)]
    pub agent: AgentKind,

    /// Override the adapter's default session-state root.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Which session to watch (single mode only). Defaults to `latest`.
    /// Locked to the resolved session at startup — newer sessions are
    /// NOT auto-followed (per spec D-5).
    #[arg(long, default_value = "latest")]
    pub session: SessionSelector,

    /// Debounce window (ms) for filesystem events.
    #[arg(long, default_value_t = 250)]
    pub debounce_ms: u64,
}

/// Cross-session watch sub-mode.
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum WatchSub {
    /// Watch cross-session aggregate (re-aggregates on any session change
    /// under `--root`). Accepts all `agentprof aggregate` flags.
    Aggregate(AggregateCmd),
}

/// Entry point for `agentprof watch`.
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is [`ExitKind`]:
/// - `UserError` (1): unknown agent, bad selector, root not found.
/// - `DataError` (2): initial session load failed, or notify init failed.
/// - `OutputError` (3): stdin or stdout is not a TTY; tui runtime failure.
///
/// # Examples
///
/// ```text
/// // Constructed by clap from argv in production; not invokable headless.
/// agentprof watch --debounce-ms 500
/// agentprof watch aggregate --by tool --since 7d
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn run(
    cmd: WatchCmd,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
) -> Result<()> {
    // Validate sub-mode arguments BEFORE the TTY check so users get a
    // crisp UserError (exit 1) instead of an environment error (exit 3)
    // when they pass conflicting flags from a non-TTY shell or CI.
    if let Some(WatchSub::Aggregate(agg)) = &cmd.sub {
        validate_watch_aggregate(agg)?;
    }

    // TTY check — same shape as cmd::analyze::run_tui.
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err(ExitKind::OutputError.into_anyhow(
            "agentprof watch requires both stdin and stdout to be TTYs; \
             headless monitoring is not supported (use `watch -n 5 agentprof analyze \
             --export md` as a workaround)"
                .to_string(),
        ));
    }

    let adapter = match cmd.agent {
        AgentKind::Copilot => CopilotAdapter,
        other => {
            return Err(ExitKind::UserError.into_anyhow(format!(
                "{other:?} adapter not yet implemented (M1.6.3 supports copilot only)"
            )));
        }
    };

    let debounce = Duration::from_millis(cmd.debounce_ms);

    match cmd.sub.clone() {
        None => run_single(adapter, &cmd, debounce, cfg, tracing_handle),
        Some(WatchSub::Aggregate(agg)) => {
            run_cross(adapter, agg, &cmd, debounce, cfg, tracing_handle)
        }
    }
}

/// Reject flags on `watch aggregate` whose effect is meaningless when
/// the output is always the interactive TUI.
fn validate_watch_aggregate(agg: &AggregateCmd) -> Result<()> {
    if !matches!(agg.export, AggExportFormat::Md) || agg.output.is_some() {
        return Err(ExitKind::UserError.into_anyhow(
            "`watch aggregate` does not accept --export or --output; \
             output is always interactive TUI. Re-run without those flags."
                .to_string(),
        ));
    }
    Ok(())
}

fn run_single(
    adapter: CopilotAdapter,
    cmd: &WatchCmd,
    debounce: Duration,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
) -> Result<()> {
    let sref = resolve_session(&adapter, cmd.root.clone(), &cmd.session)?;
    let events_jsonl = sref.path.clone();

    // Initial load.
    let initial = load_single(&adapter, &sref)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("initial load: {e:#}")))?;

    // mpsc channel between debouncer-thread → runner.
    let (tx, rx) = channel::<RefreshKind>();

    // Spawn the debounced file watcher; hold the Debouncer for the
    // entire lifetime of `run_single` — dropping it stops the watcher.
    let _watcher = spawn_watcher(
        &events_jsonl,
        RecursiveMode::NonRecursive,
        debounce,
        tx,
        "try `agentprof analyze --export md` for headless",
    )?;

    // Build the reload closure (captures adapter by value — zero-sized,
    // free move; captures sref clone for repeated reload).
    let sref_for_reload = sref;
    let reload: ReloadFn = Box::new(move || {
        if !sref_for_reload.path.exists() {
            return Err(ReloadError::SessionGone {
                path: sref_for_reload.path.clone(),
            });
        }
        load_single(&adapter, &sref_for_reload).map_err(|e| ReloadError::Pipeline(format!("{e:#}")))
    });

    enter_and_run(initial, rx, reload, cfg, tracing_handle)
}

fn run_cross(
    adapter: CopilotAdapter,
    agg: AggregateCmd,
    cmd: &WatchCmd,
    debounce: Duration,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
) -> Result<()> {
    // Compute the initial aggregate up-front (also validates --root).
    let initial_any = compute_aggregate(&adapter, &agg)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("initial aggregate: {e:#}")))?
        .0;
    let initial = WatchData::Cross(initial_any);

    let root = agg
        .root
        .clone()
        .or_else(|| adapter.default_session_root())
        .ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow("could not determine session root for watch; pass --root".to_string())
        })?;

    // Warn (before spawning the watcher) if cmd.session was set in cross
    // mode — it's ignored here, and surfacing this even when the spawn
    // later fails helps users diagnose a likely typo.
    if !matches!(cmd.session, SessionSelector::Latest) {
        eprintln!("agentprof: warning: --session is ignored in `watch aggregate` mode");
    }

    let (tx, rx) = channel::<RefreshKind>();
    let _watcher = spawn_watcher(
        &root,
        RecursiveMode::Recursive,
        debounce,
        tx,
        "try `agentprof aggregate --export md` for headless",
    )?;

    let agg_for_reload = agg;
    let reload: ReloadFn = Box::new(move || {
        compute_aggregate(&adapter, &agg_for_reload)
            .map(|(r, _)| WatchData::Cross(r))
            .map_err(|e| ReloadError::Pipeline(format!("{e:#}")))
    });

    enter_and_run(initial, rx, reload, cfg, tracing_handle)
}

type ReloadFn = Box<dyn FnMut() -> Result<WatchData, ReloadError>>;

fn enter_and_run(
    initial: WatchData,
    rx: Receiver<RefreshKind>,
    reload: ReloadFn,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
) -> Result<()> {
    // Swap the tracing writer to a rolling file BEFORE entering the
    // alt-screen — see `cmd::analyze::run_tui` for the rationale.
    let _log_guard = crate::observability::enter_tui_log_guard(cfg, tracing_handle);

    agentprof_tui::app::terminal::install_panic_hook();
    let mut term = agentprof_tui::app::terminal::enter()
        .map_err(|e| ExitKind::OutputError.into_anyhow(format!("entering tui: {e}")))?;
    let mut runner = WatchRunner::with_watcher(initial, rx, reload);
    let res = runner.run(&mut term);
    let _ = agentprof_tui::app::terminal::leave(&mut term);
    res.map_err(|e| ExitKind::OutputError.into_anyhow(format!("tui runtime: {e}")))
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn load_single(adapter: &CopilotAdapter, sref: &SessionRef) -> Result<WatchData> {
    let raw = adapter.load_session(sref).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("loading session {}: {e}", sref.path.display()))
    })?;
    let episodes = derive_episodes(&raw.events, &raw.meta);
    let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
    Ok(WatchData::Single {
        report,
        episodes,
        meta: raw.meta,
    })
}

/// Spawn a debounced filesystem watcher on `target` with the given
/// recursion mode. `headless_hint` is appended to the init-failure
/// error message so users get a workable fallback command.
///
/// `tracing::warn!` is intentionally avoided in the callback — it
/// would write to stderr during the TUI alt-screen and visually
/// corrupt the display. Errors are emitted at `debug` level so they
/// remain available via `RUST_LOG=debug` without breaking the UI.
fn spawn_watcher(
    target: &Path,
    mode: RecursiveMode,
    debounce: Duration,
    tx: Sender<RefreshKind>,
    headless_hint: &str,
) -> Result<Debouncer<RecommendedWatcher>> {
    let mut debouncer = new_debouncer(debounce, move |res: DebounceEventResult| match res {
        Ok(events) if !events.is_empty() => {
            let _ = tx.send(RefreshKind::DataChanged);
        }
        Ok(_) => {}
        Err(errors) => {
            tracing::debug!(?errors, "notify debouncer errors");
        }
    })
    .map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("init notify watcher: {e}; {headless_hint}"))
    })?;
    debouncer
        .watcher()
        .watch(target, mode)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("watch {}: {e}", target.display())))?;
    Ok(debouncer)
}
