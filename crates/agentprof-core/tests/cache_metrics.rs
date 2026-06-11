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
