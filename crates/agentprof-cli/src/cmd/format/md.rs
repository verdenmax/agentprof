//! Markdown renderer. Real implementation lands in Task 11.

use agentprof_core::analyzer::AnalysisReport;

use crate::cmd::analyze::AnalysisSection;

/// Render `report` to markdown, respecting the `--section` filter.
///
/// Stub implementation: returns a one-line placeholder so the CLI compiles
/// end-to-end. Task 11 will replace this with the full table-heavy
/// markdown shape (per spec FR-4.1..FR-4.3).
#[must_use]
pub fn render(_report: &AnalysisReport, _sections: &[AnalysisSection]) -> String {
    "# agentprof analyze\n(md renderer stub — Task 11 fills this in)\n".into()
}
