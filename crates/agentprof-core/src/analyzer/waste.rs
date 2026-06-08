//! `compute_waste` and `aggregate_waste` — per-session and cross-session
//! MCP-server waste analysis.
//!
//! `compute_waste` turns one (`AnalysisReport`, `wire_loaded`, `config_loaded`)
//! triple into a `WasteReport`; `aggregate_waste` (added in T1.4) rolls many
//! `WasteReport`s up into an `AggregateWasteReport` (used by the `mcp-waste`
//! subcommand).
//!
//! See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md` §6
//! for the algorithm and `docs/internals/adr-0015-mcp-waste-architecture.md`
//! for design decisions.

use std::collections::{BTreeMap, BTreeSet};

// `SessionRef`, `AggregateWasteReport`, `McpServerCrossWaste`, and
// `McpToolUsageAcrossSessions` are pre-imported for T1.4's appended
// `aggregate_waste` (cross-session reducer); the `allow(unused_imports)`
// keeps the import block stable across the two commits.
#[allow(unused_imports)]
use crate::adapter::SessionRef;
use crate::analyzer::AnalysisReport;
#[allow(unused_imports)]
use crate::model::{
    AggregateWasteReport, LoadedSource, McpServerCrossWaste, McpServerWaste,
    McpToolUsageAcrossSessions, McpToolWaste, ToolSource, WasteDataSource, WasteReport,
};

/// Compute per-session MCP-server waste from an analysis report and the
/// "loaded" sets contributed by wire + (optional) mcp.json baseline.
///
/// `wire_loaded`: tools observed in `<tools_changed_notice>` blocks.
/// `config_loaded`: server → tool-list map from mcp.json (`None` if
/// mcp.json was absent or unparseable).
///
/// Returns a fully-populated `WasteReport` with `server_waste` sorted by
/// `unused_count` descending (ties by `server` ascending), and tools
/// within each server sorted alphabetically by `short_name`.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::{AnalysisReport, compute_waste};
/// use agentprof_core::model::SessionMeta;
/// use chrono::{TimeZone, Utc};
///
/// let meta = SessionMeta::new(
///     "s1".into(),
///     AgentKind::Copilot,
///     Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap(),
///     false,
/// );
/// let report = AnalysisReport::new(meta);
/// let wire = BTreeSet::new();
/// let r = compute_waste(&report, &wire, None);
/// assert_eq!(r.total_loaded_tool_count, 0);
/// ```
#[must_use]
#[tracing::instrument(
    name = "analyzer.waste",
    skip_all,
    fields(
        wire_size = wire_loaded.len(),
        has_config = config_loaded.is_some(),
    )
)]
pub fn compute_waste(
    report: &AnalysisReport,
    wire_loaded: &BTreeSet<String>,
    config_loaded: Option<&BTreeMap<String, Vec<String>>>,
) -> WasteReport {
    // Step 1: extract `called` map from report.tool_rank, MCP-only.
    let mut called: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for row in &report.tool_rank {
        if let ToolSource::Mcp { server } = &row.source {
            if let Some(short) = short_name(&row.name) {
                called
                    .entry(server.clone())
                    .or_default()
                    .insert(short.to_string(), row.call_count);
            }
        }
    }

    // Step 2: build loaded super-set with provenance.
    //   Initialize from wire (Wire), then merge config (Wire→Both / new→Config),
    //   then merge called (any new → InferredFromCall).
    let mut loaded: BTreeMap<(String, String), LoadedSource> = BTreeMap::new();
    for tool_name in wire_loaded {
        if let Some((server, short)) = split_full_name(tool_name) {
            loaded.insert((server, short), LoadedSource::Wire);
        }
    }
    if let Some(cfg) = config_loaded {
        for (server, tools) in cfg {
            for short in tools {
                let key = (server.clone(), short.clone());
                loaded
                    .entry(key)
                    .and_modify(|src| {
                        if matches!(src, LoadedSource::Wire) {
                            *src = LoadedSource::Both;
                        }
                    })
                    .or_insert(LoadedSource::Config);
            }
        }
    }
    for (server, tools_map) in &called {
        for short in tools_map.keys() {
            loaded
                .entry((server.clone(), short.clone()))
                .or_insert(LoadedSource::InferredFromCall);
        }
    }

    // Step 3: group by server.
    let mut by_server: BTreeMap<String, Vec<McpToolWaste>> = BTreeMap::new();
    for ((server, short), src) in &loaded {
        let call_count = called
            .get(server)
            .and_then(|m| m.get(short))
            .copied()
            .unwrap_or(0);
        by_server
            .entry(server.clone())
            .or_default()
            .push(McpToolWaste {
                tool_name: format!("mcp__{server}__{short}"),
                short_name: short.clone(),
                call_count,
                loaded_source: *src,
            });
    }

    // Step 4: build McpServerWaste vec, sort tools alphabetically, server-level totals.
    let mut server_waste: Vec<McpServerWaste> = by_server
        .into_iter()
        .map(|(server, mut tools)| {
            tools.sort_by(|a, b| a.short_name.cmp(&b.short_name));
            let loaded_count = tools.len();
            let called_count = tools.iter().filter(|t| t.call_count > 0).count();
            let unused_count = loaded_count - called_count;
            McpServerWaste {
                server,
                tools,
                loaded_count,
                called_count,
                unused_count,
                is_fully_unused: called_count == 0,
            }
        })
        .collect();

    // Step 5: sort servers by unused_count desc, ties by server asc.
    server_waste.sort_by(|a, b| {
        b.unused_count
            .cmp(&a.unused_count)
            .then_with(|| a.server.cmp(&b.server))
    });

    // Step 6: derive data_source enum + totals.
    let data_source = match (wire_loaded.is_empty(), config_loaded.is_some()) {
        (true, true) => WasteDataSource::Config,
        (false, true) => WasteDataSource::Both,
        (false, false) => WasteDataSource::Wire,
        (true, false) => WasteDataSource::None,
    };
    let total_loaded_tool_count = server_waste.iter().map(|s| s.loaded_count).sum();
    let total_unused_tool_count = server_waste.iter().map(|s| s.unused_count).sum();

    WasteReport {
        server_waste,
        data_source,
        total_loaded_tool_count,
        total_unused_tool_count,
    }
}

/// Split `mcp__<server>__<short>` into `("<server>", "<short>")`.
/// Returns `None` if the name does not match the MCP convention.
fn split_full_name(full: &str) -> Option<(String, String)> {
    let after_prefix = full.strip_prefix("mcp__")?;
    let (server, short) = after_prefix.split_once("__")?;
    if server.is_empty() || short.is_empty() {
        return None;
    }
    Some((server.to_string(), short.to_string()))
}

/// Extract the `short_name` (`<short>`) from `mcp__<server>__<short>`.
/// Returns `None` if the name does not match the MCP convention.
fn short_name(full: &str) -> Option<&str> {
    let after_prefix = full.strip_prefix("mcp__")?;
    let (_server, short) = after_prefix.split_once("__")?;
    if short.is_empty() {
        return None;
    }
    Some(short)
}

#[cfg(test)]
#[allow(clippy::iter_on_single_items)]
mod tests {
    use super::*;
    use crate::adapter::AgentKind;
    use crate::analyzer::ToolRankRow;
    use crate::model::SessionMeta;
    use chrono::{TimeZone, Utc};

    fn empty_report() -> AnalysisReport {
        AnalysisReport::new(SessionMeta::new(
            "s1".into(),
            AgentKind::Copilot,
            Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap(),
            false,
        ))
    }

    fn mcp_row(server: &str, tool: &str, calls: usize) -> ToolRankRow {
        ToolRankRow {
            name: format!("mcp__{server}__{tool}"),
            source: ToolSource::Mcp {
                server: server.into(),
            },
            call_count: calls,
            success_count: calls,
            failure_count: 0,
            orphan_count: 0,
            user_requested_count: 0,
            total_duration: chrono::Duration::zero(),
            p50_duration: chrono::Duration::zero(),
            p95_duration: chrono::Duration::zero(),
            max_duration: chrono::Duration::zero(),
            is_user_blocking: false,
        }
    }

    fn builtin_row(name: &str, calls: usize) -> ToolRankRow {
        ToolRankRow {
            name: name.into(),
            source: ToolSource::Builtin,
            call_count: calls,
            success_count: calls,
            failure_count: 0,
            orphan_count: 0,
            user_requested_count: 0,
            total_duration: chrono::Duration::zero(),
            p50_duration: chrono::Duration::zero(),
            p95_duration: chrono::Duration::zero(),
            max_duration: chrono::Duration::zero(),
            is_user_blocking: false,
        }
    }

    #[test]
    fn empty_inputs_produce_empty_report_with_none_source() {
        let r = compute_waste(&empty_report(), &BTreeSet::new(), None);
        assert!(r.server_waste.is_empty());
        assert!(matches!(r.data_source, WasteDataSource::None));
        assert_eq!(r.total_loaded_tool_count, 0);
        assert_eq!(r.total_unused_tool_count, 0);
    }

    #[test]
    fn wire_only_all_unused_yields_wire_data_source() {
        let wire: BTreeSet<String> = ["mcp__github__search", "mcp__github__create"]
            .into_iter()
            .map(String::from)
            .collect();
        let r = compute_waste(&empty_report(), &wire, None);
        assert!(matches!(r.data_source, WasteDataSource::Wire));
        assert_eq!(r.server_waste.len(), 1);
        assert_eq!(r.server_waste[0].server, "github");
        assert_eq!(r.server_waste[0].loaded_count, 2);
        assert_eq!(r.server_waste[0].called_count, 0);
        assert_eq!(r.server_waste[0].unused_count, 2);
        assert!(r.server_waste[0].is_fully_unused);
        for t in &r.server_waste[0].tools {
            assert!(matches!(t.loaded_source, LoadedSource::Wire));
        }
    }

    #[test]
    fn called_only_no_baseline_yields_inferred_from_call_source() {
        let mut report = empty_report();
        report.tool_rank.push(mcp_row("github", "search", 3));
        let r = compute_waste(&report, &BTreeSet::new(), None);
        assert_eq!(r.server_waste.len(), 1);
        assert!(matches!(
            r.server_waste[0].tools[0].loaded_source,
            LoadedSource::InferredFromCall
        ));
        // Note: `data_source` is None — neither wire nor config contributed.
        assert!(matches!(r.data_source, WasteDataSource::None));
    }

    #[test]
    fn wire_plus_config_marks_tool_as_both() {
        let wire: BTreeSet<String> = ["mcp__github__search"]
            .into_iter()
            .map(String::from)
            .collect();
        let cfg: BTreeMap<String, Vec<String>> =
            [("github".to_string(), vec!["search".to_string()])]
                .into_iter()
                .collect();
        let r = compute_waste(&empty_report(), &wire, Some(&cfg));
        assert!(matches!(r.data_source, WasteDataSource::Both));
        assert_eq!(r.server_waste[0].tools[0].loaded_source, LoadedSource::Both);
    }

    #[test]
    fn config_only_no_wire_yields_config_data_source() {
        let cfg: BTreeMap<String, Vec<String>> = [(
            "github".to_string(),
            vec!["search".to_string(), "create".to_string()],
        )]
        .into_iter()
        .collect();
        let r = compute_waste(&empty_report(), &BTreeSet::new(), Some(&cfg));
        assert!(matches!(r.data_source, WasteDataSource::Config));
        assert_eq!(r.server_waste[0].tools.len(), 2);
        for t in &r.server_waste[0].tools {
            assert!(matches!(t.loaded_source, LoadedSource::Config));
        }
    }

    #[test]
    fn partial_usage_is_not_fully_unused() {
        let wire: BTreeSet<String> = ["mcp__github__search", "mcp__github__create"]
            .into_iter()
            .map(String::from)
            .collect();
        let mut report = empty_report();
        report.tool_rank.push(mcp_row("github", "search", 5));
        let r = compute_waste(&report, &wire, None);
        assert_eq!(r.server_waste[0].called_count, 1);
        assert_eq!(r.server_waste[0].unused_count, 1);
        assert!(!r.server_waste[0].is_fully_unused);
    }

    #[test]
    fn multi_server_sorts_by_unused_count_desc() {
        let wire: BTreeSet<String> = [
            "mcp__a__t1",
            "mcp__a__t2",
            "mcp__a__t3",
            "mcp__b__t1",
            "mcp__c__t1",
            "mcp__c__t2",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let r = compute_waste(&empty_report(), &wire, None);
        assert_eq!(r.server_waste[0].server, "a");
        assert_eq!(r.server_waste[1].server, "c");
        assert_eq!(r.server_waste[2].server, "b");
    }

    #[test]
    fn builtin_tools_are_filtered_out() {
        let mut report = empty_report();
        report.tool_rank.push(builtin_row("bash", 10));
        report.tool_rank.push(mcp_row("github", "search", 1));
        let r = compute_waste(&report, &BTreeSet::new(), None);
        assert_eq!(r.server_waste.len(), 1, "only MCP servers in report");
        assert_eq!(r.server_waste[0].server, "github");
    }

    #[test]
    fn totals_sum_across_servers() {
        let wire: BTreeSet<String> = ["mcp__a__t1", "mcp__a__t2", "mcp__b__t1"]
            .into_iter()
            .map(String::from)
            .collect();
        let r = compute_waste(&empty_report(), &wire, None);
        assert_eq!(r.total_loaded_tool_count, 3);
        assert_eq!(r.total_unused_tool_count, 3);
    }

    #[test]
    fn tools_within_server_sorted_alphabetically_by_short_name() {
        let wire: BTreeSet<String> = [
            "mcp__github__zebra",
            "mcp__github__alpha",
            "mcp__github__mango",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let r = compute_waste(&empty_report(), &wire, None);
        let shorts: Vec<&str> = r.server_waste[0]
            .tools
            .iter()
            .map(|t| t.short_name.as_str())
            .collect();
        assert_eq!(shorts, vec!["alpha", "mango", "zebra"]);
    }
}
