//! `agentprof aggregate` subcommand (M1.6.2).
//!
//! Discovers sessions under `--root` (or the adapter's default
//! session-state root), filters by `--since DURATION` (mtime),
//! parses sequentially (D-2 — rayon deferred), computes the
//! requested cross-session aggregate, and renders to md / json /
//! csv / html.

use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};
use clap::{Args, ValueEnum};

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::{Adapter, AgentKind, SessionRef};
use agentprof_core::analyzer::aggregate::{
    group_by_day::aggregate_by_day, group_by_mcp::aggregate_by_mcp_server,
    group_by_model::aggregate_by_model, group_by_tool::aggregate_by_tool, AggregateKey,
    AnyAggregateReport,
};
use agentprof_core::analyzer::{analyze, AnalysisReport};
use agentprof_core::episode::{derive_episodes, Episodes};

use crate::cmd::exit::ExitKind;
use crate::cmd::format;
use crate::cmd::since::parse_since;

/// Arguments for `agentprof aggregate`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct AggregateCmd {
    /// Agent whose sessions to aggregate. M1.6.2 supports only `copilot`.
    #[arg(long, value_enum, default_value_t = AgentKind::Copilot)]
    pub agent: AgentKind,

    /// Override the adapter's default session-state root.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Group-by key. Required.
    #[arg(long, value_enum)]
    pub by: AggBy,

    /// Time window: `<N>d` / `<N>h` / `<N>m` / `<N>s` / `all`. Default `30d`.
    #[arg(long, default_value = "30d")]
    pub since: String,

    /// Maximum bucket rows in the final report. `0` = unlimited.
    #[arg(long, default_value_t = 0)]
    pub limit: usize,

    /// Output format. `tui` opens a static cross-session aggregate TUI
    /// (shipped M1.6.3); for live refresh use `agentprof watch aggregate ...`.
    #[arg(long, value_enum, default_value_t = AggExportFormat::Md)]
    pub export: AggExportFormat,

    /// Write to file instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Percentage threshold below which `--by day` rows are marked
    /// low-utilization. Range 0.0 to 100.0. Default 20.
    #[arg(long, default_value_t = 20.0)]
    pub low_utilization_threshold: f32,

    /// Heuristic token cost per MCP tool when no sidecar is provided.
    /// Default 200 (≈ description + small inputSchema). M1.6.6.
    /// Only consulted by `--by mcp-server`; ignored otherwise.
    #[arg(long, default_value_t = agentprof_core::analyzer::waste::DEFAULT_HEURISTIC_TOKENS)]
    pub tokens_per_tool: u64,

    /// Optional sidecar path with per-tool descriptions for exact
    /// token counts. Path → file: global JSON `{"<server>": [<ToolEntry>, ...]}`.
    /// Path → dir: per-server `<server>.json` files (MCP `tools/list` shape).
    /// Absent → heuristic-only mode. M1.6.6.
    /// Only consulted by `--by mcp-server`; ignored otherwise.
    #[arg(long, value_name = "PATH")]
    pub tool_descriptions: Option<PathBuf>,
}

/// Output format for `aggregate`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggExportFormat {
    /// Markdown to stdout/file (default).
    Md,
    /// JSON of [`AnyAggregateReport`].
    Json,
    /// CSV with header row, one row per bucket.
    Csv,
    /// Self-contained static HTML (no JS, askama-templated).
    Html,
    /// Interactive ratatui TUI (cross-session aggregate view, M1.6.3).
    /// Requires both stdin and stdout to be TTYs; otherwise exits with
    /// `OutputError` (3). `--output` is ignored.
    Tui,
}

/// CLI-facing mirror of [`AggregateKey`].
///
/// Kept local to `agentprof-cli` so `agentprof-core` need not depend on
/// `clap`. Maps 1:1 to [`AggregateKey`] via [`AggBy::to_key`].
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggBy {
    /// Group by tool name.
    Tool,
    /// Group by MCP server (`mcp__<server>__<tool>` prefix).
    McpServer,
    /// Group by UTC calendar date.
    Day,
    /// Group by first-turn model id.
    Model,
}

impl AggBy {
    /// Convert to the core [`AggregateKey`].
    #[must_use]
    pub const fn to_key(self) -> AggregateKey {
        match self {
            Self::Tool => AggregateKey::Tool,
            Self::McpServer => AggregateKey::McpServer,
            Self::Day => AggregateKey::Day,
            Self::Model => AggregateKey::Model,
        }
    }
}

/// Entry point for `agentprof aggregate`.
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is [`ExitKind`]:
/// - `UserError` (1): invalid `--low-utilization-threshold`, unknown
///   agent, missing/non-existent `--root`, bad `--since`.
/// - `DataError` (2): all discovered sessions failed to parse, or the
///   adapter could not enumerate the root.
/// - `OutputError` (3): I/O failure writing to stdout or `--output`;
///   TTY missing when `--export tui`.
#[allow(clippy::needless_pass_by_value)]
#[tracing::instrument(
    name = "cmd.aggregate",
    skip_all,
    fields(
        agent = "copilot",
        by = ?cmd.by,
        since = %cmd.since,
        limit = cmd.limit,
        export = ?cmd.export,
    )
)]
pub fn run(
    cmd: AggregateCmd,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
    no_cache: bool,
    storage_path: Option<PathBuf>,
    quiet: bool,
) -> Result<()> {
    // Resolve adapter early so we can validate `AgentKind` once.
    // The actual session loading now goes through `build_data_source`
    // (M2.1.1 T5.1) so aggregate gets the same SQLite-cache
    // acceleration `list` / `mcp-waste` / `analyze` got in M2.1.
    let agent_name = match cmd.agent {
        AgentKind::Copilot => "copilot",
        other => {
            return Err(ExitKind::UserError.into_anyhow(format!(
                "{other:?} adapter not yet implemented (M1.6.3 supports copilot only)"
            )));
        }
    };

    // CLI #2 — validate TTY presence BEFORE walking the session root
    // when `--export tui`. Pre-fix, a user piping the TUI form
    // (`aggregate --export tui > foo`) would do seconds of session-load
    // work then exit 3. Now the precondition check happens up-front so
    // the user sees the error immediately. Single-letter `--export tui`
    // typos no longer waste a real-session round-trip.
    if matches!(cmd.export, AggExportFormat::Tui) {
        check_tty_for_tui()?;
    }

    let (any_report, refs_total) =
        compute_aggregate_via_ds(&cmd, agent_name, no_cache, storage_path, quiet)?;

    // Empty-root warning lives in `run()` (not `compute_aggregate`) so
    // `watch aggregate`'s reload tick does NOT spam stderr — which during
    // the TUI alt-screen would visually corrupt the display.
    //
    // CLI #3 — also suppress when `--export tui`: the one-shot TUI
    // launch is about to enter alt-screen and the stderr warning would
    // momentarily flash before being overwritten. The empty-state is
    // surfaced inside the TUI's own cross-aggregate view instead.
    if refs_total == 0 && !matches!(cmd.export, AggExportFormat::Tui) {
        let root_label = cmd.root.as_ref().map_or_else(
            || "<adapter default>".to_string(),
            |p| agentprof_core::observability::pii::hash_path(p),
        );
        tracing::warn!(
            since = %cmd.since,
            root = %root_label,
            "no sessions matching window under root"
        );
    }

    // Dispatch TUI before the renderers (it needs the raw report).
    if matches!(cmd.export, AggExportFormat::Tui) {
        return run_tui_for_aggregate(any_report, cfg, tracing_handle);
    }

    // Render.
    let output = match cmd.export {
        AggExportFormat::Md => format::aggregate_md::render(&any_report),
        AggExportFormat::Json => serde_json::to_string_pretty(&any_report)
            .context("serialize AnyAggregateReport to JSON")?,
        AggExportFormat::Csv => {
            format::aggregate_csv::render(&any_report).context("render aggregate CSV")?
        }
        AggExportFormat::Html => format::aggregate_html::render(
            &any_report,
            cmd.low_utilization_threshold,
            env!("CARGO_PKG_VERSION"),
        ),
        AggExportFormat::Tui => unreachable!("handled above"),
    };

    // Write.
    if let Some(path) = &cmd.output {
        std::fs::write(path, &output).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!("write {}: {e}", path.display()))
        })?;
    } else {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        handle
            .write_all(output.as_bytes())
            .map_err(|e| ExitKind::OutputError.into_anyhow(format!("stdout: {e}")))?;
        if !output.ends_with('\n') {
            let _ = handle.write_all(b"\n");
        }
    }

    Ok(())
}

/// Run the load-and-compute half of `agentprof aggregate`, returning
/// the populated [`AnyAggregateReport`] alongside the total number of
/// session refs discovered (post-`--since` filtering, pre-parse).
///
/// Used by both [`run`] (for md/json/csv/html/tui dispatch) and by
/// `cmd::watch::run` (for live reload in `watch aggregate` mode).
///
/// The discovered-refs count is returned (instead of e.g. logged here)
/// so callers can decide whether to surface an "empty root" notice;
/// `watch aggregate`'s reload tick deliberately suppresses it to keep
/// the TUI alt-screen clean.
///
/// Per-session parse failures degrade gracefully: a warning is printed
/// to stderr and `failure_count` on the returned report is incremented.
/// A summary line is printed to stderr when at least one session failed.
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is [`ExitKind`]:
/// - `UserError` (1): invalid `--low-utilization-threshold`, bad
///   `--since`, missing/non-existent `--root`.
/// - `DataError` (2): all discovered sessions failed to parse, or the
///   adapter could not enumerate the root.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref
)]
pub fn compute_aggregate(
    adapter: &CopilotAdapter,
    cmd: &AggregateCmd,
) -> Result<(AnyAggregateReport, usize)> {
    // Validate threshold range early.
    if !(0.0..=100.0).contains(&cmd.low_utilization_threshold) {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "--low-utilization-threshold must be between 0.0 and 100.0; got {}",
            cmd.low_utilization_threshold
        )));
    }

    // Resolve root.
    let root = cmd
        .root
        .clone()
        .or_else(|| adapter.default_session_root())
        .ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow("could not determine session root; pass --root explicitly".to_string())
        })?;

    if !root.is_dir() {
        return Err(
            ExitKind::UserError.into_anyhow(format!("session root not found: {}", root.display()))
        );
    }

    // Parse --since.
    let since_dur = parse_since(&cmd.since)
        .map_err(|msg| ExitKind::UserError.into_anyhow(format!("invalid --since: {msg}")))?;

    // Discover + filter by --since mtime.
    let all_refs = adapter.discover_sessions(&root).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("scanning {}: {e}", root.display()))
    })?;
    let now = SystemTime::now();
    let refs: Vec<SessionRef> = all_refs
        .into_iter()
        .filter(|s| {
            now.duration_since(s.modified_at)
                .map_or(true, |elapsed| elapsed <= since_dur)
        })
        .collect();

    // Sequential parse (D-2; rayon deferred).
    let mut reports: Vec<AnalysisReport> = Vec::with_capacity(refs.len());
    let mut episodes_vec: Vec<Episodes> = Vec::with_capacity(refs.len());
    // M2.1 T5.2.5: loaded_mcp_tools now lives on AnalysisReport itself
    // (populated by analyzer during analyze()), so `--by mcp-server`
    // no longer needs to hold per-session raw event vectors just to
    // re-run extract_loaded_set_from_session. The waste computation
    // below borrows report.loaded_mcp_tools directly.
    let mut failure_count: usize = 0;

    for sref in &refs {
        match load_and_analyze(adapter, sref) {
            Ok((report, episodes)) => {
                reports.push(report);
                episodes_vec.push(episodes);
            }
            Err(e) => {
                tracing::warn!(
                    session = %agentprof_core::observability::pii::hash_short(&sref.id),
                    error = %format_args!("{e:#}"),
                    "failed to parse session"
                );
                failure_count += 1;
            }
        }
    }

    let refs_total = refs.len();
    let any_report = compute_phase2(
        cmd,
        reports,
        episodes_vec,
        failure_count,
        since_dur,
        refs_total,
    )?;
    Ok((any_report, refs_total))
}

/// Verify both stdin and stdout are TTYs (precondition for `--export tui`).
///
/// Extracted to a standalone helper per CLI #2 so the check can run
/// at the top of `run()` (before any session-load work) AND inside
/// `run_tui_for_aggregate` as a defence-in-depth fallback when called
/// from non-`run` entry points.
fn check_tty_for_tui() -> Result<()> {
    use std::io::IsTerminal as _;

    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err(ExitKind::OutputError.into_anyhow(
            "agentprof aggregate --export tui requires both stdin and stdout to be TTYs; \
             use --export md|json|csv|html for headless output (e.g. `agentprof aggregate \
             --by tool --since 7d --export md`)"
                .to_string(),
        ));
    }
    Ok(())
}

fn run_tui_for_aggregate(
    any_report: AnyAggregateReport,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
) -> Result<()> {
    // Swap the tracing writer to a rolling file BEFORE entering the
    // alt-screen — see `cmd::analyze::run_tui` for the rationale.
    let _log_guard = crate::observability::enter_tui_log_guard(cfg, tracing_handle);

    // CLI #2 — the up-front TTY check in `run()` is the primary
    // guard; this is a defence-in-depth fallback for callers (if any)
    // that bypass `run()`.
    check_tty_for_tui()?;
    agentprof_tui::app::terminal::install_panic_hook();
    let mut term = agentprof_tui::app::terminal::enter()
        .map_err(|e| ExitKind::OutputError.into_anyhow(format!("entering tui: {e}")))?;
    let data = agentprof_tui::watch::WatchData::Cross(any_report);
    let mut runner = agentprof_tui::watch::WatchRunner::new_static(data);
    let res = runner.run(&mut term);
    let _ = agentprof_tui::app::terminal::leave(&mut term);
    res.map_err(|e| ExitKind::OutputError.into_anyhow(format!("tui runtime: {e}")))
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn load_and_analyze(
    adapter: &CopilotAdapter,
    sref: &SessionRef,
) -> Result<(AnalysisReport, Episodes)> {
    let raw = adapter
        .load_session(sref)
        .with_context(|| format!("loading session {}", sref.path.display()))?;
    let episodes = derive_episodes(&raw.events, &raw.meta);
    let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
    // M2.1 T5.2.5: raw.events are no longer needed downstream — the
    // ever-loaded MCP tool set is now carried by
    // AnalysisReport::loaded_mcp_tools, so `--by mcp-server` reads
    // it straight off `report`. Dropping `raw.events` here keeps
    // peak memory close to the pre-M1.6.5 baseline.
    Ok((report, episodes))
}

// `parse_since` moved to `crate::cmd::since` per full-review CLI #1
// (consolidation + saturating_mul fix; was already saturating here
// but the `cmd::list` copy wasn't).

/// Convert the CLI's [`std::time::Duration`] (from `parse_since`) into the
/// optional `chrono::Duration` that flows into `AggregateReport.since`.
///
/// Wave C item 1: `Duration::MAX` (the in-band sentinel that
/// [`crate::cmd::since::parse_since`] returns for `--since all`) collapses
/// to `None`, modelling "no lower time bound" honestly. JSON output then
/// omits the field entirely instead of serialising the meaningless raw
/// integer `9223372036854775807` ms.
fn since_to_opt_chrono(d: Duration) -> Option<chrono::Duration> {
    if d == Duration::MAX {
        return None;
    }
    let secs = i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
    Some(chrono::Duration::try_seconds(secs).unwrap_or(chrono::Duration::MAX))
}

fn fill_metadata(
    r: &mut AnyAggregateReport,
    since: Option<chrono::Duration>,
    failure_count: usize,
) {
    match r {
        AnyAggregateReport::Tool(x) => {
            x.since = since;
            x.failure_count = failure_count;
        }
        AnyAggregateReport::McpServer(x) => {
            x.since = since;
            x.failure_count = failure_count;
        }
        AnyAggregateReport::Day(x) => {
            x.since = since;
            x.failure_count = failure_count;
        }
        AnyAggregateReport::Model(x) => {
            x.since = since;
            x.failure_count = failure_count;
        }
        _ => unreachable!(
            "new AnyAggregateReport variant; add an explicit arm here AND in \
             cmd::format::aggregate_{{md,csv,html}}::{{meta,render,render_buckets}}"
        ),
    }
}

fn truncate_buckets(r: &mut AnyAggregateReport, limit: usize) {
    match r {
        AnyAggregateReport::Tool(x) => x.buckets.truncate(limit),
        AnyAggregateReport::McpServer(x) => x.buckets.truncate(limit),
        AnyAggregateReport::Day(x) => x.buckets.truncate(limit),
        AnyAggregateReport::Model(x) => x.buckets.truncate(limit),
        _ => unreachable!(
            "new AnyAggregateReport variant; add an explicit arm here AND in \
             cmd::format::aggregate_{{md,csv,html}}::{{meta,render,render_buckets}}"
        ),
    }
}

/// Phase-2 of `aggregate`: compute the [`AnyAggregateReport`] from
/// already-loaded reports + episodes. Shared by:
///
/// - `compute_aggregate` (single-path adapter load, used by `watch`)
/// - `compute_aggregate_via_ds` (dual-path data source, used by `run`)
///
/// Handles `--by` dispatch (tool/mcp-server/day/model), metadata fill-in
/// (`--since`, `failure_count`), `--limit` truncation, and the partial-
/// failure stderr summary.
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
fn compute_phase2(
    cmd: &AggregateCmd,
    reports: Vec<AnalysisReport>,
    episodes_vec: Vec<Episodes>,
    failure_count: usize,
    since_dur: Duration,
    _refs_total: usize,
) -> Result<AnyAggregateReport> {
    if reports.is_empty() && failure_count > 0 {
        return Err(ExitKind::DataError
            .into_anyhow(format!("all {failure_count} session(s) failed to parse")));
    }

    // Dispatch on --by.
    let mut any_report = match cmd.by.to_key() {
        AggregateKey::Tool => AnyAggregateReport::Tool(aggregate_by_tool(&reports, &episodes_vec)),
        AggregateKey::McpServer => {
            let mcp_config_path = crate::cmd::mcp_waste::resolve_mcp_config_path(None).ok();
            let parsed_cfg = mcp_config_path
                .as_deref()
                .and_then(agentprof_adapters::copilot::load_mcp_config);
            let config_loaded = parsed_cfg.as_ref().map(|c| {
                c.servers
                    .iter()
                    .filter_map(|(name, info)| {
                        info.tools.as_ref().map(|t| (name.clone(), t.clone()))
                    })
                    .collect::<std::collections::BTreeMap<_, _>>()
            });
            let sidecar = if let Some(path) = cmd.tool_descriptions.as_deref() {
                Some(
                    agentprof_adapters::copilot::tool_sidecar::load_sidecar(path).map_err(|e| {
                        ExitKind::UserError
                            .into_anyhow(format!("sidecar load failed for {}: {e}", path.display()))
                    })?,
                )
            } else {
                None
            };
            let bpe_cl100k = agentprof_core::analyzer::waste::build_bpe(
                agentprof_core::model::TokenizerKind::Cl100kBase,
            )
            .map(std::sync::Arc::new);
            let bpe_o200k = agentprof_core::analyzer::waste::build_bpe(
                agentprof_core::model::TokenizerKind::O200kBase,
            )
            .map(std::sync::Arc::new);
            let waste_per_report: Vec<agentprof_core::model::WasteReport> = reports
                .iter()
                .map(|r| {
                    let wire = &r.loaded_mcp_tools;
                    let model_hint = crate::cmd::model_hint::dominant_model(r);
                    let tokenizer =
                        agentprof_core::analyzer::waste::infer_tokenizer(model_hint.as_deref());
                    let mut waste_ctx =
                        agentprof_core::analyzer::waste::WasteComputeContext::new(wire)
                            .with_tokenizer(tokenizer)
                            .with_heuristic(cmd.tokens_per_tool);
                    let shared_bpe = match tokenizer {
                        agentprof_core::model::TokenizerKind::Cl100kBase => bpe_cl100k.as_ref(),
                        agentprof_core::model::TokenizerKind::O200kBase => bpe_o200k.as_ref(),
                        _ => None,
                    };
                    if let Some(b) = shared_bpe {
                        waste_ctx = waste_ctx.with_bpe(b.clone());
                    }
                    if let Some(c) = config_loaded.as_ref() {
                        waste_ctx = waste_ctx.with_config(c);
                    }
                    if let Some(s) = sidecar.as_ref() {
                        waste_ctx = waste_ctx.with_sidecar(s);
                    }
                    agentprof_core::analyzer::compute_waste(r, &waste_ctx)
                })
                .collect();
            AnyAggregateReport::McpServer(aggregate_by_mcp_server(
                &reports,
                &episodes_vec,
                &waste_per_report,
            ))
        }
        AggregateKey::Day => AnyAggregateReport::Day(aggregate_by_day(
            &reports,
            &episodes_vec,
            cmd.low_utilization_threshold,
        )),
        AggregateKey::Model => {
            AnyAggregateReport::Model(aggregate_by_model(&reports, &episodes_vec))
        }
        other => {
            return Err(ExitKind::UserError.into_anyhow(format!("unsupported --by key: {other:?}")));
        }
    };

    fill_metadata(
        &mut any_report,
        since_to_opt_chrono(since_dur),
        failure_count,
    );

    if cmd.limit > 0 {
        truncate_buckets(&mut any_report, cmd.limit);
    }

    if failure_count > 0 {
        let total = reports.len() + failure_count;
        tracing::warn!(
            sessions_ok = reports.len(),
            sessions_total = total,
            failure_count,
            "partial aggregate: some sessions failed"
        );
    }

    Ok(any_report)
}

/// Dual-path variant of [`compute_aggregate`] (M2.1.1 T5.1).
///
/// Constructs the data source via
/// [`crate::data_source_factory::build_data_source`] (same code path
/// as `list` / `mcp-waste` / `analyze`), discovers + sorts refs, then
/// for each ref calls both `load_session` (for the rollup report) and
/// `load_episodes` (for the per-call vec aggregate's percentile pool
/// needs). `load_episodes` failure is non-fatal — falls back to
/// `Episodes::default()` (skipped from the percentile pool).
///
/// Drains dual-path divergence warnings to stderr (unless `quiet`) at
/// the end of the load loop, matching `list` / `mcp-waste` UX.
fn compute_aggregate_via_ds(
    cmd: &AggregateCmd,
    agent_name: &str,
    no_cache: bool,
    storage_path: Option<PathBuf>,
    quiet: bool,
) -> Result<(AnyAggregateReport, usize)> {
    use agentprof_cli::config::resolve_storage_config;
    use agentprof_cli::data_source_factory::build_data_source;
    use agentprof_storage::config::PartialStorageConfig;

    if !(0.0..=100.0).contains(&cmd.low_utilization_threshold) {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "--low-utilization-threshold must be between 0.0 and 100.0; got {}",
            cmd.low_utilization_threshold
        )));
    }

    let adapter_for_root = CopilotAdapter;
    let root = cmd
        .root
        .clone()
        .or_else(|| adapter_for_root.default_session_root())
        .ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow("could not determine session root; pass --root explicitly".to_string())
        })?;
    if !root.is_dir() {
        return Err(
            ExitKind::UserError.into_anyhow(format!("session root not found: {}", root.display()))
        );
    }

    let since_dur = parse_since(&cmd.since)
        .map_err(|msg| ExitKind::UserError.into_anyhow(format!("invalid --since: {msg}")))?;

    let storage_cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let (ds, warnings_handle) = build_data_source(agent_name, &root, &storage_cfg, no_cache)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("{e}")))?;

    let mut all_refs = ds.discover(since_dur).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("scanning {}: {e}", root.display()))
    })?;
    all_refs.sort_by(|a, b| {
        b.started_at_ms
            .cmp(&a.started_at_ms)
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut reports: Vec<AnalysisReport> = Vec::with_capacity(all_refs.len());
    let mut episodes_vec: Vec<Episodes> = Vec::with_capacity(all_refs.len());
    let mut failure_count: usize = 0;
    for sref in &all_refs {
        let report = match ds.load_session(&sref.id) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    session = %agentprof_core::observability::pii::hash_short(&sref.id),
                    error = %e,
                    "load_session failed; skipping session"
                );
                failure_count += 1;
                continue;
            }
        };
        let episodes = match ds.load_episodes(&sref.id) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    session = %agentprof_core::observability::pii::hash_short(&sref.id),
                    error = %e,
                    "load_episodes failed; using empty Episodes for percentile pool"
                );
                Episodes::default()
            }
        };
        reports.push(report);
        episodes_vec.push(episodes);
    }

    let refs_total = all_refs.len();
    let any_report = compute_phase2(
        cmd,
        reports,
        episodes_vec,
        failure_count,
        since_dur,
        refs_total,
    )?;
    crate::cmd::list::drain_and_emit_warnings(&warnings_handle, quiet);
    Ok((any_report, refs_total))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_since_recognises_dhms_and_all() {
        assert_eq!(parse_since("7d").unwrap(), Duration::from_secs(7 * 86400));
        assert_eq!(parse_since("3h").unwrap(), Duration::from_secs(3 * 3600));
        assert_eq!(parse_since("5m").unwrap(), Duration::from_secs(5 * 60));
        assert_eq!(parse_since("45s").unwrap(), Duration::from_secs(45));
        assert_eq!(parse_since("all").unwrap(), Duration::MAX);
        assert!(parse_since("foo").is_err());
    }

    #[test]
    fn compute_aggregate_returns_empty_on_empty_root() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let cmd = AggregateCmd {
            agent: AgentKind::Copilot,
            root: Some(tmp.path().to_path_buf()),
            by: AggBy::Tool,
            since: "30d".to_string(),
            limit: 0,
            export: AggExportFormat::Md,
            output: None,
            low_utilization_threshold: 20.0,
            tokens_per_tool: agentprof_core::analyzer::waste::DEFAULT_HEURISTIC_TOKENS,
            tool_descriptions: None,
        };
        let adapter = CopilotAdapter;
        let (any, refs_total) = compute_aggregate(&adapter, &cmd)
            .expect("compute_aggregate should succeed on empty root");
        assert_eq!(refs_total, 0);
        let bucket_count = match &any {
            AnyAggregateReport::Tool(r) => r.buckets.len(),
            AnyAggregateReport::McpServer(r) => r.buckets.len(),
            AnyAggregateReport::Day(r) => r.buckets.len(),
            AnyAggregateReport::Model(r) => r.buckets.len(),
            _ => panic!("new AnyAggregateReport variant; extend test"),
        };
        assert_eq!(bucket_count, 0);
    }
}
