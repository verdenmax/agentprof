//! `agentprof list` subcommand.
//!
//! Discovers Copilot sessions in the default session-state root (or
//! `--root` override), filters by `--since DURATION` (mtime), truncates
//! to `--limit N`, runs the full `analyze` pipeline per session, and
//! prints a 7-column plain-text table to stdout. Per-session failures
//! degrade gracefully (skipped row, summary line to stderr at end).
//!
//! Defaults `--since 7d --limit 20` keep typical invocations bounded
//! (≈ a few seconds for the developer's own session-state dir).
//!
//! ## M2.1 T5.2 — dual-path data source
//!
//! Discovery and per-session load go through
//! [`build_data_source`]
//! instead of [`agentprof_adapters::copilot::CopilotAdapter`] directly.
//! When `--no-cache` is **not** set and the storage opens cleanly, the
//! returned source is a [`DualPathDataSource`]
//! that merges the live file system with the `SQLite` cache; otherwise
//! it falls back to a bare adapter wrapper. Per-session divergences
//! detected during merge are drained to stderr after the loop unless
//! `--quiet` is set.
//!
//! [`build_data_source`]: agentprof_cli::data_source_factory::build_data_source
//! [`DualPathDataSource`]: agentprof_cli::data_source::DualPathDataSource

use std::fmt::Write as _;
use std::io::IsTerminal as _;
use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Args;

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::{Adapter, AgentKind};
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::datasource::{SessionDataSource, SessionRef};

use crate::cmd::since::parse_since;

use crate::cmd::exit::ExitKind;
use agentprof_cli::config::resolve_storage_config;
use agentprof_cli::data_source_factory::build_data_source;
use agentprof_storage::config::PartialStorageConfig;

/// Arguments for `agentprof list`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct ListCmd {
    /// Agent whose sessions to list. M1.6.1 supports only `copilot`.
    #[arg(long, value_enum, default_value_t = AgentKind::Copilot)]
    pub agent: AgentKind,

    /// Custom session-state root directory. Defaults to the adapter's
    /// own convention (e.g. `~/.copilot/session-state/` for Copilot).
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Show sessions modified within this duration. Format:
    /// `<N>d` / `<N>h` / `<N>m` / `<N>s` / `all`.
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Maximum sessions to show. `0` means unlimited.
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

/// Successfully-analyzed row destined for the output table.
#[derive(Debug, Clone)]
struct ListRow {
    id: String,
    started_at: DateTime<Utc>,
    model: Option<String>,
    turns: usize,
    out_tokens: u64,
    duration: Option<chrono::Duration>,
    size_bytes: u64,
}

/// Run `agentprof list`.
///
/// # Errors
///
/// Returns `anyhow::Error` whose downcast target is [`ExitKind`]:
/// - `UserError` (1): unknown adapter / missing root / bad `--since`.
/// - `DataError` (2): ALL sessions failed to parse (returns empty table).
/// - Otherwise exits 0 even on partial per-session failures (summarized
///   to stderr).
#[allow(clippy::needless_pass_by_value)]
#[tracing::instrument(
    name = "cmd.list",
    skip_all,
    fields(agent = "copilot", since = %cmd.since, limit = cmd.limit, no_cache = no_cache, quiet = quiet)
)]
pub fn run(
    cmd: ListCmd,
    _cfg: &crate::cmd::LogConfig,
    _tracing_handle: &crate::cmd::TracingHandle,
    no_cache: bool,
    storage_path: Option<PathBuf>,
    quiet: bool,
) -> Result<()> {
    let agent_name = match cmd.agent {
        AgentKind::Copilot => "copilot",
        other => {
            return Err(ExitKind::UserError.into_anyhow(format!(
                "{other:?} adapter not yet implemented (M1.6.1 supports copilot only)",
            )));
        }
    };

    let since = parse_since(&cmd.since)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("invalid --since: {e}")))?;

    // Resolve root via the agent's adapter convention. The data source
    // factory needs a concrete path; for default-root cases we still
    // delegate to `CopilotAdapter::default_session_root()` rather than
    // duplicating the resolution rule here.
    let adapter_for_root = CopilotAdapter;
    let root = cmd
        .root
        .or_else(|| adapter_for_root.default_session_root())
        .ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow("could not determine session root (set --root or HOME)".to_string())
        })?;
    if !root.is_dir() {
        return Err(
            ExitKind::UserError.into_anyhow(format!("session root not found: {}", root.display()))
        );
    }

    // M2.1 T5.2 — go through the data-source factory. The factory may
    // emit dual-path divergence warnings into the returned handle; we
    // drain them after the per-session loop and surface to stderr
    // unless `--quiet` is set.
    let storage_cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let (ds, warnings_handle) = build_data_source(agent_name, &root, &storage_cfg, no_cache)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("{e}")))?;

    let all_refs = ds.discover(since).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("scanning {}: {e}", root.display()))
    })?;
    let total_discovered = all_refs.len();
    let filtered: Vec<SessionRef> = all_refs
        .into_iter()
        .take(if cmd.limit == 0 {
            usize::MAX
        } else {
            cmd.limit
        })
        .collect();

    if filtered.is_empty() {
        println!(
            "(no sessions matching --since={} in {})",
            cmd.since,
            root.display()
        );
        drain_and_emit_warnings(&warnings_handle, quiet);
        return Ok(());
    }

    let mut rows: Vec<ListRow> = Vec::with_capacity(filtered.len());
    let mut failures: Vec<(String, anyhow::Error)> = Vec::new();
    for sref in &filtered {
        match analyze_one(ds.as_ref(), sref) {
            Ok(row) => rows.push(row),
            Err(e) => failures.push((sref.id.clone(), e)),
        }
    }

    if rows.is_empty() {
        drain_and_emit_warnings(&warnings_handle, quiet);
        return Err(ExitKind::DataError
            .into_anyhow(format!("all {} session(s) failed to parse", failures.len())));
    }

    let use_bold = std::io::stdout().is_terminal();
    let table = format_table(&rows, use_bold);
    print!("{table}");
    println!(
        "\n({} of {} sessions shown; --since={} --limit={})",
        rows.len(),
        total_discovered,
        cmd.since,
        cmd.limit,
    );
    if !failures.is_empty() {
        tracing::warn!(
            failure_count = failures.len(),
            "session(s) failed to parse (use `agentprof analyze --session <id>` for details)"
        );
        for (id, e) in &failures {
            tracing::warn!(session = %agentprof_core::observability::pii::hash_short(id), error = %format_args!("{e:#}"), "parse failure detail");
        }
    }
    drain_and_emit_warnings(&warnings_handle, quiet);
    Ok(())
}

/// Drain accumulated dual-path warnings from the shared handle and
/// emit each one as a single `agentprof: warn: …` line on stderr.
///
/// A no-op when `quiet` is `true` or the handle is empty (the typical
/// case for `--no-cache` or freshly-created caches). The format
/// matches spec §7.3:
///
/// ```text
/// agentprof: warn: session <id>: N fields differ (<a>, <b>, …); using adapter; will re-upsert
/// ```
fn drain_and_emit_warnings(
    handle: &agentprof_cli::data_source_factory::WarningsHandle,
    quiet: bool,
) {
    let warnings = {
        let mut guard = handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *guard)
    };
    if quiet || warnings.is_empty() {
        return;
    }
    for w in &warnings {
        eprintln!(
            "agentprof: warn: session {}: {} fields differ ({}); using adapter; will re-upsert",
            w.session_id,
            w.differing_fields.len(),
            w.differing_fields.join(", "),
        );
    }
}

fn analyze_one(ds: &dyn SessionDataSource, sref: &SessionRef) -> anyhow::Result<ListRow> {
    let report: AnalysisReport = ds.load_session(&sref.id)?;
    let model = report.turn_summary.iter().find_map(|t| t.model.clone());
    let turns = report.turn_summary.len();
    let out_tokens: u64 = report
        .turn_summary
        .iter()
        .filter_map(|t| t.output_tokens)
        .map(u64::from)
        .sum();
    let duration = match (report.turn_summary.first(), report.turn_summary.last()) {
        (Some(first), Some(last)) => Some(last.started_at - first.started_at),
        _ => None,
    };
    // Size is a file-system attribute; the `SessionDataSource` trait
    // does not surface it, so we compute it on the fly from
    // `sref.raw_path` (set by `AdapterDataSource`). Store-only entries
    // — id present in `SQLite` but no live file — naturally fall back
    // to `0` and render as `"0"` in the Size column.
    let size_bytes = sref
        .raw_path
        .as_deref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map_or(0, |m| m.len());
    Ok(ListRow {
        id: sref.id.clone(),
        started_at: report.meta.started_at,
        model,
        turns,
        out_tokens,
        duration,
        size_bytes,
    })
}

// `parse_since` moved to `crate::cmd::since` per full-review CLI #1
// (consolidation + saturating_mul). Re-export was considered; we
// import via `use` in the imports block above instead so each call
// site references the canonical path.

/// Compact human-readable count (1, 1.2k, 2.45M, 12.3G).
#[allow(clippy::cast_precision_loss)]
fn compact_count(n: u64) -> String {
    if n < 1_000 {
        format!("{n}")
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else if n < 1_000_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else {
        format!("{:.2}G", n as f64 / 1_000_000_000.0)
    }
}

/// Compact human-readable byte size (850, 50.0k, 40.1M, 1.2G).
#[allow(clippy::cast_precision_loss)]
fn compact_size(n: u64) -> String {
    const KB: f64 = 1024.0;
    if n < 1024 {
        format!("{n}")
    } else if n < 1024 * 1024 {
        format!("{:.1}k", n as f64 / KB)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1}M", n as f64 / (KB * KB))
    } else {
        format!("{:.2}G", n as f64 / (KB * KB * KB))
    }
}

/// Compact duration (mirrors `crates/agentprof-tui/src/views/format.rs::human_short`
/// — duplicated here to avoid an agentprof-tui dep just for one helper).
#[allow(clippy::cast_precision_loss)]
fn human_duration(d: chrono::Duration) -> String {
    let ms = d.num_milliseconds().abs();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

/// Render the 7-column table.
fn format_table(rows: &[ListRow], use_bold: bool) -> String {
    let headers = [
        "ID",
        "Started",
        "Model",
        "Turns",
        "Out-tokens",
        "Duration",
        "Size",
    ];
    let cells: Vec<[String; 7]> = rows
        .iter()
        .map(|r| {
            [
                r.id.clone(),
                r.started_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                r.model.clone().unwrap_or_else(|| "-".to_string()),
                format!("{}", r.turns),
                compact_count(r.out_tokens),
                r.duration.map_or_else(|| "-".to_string(), human_duration),
                compact_size(r.size_bytes),
            ]
        })
        .collect();
    let mut widths = [0usize; 7];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.len();
    }
    for row in &cells {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let bold_on = if use_bold { "\x1b[1m" } else { "" };
    let bold_off = if use_bold { "\x1b[0m" } else { "" };
    out.push_str(bold_on);
    for (i, h) in headers.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        let _ = write!(out, "{:width$}", h, width = widths[i]);
    }
    out.push_str(bold_off);
    out.push('\n');
    for row in &cells {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push_str("  ");
            }
            let _ = write!(out, "{:width$}", cell, width = widths[i]);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parse_since_recognises_dhms() {
        assert_eq!(parse_since("7d").unwrap(), Duration::from_secs(7 * 86400));
        assert_eq!(parse_since("24h").unwrap(), Duration::from_secs(24 * 3600));
        assert_eq!(parse_since("30m").unwrap(), Duration::from_secs(30 * 60));
        assert_eq!(parse_since("45s").unwrap(), Duration::from_secs(45));
    }

    #[test]
    fn parse_since_recognises_all() {
        assert_eq!(parse_since("all").unwrap(), Duration::MAX);
    }

    #[test]
    fn parse_since_rejects_garbage() {
        assert!(parse_since("foo").is_err());
        assert!(parse_since("7x").is_err());
        assert!(parse_since("xx7d").is_err());
    }

    #[test]
    fn compact_count_formats_buckets() {
        assert_eq!(compact_count(0), "0");
        assert_eq!(compact_count(42), "42");
        assert_eq!(compact_count(999), "999");
        assert_eq!(compact_count(1_000), "1.0k");
        assert_eq!(compact_count(1_234), "1.2k");
        assert_eq!(compact_count(999_999), "1000.0k");
        assert_eq!(compact_count(2_450_000), "2.45M");
        assert_eq!(compact_count(3_500_000_000), "3.50G");
    }

    #[test]
    fn compact_size_formats_buckets() {
        assert_eq!(compact_size(850), "850");
        assert_eq!(compact_size(1024 * 50), "50.0k");
        assert_eq!(compact_size(42_000_000), "40.1M");
        assert_eq!(compact_size(2 * 1024 * 1024 * 1024), "2.00G");
    }

    #[test]
    fn format_table_handles_empty_rows() {
        let table = format_table(&[], false);
        // Empty rows still emits the header row (1 line).
        assert_eq!(table.lines().count(), 1);
        assert!(table.contains("ID"));
        assert!(table.contains("Started"));
    }

    #[test]
    fn format_table_aligns_columns() {
        let rows = vec![
            ListRow {
                id: "shortid".to_string(),
                started_at: chrono::Utc::now(),
                model: Some("model-a".to_string()),
                turns: 5,
                out_tokens: 100,
                duration: Some(chrono::Duration::seconds(3)),
                size_bytes: 1024,
            },
            ListRow {
                id: "a-much-longer-session-id-here".to_string(),
                started_at: chrono::Utc::now(),
                model: None,
                turns: 50,
                out_tokens: 1_234_567,
                duration: None,
                size_bytes: 42_000_000,
            },
        ];
        let table = format_table(&rows, false);
        // 1 header + 2 data rows.
        assert_eq!(table.lines().count(), 3);
        // ID column wide enough to fit longest.
        assert!(table.contains("a-much-longer-session-id-here"));
        // None model rendered as "-".
        assert!(table.contains(" -      "));
    }
}
