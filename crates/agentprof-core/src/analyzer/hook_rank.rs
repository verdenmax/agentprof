//! Per-hook ranking rollup.
//!
//! One [`HookRankRow`] per hook name, sorted by `total_duration` descending.
//! Symmetric to [`crate::analyzer::tool_rank`](mod@crate::analyzer::tool_rank)
//! but tracks `synthesized_start` (orphan-end synthesis) instead of orphan-status.

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::analyzer::{duration_ms, tool_rank::percentile};
use crate::episode::{Episodes, HookCall};

/// Ranking row for one hook name.
///
/// `success_count` + `failure_count` == `call_count` (mutually exclusive).
/// `synthesized_start_count` is orthogonal (a synthesized hook is still
/// classified by its `success` flag).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::{hook_rank, HookRankRow};
/// use agentprof_core::episode::Episodes;
///
/// let rows: Vec<HookRankRow> = hook_rank(&Episodes::new());
/// assert!(rows.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookRankRow {
    /// Hook name (e.g. `"PreToolUse"`, `"SessionStart"`).
    pub name: String,
    /// Total invocations for this hook (success + failure).
    pub call_count: usize,
    /// Calls with `HookCall.success == true`.
    pub success_count: usize,
    /// Calls with `HookCall.success == false`.
    pub failure_count: usize,
    /// Calls with `HookCall.synthesized_start == true` (orphan end synthesized a start).
    pub synthesized_start_count: usize,
    /// Sum of every call's `span.duration()`.
    #[serde(with = "duration_ms")]
    pub total_duration: Duration,
    /// Approximate median per-call duration (nearest-rank percentile,
    /// not the averaged-when-even statistical median). For an even-sized
    /// sample this rounds up to the upper midpoint — see
    /// [`tool_rank::percentile`](crate::analyzer::tool_rank::percentile).
    #[serde(with = "duration_ms")]
    pub p50_duration: Duration,
    /// 95th-percentile per-call duration (nearest-rank). See
    /// [`tool_rank::percentile`](crate::analyzer::tool_rank::percentile).
    #[serde(with = "duration_ms")]
    pub p95_duration: Duration,
}

impl HookRankRow {
    /// Explicit constructor for cross-crate test code. See
    /// [`crate::analyzer::ToolRankRow::new`] rationale.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::HookRankRow;
    /// use chrono::Duration;
    ///
    /// let row = HookRankRow::new(
    ///     "PreToolUse".into(), 1, 1, 0, 0,
    ///     Duration::milliseconds(5),
    ///     Duration::milliseconds(5),
    ///     Duration::milliseconds(5),
    /// );
    /// assert_eq!(row.name, "PreToolUse");
    /// ```
    #[doc(hidden)]
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        name: String,
        call_count: usize,
        success_count: usize,
        failure_count: usize,
        synthesized_start_count: usize,
        total_duration: Duration,
        p50_duration: Duration,
        p95_duration: Duration,
    ) -> Self {
        Self {
            name,
            call_count,
            success_count,
            failure_count,
            synthesized_start_count,
            total_duration,
            p50_duration,
            p95_duration,
        }
    }
}

/// Compute per-hook rank rows, sorted by `total_duration` descending.
///
/// Hooks with zero calls are omitted. Reuses the percentile helper from
/// the [`crate::analyzer::tool_rank`](mod@crate::analyzer::tool_rank) module.
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
pub fn hook_rank(episodes: &Episodes) -> Vec<HookRankRow> {
    let mut rows: Vec<HookRankRow> = episodes
        .hooks
        .iter()
        .filter(|(_, ep)| !ep.calls.is_empty())
        .map(|(name, ep)| {
            let success_count = ep.calls.iter().filter(|c| c.success).count();
            let failure_count = ep.calls.iter().filter(|c| !c.success).count();
            let synthesized_start_count = ep.calls.iter().filter(|c| c.synthesized_start).count();
            let durations = collect_sorted_durations(&ep.calls);
            let p50_duration = percentile(&durations, 50.0);
            let p95_duration = percentile(&durations, 95.0);
            HookRankRow {
                name: name.clone(),
                call_count: ep.calls.len(),
                success_count,
                failure_count,
                synthesized_start_count,
                total_duration: ep.total_duration,
                p50_duration,
                p95_duration,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.total_duration.cmp(&a.total_duration));
    rows
}

fn collect_sorted_durations(calls: &[HookCall]) -> Vec<Duration> {
    let mut v: Vec<Duration> = calls.iter().map(|c| c.span.duration()).collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::episode::{turn::Span, Episodes, HookCall, HookEpisode};
    use chrono::{TimeZone, Utc};

    fn at(s: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, s).unwrap()
    }

    #[test]
    fn empty_episodes_returns_empty_rows() {
        assert!(hook_rank(&Episodes::new()).is_empty());
    }

    #[test]
    fn synthesized_start_counted_separately_from_failure() {
        let mut ep = Episodes::new();
        let mut hep = HookEpisode::new("PreToolUse".into());
        let mut c1 = HookCall::new(Span::new(at(0), at(1)));
        c1.synthesized_start = true;
        let mut c2 = HookCall::new(Span::new(at(2), at(4)));
        c2.success = false;
        hep.calls.push(c1);
        hep.calls.push(c2);
        hep.total_duration = Duration::seconds(3);
        ep.hooks.insert("PreToolUse".into(), hep);

        let rows = hook_rank(&ep);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].call_count, 2);
        assert_eq!(rows[0].success_count, 1);
        assert_eq!(rows[0].failure_count, 1);
        assert_eq!(rows[0].synthesized_start_count, 1);
        assert_eq!(
            rows[0].success_count + rows[0].failure_count,
            rows[0].call_count
        );
    }

    #[test]
    fn multi_hook_sorted_by_total_duration_desc() {
        let mut ep = Episodes::new();
        for (name, secs) in &[
            ("PreToolUse", 10_u32),
            ("SessionStart", 2),
            ("PostToolUse", 5),
        ] {
            let mut hep = HookEpisode::new((*name).into());
            hep.calls.push(HookCall::new(Span::new(at(0), at(*secs))));
            hep.total_duration = Duration::seconds(i64::from(*secs));
            ep.hooks.insert((*name).into(), hep);
        }
        let rows = hook_rank(&ep);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "PreToolUse");
        assert_eq!(rows[1].name, "PostToolUse");
        assert_eq!(rows[2].name, "SessionStart");
    }
}
