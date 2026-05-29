//! Per-tool ranking rollup.
//!
//! One [`ToolRankRow`] per tool name, sorted by `total_duration` descending.
//! Tools with zero recorded calls are filtered out.

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::analyzer::duration_ms;
use crate::episode::{Episodes, ToolCall, ToolCallStatus};
use crate::model::ToolSource;

/// Ranking row for one tool name.
///
/// Counts categorize the tool's calls by status; the duration percentiles
/// summarize per-call latency. Sort key: `total_duration` (descending) at
/// the [`tool_rank`] level — the row itself stores raw fields without
/// embedded sort metadata.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::{tool_rank, ToolRankRow};
/// use agentprof_core::episode::Episodes;
///
/// let rows: Vec<ToolRankRow> = tool_rank(&Episodes::new());
/// assert!(rows.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolRankRow {
    /// Tool name (e.g. `"bash"`, `"view"`, `"mcp__github__search_issues"`).
    pub name: String,
    /// Classified source (Builtin / Mcp / Skill / User / Unknown).
    pub source: ToolSource,
    /// Total invocations recorded for this tool (success + failure + orphan).
    pub call_count: usize,
    /// Calls with `ToolCallStatus::Success`.
    pub success_count: usize,
    /// Calls with `ToolCallStatus::Failure`.
    pub failure_count: usize,
    /// Calls with `ToolCallStatus::OrphanSynthesizedStart` OR
    /// `ToolCallStatus::OpenAtEndOfSession`.
    pub orphan_count: usize,
    /// Calls flagged `user_requested = true`.
    pub user_requested_count: usize,
    /// Sum of every call's `span.duration()`.
    #[serde(with = "duration_ms")]
    pub total_duration: Duration,
    /// Approximate median per-call duration (nearest-rank percentile,
    /// not the averaged-when-even statistical median). For an even-sized
    /// sample this rounds up to the upper midpoint — see [`percentile`].
    #[serde(with = "duration_ms")]
    pub p50_duration: Duration,
    /// 95th-percentile per-call duration (nearest-rank). See [`percentile`].
    #[serde(with = "duration_ms")]
    pub p95_duration: Duration,
    /// Longest single call.
    #[serde(with = "duration_ms")]
    pub max_duration: Duration,
}

/// Compute per-tool rank rows, sorted by `total_duration` descending.
///
/// Tools with zero calls are omitted from the output. Iteration order
/// of `Episodes.tools` is stable (`BTreeMap`), but the output `Vec` is
/// re-sorted by duration for "biggest first" reporting.
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
pub fn tool_rank(episodes: &Episodes) -> Vec<ToolRankRow> {
    let mut rows: Vec<ToolRankRow> = episodes
        .tools
        .iter()
        .filter(|(_, ep)| !ep.calls.is_empty())
        .map(|(name, ep)| {
            let success_count = ep
                .calls
                .iter()
                .filter(|c| matches!(c.status, ToolCallStatus::Success))
                .count();
            let failure_count = ep
                .calls
                .iter()
                .filter(|c| matches!(c.status, ToolCallStatus::Failure { .. }))
                .count();
            let orphan_count = ep
                .calls
                .iter()
                .filter(|c| {
                    matches!(
                        c.status,
                        ToolCallStatus::OrphanSynthesizedStart | ToolCallStatus::OpenAtEndOfSession
                    )
                })
                .count();
            let user_requested_count = ep.calls.iter().filter(|c| c.user_requested).count();
            let durations = collect_sorted_durations(&ep.calls);
            let p50_duration = percentile(&durations, 50.0);
            let p95_duration = percentile(&durations, 95.0);
            let max_duration = durations.last().copied().unwrap_or_else(Duration::zero);
            ToolRankRow {
                name: name.clone(),
                source: ep.source.clone(),
                call_count: ep.calls.len(),
                success_count,
                failure_count,
                orphan_count,
                user_requested_count,
                total_duration: ep.total_duration,
                p50_duration,
                p95_duration,
                max_duration,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.total_duration.cmp(&a.total_duration));
    rows
}

fn collect_sorted_durations(calls: &[ToolCall]) -> Vec<Duration> {
    let mut v: Vec<Duration> = calls.iter().map(|c| c.span.duration()).collect();
    v.sort_unstable();
    v
}

/// Percentile by sort-and-index. `pct` is `0.0..=100.0`.
///
/// Algorithm: nearest-rank — `slice[round((pct/100) * (len-1))]` with
/// `f64::round` (half-away-from-zero). This is NOT the averaged-when-even
/// statistical median: for an even-sized sample, p50 rounds **up** to the
/// upper midpoint (e.g. `[1s, 2s]` → `p50 = 2s`, not `1.5s`).
///
/// Trade-off: lossless on percentile-of-existing-element queries (a
/// returned value is always a real observation, never an interpolated
/// average), at the cost of slight upward bias on even samples. Fine for
/// the small per-tool / per-hook samples M1.4 emits; consider linear
/// interpolation if M1.5+ needs strict statistical medians.
///
/// Edge cases:
/// - Empty slice → `Duration::zero()`.
/// - 1-element slice → that element (p50 == p95 == max).
/// - 2+ elements → indexed by rounded position.
///
/// `sorted` MUST be ascending; this is enforced by the only call site
/// (`collect_sorted_durations`) which uses `sort_unstable`. Caller is
/// responsible for clamping `pct` to `[0.0, 100.0]`; out-of-range values
/// silently truncate at the slice bounds via `idx.min(last_idx)`.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::tool_rank::percentile;
/// use chrono::Duration;
///
/// assert_eq!(percentile(&[], 50.0), Duration::zero());
/// assert_eq!(
///     percentile(&[Duration::seconds(5)], 95.0),
///     Duration::seconds(5)
/// );
/// let durs = vec![Duration::seconds(1), Duration::seconds(2), Duration::seconds(10)];
/// assert_eq!(percentile(&durs, 50.0), Duration::seconds(2));
/// assert_eq!(percentile(&durs, 95.0), Duration::seconds(10));
///
/// // Even sample: p50 rounds up (nearest-rank), NOT 1.5s averaged.
/// assert_eq!(
///     percentile(&[Duration::seconds(1), Duration::seconds(2)], 50.0),
///     Duration::seconds(2)
/// );
/// ```
#[must_use]
pub fn percentile(sorted: &[Duration], pct: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::zero();
    }
    let last_idx = sorted.len() - 1;
    // Casts are bounded: pct ∈ [0, 100], len ≥ 1; index is clipped below.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let idx_f = (pct / 100.0) * (last_idx as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let idx = idx_f.round() as usize;
    sorted[idx.min(last_idx)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::episode::{turn::Span, Episodes, ToolCall, ToolEpisode};
    use crate::model::ToolSource;
    use chrono::{TimeZone, Utc};

    fn at(s: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, s).unwrap()
    }

    #[test]
    fn empty_episodes_returns_empty_rows() {
        let rows = tool_rank(&Episodes::new());
        assert!(rows.is_empty());
    }

    #[test]
    fn single_call_p50_equals_p95_equals_max() {
        let mut ep = Episodes::new();
        let mut tool = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        let mut call = ToolCall::new(Span::new(at(1), at(4)));
        call.turn_id = Some("t1".into());
        tool.calls.push(call);
        tool.total_duration = Duration::seconds(3);
        ep.tools.insert("bash".into(), tool);

        let rows = tool_rank(&ep);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "bash");
        assert_eq!(rows[0].call_count, 1);
        assert_eq!(rows[0].success_count, 1);
        assert_eq!(rows[0].failure_count, 0);
        assert_eq!(rows[0].orphan_count, 0);
        assert_eq!(rows[0].p50_duration, Duration::seconds(3));
        assert_eq!(rows[0].p95_duration, Duration::seconds(3));
        assert_eq!(rows[0].max_duration, Duration::seconds(3));
        assert_eq!(rows[0].total_duration, Duration::seconds(3));
    }

    #[test]
    fn multi_tool_sorted_by_total_duration_desc() {
        let mut ep = Episodes::new();
        for (name, secs) in &[("bash", 10_u32), ("view", 3), ("edit", 7)] {
            let mut tool = ToolEpisode::new((*name).into(), ToolSource::Builtin);
            tool.calls.push(ToolCall::new(Span::new(at(0), at(*secs))));
            tool.total_duration = Duration::seconds(i64::from(*secs));
            ep.tools.insert((*name).into(), tool);
        }
        let rows = tool_rank(&ep);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "bash"); // 10s
        assert_eq!(rows[1].name, "edit"); // 7s
        assert_eq!(rows[2].name, "view"); // 3s
    }

    #[test]
    fn percentile_handles_edges() {
        assert_eq!(percentile(&[], 50.0), Duration::zero());
        assert_eq!(
            percentile(&[Duration::seconds(5)], 95.0),
            Duration::seconds(5)
        );
        let durs = vec![
            Duration::seconds(1),
            Duration::seconds(2),
            Duration::seconds(10),
        ];
        assert_eq!(percentile(&durs, 50.0), Duration::seconds(2));
        assert_eq!(percentile(&durs, 95.0), Duration::seconds(10));
    }

    #[test]
    fn failure_and_orphan_counts() {
        let mut ep = Episodes::new();
        let mut tool = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        // 1 success
        tool.calls.push(ToolCall::new(Span::new(at(0), at(1))));
        // 1 failure
        let mut fail = ToolCall::new(Span::new(at(1), at(2)));
        fail.status = ToolCallStatus::Failure { message: None };
        tool.calls.push(fail);
        // 1 orphan (synthesized start)
        let mut orph = ToolCall::new(Span::new(at(2), at(2)));
        orph.status = ToolCallStatus::OrphanSynthesizedStart;
        tool.calls.push(orph);
        // 1 user-requested success
        let mut ur = ToolCall::new(Span::new(at(3), at(4)));
        ur.user_requested = true;
        tool.calls.push(ur);
        tool.total_duration = Duration::seconds(3);
        ep.tools.insert("bash".into(), tool);

        let rows = tool_rank(&ep);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].call_count, 4);
        assert_eq!(rows[0].success_count, 2);
        assert_eq!(rows[0].failure_count, 1);
        assert_eq!(rows[0].orphan_count, 1);
        assert_eq!(rows[0].user_requested_count, 1);
    }
}
