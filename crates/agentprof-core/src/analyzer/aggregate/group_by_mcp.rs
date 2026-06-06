//! `aggregate_by_mcp_server` — group tools by MCP server prefix.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Duration;

use crate::analyzer::aggregate::{wall, AggregateKey, AggregateReport, McpServerBucket};
use crate::analyzer::AnalysisReport;
use crate::episode::Episodes;
use crate::model::ToolSource;

/// Aggregate MCP-only tools by their server prefix across N sessions.
///
/// Only [`ToolSource::Mcp`] rows are considered; built-in and skill
/// tools are dropped. Distinct tool names per server and distinct
/// sessions per server are counted with [`BTreeSet`]s.
///
/// Buckets are sorted by `total_duration` descending.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::group_by_mcp::aggregate_by_mcp_server;
/// let r = aggregate_by_mcp_server(&[], &[]);
/// assert!(r.buckets.is_empty());
/// ```
///
/// # Panics
///
/// If `reports.len() != episodes_per_report.len()`.
#[must_use]
#[tracing::instrument(name = "aggregator.group_by", skip_all, fields(key = "mcp-server", sessions = reports.len()))]
pub fn aggregate_by_mcp_server(
    reports: &[AnalysisReport],
    episodes_per_report: &[Episodes],
) -> AggregateReport<McpServerBucket> {
    assert_eq!(
        reports.len(),
        episodes_per_report.len(),
        "aggregate_by_mcp_server: reports and episodes_per_report length mismatch",
    );

    let mut acc: BTreeMap<String, TempMcpAcc> = BTreeMap::new();
    let mut total_wall = Duration::zero();

    for (idx, report) in reports.iter().enumerate() {
        let episodes = &episodes_per_report[idx];
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
                });
                entry.tool_names.insert(row.name.clone());
                entry.call_count += row.call_count;
                entry.failure_count += row.failure_count;
                entry.total_duration += row.total_duration;
                entry.sessions.insert(idx);
            }
        }
    }

    let mut buckets: Vec<McpServerBucket> = acc
        .into_values()
        .map(|t| {
            McpServerBucket::new(
                t.server,
                t.tool_names.len(),
                t.call_count,
                t.failure_count,
                t.total_duration,
                t.sessions.len(),
            )
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
}
