//! Integration tests for `AnalysisReport::redact`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::redact::PrivacyLevel;
use agentprof_core::analyzer::{AnalysisReport, TurnSummaryRow};
use agentprof_core::episode::TurnStatus;
use agentprof_core::model::SessionMeta;
use chrono::{TimeZone, Utc};

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
