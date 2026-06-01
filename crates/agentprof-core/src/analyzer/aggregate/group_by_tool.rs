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

    AggregateReport::new(
        AggregateKey::Tool,
        Duration::zero(),
        reports.len(),
        0,
        total_wall,
        buckets,
    )
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

/// Nearest-rank percentile on a pre-sorted slice. `p` is `0.0..=1.0`.
///
/// Returns `Duration::zero()` for empty input; otherwise
/// `sorted[((len * p).floor() as usize).min(len - 1)]`.
fn percentile(sorted_pool: &[Duration], p: f64) -> Duration {
    if sorted_pool.is_empty() {
        return Duration::zero();
    }
    let len = sorted_pool.len();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let idx = ((len as f64) * p).floor() as usize;
    let idx = idx.min(len.saturating_sub(1));
    sorted_pool[idx]
}
