//! Per-tool ranking rollup.
//!
//! One [`ToolRankRow`] per tool name, sorted by `total_duration` descending.
//! Tools with zero recorded calls are filtered out.
//!
//! ## User-blocking tools
//!
//! Some tools (e.g. `ask_user`) block on **user think time** rather than
//! agent / machine work. Their `total_duration` reflects how long the
//! human took to respond, not engineering cost. Mixing them into the
//! main Tool Rank skews the perceived "where time goes" picture: in a
//! real 56 h session, `ask_user` alone was 90 % of total tool wall-clock.
//!
//! [`USER_BLOCKING_TOOLS`] lists these tools by exact name. The
//! [`ToolRankRow::is_user_blocking`] field is set per row at analyzer
//! time so downstream renderers (markdown, JSON, future TUI) can split
//! them out without recomputing the membership rule themselves.

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::analyzer::duration_ms;
use crate::episode::{Episodes, ToolCall, ToolCallStatus};
use crate::model::ToolSource;

/// Tool names whose wall-clock duration is dominated by **user think time**,
/// not by agent or machine work.
///
/// Currently exactly one entry: `ask_user`. The constant is `pub` so
/// downstream renderers and tests can reference it directly (no string
/// duplication). Extend with future user-blocking tool names as they
/// appear in adapter vocabularies.
///
/// Used by [`tool_rank`] to populate [`ToolRankRow::is_user_blocking`].
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::tool_rank::USER_BLOCKING_TOOLS;
/// assert!(USER_BLOCKING_TOOLS.contains(&"ask_user"));
/// ```
pub const USER_BLOCKING_TOOLS: &[&str] = &["ask_user"];

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
    /// `true` if this tool blocks on **user think time** rather than agent
    /// or machine work. Membership rule: name appears in
    /// [`USER_BLOCKING_TOOLS`].
    ///
    /// Renderers should typically split user-blocking tools into a separate
    /// section (or otherwise visually distinguish them) so they don't skew
    /// the perceived "where time goes" picture in the main Tool Rank.
    ///
    /// Additive field: JSON consumers from before this field existed treat
    /// it as default (`false`) via serde's `default` attribute, so older
    /// stored reports remain deserializable.
    #[serde(default)]
    pub is_user_blocking: bool,
}

impl ToolRankRow {
    /// Explicit constructor for cross-crate test code (TUI render / sort
    /// tests). Production callers should consume [`tool_rank`] output.
    ///
    /// Bypasses the `#[non_exhaustive]` struct-literal restriction so
    /// `agentprof-tui` tests can build synthetic rows. Adding a field to
    /// `ToolRankRow` is a breaking change for callers of this constructor —
    /// that breakage is the desired signal.
    ///
    /// `is_user_blocking` is computed from [`USER_BLOCKING_TOOLS`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::ToolRankRow;
    /// use agentprof_core::model::ToolSource;
    /// use chrono::Duration;
    ///
    /// let row = ToolRankRow::new(
    ///     "bash".into(),
    ///     ToolSource::Builtin,
    ///     1, 1, 0, 0, 0,
    ///     Duration::milliseconds(10),
    ///     Duration::milliseconds(10),
    ///     Duration::milliseconds(10),
    ///     Duration::milliseconds(10),
    /// );
    /// assert_eq!(row.name, "bash");
    /// ```
    #[doc(hidden)]
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        source: ToolSource,
        call_count: usize,
        success_count: usize,
        failure_count: usize,
        orphan_count: usize,
        user_requested_count: usize,
        total_duration: Duration,
        p50_duration: Duration,
        p95_duration: Duration,
        max_duration: Duration,
    ) -> Self {
        let is_user_blocking = USER_BLOCKING_TOOLS.contains(&name.as_str());
        Self {
            name,
            source,
            call_count,
            success_count,
            failure_count,
            orphan_count,
            user_requested_count,
            total_duration,
            p50_duration,
            p95_duration,
            max_duration,
            is_user_blocking,
        }
    }
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
                is_user_blocking: USER_BLOCKING_TOOLS.contains(&name.as_str()),
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

/// Percentile (nearest-rank). **Re-exported from
/// [`crate::analyzer::stats::percentile_nearest_rank`]** per full-review
/// CORE #1 (`percentile-divergence`).
///
/// Pre-extraction this lived here and used `round((pct/100)*(n-1))`
/// (upper-midpoint), diverging from `aggregate::group_by_tool`'s
/// `ceil(p*n)-1` (lower-midpoint). They now share the lower-midpoint
/// convention via the shared `stats` module — see that module's docs
/// for the rationale + behavior change.
///
/// Kept as a `pub use` re-export to avoid breaking external callers
/// that import `agentprof_core::analyzer::tool_rank::percentile`.
/// New code should import via `crate::analyzer::stats::percentile_nearest_rank`.
pub use crate::analyzer::stats::percentile_nearest_rank as percentile;

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
        assert!(
            !rows[0].is_user_blocking,
            "bash is not user-blocking by membership rule"
        );
    }

    #[test]
    fn ask_user_is_flagged_as_user_blocking() {
        // Lock the membership rule contract: any tool name in
        // USER_BLOCKING_TOOLS is flagged at analyzer time so renderers
        // can split or otherwise visually distinguish it from work tools.
        let mut ep = Episodes::new();
        let mut tool = ToolEpisode::new("ask_user".into(), ToolSource::Builtin);
        let mut call = ToolCall::new(Span::new(at(0), at(30)));
        call.turn_id = Some("t1".into());
        tool.calls.push(call);
        tool.total_duration = Duration::seconds(30);
        ep.tools.insert("ask_user".into(), tool);

        let rows = tool_rank(&ep);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "ask_user");
        assert!(
            rows[0].is_user_blocking,
            "ask_user must be flagged is_user_blocking"
        );
    }

    #[test]
    fn user_blocking_constant_contains_ask_user() {
        // Membership-rule documentation guard.
        assert!(USER_BLOCKING_TOOLS.contains(&"ask_user"));
    }
}
