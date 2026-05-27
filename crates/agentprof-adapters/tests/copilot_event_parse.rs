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

#[test]
fn user_message_parses() {
    let line = r#"{"type":"user.message","data":{"content":"Hello","transformedContent":"<context>Hello</context>","source":"cli","attachments":[],"interactionId":"int-1"},"id":"e5","timestamp":"2026-05-26T10:01:00Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::UserMessage(env) => {
            assert_eq!(env.data.content, "Hello");
            assert_eq!(
                env.data.transformed_content.as_deref(),
                Some("<context>Hello</context>")
            );
            assert_eq!(env.data.interaction_id, "int-1");
        }
        other => panic!("expected UserMessage, got {other:?}"),
    }
}

#[test]
fn turn_start_and_end_parse() {
    let start = r#"{"type":"assistant.turn_start","data":{"turnId":"0","interactionId":"int-1"},"id":"e6","timestamp":"2026-05-26T10:01:01Z","parentId":"e5"}"#;
    let end = r#"{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e7","timestamp":"2026-05-26T10:01:10Z","parentId":"e6"}"#;

    let evt: CopilotEvent = serde_json::from_str(start).unwrap();
    match evt {
        CopilotEvent::TurnStart(env) => assert_eq!(env.data.turn_id, "0"),
        other => panic!("expected TurnStart, got {other:?}"),
    }

    let evt: CopilotEvent = serde_json::from_str(end).unwrap();
    match evt {
        CopilotEvent::TurnEnd(env) => assert_eq!(env.data.turn_id, "0"),
        other => panic!("expected TurnEnd, got {other:?}"),
    }
}

#[test]
fn assistant_message_parses_with_tool_requests() {
    let line = r#"{"type":"assistant.message","data":{"messageId":"m1","model":"gpt-5-mini","content":"Let me check.","toolRequests":[{"toolCallId":"tc1","name":"bash","arguments":{"command":"ls"},"type":"function","intentionSummary":"list files"}],"interactionId":"int-1","turnId":"0","outputTokens":42},"id":"e8","timestamp":"2026-05-26T10:01:05Z","parentId":"e6"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::AssistantMessage(env) => {
            assert_eq!(env.data.message_id, "m1");
            assert_eq!(env.data.model, "gpt-5-mini");
            assert_eq!(env.data.tool_requests.len(), 1);
            assert_eq!(env.data.tool_requests[0].name, "bash");
            assert_eq!(env.data.output_tokens, 42);
            assert_eq!(env.data.turn_id, "0");
        }
        other => panic!("expected AssistantMessage, got {other:?}"),
    }
}

#[test]
fn assistant_message_handles_absent_optional_fields() {
    let line = r#"{"type":"assistant.message","data":{"messageId":"m2","model":"gpt-5-mini","content":"OK","toolRequests":[],"interactionId":"int-2","turnId":"1","outputTokens":5},"id":"e9","timestamp":"2026-05-26T10:02:00Z","parentId":"e8"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::AssistantMessage(env) => {
            assert!(env.data.reasoning_text.is_none());
            assert!(env.data.reasoning_opaque.is_none());
            assert!(env.data.encrypted_content.is_none());
            assert!(env.data.request_id.is_none());
        }
        _ => panic!("expected AssistantMessage"),
    }
}

#[test]
fn system_message_parses() {
    let line = r#"{"type":"system.message","data":{"role":"system","content":"You are an AI."},"id":"e10","timestamp":"2026-05-26T10:00:30Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::SystemMessage(env) => {
            assert_eq!(env.data.role, "system");
            assert!(env.data.content.starts_with("You are"));
        }
        _ => panic!("expected SystemMessage"),
    }
}
