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
        Duration::days(30),
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
        Duration::days(7),
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
