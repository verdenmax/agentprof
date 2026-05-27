//! Per-variant round-trip tests for `CopilotEvent`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use agentprof_adapters::copilot::CopilotEvent;

#[test]
fn unknown_event_type_falls_through_to_unknown_variant() {
    let line = r#"{"type":"some.future.event","data":{},"id":"e1","timestamp":"2026-05-26T10:00:00Z","parentId":null}"#;
    let event: CopilotEvent =
        serde_json::from_str(line).expect("parse must succeed via #[serde(other)]");
    assert!(matches!(event, CopilotEvent::Unknown));
}

#[test]
fn empty_object_fails_to_parse_as_event() {
    let line = "{}";
    let result = serde_json::from_str::<CopilotEvent>(line);
    assert!(result.is_err(), "missing `type` tag should fail to parse");
}
