//! Integration tests for `AnalysisReport::redact`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::redact::PrivacyLevel;
use agentprof_core::analyzer::{AnalysisReport, ModelUsage, ToolRankRow, TurnSummaryRow};
use agentprof_core::episode::TurnStatus;
use agentprof_core::model::{SessionMeta, ToolSource};
use chrono::{Duration, TimeZone, Utc};
use std::collections::BTreeMap;

fn sample() -> AnalysisReport {
    let mut meta = SessionMeta::new(
        "11111111-1111-1111-1111-111111111111".into(),
        AgentKind::Copilot,
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        false,
    );
    meta.cwd = Some("/home/alice/projects/secret".into());
    meta.branch = Some("feat/secret".into());
    meta.repository = Some("alice/secret-repo".into());
    meta.agent_version = Some("1.0.54".into());
    let mut r = AnalysisReport::new(meta);
    let row = TurnSummaryRow::new(
        "22222222-2222-2222-2222-222222222222".into(),
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        None,
        TurnStatus::Open,
        Some("claude-opus-4.7-1m-internal".into()),
        None,
        None,
        0,
        0,
        0,
    );
    r.turn_summary.push(row);
    r
}

#[test]
fn redact_strips_high_tier_and_keeps_map_empty() {
    let (out, map) = sample().redact(PrivacyLevel::Redact);
    assert_eq!(out.meta.cwd.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.branch.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.repository.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.id, "<uuid-0>");
    assert_eq!(out.turn_summary[0].turn_id, "<uuid-1>");
    assert_eq!(out.turn_summary[0].model.as_deref(), Some("claude-opus"));
    assert_eq!(out.meta.agent_version.as_deref(), Some("1.0.54")); // kept at redact
    assert!(map.is_empty(), "redact level → empty map");
}

#[test]
fn anonymize_strips_version_and_fills_map() {
    let (out, map) = sample().redact(PrivacyLevel::Anonymize);
    assert_eq!(out.meta.agent_version.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.started_at, chrono::DateTime::<Utc>::UNIX_EPOCH);
    assert_eq!(
        map.uuids.get("<uuid-0>").map(String::as_str),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(
        map.models.get("claude-opus").map(String::as_str),
        Some("claude-opus-4.7-1m-internal")
    );
}

#[test]
fn none_is_identity() {
    let (out, map) = sample().redact(PrivacyLevel::None);
    assert_eq!(out, sample());
    assert!(map.is_empty());
}

#[test]
fn anonymize_hashes_mcp_tool_and_records_server() {
    let mut r = sample();
    r.tool_rank.push(ToolRankRow::new(
        "mcp__github__search_issues".into(),
        ToolSource::Mcp {
            server: "github".into(),
        },
        1,
        1,
        0,
        0,
        0,
        Duration::zero(),
        Duration::zero(),
        Duration::zero(),
        Duration::zero(),
    ));
    r.loaded_mcp_tools
        .insert("mcp__github__search_issues".into());

    let (out, map) = r.redact(PrivacyLevel::Anonymize);

    let name = &out.tool_rank[0].name;
    assert!(name.starts_with("mcp__"), "got {name}");
    assert!(name.ends_with("__search_issues"), "got {name}");
    assert_ne!(name, "mcp__github__search_issues"); // server segment changed
    assert!(
        map.mcp_servers.values().any(|v| v == "github"),
        "mcp_servers should map a hash back to github: {:?}",
        map.mcp_servers
    );
}

#[test]
fn cross_site_uuid_stability() {
    let shared = "33333333-3333-3333-3333-333333333333";
    let mut meta = SessionMeta::new(
        shared.into(),
        AgentKind::Copilot,
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        false,
    );
    meta.cwd = Some("/home/alice/x".into());
    let mut r = AnalysisReport::new(meta);
    r.turn_summary.push(TurnSummaryRow::new(
        shared.into(),
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        None,
        TurnStatus::Open,
        None,
        None,
        None,
        0,
        0,
        0,
    ));

    let (out, _map) = r.redact(PrivacyLevel::Redact);
    assert_eq!(
        out.meta.id, out.turn_summary[0].turn_id,
        "same source UUID must map to the same placeholder"
    );
}

#[test]
fn model_metrics_merge_on_family_collision() {
    let mut r = sample();
    let mut a = ModelUsage::new();
    a.input_tokens = 100;
    a.output_tokens = 10;
    a.cache_read_tokens = 5;
    a.cache_write_tokens = 1;
    let mut b = ModelUsage::new();
    b.input_tokens = 200;
    b.output_tokens = 20;
    b.cache_read_tokens = 7;
    b.cache_write_tokens = 2;
    let mut mm = BTreeMap::new();
    mm.insert("gpt-5".to_string(), a);
    mm.insert("gpt-5-mini".to_string(), b);
    r.model_metrics = Some(mm);

    let (out, _map) = r.redact(PrivacyLevel::Redact);
    let merged = out.model_metrics.expect("model_metrics present");
    assert_eq!(merged.len(), 1, "both collapse to one gpt-5 family");
    let g = merged.get("gpt-5").expect("gpt-5 family key");
    assert_eq!(g.input_tokens, 300);
    assert_eq!(g.output_tokens, 30);
    assert_eq!(g.cache_read_tokens, 12);
    assert_eq!(g.cache_write_tokens, 3);
}

#[test]
fn redaction_clears_diagnostics() {
    use agentprof_core::adapter::EventKind;
    use agentprof_core::episode::DeriveWarning;
    use agentprof_core::error::ParseWarning;

    let mut r = sample();
    r.warnings.push(DeriveWarning::PayloadNameMissing {
        kind: EventKind::ToolExecStart,
        event_id: "44444444-4444-4444-4444-444444444444".into(),
    });
    r.parse_warnings.push(ParseWarning::UnclosedTurn {
        turn_id: "55555555-5555-5555-5555-555555555555".into(),
    });

    let (redacted, _m) = r.clone().redact(PrivacyLevel::Redact);
    assert!(redacted.warnings.is_empty(), "warnings must be cleared");
    assert!(
        redacted.parse_warnings.is_empty(),
        "parse_warnings must be cleared"
    );

    let (anon, _m) = r.redact(PrivacyLevel::Anonymize);
    assert!(anon.warnings.is_empty());
    assert!(anon.parse_warnings.is_empty());
}

// --- L-1 T3: AggregateReport::redact ---------------------------------------

use agentprof_core::analyzer::aggregate::bucket::{DayBucket, McpServerBucket, ModelBucket};
use agentprof_core::analyzer::aggregate::{AggregateKey, AggregateReport};
use chrono::NaiveDate;

const fn report<B>(by: AggregateKey, buckets: Vec<B>) -> AggregateReport<B> {
    AggregateReport::new(by, None, 0, 0, Duration::zero(), buckets)
}
fn model_bucket(model: &str) -> ModelBucket {
    ModelBucket::new(model.into(), 0, 0, 0, Duration::zero())
}

fn mcp_bucket(server: &str) -> McpServerBucket {
    McpServerBucket::new(server.into(), 0, 0, 0, Duration::zero(), 0)
}

fn day_bucket(date: &str) -> DayBucket {
    DayBucket::new(
        NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
        0,
        Duration::zero(),
        Duration::zero(),
        0,
        0.0,
        false,
    )
}

#[test]
fn aggregate_model_bucket_redacts_to_family() {
    let report: AggregateReport<ModelBucket> = report(
        AggregateKey::Model,
        vec![model_bucket("claude-opus-4.7-1m-internal")],
    );
    let (out, _map) = report.redact(PrivacyLevel::Redact);
    assert_eq!(out.buckets[0].model, "claude-opus");
}

#[test]
fn aggregate_mcp_server_hashed_only_at_anonymize() {
    let report: AggregateReport<McpServerBucket> =
        report(AggregateKey::McpServer, vec![mcp_bucket("github")]);
    let (redacted, m1) = report.redact(PrivacyLevel::Redact);
    assert_eq!(redacted.buckets[0].server, "github"); // redact: unchanged
    assert!(m1.is_empty());
    let (anon, m2) = report.redact(PrivacyLevel::Anonymize);
    assert_ne!(anon.buckets[0].server, "github");
    assert!(m2.mcp_servers.values().any(|v| v == "github"));
}

#[test]
fn aggregate_day_bucket_never_redacted() {
    let report: AggregateReport<DayBucket> =
        report(AggregateKey::Day, vec![day_bucket("2026-05-26")]);
    let (out, _) = report.redact(PrivacyLevel::Anonymize);
    assert_eq!(
        out.buckets[0].date,
        NaiveDate::parse_from_str("2026-05-26", "%Y-%m-%d").unwrap()
    );
}
