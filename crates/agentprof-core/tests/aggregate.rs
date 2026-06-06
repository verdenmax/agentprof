//! Integration tests for the shape + serde behaviour of
//! `agentprof_core::analyzer::aggregate`.
//!
//! Fixture-driven behavioural tests live in
//! `agentprof-adapters/tests/aggregate_on_fixtures.rs` to avoid a
//! dev-dep cycle (adapters depend on core, not vice versa) — same
//! arrangement as M1.6.4's `export_on_fixtures.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentprof_core::analyzer::aggregate::{
    AggregateKey, AggregateReport, AnyAggregateReport, ToolBucket,
};
use agentprof_core::model::ToolSource;
use chrono::Duration;

#[test]
fn aggregate_report_constructor_starts_empty() {
    let r: AggregateReport<ToolBucket> = AggregateReport::new(
        AggregateKey::Tool,
        Some(Duration::days(30)),
        0,
        0,
        Duration::zero(),
        Vec::new(),
    );
    assert_eq!(r.session_count, 0);
    assert!(r.buckets.is_empty());
    assert_eq!(r.by, AggregateKey::Tool);
}

#[test]
fn any_aggregate_report_serde_round_trip_lossless() {
    let inner: AggregateReport<ToolBucket> = AggregateReport::new(
        AggregateKey::Tool,
        Some(Duration::days(7)),
        3,
        0,
        Duration::seconds(120),
        vec![ToolBucket::new(
            "bash".to_string(),
            ToolSource::Builtin,
            5,
            5,
            0,
            Duration::seconds(10),
            Duration::seconds(2),
            Duration::seconds(3),
            2,
        )],
    );
    let any = AnyAggregateReport::Tool(inner);
    let json = serde_json::to_string(&any).expect("serialize");
    let back: AnyAggregateReport = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(any, back);
}

// ──────────────────────────────────────────────────────────────────────
// Wave C item 1 — json-since-sentinel: `since: None` omits the JSON
// field entirely; `since: Some(d)` renders as integer milliseconds.
// Closes the bug where the CLI's `--since all` flowed Duration::MAX
// in-band and JSON serialized `"since": 9223372036854775807`.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn aggregate_since_none_omits_json_field() {
    let inner: AggregateReport<ToolBucket> =
        AggregateReport::new(AggregateKey::Tool, None, 0, 0, Duration::zero(), Vec::new());
    let any = AnyAggregateReport::Tool(inner);
    let v: serde_json::Value = serde_json::to_value(&any).unwrap();
    let data = v.get("data").expect("envelope has data");
    assert!(
        data.get("since").is_none(),
        "since field must be omitted from JSON when None (Wave C item 1); \
         got: {data}"
    );
}

#[test]
fn aggregate_since_some_serializes_as_ms_integer() {
    let inner: AggregateReport<ToolBucket> = AggregateReport::new(
        AggregateKey::Tool,
        Some(Duration::seconds(42)),
        0,
        0,
        Duration::zero(),
        Vec::new(),
    );
    let any = AnyAggregateReport::Tool(inner);
    let v: serde_json::Value = serde_json::to_value(&any).unwrap();
    let since = v
        .get("data")
        .and_then(|d| d.get("since"))
        .and_then(serde_json::Value::as_i64)
        .expect("since must be a JSON integer when Some");
    assert_eq!(
        since, 42_000,
        "Some(Duration::seconds(42)) must serialize as 42000 ms"
    );
}

#[test]
fn aggregate_since_round_trip_some_and_none() {
    // None side
    let inner_none: AggregateReport<ToolBucket> =
        AggregateReport::new(AggregateKey::Tool, None, 0, 0, Duration::zero(), Vec::new());
    let any_none = AnyAggregateReport::Tool(inner_none);
    let json = serde_json::to_string(&any_none).unwrap();
    let back: AnyAggregateReport = serde_json::from_str(&json).unwrap();
    assert_eq!(any_none, back);

    // Some side
    let inner_some: AggregateReport<ToolBucket> = AggregateReport::new(
        AggregateKey::Tool,
        Some(Duration::days(7)),
        0,
        0,
        Duration::zero(),
        Vec::new(),
    );
    let any_some = AnyAggregateReport::Tool(inner_some);
    let json2 = serde_json::to_string(&any_some).unwrap();
    let back2: AnyAggregateReport = serde_json::from_str(&json2).unwrap();
    assert_eq!(any_some, back2);
}
