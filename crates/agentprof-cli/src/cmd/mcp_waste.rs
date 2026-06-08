//! `agentprof mcp-waste` — cross-session report of MCP tools loaded but never
//! called. See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md`
//! §7.3 for the user-facing contract.
//!
//! As of T4.2 the [`run`] entry point wires the full pipeline: discover
//! sessions (Copilot adapter) → filter by `--since` mtime → load
//! `mcp.json` once → per-session
//! `load_session → derive_episodes → analyze → extract_loaded_set →
//! compute_waste` → `aggregate_waste` → render → stdout/file.
//! Renderers (`render_md`, `render_json`, `render_html`) are stubbed and
//! filled in by T4.3; smoke-running today produces empty output and exit
//! code 0 on a non-empty session root.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

use clap::ValueEnum;

use crate::cmd::analyze::ExitKind;
use crate::cmd::since::parse_since;
use crate::cmd::{LogConfig, TracingHandle};

/// Arguments for `agentprof mcp-waste`.
///
/// Mirrors the spec §7.3 CLI surface: time-window filter, optional adapter
/// root override, `mcp.json` path override, and an export format that
/// deliberately excludes TUI (see [`McpWasteExport`] for rationale).
#[derive(Debug, clap::Args)]
#[non_exhaustive]
pub struct McpWasteArgs {
    /// Adapter session-state root (default: per-adapter standard path).
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Time-window filter (e.g. `7d`, `30d`, `all`). Default: `7d`.
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Limit "Always unused" table to top N entries. Default 20.
    #[arg(long, default_value_t = 20)]
    pub top: usize,

    /// Override `mcp.json` path (default: `~/.copilot/mcp.json`).
    /// Relative paths resolve against CWD; `~`-prefixed paths expand
    /// via `directories::BaseDirs::home_dir()` to match `--root`
    /// convention.
    #[arg(long)]
    pub mcp_config: Option<PathBuf>,

    /// Output format. TUI is deferred to a future milestone — see
    /// spec §10 for the rationale.
    #[arg(long, value_enum, default_value = "md")]
    pub export: McpWasteExport,

    /// Output file (default: stdout).
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Output formats supported by `agentprof mcp-waste`.
///
/// TUI is intentionally **not** included; per spec §7.3 / §10 the live
/// dashboard for waste lives in `agentprof watch` and the dedicated TUI
/// view is deferred to a future milestone.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
#[non_exhaustive]
pub enum McpWasteExport {
    /// Markdown report (default; renders the same template family as
    /// `analyze --export md`).
    Md,
    /// JSON document suitable for machine consumption / CI gating.
    Json,
    /// Self-contained HTML report (askama template).
    Html,
}

/// Resolve `~/.copilot/mcp.json` (or the override) into an absolute
/// [`PathBuf`].
///
/// This helper is shared with `cmd::analyze`'s `--section mcp-waste`
/// dispatch (T3.1) so both code paths read the same configuration with
/// identical `~`-expansion semantics.
///
/// # Examples
///
/// ```ignore
/// # use std::path::Path;
/// # use agentprof_cli::cmd::mcp_waste::resolve_mcp_config_path;
/// // Override wins over the default home-based lookup.
/// let p = resolve_mcp_config_path(Some(Path::new("./mcp.json"))).unwrap();
/// assert!(p.ends_with("mcp.json"));
/// ```
///
/// # Errors
///
/// Returns an `anyhow::Error` when `override_path` is `None` and
/// `directories::BaseDirs::new()` fails to determine the user's home
/// directory (e.g. on a minimal CI container without `HOME` set and no
/// `/etc/passwd` entry for the current uid).
pub fn resolve_mcp_config_path(override_path: Option<&std::path::Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(expand_tilde(p));
    }
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("could not resolve user home directory"))?;
    Ok(base.home_dir().join(".copilot").join("mcp.json"))
}

/// Expand a leading `~/` against the current user's home directory.
///
/// Falls back to the original path when no home directory can be
/// determined (preserves the user's literal input for the caller's
/// downstream error message).
fn expand_tilde(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(base) = directories::BaseDirs::new() {
            return base.home_dir().join(rest);
        }
    }
    p.to_path_buf()
}

/// Subcommand entry point.
///
/// Implements the 7-stage `mcp-waste` pipeline:
///
/// 1. Discover sessions via [`agentprof_adapters::copilot::CopilotAdapter`]
///    under `--root` (defaults to `~/.copilot/session-state`).
/// 2. Filter by `--since` window using `SessionRef::modified_at`
///    (mirrors `aggregate`'s mtime-based filter).
/// 3. Load `mcp.json` once (resolved via [`resolve_mcp_config_path`]);
///    parse failures degrade silently to `None` so the CLI stays usable
///    on hosts without a Copilot MCP config.
/// 4. For each surviving session, run
///    `load_session → derive_episodes → analyze → extract_loaded_set →
///    compute_waste`. Per-session failures are warned to stderr (via
///    `tracing::warn!`) and skipped, matching `aggregate`'s policy.
/// 5. Cross-session aggregation via
///    [`agentprof_core::analyzer::aggregate_waste`].
/// 6. Render (md / json / html — stubbed in T4.2; filled by T4.3).
/// 7. Write to `--output` or stdout.
///
/// `_cfg` and `_tracing_handle` are accepted for signature uniformity
/// with the other `cmd::*::run` entry points and are unused here today.
///
/// # Examples
///
/// ```ignore
/// # use agentprof_cli::cmd::mcp_waste::{run, McpWasteArgs, McpWasteExport};
/// # let args = McpWasteArgs {
/// #     root: Some("crates/agentprof-adapters/tests/fixtures/copilot".into()),
/// #     since: "all".into(), top: 20, mcp_config: None,
/// #     export: McpWasteExport::Md, output: None,
/// # };
/// // run(args, &cfg, &handle)?;
/// ```
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is [`ExitKind`]:
///
/// - [`ExitKind::UserError`] — invalid `--since`, or `mcp.json` override
///   path that cannot be resolved.
/// - [`ExitKind::DataError`] — session discovery failed, no sessions
///   matched `--since`, or every discovered session failed to parse.
/// - [`ExitKind::OutputError`] — writing to `--output` failed.
pub fn run(
    args: McpWasteArgs,
    _cfg: &LogConfig,
    _tracing_handle: &TracingHandle,
) -> anyhow::Result<()> {
    use agentprof_adapters::copilot::{
        extract_loaded_set_from_session, load_mcp_config, CopilotAdapter,
    };
    use agentprof_core::adapter::Adapter;
    use agentprof_core::analyzer::{aggregate_waste, analyze, compute_waste};
    use agentprof_core::episode::derive_episodes;

    // 1. Discover sessions (Copilot is the only adapter with MCP wire
    //    data today; see spec §7.3).
    let adapter = CopilotAdapter;
    let root = args.root.unwrap_or_else(|| {
        directories::BaseDirs::new().map_or_else(
            || PathBuf::from(".copilot/session-state"),
            |b| b.home_dir().join(".copilot").join("session-state"),
        )
    });
    let sessions = adapter.discover_sessions(&root).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!(
            "discovering sessions under {}: {e}",
            root.display()
        ))
    })?;

    // 2. Filter by --since mtime window (mirrors aggregate.rs).
    let since_dur = parse_since(&args.since)
        .map_err(|msg| ExitKind::UserError.into_anyhow(format!("invalid --since: {msg}")))?;
    let now = SystemTime::now();
    let filtered: Vec<_> = sessions
        .into_iter()
        .filter(|s| {
            now.duration_since(s.modified_at)
                .map_or(true, |elapsed| elapsed <= since_dur)
        })
        .collect();

    if filtered.is_empty() {
        return Err(ExitKind::DataError.into_anyhow(format!(
            "no sessions matched --since {} under {}",
            args.since,
            root.display()
        )));
    }

    // 3. Load mcp.json once. Parse failures degrade to None so the CLI
    //    remains usable on hosts without a Copilot MCP config.
    let mcp_config_path = resolve_mcp_config_path(args.mcp_config.as_deref())?;
    let parsed_cfg = load_mcp_config(&mcp_config_path);
    let cfg_map: Option<BTreeMap<String, Vec<String>>> = parsed_cfg.as_ref().map(|c| {
        c.servers
            .iter()
            .filter_map(|(name, info)| info.tools.as_ref().map(|t| (name.clone(), t.clone())))
            .collect()
    });

    // 4. Per session: load, derive episodes, analyze, compute_waste.
    let mut per_session: Vec<(
        agentprof_core::adapter::SessionRef,
        agentprof_core::model::WasteReport,
    )> = Vec::with_capacity(filtered.len());
    let mut failed: usize = 0;
    for sref in &filtered {
        let result: Result<agentprof_core::model::WasteReport, anyhow::Error> = (|| {
            let raw = adapter.load_session(sref)?;
            let episodes = derive_episodes(&raw.events, &raw.meta);
            let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
            let wire = extract_loaded_set_from_session(&raw.events);
            Ok(compute_waste(&report, &wire, cfg_map.as_ref()))
        })();
        match result {
            Ok(waste) => per_session.push((sref.clone(), waste)),
            Err(e) => {
                tracing::warn!(
                    session = %agentprof_core::observability::pii::hash_short(&sref.id),
                    error = %format_args!("{e:#}"),
                    "failed to analyze session"
                );
                failed += 1;
            }
        }
    }
    if per_session.is_empty() {
        return Err(
            ExitKind::DataError.into_anyhow(format!("all {failed} session(s) failed to parse"))
        );
    }
    if failed > 0 {
        tracing::warn!(
            sessions_ok = per_session.len(),
            sessions_failed = failed,
            sessions_total = filtered.len(),
            "partial parse failures; report covers successful sessions only"
        );
    }

    // 5. Cross-session aggregation.
    let agg = aggregate_waste(&per_session);

    // 6. Render. (T4.3 fills these in; today they return empty strings.)
    let rendered = match args.export {
        McpWasteExport::Md => render_md(&agg, args.top),
        McpWasteExport::Json => render_json(&agg)?,
        McpWasteExport::Html => render_html(&agg, args.top)?,
    };

    // 7. Output.
    if let Some(path) = &args.output {
        std::fs::write(path, &rendered).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!("writing to {}: {e}", path.display()))
        })?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

/// Render the aggregate waste report as Markdown.
///
/// **T4.2 stub**: returns an empty string. T4.3 supplies the real
/// template (mirrors `aggregate --by mcp-server --export md`) and will
/// consume both arguments — hence the present lint suppression.
#[allow(clippy::missing_const_for_fn)]
fn render_md(_agg: &agentprof_core::model::AggregateWasteReport, _top: usize) -> String {
    String::new()
}

/// Render the aggregate waste report as JSON.
///
/// **T4.2 stub**: returns an empty string. T4.3 will serialise via
/// `serde_json::to_string_pretty`, which is fallible — the `Result`
/// return type is preserved now so T4.3 is a body-only edit.
///
/// # Errors
///
/// Currently infallible (returns `Ok(String::new())`). T4.3 propagates
/// `serde_json::Error` via `?`.
#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)]
fn render_json(_agg: &agentprof_core::model::AggregateWasteReport) -> anyhow::Result<String> {
    Ok(String::new())
}

/// Render the aggregate waste report as self-contained HTML.
///
/// **T4.2 stub**: returns an empty string. T4.3 will render via
/// `askama` (`templates/mcp_waste_full.html.jinja`), which is fallible
/// — the `Result` return type is preserved now so T4.3 is a body-only
/// edit.
///
/// # Errors
///
/// Currently infallible (returns `Ok(String::new())`). T4.3 propagates
/// `askama::Error` via `?`.
#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)]
fn render_html(
    _agg: &agentprof_core::model::AggregateWasteReport,
    _top: usize,
) -> anyhow::Result<String> {
    Ok(String::new())
}
