//! Integration tests locking [`AnalysisReport::cache_metrics`] behavior
//! (M2.5 Task 2). Verifies the accessor sums raw per-model cache token
//! fields across `model_metrics` and delegates to `CacheMetrics::from_raw`,
//! returning `None` when no model entry has any cache activity.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::analyzer::ModelUsage;
use agentprof_core::model::SessionMeta;
use chrono::Utc;

fn empty_report() -> AnalysisReport {
    AnalysisReport::new(SessionMeta::new(
        "s".into(),
        AgentKind::Copilot,
        Utc::now(),
        false,
    ))
}

#[test]
fn analysis_report_cache_metrics_none_on_empty() {
    let r = empty_report();
    assert!(
        r.cache_metrics().is_none(),
        "fresh report (no model_metrics) → None"
    );
}

fn usage(input: u64, read: u64, creation: u64) -> ModelUsage {
    let mut u = ModelUsage::default();
    u.input_tokens = input;
    u.cache_read_tokens = read;
    u.cache_write_tokens = creation;
    u
}

#[test]
fn analysis_report_cache_metrics_sums_across_models() {
    let mut m: BTreeMap<String, ModelUsage> = BTreeMap::new();
    m.insert("claude-sonnet-4.5".into(), usage(1_000, 8_000, 2_000));
    m.insert("claude-haiku-4.5".into(), usage(500, 4_000, 1_000));
    let mut r = empty_report();
    r.model_metrics = Some(m);

    let cm = r.cache_metrics().expect("two models with cache activity");
    assert_eq!(cm.creation, 3_000, "2_000 + 1_000");
    assert_eq!(cm.read, 12_000, "8_000 + 4_000");
    assert_eq!(cm.input, 1_500, "1_000 + 500");
    // honest = 100 * read / (read + creation) = 100 * 12_000 / 15_000 = 80.0
    assert!(
        (cm.hit_rate_honest_pct - 80.0).abs() < 0.001,
        "honest hit-rate should be 80%, got {}",
        cm.hit_rate_honest_pct
    );
}

#[test]
fn analysis_report_cache_metrics_none_when_only_zeros() {
    let mut m: BTreeMap<String, ModelUsage> = BTreeMap::new();
    let mut only_input = ModelUsage::default();
    only_input.input_tokens = 1_234;
    only_input.output_tokens = 99;
    m.insert("claude-sonnet-4.5".into(), only_input);
    let mut r = empty_report();
    r.model_metrics = Some(m);
    assert!(
        r.cache_metrics().is_none(),
        "creation == 0 && read == 0 across all models → None (input alone doesn't count)"
    );
}

#[test]
fn analysis_report_cache_metrics_handles_saturating() {
    // Two models each at u64::MAX on every cache field. Naive `+` would
    // panic in debug; saturating_add must clamp at u64::MAX without panic.
    let mut m: BTreeMap<String, ModelUsage> = BTreeMap::new();
    m.insert("a".into(), usage(u64::MAX, u64::MAX, u64::MAX));
    m.insert("b".into(), usage(u64::MAX, u64::MAX, u64::MAX));
    let mut r = empty_report();
    r.model_metrics = Some(m);

    let cm = r
        .cache_metrics()
        .expect("u64::MAX summed values still have cache activity");
    assert_eq!(cm.creation, u64::MAX, "saturating_add clamps at u64::MAX");
    assert_eq!(cm.read, u64::MAX);
    assert_eq!(cm.input, u64::MAX);
    assert!(
        cm.hit_rate_honest_pct.is_finite(),
        "honest hit-rate must remain finite even at saturation"
    );
    assert!(
        cm.hit_rate_naive_pct.is_finite(),
        "naive hit-rate must remain finite even at saturation"
    );
}

// ──────────────────────────────────────────────────────────────────────
// M2.5 Task 3 — AggregateReport::cache_metrics_per_bucket() coverage
// for `--by model` / `--by day`. ToolBucket + McpServerBucket
// deliberately do NOT implement `CacheAttributable` (ADR-0023 D-3:
// per-tool/per-server cache attribution is undefined), so the
// generic accessor is type-level inaccessible for those two; the
// runtime `supports_cache_attribution` mirror is asserted instead.
// ──────────────────────────────────────────────────────────────────────

use agentprof_core::analyzer::aggregate::{
    supports_cache_attribution, AggregateKey, AggregateReport, DayBucket, ModelBucket,
};
use chrono::{Duration, NaiveDate};

#[test]
fn aggregate_cache_metrics_per_bucket_model() {
    let sonnet = ModelBucket::new("claude-sonnet-4.5".into(), 2, 0, 0, Duration::zero())
        .with_cache_metrics(1_000, 8_000, 2_000);
    let haiku = ModelBucket::new("claude-haiku-4.5".into(), 1, 0, 0, Duration::zero())
        .with_cache_metrics(500, 4_000, 1_000);
    let report: AggregateReport<ModelBucket> = AggregateReport::new(
        AggregateKey::Model,
        None,
        3,
        0,
        Duration::zero(),
        vec![sonnet, haiku],
    );

    let map = report
        .cache_metrics_per_bucket()
        .expect("two buckets with cache activity → Some");
    assert_eq!(map.len(), 2, "one entry per bucket with cache activity");

    let s = map.get("claude-sonnet-4.5").expect("sonnet bucket present");
    assert_eq!(s.creation, 2_000);
    assert_eq!(s.read, 8_000);
    assert_eq!(s.input, 1_000);
    // honest = 100 * 8000 / (8000 + 2000) = 80.0
    assert!(
        (s.hit_rate_honest_pct - 80.0).abs() < 0.001,
        "sonnet honest hit-rate: got {}",
        s.hit_rate_honest_pct
    );

    let h = map.get("claude-haiku-4.5").expect("haiku bucket present");
    assert_eq!(h.creation, 1_000);
    assert_eq!(h.read, 4_000);
    assert_eq!(h.input, 500);
}

#[test]
fn aggregate_cache_metrics_per_bucket_day() {
    let day1 = DayBucket::new(
        NaiveDate::from_ymd_opt(2026, 5, 30).unwrap(),
        1,
        Duration::zero(),
        Duration::zero(),
        0,
        0.0,
        false,
    )
    .with_cache_metrics(1_000, 8_000, 2_000);
    let day2 = DayBucket::new(
        NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
        1,
        Duration::zero(),
        Duration::zero(),
        0,
        0.0,
        false,
    )
    .with_cache_metrics(500, 0, 0);
    let report: AggregateReport<DayBucket> = AggregateReport::new(
        AggregateKey::Day,
        None,
        2,
        0,
        Duration::zero(),
        vec![day1, day2],
    );

    let map = report
        .cache_metrics_per_bucket()
        .expect("one bucket with cache activity → Some");
    assert_eq!(
        map.len(),
        1,
        "only day1 has cache activity; day2 (read=0 && creation=0) is skipped"
    );
    let d = map.get("2026-05-30").expect("day1 bucket present");
    assert_eq!(d.creation, 2_000);
    assert_eq!(d.read, 8_000);
    assert_eq!(d.input, 1_000);
    assert!(
        !map.contains_key("2026-05-31"),
        "day with no cache activity must be absent from the map"
    );
}

#[test]
fn aggregate_cache_metrics_supports_attribution_only_for_model_and_day() {
    // Type-level: cache_metrics_per_bucket() exists only for
    // ModelBucket + DayBucket reports. The runtime mirror
    // `supports_cache_attribution` reports the same partition for
    // render-layer / AnyAggregateReport dispatch where the bucket
    // type has been erased to AggregateKey. ADR-0023 D-3.
    assert!(supports_cache_attribution(AggregateKey::Model));
    assert!(supports_cache_attribution(AggregateKey::Day));
    assert!(
        !supports_cache_attribution(AggregateKey::Tool),
        "per-tool cache attribution is undefined (ADR-0023 D-3)"
    );
    assert!(
        !supports_cache_attribution(AggregateKey::McpServer),
        "per-mcp-server cache attribution is undefined (ADR-0023 D-3)"
    );
}

#[test]
fn aggregate_cache_metrics_none_when_all_buckets_empty() {
    // Every bucket has cache_read == 0 && cache_creation == 0:
    // CacheMetrics::from_raw returns None for each, so the
    // accessor's `if out.is_empty()` guard collapses to None
    // (avoids surfacing a row of zeros to renderers).
    let m1 = ModelBucket::new("model-a".into(), 1, 0, 0, Duration::zero())
        .with_cache_metrics(1_000, 0, 0);
    let m2 = ModelBucket::new("model-b".into(), 1, 0, 0, Duration::zero())
        .with_cache_metrics(2_000, 0, 0);
    let report: AggregateReport<ModelBucket> = AggregateReport::new(
        AggregateKey::Model,
        None,
        2,
        0,
        Duration::zero(),
        vec![m1, m2],
    );

    assert!(
        report.cache_metrics_per_bucket().is_none(),
        "no bucket has cache activity → None (matches per-report semantics)"
    );
}
