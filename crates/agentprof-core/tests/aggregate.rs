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

// ──────────────────────────────────────────────────────────────────────
// Wave C item 2 — total_wall_duration sum-invariant tests (closes
// m1.6.2-followup-i4-total-wall-test). Each aggregator MUST set
// `report.total_wall_duration` to Σ per-session wall durations. Pre-
// Wave-C the field was pub + rendered by md/html/TUI but no test
// asserted the sum invariant — a future refactor that breaks the
// sum would silently mis-report headline totals.
// ──────────────────────────────────────────────────────────────────────

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::aggregate::group_by_day::aggregate_by_day;
use agentprof_core::analyzer::aggregate::group_by_mcp::aggregate_by_mcp_server;
use agentprof_core::analyzer::aggregate::group_by_model::aggregate_by_model;
use agentprof_core::analyzer::aggregate::group_by_tool::aggregate_by_tool;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::turn::{Turn, TurnStatus};
use agentprof_core::episode::Episodes;
use agentprof_core::model::SessionMeta;
use chrono::{TimeZone, Utc};

/// Build a synthetic (`AnalysisReport`, `Episodes`) pair whose wall
/// duration equals `wall_secs` seconds. Constructed by:
/// - `SessionMeta.started_at` = a fixed anchor minus 0 offset
/// - one `Turn` with `ended_at` = `started_at + wall_secs`
///
/// `compute_wall()` walks every endpoint (turn / tool / hook / skill /
/// mode segment); the single closed `Turn` is sufficient to set the
/// latest endpoint = `started_at + wall_secs`, so wall = `wall_secs`.
fn synthetic_session(
    session_id: &str,
    started_offset_secs: i64,
    wall_secs: i64,
) -> (AnalysisReport, Episodes) {
    let anchor = Utc.with_ymd_and_hms(2026, 6, 6, 0, 0, 0).unwrap();
    let started_at = anchor + Duration::seconds(started_offset_secs);
    let ended_at = started_at + Duration::seconds(wall_secs);

    let meta = SessionMeta::new(session_id.into(), AgentKind::Copilot, started_at, false);
    let report = AnalysisReport::new(meta);

    let mut episodes = Episodes::new();
    let mut turn = Turn::new("t1".into(), started_at);
    turn.ended_at = Some(ended_at);
    turn.status = TurnStatus::Completed;
    episodes.turns.push(turn);

    (report, episodes)
}

#[test]
fn aggregate_by_tool_total_wall_duration_equals_sum() {
    let (r1, e1) = synthetic_session("s1", 0, 10);
    let (r2, e2) = synthetic_session("s2", 100, 30);
    let (r3, e3) = synthetic_session("s3", 1000, 7);

    let report = aggregate_by_tool(&[r1, r2, r3], &[e1, e2, e3]);
    assert_eq!(
        report.total_wall_duration,
        Duration::seconds(10 + 30 + 7),
        "by_tool: total_wall_duration must equal sum of per-session walls"
    );
}

#[test]
fn aggregate_by_mcp_total_wall_duration_equals_sum() {
    let (r1, e1) = synthetic_session("s1", 0, 5);
    let (r2, e2) = synthetic_session("s2", 50, 15);

    let report = aggregate_by_mcp_server(&[r1, r2], &[e1, e2]);
    assert_eq!(
        report.total_wall_duration,
        Duration::seconds(5 + 15),
        "by_mcp_server: total_wall_duration must equal sum of per-session walls"
    );
}

#[test]
fn aggregate_by_day_total_wall_duration_equals_sum() {
    let (r1, e1) = synthetic_session("s1", 0, 120);
    let (r2, e2) = synthetic_session("s2", 200, 60);
    let (r3, e3) = synthetic_session("s3", 500, 1);

    // aggregate_by_day takes a 3rd arg (low_util_threshold_pct) — value
    // doesn't affect total_wall_duration, only the per-bucket low_util
    // flag. Pass 50.0 (a typical mid-range value).
    let report = aggregate_by_day(&[r1, r2, r3], &[e1, e2, e3], 50.0);
    assert_eq!(
        report.total_wall_duration,
        Duration::seconds(120 + 60 + 1),
        "by_day: total_wall_duration must equal sum of per-session walls"
    );
}

#[test]
fn aggregate_by_model_total_wall_duration_equals_sum() {
    let (r1, e1) = synthetic_session("s1", 0, 8);
    let (r2, e2) = synthetic_session("s2", 100, 12);
    let (r3, e3) = synthetic_session("s3", 500, 4);
    let (r4, e4) = synthetic_session("s4", 800, 16);

    let report = aggregate_by_model(&[r1, r2, r3, r4], &[e1, e2, e3, e4]);
    assert_eq!(
        report.total_wall_duration,
        Duration::seconds(8 + 12 + 4 + 16),
        "by_model: total_wall_duration must equal sum of per-session walls"
    );
}

#[test]
fn aggregate_total_wall_duration_zero_when_no_sessions() {
    // Edge case: empty input → zero wall.
    let report = aggregate_by_tool(&[], &[]);
    assert_eq!(report.total_wall_duration, Duration::zero());
}
