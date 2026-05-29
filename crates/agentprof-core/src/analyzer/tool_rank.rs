//! Per-tool ranking rollup. Stub — replaced by Task 7.

use serde::{Deserialize, Serialize};

use crate::episode::Episodes;

/// One row per tool name.
///
/// Stub — Task 7 adds fields (call counts, p50/p95/max duration).
/// For now the struct is empty so [`AnalysisReport`] compiles.
///
/// [`AnalysisReport`]: crate::analyzer::AnalysisReport
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolRankRow {}

/// Compute per-tool rank rows.
///
/// Stub returning an empty `Vec`; Task 7 implements the real rollup.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::tool_rank;
/// use agentprof_core::episode::Episodes;
///
/// let rows = tool_rank(&Episodes::new());
/// assert!(rows.is_empty());
/// ```
#[must_use]
pub const fn tool_rank(_episodes: &Episodes) -> Vec<ToolRankRow> {
    Vec::new()
}
