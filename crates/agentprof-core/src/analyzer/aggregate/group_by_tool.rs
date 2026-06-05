//! `aggregate_by_tool` — aggregate per-tool [`crate::analyzer::ToolRankRow`]
//! data across N sessions.

use std::collections::HashMap;

use chrono::Duration;

use crate::analyzer::aggregate::{wall, AggregateKey, AggregateReport, ToolBucket};
use crate::analyzer::AnalysisReport;
use crate::episode::Episodes;
use crate::model::ToolSource;

/// Aggregate per-tool stats across N input sessions.
///
/// `reports[i]` and `episodes_per_report[i]` must describe the same
/// session; lengths MUST be equal (panics otherwise).
///
/// # Algorithm
///
/// - Sums `call_count` / `success_count` / `failure_count` /
///   `total_duration` across sessions, grouped by `(name, source)`.
/// - **Re-computes** `p50` / `p95` from the pooled per-call durations
///   (NOT averaging per-session percentiles, which is statistically
///   wrong). The pool is built from `episodes.tools[name].calls[*].span`.
/// - `session_count` per bucket = how many input sessions used that tool.
/// - Buckets are sorted by `total_duration` descending.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::group_by_tool::aggregate_by_tool;
/// let r = aggregate_by_tool(&[], &[]);
/// assert!(r.buckets.is_empty());
/// ```
///
/// # Panics
///
/// If `reports.len() != episodes_per_report.len()`.
#[must_use]
#[tracing::instrument(name = "aggregator.group_by", skip_all, fields(key = "tool", sessions = reports.len()))]
pub fn aggregate_by_tool(
    reports: &[AnalysisReport],
    episodes_per_report: &[Episodes],
) -> AggregateReport<ToolBucket> {
    assert_eq!(
        reports.len(),
        episodes_per_report.len(),
        "aggregate_by_tool: reports and episodes_per_report length mismatch",
    );

    let mut acc: HashMap<(String, ToolSource), TempToolAcc> = HashMap::new();
    let mut total_wall = Duration::zero();

    for (report, episodes) in reports.iter().zip(episodes_per_report.iter()) {
        total_wall += wall::compute_wall(episodes, report.meta.started_at);

        for row in &report.tool_rank {
            let key = (row.name.clone(), row.source.clone());
            let entry = acc.entry(key).or_insert_with(|| TempToolAcc {
                name: row.name.clone(),
                source: row.source.clone(),
                call_count: 0,
                success_count: 0,
                failure_count: 0,
                total_duration: Duration::zero(),
                session_count: 0,
                duration_pool: Vec::new(),
            });
            entry.call_count += row.call_count;
            entry.success_count += row.success_count;
            entry.failure_count += row.failure_count;
            entry.total_duration += row.total_duration;
            entry.session_count += 1;
            if let Some(tool) = episodes.tools.get(&row.name) {
                for call in &tool.calls {
                    let d = call.span.ended_at - call.span.started_at;
                    if d >= Duration::zero() {
                        entry.duration_pool.push(d);
                    }
                }
            }
        }
    }

    let mut buckets: Vec<ToolBucket> = acc
        .into_values()
        .map(|t| {
            let mut pool = t.duration_pool;
            pool.sort_unstable();
            ToolBucket::new(
                t.name,
                t.source,
                t.call_count,
                t.success_count,
                t.failure_count,
                t.total_duration,
                percentile(&pool, 0.50),
                percentile(&pool, 0.95),
                t.session_count,
            )
        })
        .collect();
    buckets.sort_by(|a, b| {
        b.total_duration
            .cmp(&a.total_duration)
            .then_with(|| a.name.cmp(&b.name))
    });

    let report = AggregateReport::new(
        AggregateKey::Tool,
        Duration::zero(),
        reports.len(),
        0,
        total_wall,
        buckets,
    );
    tracing::debug!(buckets = report.buckets.len(), "aggregated");
    report
}

struct TempToolAcc {
    name: String,
    source: ToolSource,
    call_count: usize,
    success_count: usize,
    failure_count: usize,
    total_duration: Duration,
    session_count: usize,
    duration_pool: Vec<Duration>,
}

/// Percentile (nearest-rank, upper-midpoint). **Delegates to
/// [`crate::analyzer::stats::percentile_nearest_rank`]** per full-review
/// CORE #1 (`percentile-divergence`).
///
/// Pre-extraction this was a private duplicate of the percentile logic
/// in `tool_rank` with a different convention (lower-midpoint via
/// `ceil(p*n) - 1` vs `tool_rank`'s upper-midpoint via
/// `round((pct/100) * (n-1))`). For `[1, 2, 3, 4]s`, per-session
/// reported `p50 = 3` while a cross-session aggregate of that one
/// session reported `p50 = 2` — violating the "aggregate of single
/// session equals that session" invariant. The shared helper picks
/// the upper-midpoint convention; cross-session aggregate values for
/// even-length pools change as a result (snapshots regenerated).
///
/// Note the input scale conversion: the shared helper takes
/// `pct ∈ [0, 100]`, whereas this function's call sites pass
/// `p ∈ [0, 1]`. The `* 100.0` adapter keeps the public surface
/// stable.
fn percentile(sorted_pool: &[Duration], p: f64) -> Duration {
    crate::analyzer::stats::percentile_nearest_rank(sorted_pool, p * 100.0)
}

#[cfg(test)]
mod tests {
    use super::percentile;
    use chrono::Duration;

    #[test]
    fn percentile_nearest_rank_two_element_p50() {
        let pool = vec![Duration::seconds(1), Duration::seconds(10)];
        // CORE #1: shared upper-midpoint via stats::percentile_nearest_rank.
        // round(0.5 * (2-1)) = round(0.5) = 1 → sorted[1] = 10s.
        assert_eq!(percentile(&pool, 0.50), Duration::seconds(10));
    }

    #[test]
    fn percentile_nearest_rank_ten_element_p95() {
        let pool: Vec<_> = (1..=10).map(Duration::seconds).collect();
        // CORE #1: round(0.95 * 9) = round(8.55) = 9 → sorted[9] = 10s.
        assert_eq!(percentile(&pool, 0.95), Duration::seconds(10));
    }

    #[test]
    fn percentile_nearest_rank_ten_element_p50() {
        let pool: Vec<_> = (1..=10).map(Duration::seconds).collect();
        // CORE #1: round(0.5 * 9) = round(4.5) = 5 → sorted[5] = 6s
        // (was: 5s under the pre-CORE-#1 lower-midpoint convention).
        assert_eq!(percentile(&pool, 0.50), Duration::seconds(6));
    }

    #[test]
    fn percentile_empty_pool_returns_zero() {
        let pool: Vec<Duration> = Vec::new();
        assert_eq!(percentile(&pool, 0.50), Duration::zero());
    }

    #[test]
    fn percentile_single_element_any_p_returns_that_element() {
        let pool = vec![Duration::seconds(42)];
        assert_eq!(percentile(&pool, 0.0), Duration::seconds(42));
        assert_eq!(percentile(&pool, 0.5), Duration::seconds(42));
        assert_eq!(percentile(&pool, 1.0), Duration::seconds(42));
    }
}
