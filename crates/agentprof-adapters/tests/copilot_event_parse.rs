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

#[test]
fn session_start_parses() {
    let line = r#"{"type":"session.start","data":{"sessionId":"abc-123","version":1,"producer":"copilot-agent","copilotVersion":"1.0.54","startTime":"2026-05-26T10:00:00Z","context":{"cwd":"/tmp/proj","gitRoot":"/tmp/proj","branch":"main","headCommit":"abc","repository":"owner/repo","hostType":"github"},"alreadyInUse":false},"id":"e1","timestamp":"2026-05-26T10:00:00.123Z","parentId":null}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::SessionStart(env) => {
            assert_eq!(env.data.session_id, "abc-123");
            assert_eq!(env.data.version, 1);
            assert_eq!(env.data.copilot_version, "1.0.54");
            assert_eq!(env.data.context.cwd, "/tmp/proj");
            assert_eq!(env.data.context.repository.as_deref(), Some("owner/repo"));
        }
        other => panic!("expected SessionStart, got {other:?}"),
    }
}

#[test]
fn session_shutdown_parses_with_model_metrics() {
    let line = r#"{"type":"session.shutdown","data":{"shutdownType":"normal","totalPremiumRequests":3,"totalApiDurationMs":12345,"sessionStartTime":1779700000000,"codeChanges":{"linesAdded":10,"linesRemoved":2,"filesModified":["src/a.rs"]},"modelMetrics":{"gpt-5-mini":{"requests":{"count":3},"usage":{"input":100,"output":50}}},"currentModel":"gpt-5-mini"},"id":"e2","timestamp":"2026-05-26T11:00:00Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::Shutdown(env) => {
            assert_eq!(env.data.total_premium_requests, 3);
            assert_eq!(env.data.code_changes.lines_added, 10);
            assert_eq!(env.data.code_changes.files_modified.len(), 1);
            assert!(env.data.model_metrics.contains_key("gpt-5-mini"));
        }
        other => panic!("expected Shutdown, got {other:?}"),
    }
}

#[test]
fn mode_changed_parses() {
    let line = r#"{"type":"session.mode_changed","data":{"previousMode":"interactive","newMode":"plan"},"id":"e3","timestamp":"2026-05-26T10:05:00Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::ModeChanged(env) => {
            assert_eq!(env.data.previous_mode, "interactive");
            assert_eq!(env.data.new_mode, "plan");
        }
        other => panic!("expected ModeChanged, got {other:?}"),
    }
}

#[test]
fn model_change_has_no_previous_field() {
    let line = r#"{"type":"session.model_change","data":{"newModel":"claude-opus-4.7"},"id":"e4","timestamp":"2026-05-26T10:06:00Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    assert!(matches!(evt, CopilotEvent::ModelChange(_)));
}
