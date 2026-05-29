//! JSON renderer.

use agentprof_core::analyzer::AnalysisReport;

/// Serialize the full report as pretty JSON.
///
/// The exact JSON shape matches `AnalysisReport`'s serde representation;
/// downstream tooling can rely on stable field names (`meta`,
/// `turn_summary`, `tool_rank`, `hook_rank`, `warnings`) and the
/// integer-milliseconds Duration encoding via `duration_ms` helpers.
///
/// # Errors
///
/// Returns `serde_json::Error` only if serialization of a field fails,
/// which cannot happen for the current `AnalysisReport` shape (all
/// fields use derive-Serialize-safe types). The Result is kept for
/// future-proofing.
pub fn render(report: &AnalysisReport) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}
