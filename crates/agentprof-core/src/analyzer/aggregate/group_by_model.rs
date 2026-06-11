//! `aggregate_by_model` — group sessions by first-turn model (D-12).

use std::collections::BTreeMap;

use chrono::Duration;

use crate::analyzer::aggregate::{wall, AggregateKey, AggregateReport, ModelBucket};
use crate::analyzer::AnalysisReport;
use crate::episode::Episodes;

/// Sum cache token fields across all entries in a report's
/// `model_metrics`. Returns `(input, cache_read, cache_creation)`,
/// all `u64::saturating_add`-summed. Returns `(0, 0, 0)` if
/// `model_metrics` is `None`. Used by M2.5 per-bucket cache
/// attribution (see `super::CacheAttributable`).
fn sum_cache_fields(report: &AnalysisReport) -> (u64, u64, u64) {
    let Some(m) = report.model_metrics.as_ref() else {
        return (0, 0, 0);
    };
    let input = m
        .values()
        .map(|u| u.input_tokens)
        .fold(0_u64, u64::saturating_add);
    let read = m
        .values()
        .map(|u| u.cache_read_tokens)
        .fold(0_u64, u64::saturating_add);
    let creation = m
        .values()
        .map(|u| u.cache_write_tokens)
        .fold(0_u64, u64::saturating_add);
    (input, read, creation)
}

/// Aggregate sessions by their **first-turn model** (D-12).
///
/// Sessions whose first turn has `model = None` are skipped entirely —
/// they contribute to `report.session_count` (the outer total) but not
/// to any bucket. Buckets are sorted by `session_count` descending.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::group_by_model::aggregate_by_model;
/// let r = aggregate_by_model(&[], &[]);
/// assert!(r.buckets.is_empty());
/// ```
///
/// # Panics
///
/// If `reports.len() != episodes_per_report.len()`.
#[must_use]
#[tracing::instrument(name = "aggregator.group_by", skip_all, fields(key = "model", sessions = reports.len()))]
pub fn aggregate_by_model(
    reports: &[AnalysisReport],
    episodes_per_report: &[Episodes],
) -> AggregateReport<ModelBucket> {
    assert_eq!(
        reports.len(),
        episodes_per_report.len(),
        "aggregate_by_model: reports and episodes_per_report length mismatch",
    );

    let mut acc: BTreeMap<String, TempModelAcc> = BTreeMap::new();
    let mut total_wall = Duration::zero();

    for (report, episodes) in reports.iter().zip(episodes_per_report.iter()) {
        let session_wall = wall::compute_wall(episodes, report.meta.started_at);
        total_wall += session_wall;

        let Some(model) = report.turn_summary.first().and_then(|t| t.model.clone()) else {
            continue;
        };
        let out_tokens: u64 = report
            .turn_summary
            .iter()
            .filter_map(|t| t.output_tokens)
            .map(u64::from)
            .sum();
        let (in_tokens, cache_read, cache_creation) = sum_cache_fields(report);

        let entry = acc.entry(model.clone()).or_insert_with(|| TempModelAcc {
            model,
            session_count: 0,
            turn_count: 0,
            total_output_tokens: 0,
            total_input_tokens: 0,
            total_cache_read: 0,
            total_cache_creation: 0,
            total_duration: Duration::zero(),
        });
        entry.session_count += 1;
        entry.turn_count += report.turn_summary.len();
        entry.total_output_tokens = entry.total_output_tokens.saturating_add(out_tokens);
        entry.total_input_tokens = entry.total_input_tokens.saturating_add(in_tokens);
        entry.total_cache_read = entry.total_cache_read.saturating_add(cache_read);
        entry.total_cache_creation = entry.total_cache_creation.saturating_add(cache_creation);
        entry.total_duration += session_wall;
    }

    let mut buckets: Vec<ModelBucket> = acc
        .into_values()
        .map(|t| {
            ModelBucket::new(
                t.model,
                t.session_count,
                t.turn_count,
                t.total_output_tokens,
                t.total_duration,
            )
            .with_cache_metrics(
                t.total_input_tokens,
                t.total_cache_read,
                t.total_cache_creation,
            )
        })
        .collect();
    buckets.sort_by(|a, b| {
        b.session_count
            .cmp(&a.session_count)
            .then_with(|| a.model.cmp(&b.model))
    });

    let report = AggregateReport::new(
        AggregateKey::Model,
        None,
        reports.len(),
        0,
        total_wall,
        buckets,
    );
    tracing::debug!(buckets = report.buckets.len(), "aggregated");
    report
}

struct TempModelAcc {
    model: String,
    session_count: usize,
    turn_count: usize,
    total_output_tokens: u64,
    total_input_tokens: u64,
    total_cache_read: u64,
    total_cache_creation: u64,
    total_duration: Duration,
}
