//! JSON renderer.

use serde::Serialize;

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::model::WasteReport;

/// Wire shape for `analyze --export json`.
///
/// Flattens [`AnalysisReport`] at the top level so the historical
/// field names (`meta`, `turn_summary`, `tool_rank`, `hook_rank`,
/// `warnings`, `parse_warnings`) remain stable for downstream tooling,
/// and conditionally adds `mcp_waste` when M1.6.5 `--section mcp-waste`
/// was requested. Absence of the field (when waste was not computed)
/// is encoded via `#[serde(skip_serializing_if = "Option::is_none")]`
/// so existing JSON consumers see a byte-identical payload.
///
/// Per ADR-0023 the `cache_metrics` field is **always** emitted (no
/// `skip_serializing_if`): consumers can rely on the key being present
/// and treat `null` as the unambiguous sentinel for "the session had
/// zero cache activity".
#[derive(Serialize)]
struct AnalyzeJsonOutput<'a> {
    #[serde(flatten)]
    report: &'a AnalysisReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp_waste: Option<&'a WasteReport>,
    /// Cache token analytics per ADR-0023. `null` when the session had
    /// zero cache activity (consumer scripts can rely on the field
    /// always being present and use `null` as the sentinel for
    /// "this session did not touch prompt caching").
    cache_metrics: Option<agentprof_core::analyzer::cache::CacheMetrics>,
}

/// Serialize the full report as pretty JSON.
///
/// The exact JSON shape matches `AnalysisReport`'s serde representation
/// (flattened at top level); downstream tooling can rely on stable
/// field names (`meta`, `turn_summary`, `tool_rank`, `hook_rank`,
/// `warnings`) and the integer-milliseconds Duration encoding via
/// `duration_ms` helpers. When `--section mcp-waste` is requested an
/// additional top-level `mcp_waste` object appears. A top-level
/// `cache_metrics` field is always present (per ADR-0023): a
/// structured object when the session touched prompt caching, or
/// `null` otherwise.
///
/// # Errors
///
/// Returns `serde_json::Error` only if serialization of a field fails,
/// which cannot happen for the current `AnalysisReport` shape (all
/// fields use derive-Serialize-safe types). The Result is kept for
/// future-proofing.
pub fn render(
    report: &AnalysisReport,
    mcp_waste: Option<&WasteReport>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&AnalyzeJsonOutput {
        report,
        mcp_waste,
        cache_metrics: report.cache_metrics(),
    })
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::{AnalysisReport, ModelUsage};
    use agentprof_core::model::SessionMeta;
    use chrono::Utc;

    fn make_report(cache_read: u64, cache_write: u64) -> AnalysisReport {
        let mut r = AnalysisReport::new(SessionMeta::new(
            "s".into(),
            AgentKind::Copilot,
            Utc::now(),
            false,
        ));
        let mut u = ModelUsage::default();
        u.cache_read_tokens = cache_read;
        u.cache_write_tokens = cache_write;
        u.input_tokens = 1_000;
        let mut map = std::collections::BTreeMap::new();
        map.insert("test-model".to_string(), u);
        r.model_metrics = Some(map);
        r
    }

    #[test]
    fn json_has_cache_metrics_object_when_present() {
        let report = make_report(500, 100);
        let json = render(&report, None).expect("render must succeed");
        assert!(
            json.contains("\"cache_metrics\""),
            "missing cache_metrics key: {json}"
        );
        assert!(
            json.contains("\"creation\": 100"),
            "missing creation token count: {json}"
        );
        assert!(
            json.contains("\"read\": 500"),
            "missing read token count: {json}"
        );
    }

    #[test]
    fn json_has_cache_metrics_null_when_absent() {
        let report = make_report(0, 0);
        let json = render(&report, None).expect("render must succeed");
        assert!(
            json.contains("\"cache_metrics\": null"),
            "expected cache_metrics: null, got: {json}"
        );
    }
}
