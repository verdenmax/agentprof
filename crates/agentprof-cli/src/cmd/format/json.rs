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
#[derive(Serialize)]
struct AnalyzeJsonOutput<'a> {
    #[serde(flatten)]
    report: &'a AnalysisReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp_waste: Option<&'a WasteReport>,
}

/// Serialize the full report as pretty JSON.
///
/// The exact JSON shape matches `AnalysisReport`'s serde representation
/// (flattened at top level); downstream tooling can rely on stable
/// field names (`meta`, `turn_summary`, `tool_rank`, `hook_rank`,
/// `warnings`) and the integer-milliseconds Duration encoding via
/// `duration_ms` helpers. When `--section mcp-waste` is requested an
/// additional top-level `mcp_waste` object appears.
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
    serde_json::to_string_pretty(&AnalyzeJsonOutput { report, mcp_waste })
}
