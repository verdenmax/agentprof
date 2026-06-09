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
use crate::cmd::analyze::{resolve_session, SessionSelector};
use crate::cmd::exit::ExitKind;

/// Arguments for `agentprof watch`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
#[command(
    after_help = "Note: top-level flags --debounce-ms / --agent / --root / --session \
                  are global (Wave D3 / `m1.6.3-t2-followup-clap-arg-ordering`), \
                  so they accept BOTH positions: \
                  `agentprof watch --debounce-ms 500 aggregate --by tool` AND \
                  `agentprof watch aggregate --by tool --debounce-ms 500` both work."
)]
pub struct WatchCmd {
    /// Aggregate sub-mode (cross-session). Omit for single-session watch.
    #[command(subcommand)]
    pub sub: Option<WatchSub>,

    /// Agent whose session to watch. M1.6.3 supports `copilot` only.
    #[arg(long, value_enum, default_value_t = AgentKind::Copilot, global = true)]
    pub agent: AgentKind,

    /// Override the adapter's default session-state root.
    #[arg(long, global = true)]
    pub root: Option<PathBuf>,

    /// Which session to watch (single mode only). Defaults to `latest`.
    /// Locked to the resolved session at startup — newer sessions are
    /// NOT auto-followed (per spec D-5).
    ///
    /// **Known limitation (full-review CLI #6):** because `clap`'s
    /// `default_value = "latest"` collapses "user omitted the flag"
    /// and "user wrote `--session latest` explicitly" into the same
    /// `SessionSelector::Latest` variant, the `cmd::watch::run_cross`
    /// "flag ignored in watch aggregate mode" warning fires a false
    /// negative for the former case (it stays silent). To fix
    /// properly, change to `Option<SessionSelector>` and detect
    /// `None`-vs-`Some(Latest)` — left for a future polish round to
    /// avoid breaking external scripts that may pass `--session latest`
    /// explicitly.
    #[arg(long, default_value = "latest", global = true)]
    pub session: SessionSelector,

    /// Debounce window (ms) for filesystem events.
    #[arg(long, default_value_t = 250, global = true)]
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
#[tracing::instrument(
    name = "cmd.watch",
    skip_all,
    fields(
        agent = "copilot",
        sub = if cmd.sub.is_some() { "aggregate" } else { "single" },
        debounce_ms = cmd.debounce_ms,
        no_cache = no_cache,
    )
)]
pub fn run(
    cmd: WatchCmd,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
    no_cache: bool,
    storage_path: Option<PathBuf>,
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

    // Wave D3 (`m1.6.3-t2-followup-destructure-watchcmd`): destructure
    // once at the top so we can move owned pieces into the helpers
    // (and drop the `cmd.sub.clone()` + `&WatchCmd` plumbing that
    // pre-D3 carried the whole struct through two layers of frames).
    let WatchCmd {
        sub,
        agent,
        root,
        session,
        debounce_ms,
    } = cmd;

    let adapter = match agent {
        AgentKind::Copilot => CopilotAdapter,
        other => {
            return Err(ExitKind::UserError.into_anyhow(format!(
                "{other:?} adapter not yet implemented (M1.6.3 supports copilot only)"
            )));
        }
    };

    let debounce = Duration::from_millis(debounce_ms);

    match sub {
        None => run_single(
            adapter,
            root,
            &session,
            debounce,
            cfg,
            tracing_handle,
            no_cache,
            storage_path,
        ),
        Some(WatchSub::Aggregate(agg)) => {
            // Cross-aggregate watch has no `AnalysisReport` to flush;
            // storage write-through is a per-session concept (M2.1 spec
            // §8). The `no_cache` / `storage_path` flags are accepted at
            // the CLI level for uniformity but ignored here.
            let _ = (no_cache, storage_path);
            run_cross(adapter, agg, &session, debounce, cfg, tracing_handle)
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

#[allow(clippy::too_many_arguments)] // M2.1 T5.3 threaded no_cache + storage_path
fn run_single(
    adapter: CopilotAdapter,
    root: Option<PathBuf>,
    session: &SessionSelector,
    debounce: Duration,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
    no_cache: bool,
    storage_path: Option<PathBuf>,
) -> Result<()> {
    let sref = resolve_session(&adapter, root, session)?;
    let events_jsonl = sref.path.clone();

    // Initial load.
    let initial = load_single(&adapter, &sref)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("initial load: {e:#}")))?;

    // M2.1 T5.3 — open ONE `Db` connection at watch start, write the
    // initial report through it, and **drop it immediately**. Per spec
    // §8 we do NOT write on every refresh tick (would cause high-freq
    // disk churn), so once the initial upsert is flushed the long-
    // lived handle has no further work — holding it would only keep
    // the SQLite WAL lock alive for the rest of the watch session for
    // no reason. The audit (M2.1 audit P1-2) caught the bug: an
    // earlier draft bound the handle to `_db_guard` so it lived for
    // the whole `run_single`, blocking concurrent `db ingest` from
    // another shell.
    //
    // Failures are downgraded to `tracing::warn!`: storage hiccups
    // must never block the interactive TUI.
    if !no_cache {
        match agentprof_cli::config::resolve_storage_config(
            agentprof_storage::config::PartialStorageConfig::default(),
            storage_path,
        ) {
            Ok(cfg) => match agentprof_storage::Db::open_and_migrate(&cfg.path) {
                Ok(mut db) => {
                    if let WatchData::Single { report, .. } = &initial {
                        let now_secs = chrono::Utc::now().timestamp();
                        if let Err(e) = agentprof_storage::upsert::upsert_report(
                            &mut db, report, &sref.path, now_secs,
                        ) {
                            tracing::warn!(
                                error = %e,
                                "watch initial write-through failed (non-fatal)"
                            );
                        }
                    }
                    // `db` drops here — release the WAL lock now that
                    // the one and only write we owe is flushed.
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "watch storage open failed; initial write-through skipped"
                    );
                }
            },
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "watch storage config resolution failed; initial write-through skipped"
                );
            }
        }
    }

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
    session: &SessionSelector,
    debounce: Duration,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
) -> Result<()> {
    // Compute the initial aggregate up-front (also validates --root).
    let initial_any = compute_aggregate(&adapter, &agg)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("initial aggregate: {e:#}")))?
        .0;
    let initial = WatchData::Cross(initial_any);

    // CLI #7 — root is resolved a second time here (once inside
    // `compute_aggregate`, again right below for the watcher target).
    // The resolution function is pure (clone + `default_session_root` +
    // unwrap_or → into anyhow), so the duplication is harmless but
    // architecturally smells. A future polish round can have
    // `compute_aggregate` return the resolved root and reuse it here;
    // deferred to keep this change scoped to CLI-grab-bag review #1-#10.
    let root = agg
        .root
        .clone()
        .or_else(|| adapter.default_session_root())
        .ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow("could not determine session root for watch; pass --root".to_string())
        })?;

    // Warn (before spawning the watcher) if session was set in cross
    // mode — it's ignored here, and surfacing this even when the spawn
    // later fails helps users diagnose a likely typo.
    if !matches!(session, SessionSelector::Latest) {
        tracing::warn!(
            flag = "--session",
            sub = "aggregate",
            "flag ignored in watch aggregate mode"
        );
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
