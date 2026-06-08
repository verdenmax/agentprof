//! `aggregate_by_mcp_server` — group tools by MCP server prefix.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Duration;

use crate::analyzer::aggregate::{wall, AggregateKey, AggregateReport, McpServerBucket};
use crate::analyzer::AnalysisReport;
use crate::episode::Episodes;
use crate::model::{ToolSource, WasteReport};

/// Aggregate MCP-only tools by their server prefix across N sessions.
///
/// Only [`ToolSource::Mcp`] rows are considered; built-in and skill
/// tools are dropped. Distinct tool names per server and distinct
/// sessions per server are counted with [`BTreeSet`]s.
///
/// `waste_per_report` is the parallel per-session [`WasteReport`]
/// produced by the cli layer (it owns the wire-parser and `mcp.json`
/// loader, both living in `agentprof-adapters`; core stays a
/// dependency leaf). Each [`WasteReport`] contributes
/// `unused_tool_count` (summed) and `fully_unused_session_count`
/// (`+1` per session whose `is_fully_unused` is `true`) to the
/// corresponding [`McpServerBucket`]. Pass
/// `vec![WasteReport::default(); reports.len()]` (or an equal-length
/// empty-waste vector) when no waste data is available — bucket
/// waste fields then stay at `0`.
///
/// Buckets are sorted by `total_duration` descending.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::group_by_mcp::aggregate_by_mcp_server;
/// let r = aggregate_by_mcp_server(&[], &[], &[]);
/// assert!(r.buckets.is_empty());
/// ```
///
/// # Panics
///
/// If `reports.len() != episodes_per_report.len()` or
/// `reports.len() != waste_per_report.len()`.
#[must_use]
#[tracing::instrument(name = "aggregator.group_by", skip_all, fields(key = "mcp-server", sessions = reports.len()))]
pub fn aggregate_by_mcp_server(
    reports: &[AnalysisReport],
    episodes_per_report: &[Episodes],
    waste_per_report: &[WasteReport],
) -> AggregateReport<McpServerBucket> {
    assert_eq!(
        reports.len(),
        episodes_per_report.len(),
        "aggregate_by_mcp_server: reports and episodes_per_report length mismatch",
    );
    assert_eq!(
        reports.len(),
        waste_per_report.len(),
        "aggregate_by_mcp_server: reports and waste_per_report length mismatch",
    );

    let mut acc: BTreeMap<String, TempMcpAcc> = BTreeMap::new();
    let mut total_wall = Duration::zero();

    for (idx, (report, episodes)) in reports.iter().zip(episodes_per_report.iter()).enumerate() {
        total_wall += wall::compute_wall(episodes, report.meta.started_at);

        for row in &report.tool_rank {
            if let ToolSource::Mcp { server } = &row.source {
                let entry = acc.entry(server.clone()).or_insert_with(|| TempMcpAcc {
                    server: server.clone(),
                    tool_names: BTreeSet::new(),
                    call_count: 0,
                    failure_count: 0,
                    total_duration: Duration::zero(),
                    sessions: BTreeSet::new(),
                    unused_tool_count: 0,
                    fully_unused_session_count: 0,
                    wasted_tokens: 0,
                });
                entry.tool_names.insert(row.name.clone());
                entry.call_count += row.call_count;
                entry.failure_count += row.failure_count;
                entry.total_duration += row.total_duration;
                entry.sessions.insert(idx);
            }
        }
    }

    // M1.6.5: merge waste data per session. Servers that appear only
    // in waste (loaded but never called) get a bucket too — they're
    // exactly the "fully unused" case spec §7.2 wants to surface.
    // M1.6.6: also sum `unused_tokens` into the bucket's
    // `wasted_tokens` field (spec §7.5).
    for w in waste_per_report {
        for sw in &w.server_waste {
            let entry = acc.entry(sw.server.clone()).or_insert_with(|| TempMcpAcc {
                server: sw.server.clone(),
                tool_names: BTreeSet::new(),
                call_count: 0,
                failure_count: 0,
                total_duration: Duration::zero(),
                sessions: BTreeSet::new(),
                unused_tool_count: 0,
                fully_unused_session_count: 0,
                wasted_tokens: 0,
            });
            entry.unused_tool_count += sw.unused_count;
            if sw.is_fully_unused {
                entry.fully_unused_session_count += 1;
            }
            entry.wasted_tokens = entry.wasted_tokens.saturating_add(sw.unused_tokens);
        }
    }

    let mut buckets: Vec<McpServerBucket> = acc
        .into_values()
        .map(|t| McpServerBucket {
            server: t.server,
            tool_count: t.tool_names.len(),
            call_count: t.call_count,
            failure_count: t.failure_count,
            total_duration: t.total_duration,
            session_count: t.sessions.len(),
            unused_tool_count: t.unused_tool_count,
            fully_unused_session_count: t.fully_unused_session_count,
            wasted_tokens: t.wasted_tokens,
        })
        .collect();
    buckets.sort_by(|a, b| {
        b.total_duration
            .cmp(&a.total_duration)
            .then_with(|| a.server.cmp(&b.server))
    });

    let report = AggregateReport::new(
        AggregateKey::McpServer,
        None,
        reports.len(),
        0,
        total_wall,
        buckets,
    );
    tracing::debug!(buckets = report.buckets.len(), "aggregated");
    report
}

struct TempMcpAcc {
    server: String,
    tool_names: BTreeSet<String>,
    call_count: usize,
    failure_count: usize,
    total_duration: Duration,
    sessions: BTreeSet<usize>,
    unused_tool_count: usize,
    fully_unused_session_count: usize,
    wasted_tokens: u64,
}
