//! Per-hook ranking rollup. Stub — replaced by Task 8.

use serde::{Deserialize, Serialize};

use crate::episode::Episodes;

/// One row per hook name.
///
/// Stub — Task 8 adds fields (call counts, p50/p95 duration,
/// success/failure). For now the struct is empty so [`AnalysisReport`]
/// compiles.
///
/// [`AnalysisReport`]: crate::analyzer::AnalysisReport
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookRankRow {}

/// Compute per-hook rank rows.
///
/// Stub returning an empty `Vec`; Task 8 implements the real rollup.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::hook_rank;
/// use agentprof_core::episode::Episodes;
///
/// let rows = hook_rank(&Episodes::new());
/// assert!(rows.is_empty());
/// ```
#[must_use]
pub const fn hook_rank(_episodes: &Episodes) -> Vec<HookRankRow> {
    Vec::new()
}
