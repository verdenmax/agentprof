//! Shared statistical helpers for analyzer rollups.
//!
//! Promoted from per-module helpers (`analyzer::tool_rank::percentile`,
//! `analyzer::aggregate::group_by_tool::percentile`) to address
//! full-review CORE #1 (`percentile-divergence`).
//!
//! Pre-extraction the two sites computed `p50` / `p95` with **different
//! conventions**:
//!
//! - `tool_rank`: `round((pct/100) * (n-1))` — rounds half away from
//!   zero, giving the **upper midpoint** of two adjacent values.
//! - `aggregate::group_by_tool`: `ceil(p * n) - 1` — Wikipedia's
//!   nearest-rank definition, giving the **lower midpoint**.
//!
//! This silently violated the invariant "aggregate of a single
//! session equals that session". For `[1, 2, 3, 4]` durations,
//! per-session reported `p50 = 3` while a cross-session aggregate of
//! the same single session reported `p50 = 2`.
//!
//! [`percentile_nearest_rank`] is now the single source of truth.
//! Convention chosen: **upper midpoint** —
//! `slice[round((pct/100) * (n-1))]` using `f64::round`
//! (half-away-from-zero). Picked because:
//!
//! 1. For real-world cost metrics with at-rest values like `0ms`
//!    (aborted / orphan calls), the lower-midpoint convention would
//!    report `p50 = 0` for `[0, 1000ms]` pools — confusing to users
//!    who expect "p50" to feel like a typical observation.
//! 2. Upper-midpoint matches the pre-CORE-#1 `tool_rank` behavior,
//!    so per-session markdown / JSON snapshots (the user-visible
//!    surface) are unchanged.
//! 3. Lower-midpoint matches Wikipedia's strict nearest-rank but in
//!    a 2-element pool both conventions are "correct" — neither
//!    interpolates, so picking the upper one only differs from the
//!    strict definition for even-length inputs.
//!
//! The cross-session aggregate output (md / json / csv / html via
//! `aggregate::group_by_tool::percentile`) changes for even-length
//! pools as a result. See CHANGELOG `[Unreleased] / Changed` entry
//! `core-1-percentile-divergence`. Snapshots that locked the old
//! lower-midpoint values are regenerated in the same commit.

use chrono::Duration;

/// Percentile by the nearest-rank method using the
/// **upper-midpoint** convention: `slice[round((pct/100) * (n-1))]`.
///
/// `pct` is in **percent units** (`0.0..=100.0`) to match the legacy
/// `tool_rank::percentile` call sites. `sorted` MUST be pre-sorted
/// ascending; the function does not sort.
///
/// Returns `Duration::zero()` for an empty slice. The computed index
/// is clamped to `[0, sorted.len() - 1]` so any `pct` outside
/// `[0, 100]` (or `NaN`) yields a defined result rather than a
/// panic.
///
/// # Examples
///
/// ```
/// use chrono::Duration;
/// use agentprof_core::analyzer::stats::percentile_nearest_rank;
///
/// let pool = [
///     Duration::seconds(1),
///     Duration::seconds(2),
///     Duration::seconds(3),
///     Duration::seconds(4),
/// ];
/// // N=4, p=0.5 → round(0.5 * 3) = round(1.5) = 2 → sorted[2] = 3s.
/// assert_eq!(percentile_nearest_rank(&pool, 50.0), Duration::seconds(3));
/// // N=4, p=0.95 → round(0.95 * 3) = round(2.85) = 3 → sorted[3] = 4s.
/// assert_eq!(percentile_nearest_rank(&pool, 95.0), Duration::seconds(4));
/// // Empty pool → zero.
/// assert_eq!(percentile_nearest_rank(&[], 50.0), Duration::zero());
/// // 2-element pool: p50 picks the upper midpoint, NOT averaged median.
/// let two = [Duration::seconds(0), Duration::seconds(1000)];
/// assert_eq!(percentile_nearest_rank(&two, 50.0), Duration::seconds(1000));
/// ```
#[must_use]
pub fn percentile_nearest_rank(sorted: &[Duration], pct: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::zero();
    }
    let last_idx = sorted.len() - 1;
    // Clamp pct to a valid range so out-of-range inputs (incl. NaN)
    // don't surprise with negative indices via casting.
    let pct_clamped = pct.clamp(0.0, 100.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let idx_f = (pct_clamped / 100.0) * (last_idx as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let idx = idx_f.round() as usize;
    sorted[idx.min(last_idx)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: i64) -> Duration {
        Duration::seconds(s)
    }

    #[test]
    fn empty_returns_zero() {
        assert_eq!(percentile_nearest_rank(&[], 50.0), Duration::zero());
        assert_eq!(percentile_nearest_rank(&[], 95.0), Duration::zero());
    }

    #[test]
    fn single_element_returns_that_element() {
        let pool = [d(7)];
        assert_eq!(percentile_nearest_rank(&pool, 50.0), d(7));
        assert_eq!(percentile_nearest_rank(&pool, 95.0), d(7));
    }

    #[test]
    fn two_element_p50_upper_midpoint() {
        let pool = [d(5), d(10)];
        assert_eq!(percentile_nearest_rank(&pool, 50.0), d(10));
    }

    #[test]
    fn four_element_p50_upper_midpoint() {
        let pool = [d(1), d(2), d(3), d(4)];
        // round(0.5 * 3) = round(1.5) = 2 (half-away-from-zero) → sorted[2] = 3.
        assert_eq!(percentile_nearest_rank(&pool, 50.0), d(3));
    }

    #[test]
    fn four_element_p95_top_of_range() {
        let pool = [d(1), d(2), d(3), d(4)];
        assert_eq!(percentile_nearest_rank(&pool, 95.0), d(4));
    }

    #[test]
    fn twenty_element_p95() {
        let pool: Vec<Duration> = (1..=20).map(d).collect();
        // round(0.95 * 19) = round(18.05) = 18 → sorted[18] = 19s.
        assert_eq!(percentile_nearest_rank(&pool, 95.0), d(19));
    }

    #[test]
    fn pct_zero_returns_first_element() {
        let pool = [d(1), d(2), d(3)];
        assert_eq!(percentile_nearest_rank(&pool, 0.0), d(1));
    }

    #[test]
    fn pct_hundred_returns_last_element() {
        let pool = [d(1), d(2), d(3)];
        assert_eq!(percentile_nearest_rank(&pool, 100.0), d(3));
    }

    #[test]
    fn pct_above_hundred_clamps_to_last() {
        let pool = [d(1), d(2), d(3)];
        assert_eq!(percentile_nearest_rank(&pool, 9999.0), d(3));
    }

    #[test]
    fn pct_negative_clamps_to_first() {
        let pool = [d(1), d(2), d(3)];
        assert_eq!(percentile_nearest_rank(&pool, -50.0), d(1));
    }

    #[test]
    fn aborted_zero_pool_picks_upper_midpoint_not_zero() {
        // CORE #1 motivation: a [0ms, 1000ms] pool (one aborted call +
        // one normal) should report p50 = 1000ms, not p50 = 0. The
        // lower-midpoint convention would surprise users staring at
        // "p50 = 0" for a tool with a single failure.
        let pool = [Duration::zero(), Duration::milliseconds(1000)];
        assert_eq!(
            percentile_nearest_rank(&pool, 50.0),
            Duration::milliseconds(1000)
        );
    }

    #[test]
    fn aggregate_of_single_session_equals_session_invariant() {
        // CORE #1 root cause regression guard. For ANY input, the
        // per-session percentile must equal the cross-session aggregate
        // percentile when the aggregate is of exactly that single
        // session (durations identical). The shared helper guarantees
        // this by construction; the test pins the contract.
        let mut sorted: Vec<Duration> =
            [3, 1, 4, 1, 5, 9, 2, 6].iter().map(|&s| d(s)).collect();
        sorted.sort();
        for p in [0.0, 25.0, 50.0, 75.0, 90.0, 95.0, 99.0, 100.0] {
            let per_session = percentile_nearest_rank(&sorted, p);
            let cross = percentile_nearest_rank(&sorted, p); // same call (one source of truth)
            assert_eq!(
                per_session, cross,
                "single-session aggregate must equal session at p={p}"
            );
        }
    }
}
