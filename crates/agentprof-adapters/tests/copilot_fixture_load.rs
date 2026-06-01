//! Fixture-driven integration tests for `parse_events_jsonl`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fs;
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

#[test]
fn every_fixture_line_parses_as_copilot_event() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/copilot");
    let dirs = [
        "minimal",
        "builtin-tools-only",
        "with-mcp-calls",
        "with-skill-invoked",
        "with-hooks-heavy",
        "with-aborts",
        "with-mode-transitions",
        "orphan-events",
        "cross-turn-tool",
        "with-post-tool-use-hooks",
        "with-span-overlap",
        // NOTE: "corrupt" intentionally contains an unparseable line.
        // NOTE: "live-truncated" intentionally has a truncated tail.
    ];
    for dir in dirs {
        let path = base.join(dir).join("events.jsonl");
        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for (line_no, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let parsed: Result<agentprof_adapters::copilot::CopilotEvent, _> =
                serde_json::from_str(line);
            assert!(
                parsed.is_ok(),
                "{} line {line_no}: {parsed:?}\n  line: {line}",
                path.display()
            );
        }
    }
}

#[test]
fn builtin_tools_only_fixture_loads() {
    let raw = parse_events_jsonl(&fixture_path("builtin-tools-only"), false).expect("parse");
    assert_eq!(raw.events.len(), 10);
    assert_eq!(raw.parse_warnings.len(), 0);
}

#[test]
fn with_mcp_calls_fixture_loads() {
    let raw = parse_events_jsonl(&fixture_path("with-mcp-calls"), false).expect("parse");
    assert_eq!(raw.parse_warnings.len(), 0);
    let tool_names: Vec<&str> = raw
        .events
        .iter()
        .filter_map(|e| match e {
            agentprof_adapters::copilot::CopilotEvent::ToolExecStart(env) => {
                Some(env.data.tool_name.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(tool_names.len(), 2);
    assert!(tool_names.iter().all(|n| n.starts_with("mcp__")));
}

#[test]
fn with_skill_invoked_fixture_loads_with_skill_event() {
    let raw = parse_events_jsonl(&fixture_path("with-skill-invoked"), false).expect("parse");
    assert!(raw.events.iter().any(|e| matches!(
        e,
        agentprof_adapters::copilot::CopilotEvent::SkillInvoked(_)
    )));
}

#[test]
fn live_truncated_with_is_live_true_silently_skips_partial_tail() {
    let raw =
        parse_events_jsonl(&fixture_path("live-truncated"), /* is_live */ true).expect("parse");
    // 3 valid lines: session.start + user.message + turn_start.
    assert_eq!(raw.events.len(), 3);
    // No warning about the truncated tail — silent skip.
    assert_eq!(
        raw.parse_warnings.len(),
        0,
        "live session suppresses partial-tail warning"
    );
    assert!(raw.meta.is_live);
}

#[test]
fn live_truncated_with_is_live_false_emits_warning_for_partial_tail() {
    let raw =
        parse_events_jsonl(&fixture_path("live-truncated"), /* is_live */ false).expect("parse");
    assert_eq!(raw.events.len(), 3);
    assert_eq!(
        raw.parse_warnings.len(),
        1,
        "closed session reports tail as broken"
    );
}
