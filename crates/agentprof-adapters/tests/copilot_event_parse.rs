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

#[test]
fn tool_exec_start_parses() {
    let line = r#"{"type":"tool.execution_start","data":{"toolCallId":"tc1","toolName":"bash","arguments":{"command":"ls -la"}},"id":"e11","timestamp":"2026-05-26T10:01:06Z","parentId":"e8"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::ToolExecStart(env) => {
            assert_eq!(env.data.tool_call_id, "tc1");
            assert_eq!(env.data.tool_name, "bash");
            assert_eq!(env.data.arguments["command"], "ls -la");
        }
        other => panic!("expected ToolExecStart, got {other:?}"),
    }
}

#[test]
fn tool_exec_complete_parses() {
    let line = r#"{"type":"tool.execution_complete","data":{"toolCallId":"tc1","model":"gpt-5-mini","interactionId":"int-1","turnId":"0","success":true,"result":{"content":"file1\nfile2","detailedContent":"file1\nfile2"},"toolTelemetry":{"properties":{"command":"ls"},"metrics":{"resultLength":11},"restrictedProperties":{}}},"id":"e12","timestamp":"2026-05-26T10:01:07Z","parentId":"e11"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::ToolExecComplete(env) => {
            assert_eq!(env.data.tool_call_id, "tc1");
            assert!(env.data.success);
            assert!(env.data.result.content.contains("file1"));
            assert_eq!(env.data.tool_telemetry.metrics["resultLength"], 11);
        }
        other => panic!("expected ToolExecComplete, got {other:?}"),
    }
}

#[test]
fn tool_exec_complete_with_error_parses() {
    let line = r#"{"type":"tool.execution_complete","data":{"toolCallId":"tc2","model":"gpt-5-mini","interactionId":"int-1","success":false,"result":{"content":"command failed","detailedContent":"command failed: exit 1"},"toolTelemetry":{"properties":{},"metrics":{},"restrictedProperties":{}},"error":{"message":"non-zero exit"}},"id":"e13","timestamp":"2026-05-26T10:01:08Z","parentId":"e11"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::ToolExecComplete(env) => {
            assert!(!env.data.success);
            assert_eq!(env.data.error.as_ref().unwrap().message, "non-zero exit");
        }
        _ => panic!("expected ToolExecComplete"),
    }
}

#[test]
fn tool_user_requested_parses() {
    let line = r#"{"type":"tool.user_requested","data":{"toolCallId":"tc3","toolName":"bash","arguments":{"command":"git status","description":"check git state"}},"id":"e14","timestamp":"2026-05-26T10:02:00Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::ToolUserRequested(env) => {
            assert_eq!(env.data.tool_name, "bash");
            assert_eq!(env.data.arguments.command, "git status");
            assert_eq!(env.data.arguments.description, "check git state");
        }
        _ => panic!("expected ToolUserRequested"),
    }
}

#[test]
fn hook_start_parses() {
    let line = r#"{"type":"hook.start","data":{"hookInvocationId":"hi1","hookType":"SessionStart","input":{"sessionId":"abc-123","timestamp":1716718800000,"cwd":"/tmp/proj","source":"startup","initialPrompt":"fix the bug"}},"id":"e15","timestamp":"2026-05-26T10:03:00Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::HookStart(env) => {
            assert_eq!(env.data.hook_invocation_id, "hi1");
            assert_eq!(env.data.hook_type, "SessionStart");
            assert_eq!(env.data.input.session_id, "abc-123");
            assert_eq!(env.data.input.timestamp, 1_716_718_800_000_u64);
            assert_eq!(env.data.input.cwd, "/tmp/proj");
            assert_eq!(
                env.data.input.initial_prompt.as_deref(),
                Some("fix the bug")
            );
        }
        _ => panic!("expected HookStart"),
    }
}

#[test]
fn hook_end_parses() {
    let line = r#"{"type":"hook.end","data":{"hookInvocationId":"hi1","hookType":"SessionStart","output":{"additionalContext":"loaded skills"},"success":true},"id":"e16","timestamp":"2026-05-26T10:03:01Z","parentId":"e15"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::HookEnd(env) => {
            assert_eq!(env.data.hook_invocation_id, "hi1");
            assert!(env.data.success);
            let output = env.data.output.as_ref().expect("output present");
            assert_eq!(output.additional_context.as_deref(), Some("loaded skills"));
        }
        _ => panic!("expected HookEnd"),
    }
}

#[test]
fn hook_end_with_no_output_parses() {
    let line = r#"{"type":"hook.end","data":{"hookInvocationId":"hi2","hookType":"PreToolUse","success":false},"id":"e17","timestamp":"2026-05-26T10:03:02Z","parentId":"e15"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::HookEnd(env) => {
            assert!(!env.data.success);
            assert!(env.data.output.is_none());
        }
        _ => panic!("expected HookEnd"),
    }
}

#[test]
fn skill_invoked_parses() {
    let line = r#"{"type":"skill.invoked","data":{"name":"using-superpowers","path":"/skills/using-superpowers","content":"...","source":"plugin","pluginName":"superpowers","pluginVersion":"0.3.0","description":"meta skill","trigger":"session.start"},"id":"e18","timestamp":"2026-05-26T10:03:03Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::SkillInvoked(env) => {
            assert_eq!(env.data.name, "using-superpowers");
            assert_eq!(env.data.source, "plugin");
            assert_eq!(env.data.plugin_name.as_deref(), Some("superpowers"));
            assert_eq!(env.data.trigger, "session.start");
        }
        _ => panic!("expected SkillInvoked"),
    }
}

#[test]
fn abort_parses() {
    let line = r#"{"type":"abort","data":{"reason":"user_interrupt"},"id":"e19","timestamp":"2026-05-26T10:03:04Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::Abort(env) => {
            assert_eq!(env.data.reason, "user_interrupt");
        }
        _ => panic!("expected Abort"),
    }
}

use agentprof_core::adapter::{Event, EventKind};

#[test]
fn event_kind_for_each_variant() {
    let cases: &[(&str, EventKind)] = &[
        (
            r#"{"type":"session.start","data":{"sessionId":"s","version":1,"producer":"p","copilotVersion":"1","startTime":"2026-05-26T10:00:00Z","context":{"cwd":"/x"},"alreadyInUse":false},"id":"e1","timestamp":"2026-05-26T10:00:00Z","parentId":null}"#,
            EventKind::SessionStart,
        ),
        (
            r#"{"type":"session.info","data":{"infoType":"folder_trust","message":"m"},"id":"e2","timestamp":"2026-05-26T10:00:00Z","parentId":"e1"}"#,
            EventKind::SessionInfo,
        ),
        (
            r#"{"type":"session.mode_changed","data":{"previousMode":"a","newMode":"b"},"id":"e3","timestamp":"2026-05-26T10:00:00Z","parentId":"e1"}"#,
            EventKind::ModeChanged,
        ),
        (
            r#"{"type":"abort","data":{"reason":"user_interrupt"},"id":"e4","timestamp":"2026-05-26T10:00:00Z","parentId":"e1"}"#,
            EventKind::Abort,
        ),
        (
            r#"{"type":"some.future.event","data":{},"id":"e5","timestamp":"2026-05-26T10:00:00Z","parentId":null}"#,
            EventKind::Unknown,
        ),
    ];

    for (line, expected_kind) in cases {
        let evt: CopilotEvent = serde_json::from_str(line).unwrap();
        assert_eq!(evt.kind(), *expected_kind, "kind mismatch for: {line}");
    }
}

#[test]
fn event_timestamp_returned_correctly() {
    fn via_trait<E: Event>(e: &E) -> (String, EventKind, String, Option<String>) {
        (
            e.id().to_string(),
            e.kind(),
            e.timestamp().to_rfc3339(),
            e.parent_id().map(str::to_string),
        )
    }
    let line = r#"{"type":"session.info","data":{"infoType":"folder_trust","message":"m"},"id":"e1","timestamp":"2026-05-26T12:34:56Z","parentId":"parent-id"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    let (id, kind, ts, parent) = via_trait(&evt);
    assert_eq!(id, "e1");
    assert_eq!(kind, EventKind::SessionInfo);
    assert_eq!(ts, "2026-05-26T12:34:56+00:00");
    assert_eq!(parent.as_deref(), Some("parent-id"));
}

#[test]
fn event_parent_id_none_when_null() {
    let line = r#"{"type":"session.start","data":{"sessionId":"s","version":1,"producer":"p","copilotVersion":"1","startTime":"2026-05-26T10:00:00Z","context":{"cwd":"/x"},"alreadyInUse":false},"id":"e1","timestamp":"2026-05-26T10:00:00Z","parentId":null}"#;
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    assert_eq!(evt.parent_id(), None);

    // Also: Unknown returns sentinel values.
    let line2 = r#"{"type":"x.unknown","data":{},"id":"e","timestamp":"2026-05-26T10:00:00Z","parentId":null}"#;
    let evt2: CopilotEvent = serde_json::from_str(line2).unwrap();
    assert_eq!(evt2.id(), "");
    assert_eq!(evt2.parent_id(), None);
    assert_eq!(evt2.kind(), EventKind::Unknown);
}
