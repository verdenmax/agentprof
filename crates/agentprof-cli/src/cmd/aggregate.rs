//! `agentprof aggregate` subcommand (M1.6.2).
//!
//! Discovers sessions under `--root` (or the adapter's default
//! session-state root), filters by `--since DURATION` (mtime),
//! parses sequentially (D-2 — rayon deferred), computes the
//! requested cross-session aggregate, and renders to md / json /
//! csv / html.

use std::io::Write as _;
use std::num::ParseIntError;
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

use crate::cmd::analyze::ExitKind;
use crate::cmd::format;

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

    /// Output format. TUI deferred to M1.6.3.
    #[arg(long, value_enum, default_value_t = AggExportFormat::Md)]
    pub export: AggExportFormat,

    /// Write to file instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Percentage threshold below which `--by day` rows are marked
    /// low-utilization. Range 0.0 to 100.0. Default 20.
    #[arg(long, default_value_t = 20.0)]
    pub low_utilization_threshold: f32,
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
pub fn run(cmd: AggregateCmd) -> Result<()> {
    // Resolve adapter (M1.6.3 = copilot only).
    let adapter = match cmd.agent {
        AgentKind::Copilot => CopilotAdapter,
        other => {
            return Err(ExitKind::UserError.into_anyhow(format!(
                "{other:?} adapter not yet implemented (M1.6.3 supports copilot only)"
            )));
        }
    };

    let any_report = compute_aggregate(&adapter, &cmd)?;

    // Dispatch TUI before the renderers (it needs the raw report).
    if matches!(cmd.export, AggExportFormat::Tui) {
        return run_tui_for_aggregate(any_report);
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
/// the populated [`AnyAggregateReport`] without rendering or writing.
///
/// Used by both [`run`] (for md/json/csv/html/tui dispatch) and by
/// `cmd::watch::run` (for live reload in `watch aggregate` mode).
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
) -> Result<AnyAggregateReport> {
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
    let mut failure_count: usize = 0;

    for sref in &refs {
        match load_and_analyze(adapter, sref) {
            Ok((report, episodes)) => {
                reports.push(report);
                episodes_vec.push(episodes);
            }
            Err(e) => {
                eprintln!(
                    "agentprof: warning: failed to parse session {}: {e:#}",
                    sref.id
                );
                failure_count += 1;
            }
        }
    }

    if reports.is_empty() && failure_count > 0 {
        return Err(ExitKind::DataError
            .into_anyhow(format!("all {failure_count} session(s) failed to parse")));
    }

    if refs.is_empty() {
        eprintln!(
            "agentprof: no sessions matching --since={} under {}",
            cmd.since,
            root.display()
        );
    }

    // Dispatch on --by.
    let mut any_report = match cmd.by.to_key() {
        AggregateKey::Tool => AnyAggregateReport::Tool(aggregate_by_tool(&reports, &episodes_vec)),
        AggregateKey::McpServer => {
            AnyAggregateReport::McpServer(aggregate_by_mcp_server(&reports, &episodes_vec))
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

    // Aggregators don't know --since or failure_count; CLI fills them.
    fill_metadata(&mut any_report, since_dur_chrono(since_dur), failure_count);

    // Apply --limit.
    if cmd.limit > 0 {
        truncate_buckets(&mut any_report, cmd.limit);
    }

    // Stderr summary on partial failures.
    if failure_count > 0 {
        let total = reports.len() + failure_count;
        eprintln!(
            "agentprof: aggregated {} of {} sessions ({} failed)",
            reports.len(),
            total,
            failure_count
        );
    }

    Ok(any_report)
}

fn run_tui_for_aggregate(any_report: AnyAggregateReport) -> Result<()> {
    use std::io::IsTerminal as _;
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err(ExitKind::OutputError.into_anyhow(
            "agentprof aggregate --export tui requires both stdin and stdout to be TTYs; \
             use --export md|json|csv|html for headless output (e.g. `agentprof aggregate \
             --by tool --since 7d --export md`)"
                .to_string(),
        ));
    }
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
    Ok((report, episodes))
}

/// Parse `--since` value into a [`Duration`].
///
/// Accepts `<N>d` / `<N>h` / `<N>m` / `<N>s` / `"all"` (unlimited).
/// Mirrors the helper in `cmd::list` to keep the two subcommands
/// independent — promoting to a shared module is a follow-up.
fn parse_since(s: &str) -> std::result::Result<Duration, String> {
    if s == "all" {
        return Ok(Duration::MAX);
    }
    let (n_str, unit_secs): (&str, u64) = match s.chars().last() {
        Some('s') => (&s[..s.len() - 1], 1),
        Some('m') => (&s[..s.len() - 1], 60),
        Some('h') => (&s[..s.len() - 1], 3600),
        Some('d') => (&s[..s.len() - 1], 86400),
        _ => {
            return Err(format!(
                "unrecognized --since: {s}; use <N>d/h/m/s or 'all'"
            ));
        }
    };
    let n: u64 = n_str
        .parse()
        .map_err(|e: ParseIntError| format!("not a number: {n_str} ({e})"))?;
    Ok(Duration::from_secs(n.saturating_mul(unit_secs)))
}

fn since_dur_chrono(d: Duration) -> chrono::Duration {
    let secs = i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
    chrono::Duration::try_seconds(secs).unwrap_or(chrono::Duration::MAX)
}

fn fill_metadata(r: &mut AnyAggregateReport, since: chrono::Duration, failure_count: usize) {
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
        };
        let adapter = CopilotAdapter;
        let any = compute_aggregate(&adapter, &cmd)
            .expect("compute_aggregate should succeed on empty root");
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
