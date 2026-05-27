//! Fixture-driven integration tests for `parse_events_jsonl`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use agentprof_adapters::copilot::parser::parse_events_jsonl;
use agentprof_core::error::ParseWarning;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(name)
        .join("events.jsonl")
}

#[test]
fn minimal_fixture_loads_6_events_and_no_warnings() {
    let raw = parse_events_jsonl(&fixture_path("minimal"), false).expect("must parse");
    assert_eq!(raw.events.len(), 6, "minimal fixture has 6 events");
    assert_eq!(
        raw.parse_warnings.len(),
        0,
        "minimal fixture has no warnings"
    );
    assert_eq!(raw.meta.id, "00000000-0000-0000-0000-000000000001");
    assert_eq!(raw.meta.agent, agentprof_core::adapter::AgentKind::Copilot);
    assert!(!raw.meta.is_live);
}

#[test]
fn corrupt_fixture_skips_bad_line_and_emits_warning() {
    let raw = parse_events_jsonl(&fixture_path("corrupt"), false).expect("must parse");
    assert_eq!(raw.events.len(), 5, "corrupt fixture: 5 valid events");
    assert_eq!(raw.parse_warnings.len(), 1, "exactly one parse warning");
    match &raw.parse_warnings[0] {
        ParseWarning::Json { line_no, .. } => assert_eq!(*line_no, 2),
        other => panic!("expected ParseWarning::Json, got {other:?}"),
    }
}

#[test]
fn minimal_fixture_snapshot() {
    let raw = parse_events_jsonl(&fixture_path("minimal"), false).expect("must parse");
    insta::assert_json_snapshot!("minimal_raw_session", raw);
}
