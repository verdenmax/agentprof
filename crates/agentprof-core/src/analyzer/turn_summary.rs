//! Per-Turn summary rollup. Stub — replaced by Task 6.

use serde::{Deserialize, Serialize};

use crate::episode::Episodes;

/// One row per Turn.
///
/// Stub — Task 6 adds fields (status, duration, model, mode, counts,
/// `output_tokens`). For now the struct is empty so [`AnalysisReport`]
/// compiles with its `Vec<TurnSummaryRow>` field.
///
/// [`AnalysisReport`]: crate::analyzer::AnalysisReport
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TurnSummaryRow {}

/// Compute per-Turn summary rows.
///
/// Stub returning an empty `Vec`; Task 6 implements the real rollup.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::turn_summary;
/// use agentprof_core::episode::Episodes;
///
/// let rows = turn_summary(&Episodes::new());
/// assert!(rows.is_empty());
/// ```
#[must_use]
pub const fn turn_summary(_episodes: &Episodes) -> Vec<TurnSummaryRow> {
    Vec::new()
}
