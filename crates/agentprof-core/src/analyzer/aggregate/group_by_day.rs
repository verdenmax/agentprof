//! `aggregate_by_day` — group sessions by UTC calendar date, with a
//! utilization metric (see ADR-0008 / D-5 / D-6).

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};

use crate::analyzer::aggregate::{wall, AggregateKey, AggregateReport, DayBucket};
use crate::analyzer::AnalysisReport;
use crate::episode::Episodes;

/// Aggregate sessions by `meta.started_at.date_naive()` (UTC, D-9).
///
/// Per bucket:
/// - `total_wall_duration` = Σ per-session wall time
/// - `total_tool_duration` = Σ per-tool-call durations
/// - `total_output_tokens` = Σ `turn_summary.output_tokens`
/// - `utilization_pct` = `tool/wall × 100`, clamped to `[0, 100]`
///   (returns `0.0` when wall is zero, never `NaN`)
/// - `is_low_utilization` = `utilization_pct < low_util_threshold_pct`
///
/// Buckets are returned in date-ascending order (the natural iteration
/// order of [`BTreeMap`]).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::group_by_day::aggregate_by_day;
/// let r = aggregate_by_day(&[], &[], 20.0);
/// assert!(r.buckets.is_empty());
/// ```
///
/// # Panics
///
/// If `reports.len() != episodes_per_report.len()`.
#[must_use]
#[tracing::instrument(name = "aggregator.group_by", skip_all, fields(key = "day", sessions = reports.len()))]
pub fn aggregate_by_day(
    reports: &[AnalysisReport],
    episodes_per_report: &[Episodes],
    low_util_threshold_pct: f32,
) -> AggregateReport<DayBucket> {
    assert_eq!(
        reports.len(),
        episodes_per_report.len(),
        "aggregate_by_day: reports and episodes_per_report length mismatch",
    );

    let mut acc: BTreeMap<NaiveDate, TempDayAcc> = BTreeMap::new();
    let mut total_wall = Duration::zero();

    for (report, episodes) in reports.iter().zip(episodes_per_report.iter()) {
        let date = report.meta.started_at.date_naive();
        let session_wall = wall::compute_wall(episodes, report.meta.started_at);
        let tool_time = sum_tool_duration(episodes);
        let out_tokens: u64 = report
            .turn_summary
            .iter()
            .filter_map(|t| t.output_tokens)
            .map(u64::from)
            .sum();

        total_wall += session_wall;

        let entry = acc.entry(date).or_insert_with(|| TempDayAcc {
            date,
            session_count: 0,
            total_wall_duration: Duration::zero(),
            total_tool_duration: Duration::zero(),
            total_output_tokens: 0,
        });
        entry.session_count += 1;
        entry.total_wall_duration += session_wall;
        entry.total_tool_duration += tool_time;
        entry.total_output_tokens += out_tokens;
    }

    let buckets: Vec<DayBucket> = acc
        .into_values()
        .map(|t| {
            let wall_ms = t.total_wall_duration.num_milliseconds();
            let tool_ms = t.total_tool_duration.num_milliseconds();
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let util = if wall_ms > 0 {
                ((tool_ms as f64) * 100.0 / (wall_ms as f64)) as f32
            } else {
                0.0
            };
            let util = util.clamp(0.0, 100.0);
            DayBucket::new(
                t.date,
                t.session_count,
                t.total_wall_duration,
                t.total_tool_duration,
                t.total_output_tokens,
                util,
                util < low_util_threshold_pct,
            )
        })
        .collect();

    let report = AggregateReport::new(
        AggregateKey::Day,
        Duration::zero(),
        reports.len(),
        0,
        total_wall,
        buckets,
    );
    tracing::debug!(buckets = report.buckets.len(), "aggregated");
    report
}

struct TempDayAcc {
    date: NaiveDate,
    session_count: usize,
    total_wall_duration: Duration,
    total_tool_duration: Duration,
    total_output_tokens: u64,
}

fn sum_tool_duration(episodes: &Episodes) -> Duration {
    let mut total = Duration::zero();
    for tool in episodes.tools.values() {
        for call in &tool.calls {
            let d = call.span.ended_at - call.span.started_at;
            if d >= Duration::zero() {
                total += d;
            }
        }
    }
    total
}
