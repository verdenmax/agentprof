//! Fixture-driven integration tests for the 4 aggregators in
//! `agentprof_core::analyzer::aggregate`.
//!
//! Placed under `agentprof-adapters/tests/` (not `agentprof-core/tests/`)
//! to avoid a dev-dependency cycle — adapters depend on core, not vice
//! versa. Mirrors `export_on_fixtures.rs` from M1.6.4.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::Adapter;
use agentprof_core::analyzer::aggregate::group_by_day::aggregate_by_day;
use agentprof_core::analyzer::aggregate::group_by_mcp::aggregate_by_mcp_server;
use agentprof_core::analyzer::aggregate::group_by_model::aggregate_by_model;
use agentprof_core::analyzer::aggregate::group_by_tool::aggregate_by_tool;
use agentprof_core::analyzer::{analyze, AnalysisReport};
use agentprof_core::episode::{derive_episodes, Episodes};

fn fixture(slug: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(slug)
}

fn load_session(slug: &str) -> (AnalysisReport, Episodes) {
    let adapter = CopilotAdapter;
    let root = fixture(slug).parent().unwrap().to_path_buf();
    let sessions = adapter.discover_sessions(&root).expect("discover");
    let sref = sessions
        .into_iter()
        .find(|s| s.path.parent().unwrap().ends_with(slug))
        .unwrap_or_else(|| panic!("fixture {slug} not discovered"));
    let raw = adapter.load_session(&sref).expect("load");
    let episodes = derive_episodes(&raw.events, &raw.meta);
    let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
    (report, episodes)
}

fn load_three_sessions() -> (Vec<AnalysisReport>, Vec<Episodes>) {
    let (ra, ea) = load_session("multi-sess-a");
    let (rb, eb) = load_session("multi-sess-b");
    let (rc, ec) = load_session("multi-sess-c");
    (vec![ra, rb, rc], vec![ea, eb, ec])
}

#[test]
fn aggregate_by_tool_sums_call_counts_across_sessions() {
    let (reports, eps) = load_three_sessions();
    let r = aggregate_by_tool(&reports, &eps);
    let bash = r
        .buckets
        .iter()
        .find(|b| b.name == "bash")
        .expect("bash bucket should exist");
    assert_eq!(bash.call_count, 5, "bash call_count across 3 sessions");
    // NOTE: as of M1.4, the Copilot derive layer always marks tool calls
    // as Success — the `success` bit on tool.execution_complete is wired
    // up in a later task ("Task 10b" per derive.rs). The aggregator
    // faithfully forwards whatever the analyzer reports, so we assert
    // the invariant rather than a hard failure count.
    assert_eq!(
        bash.success_count + bash.failure_count,
        bash.call_count,
        "success + failure must sum to call_count"
    );
    assert_eq!(bash.session_count, 3, "bash used in all 3 sessions");
}

#[test]
fn aggregate_by_tool_recomputes_percentiles_from_pool() {
    let (reports, eps) = load_three_sessions();
    let r = aggregate_by_tool(&reports, &eps);
    let bash = r.buckets.iter().find(|b| b.name == "bash").unwrap();
    // All 5 bash calls are exactly 1s each → p50 = p95 = 1s.
    assert_eq!(bash.p50_duration.num_seconds(), 1);
    assert_eq!(bash.p95_duration.num_seconds(), 1);
}

#[test]
fn aggregate_by_mcp_server_groups_by_server_prefix() {
    let (reports, eps) = load_three_sessions();
    let r = aggregate_by_mcp_server(&reports, &eps);
    let github = r
        .buckets
        .iter()
        .find(|b| b.server == "github")
        .expect("github MCP bucket should exist");
    // multi-sess-a: 1 call to mcp__github__list_pulls
    // multi-sess-c: 1 call to mcp__github__create_pr
    assert_eq!(github.call_count, 2);
    assert_eq!(github.tool_count, 2, "two distinct tool names under github");
    assert_eq!(github.session_count, 2);
}

#[test]
fn aggregate_by_day_emits_one_bucket_per_utc_date() {
    let (reports, eps) = load_three_sessions();
    let r = aggregate_by_day(&reports, &eps, 20.0);
    assert_eq!(r.buckets.len(), 3, "3 distinct UTC dates → 3 buckets");
    let dates: Vec<_> = r.buckets.iter().map(|b| b.date).collect();
    let mut sorted = dates.clone();
    sorted.sort();
    assert_eq!(
        dates, sorted,
        "day buckets should be in date-ascending order"
    );
}

#[test]
fn aggregate_by_day_utilization_threshold_flag() {
    let (reports, eps) = load_three_sessions();
    // Threshold above 100% → every bucket is "low".
    let r = aggregate_by_day(&reports, &eps, 99.0);
    assert!(
        r.buckets.iter().all(|b| b.is_low_utilization),
        "threshold=99 should flag every bucket as low"
    );
    // Threshold of 0 → no bucket flagged (clamp lower bound is 0).
    let r2 = aggregate_by_day(&reports, &eps, 0.0);
    assert!(
        r2.buckets.iter().all(|b| !b.is_low_utilization),
        "threshold=0 should flag none"
    );
}

#[test]
fn aggregate_by_day_zero_wall_no_panic() {
    let (reports, eps) = load_three_sessions();
    let r = aggregate_by_day(&reports, &eps, 20.0);
    for b in &r.buckets {
        assert!(
            b.utilization_pct.is_finite(),
            "utilization must be finite: {b:?}"
        );
        assert!(
            (0.0..=100.0).contains(&b.utilization_pct),
            "utilization clamped [0,100]: {b:?}"
        );
    }
}

#[test]
fn aggregate_by_model_picks_first_turn_model() {
    let (reports, eps) = load_three_sessions();
    let r = aggregate_by_model(&reports, &eps);
    let gpt = r.buckets.iter().find(|b| b.model == "gpt-5").unwrap();
    assert_eq!(gpt.session_count, 2, "a + b both use gpt-5");
    let claude = r
        .buckets
        .iter()
        .find(|b| b.model == "claude-sonnet-4.6")
        .unwrap();
    assert_eq!(claude.session_count, 1, "c uses claude-sonnet-4.6");
}

#[test]
fn aggregate_by_tool_buckets_sorted_by_total_duration_desc() {
    let (reports, eps) = load_three_sessions();
    let r = aggregate_by_tool(&reports, &eps);
    for w in r.buckets.windows(2) {
        assert!(
            w[0].total_duration >= w[1].total_duration,
            "buckets must be sorted by total_duration desc; got {:?}, {:?}",
            w[0].total_duration,
            w[1].total_duration
        );
    }
}
