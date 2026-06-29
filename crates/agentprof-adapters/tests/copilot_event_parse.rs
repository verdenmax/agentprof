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
            assert_eq!(env.data.turn_id.as_deref(), Some("0"));
            assert!(env.data.parent_tool_call_id.is_none());
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
            let result = env.data.result.as_ref().expect("result present on success");
            assert!(result.content.as_deref().unwrap_or("").contains("file1"));
            let telemetry = env.data.tool_telemetry.as_ref().expect("telemetry present");
            assert_eq!(telemetry.metrics["resultLength"], 11);
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
fn tool_telemetry_absent_restricted_properties_does_not_serialize_null() {
    // P2 backlog `tooltelemetry-restricted-props-skip-if`: when the
    // wire payload omits `restrictedProperties` entirely (older Copilot
    // CLI versions), we deserialize to Value::Null via #[serde(default)]
    // and must skip re-emitting it on serialize. Otherwise round-trip
    // gains a spurious `"restrictedProperties": null` field that wasn't
    // in the source.
    let line = r#"{"type":"tool.execution_complete","data":{"toolCallId":"tc-skip","model":"gpt-5-mini","interactionId":"int-1","success":true,"result":{"content":"ok","detailedContent":"ok"},"toolTelemetry":{"properties":{"k":"v"},"metrics":{}}},"id":"e-skip","timestamp":"2026-05-26T10:01:09Z","parentId":"e11"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).expect("parse");
    let CopilotEvent::ToolExecComplete(env) = evt else {
        panic!("expected ToolExecComplete");
    };
    let tel = env.data.tool_telemetry.expect("toolTelemetry present");
    assert!(
        tel.restricted_properties.is_null(),
        "absent input must deserialize to Null"
    );
    let round = serde_json::to_string(&tel).expect("serialize");
    assert!(
        !round.contains("restrictedProperties"),
        "null restricted_properties must NOT be emitted on serialize; got: {round}"
    );
}

#[test]
fn tool_telemetry_present_restricted_properties_round_trips() {
    // Regression guard: when the field IS present (even as `{}`), it
    // must still round-trip through serialize.
    let line = r#"{"type":"tool.execution_complete","data":{"toolCallId":"tc-keep","model":"gpt-5-mini","interactionId":"int-1","success":true,"result":{"content":"ok","detailedContent":"ok"},"toolTelemetry":{"properties":{},"metrics":{},"restrictedProperties":{"redacted":"value"}}},"id":"e-keep","timestamp":"2026-05-26T10:01:10Z","parentId":"e11"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).expect("parse");
    let CopilotEvent::ToolExecComplete(env) = evt else {
        panic!("expected ToolExecComplete");
    };
    let tel = env.data.tool_telemetry.expect("toolTelemetry present");
    let round = serde_json::to_string(&tel).expect("serialize");
    assert!(
        round.contains("restrictedProperties"),
        "non-null restricted_properties MUST be emitted on serialize; got: {round}"
    );
}

#[test]
fn tool_execution_start_with_object_arguments_round_trips() {
    // Real Copilot CLI 1.0.x wire shape: arbitrary JSON object in `arguments`,
    // plus a `turnId` sibling not present in the M1.2 clean-room schema.
    let raw = r#"{
        "type": "tool.execution_start",
        "id": "evt-tes-1",
        "timestamp": "2026-05-26T12:30:30.001Z",
        "parentId": "p-1",
        "data": {
            "arguments": { "path": "/tmp/x.rs", "view_range": [100, 165] },
            "toolCallId": "toolu_abc",
            "toolName": "view",
            "turnId": "78"
        }
    }"#;
    let ev: CopilotEvent = serde_json::from_str(raw).expect("parse");
    let env = match &ev {
        CopilotEvent::ToolExecStart(e) => e,
        other => panic!("expected ToolExecStart, got {other:?}"),
    };
    assert_eq!(env.data.tool_call_id, "toolu_abc");
    assert_eq!(env.data.tool_name, "view");
    assert_eq!(env.data.turn_id.as_deref(), Some("78"));
    assert_eq!(env.data.arguments["path"], "/tmp/x.rs");

    let back = serde_json::to_string(&ev).expect("serialize");
    let again: CopilotEvent = serde_json::from_str(&back).expect("re-parse");
    assert!(matches!(again, CopilotEvent::ToolExecStart(_)));
}

#[test]
fn tool_execution_complete_with_telemetry_round_trips() {
    // Real Copilot CLI 1.0.x success payload: `result.{content,detailedContent}`,
    // plus `toolTelemetry.{metrics,properties}` and `model` / `interactionId`.
    let raw = r#"{
        "type": "tool.execution_complete",
        "id": "evt-tec-1",
        "timestamp": "2026-05-26T12:30:33.098Z",
        "parentId": "p-1",
        "data": {
            "interactionId": "i-1",
            "model": "claude-opus-4.7-1m-internal",
            "result": { "content": "ok", "detailedContent": "ok details" },
            "success": true,
            "toolCallId": "tc-1",
            "toolTelemetry": {
                "metrics": { "commandTimeout": 30000 },
                "properties": { "executionMode": "sync" }
            },
            "turnId": "87"
        }
    }"#;
    let ev: CopilotEvent = serde_json::from_str(raw).expect("parse");
    let env = match &ev {
        CopilotEvent::ToolExecComplete(e) => e,
        other => panic!("expected ToolExecComplete, got {other:?}"),
    };
    assert_eq!(env.data.tool_call_id, "tc-1");
    assert_eq!(
        env.data.model.as_deref(),
        Some("claude-opus-4.7-1m-internal")
    );
    assert!(env.data.success);
    let result = env.data.result.as_ref().expect("result present");
    assert_eq!(result.content.as_deref(), Some("ok"));
    assert_eq!(result.detailed_content.as_deref(), Some("ok details"));
    let telemetry = env.data.tool_telemetry.as_ref().expect("telemetry");
    assert_eq!(telemetry.metrics["commandTimeout"], 30000);

    let back = serde_json::to_string(&ev).expect("serialize");
    let again: CopilotEvent = serde_json::from_str(&back).expect("re-parse");
    assert!(matches!(again, CopilotEvent::ToolExecComplete(_)));
}

#[test]
fn tool_execution_complete_with_error_round_trips() {
    // Real Copilot CLI 1.0.x failure payload: `result` is OMITTED, telemetry
    // is `{}`, and `error.{code,message}` carries the failure reason.
    let raw = r#"{
        "type": "tool.execution_complete",
        "id": "evt-tec-fail-1",
        "timestamp": "2026-05-26T12:31:00.000Z",
        "parentId": "p-1",
        "data": {
            "error": { "code": "failure", "message": "\"command\": Required" },
            "interactionId": "i-2",
            "model": "claude-opus-4.7-1m-internal",
            "success": false,
            "toolCallId": "tc-2",
            "toolTelemetry": {},
            "turnId": "95"
        }
    }"#;
    let ev: CopilotEvent = serde_json::from_str(raw).expect("parse");
    let env = match &ev {
        CopilotEvent::ToolExecComplete(e) => e,
        other => panic!("expected ToolExecComplete, got {other:?}"),
    };
    assert!(!env.data.success);
    assert!(env.data.result.is_none(), "failure case has no result");
    let err = env.data.error.as_ref().expect("error present");
    assert_eq!(err.code.as_deref(), Some("failure"));
    assert!(err.message.contains("Required"));

    let back = serde_json::to_string(&ev).expect("serialize");
    let again: CopilotEvent = serde_json::from_str(&back).expect("re-parse");
    assert!(matches!(again, CopilotEvent::ToolExecComplete(_)));
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

// -- M1.3 Task 4: round-trip tests for the 10 newly added variants. --

fn assert_round_trips(line: &str, expect: impl Fn(&CopilotEvent) -> bool) {
    let evt: CopilotEvent = serde_json::from_str(line).expect("initial parse");
    assert!(expect(&evt), "wrong variant: {evt:?}");
    let back = serde_json::to_string(&evt).expect("serialize");
    let again: CopilotEvent = serde_json::from_str(&back).expect("re-parse");
    assert!(expect(&again), "re-parsed wrong variant: {again:?}");
}

#[test]
fn subagent_started_round_trips() {
    let line = r#"{"type":"subagent.started","agentId":"toolu_xyz","data":{"agentDescription":"Full-capability agent.","agentDisplayName":"General Purpose Agent","agentName":"general-purpose","toolCallId":"toolu_xyz"},"id":"e-sub-1","timestamp":"2026-05-26T13:06:40.114Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::SubagentStarted(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::SubagentStarted(env) => {
            assert_eq!(env.agent_id.as_deref(), Some("toolu_xyz"));
            assert_eq!(env.data.agent_name.as_deref(), Some("general-purpose"));
            assert_eq!(env.data.tool_call_id.as_deref(), Some("toolu_xyz"));
        }
        _ => panic!("expected SubagentStarted"),
    }
}

#[test]
fn subagent_completed_round_trips_with_metrics() {
    let line = r#"{"type":"subagent.completed","agentId":"toolu_xyz","data":{"agentDisplayName":"General Purpose Agent","agentName":"general-purpose","durationMs":80539,"model":"claude-opus-4.7-1m-internal","toolCallId":"toolu_xyz","totalTokens":197023,"totalToolCalls":4},"id":"e-sub-2","timestamp":"2026-05-26T13:14:30.424Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::SubagentCompleted(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    match evt {
        CopilotEvent::SubagentCompleted(env) => {
            assert_eq!(env.data.duration_ms, Some(80_539));
            assert_eq!(env.data.total_tokens, Some(197_023));
            assert_eq!(env.data.total_tool_calls, Some(4));
        }
        _ => panic!("expected SubagentCompleted"),
    }
}

#[test]
fn subagent_completed_round_trips_without_metrics() {
    let line = r#"{"type":"subagent.completed","agentId":"toolu_zzz","data":{"agentDisplayName":"Code Review Agent","agentName":"code-review","toolCallId":"toolu_zzz"},"id":"e-sub-3","timestamp":"2026-05-27T02:12:23.331Z","parentId":"p2"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::SubagentCompleted(_)));
}

#[test]
fn subagent_failed_round_trips() {
    let line = r#"{"type":"subagent.failed","data":{"agentDisplayName":"Rubber Duck Agent","agentName":"rubber-duck","durationMs":7036,"error":"CAPIError: 400 Invalid schema","toolCallId":"tooluse_x","totalToolCalls":0},"id":"e-sub-4","timestamp":"2026-04-15T13:56:49.046Z","parentId":"p3"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::SubagentFailed(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::SubagentFailed(env) = evt {
        assert!(env.data.error.starts_with("CAPIError"));
        assert_eq!(env.data.total_tool_calls, Some(0));
    } else {
        panic!("expected SubagentFailed");
    }
}

#[test]
fn session_warning_round_trips() {
    let line = r#"{"type":"session.warning","data":{"message":"MCP server 'protein-copilot' is slow","warningType":"mcp"},"id":"e-sw-1","timestamp":"2026-04-28T12:50:51.432Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::SessionWarning(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::SessionWarning(env) = evt {
        assert_eq!(env.data.warning_type.as_deref(), Some("mcp"));
    } else {
        panic!("expected SessionWarning");
    }
}

#[test]
fn session_resume_round_trips() {
    let line = r#"{"type":"session.resume","data":{"alreadyInUse":false,"context":{"cwd":"/home/USER/proj"},"eventCount":281,"reasoningEffort":"high","resumeTime":"2026-05-04T06:55:09.418Z","selectedModel":"claude-opus-4.6"},"id":"e-sr-1","timestamp":"2026-05-04T06:55:09.418Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::SessionResume(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::SessionResume(env) = evt {
        assert_eq!(env.data.event_count, Some(281));
        assert_eq!(env.data.selected_model.as_deref(), Some("claude-opus-4.6"));
        assert_eq!(
            env.data.context.as_ref().map(|c| c.cwd.as_str()),
            Some("/home/USER/proj"),
        );
    } else {
        panic!("expected SessionResume");
    }
}

#[test]
fn session_compaction_start_round_trips() {
    let line = r#"{"type":"session.compaction_start","data":{"conversationTokens":944498,"systemTokens":8279,"toolDefinitionsTokens":7836},"id":"e-cs-1","timestamp":"2026-05-14T12:41:30.551Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| {
        matches!(e, CopilotEvent::SessionCompactionStart(_))
    });
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::SessionCompactionStart(env) = evt {
        assert_eq!(env.data.conversation_tokens, Some(944_498));
        assert_eq!(env.data.system_tokens, Some(8_279));
        assert_eq!(env.data.tool_definitions_tokens, Some(7_836));
    } else {
        panic!("expected SessionCompactionStart");
    }
}

#[test]
fn session_compaction_complete_round_trips_newer_shape() {
    let line = r#"{"type":"session.compaction_complete","data":{"checkpointNumber":3,"checkpointPath":"/tmp/x","compactionTokensUsed":{"cacheReadTokens":36226,"cacheWriteTokens":0,"duration":28204,"inputTokens":36232,"model":"claude-opus-4.7-xhigh","outputTokens":2463},"preCompactionMessagesLength":5,"preCompactionTokens":713261,"success":true,"summaryContent":"<overview>..."},"id":"e-cc-1","timestamp":"2026-05-13T02:25:35.332Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| {
        matches!(e, CopilotEvent::SessionCompactionComplete(_))
    });
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::SessionCompactionComplete(env) = evt {
        assert_eq!(env.data.checkpoint_number, Some(3));
        assert_eq!(env.data.success, Some(true));
        let tokens = env.data.compaction_tokens_used.as_ref().unwrap();
        assert_eq!(tokens.cache_read_tokens, Some(36_226));
        assert_eq!(tokens.input_tokens, Some(36_232));
    } else {
        panic!("expected SessionCompactionComplete");
    }
}

#[test]
fn session_compaction_complete_round_trips_older_shape() {
    let line = r#"{"type":"session.compaction_complete","data":{"checkpointNumber":4,"checkpointPath":"/tmp/y","compactionTokensUsed":{"cachedInput":0,"input":109298,"output":3742},"preCompactionMessagesLength":253,"preCompactionTokens":135484,"requestId":"00000-x","success":true,"summaryContent":"<overview>"},"id":"e-cc-2","timestamp":"2026-04-08T12:56:24.469Z","parentId":"p2"}"#;
    assert_round_trips(line, |e| {
        matches!(e, CopilotEvent::SessionCompactionComplete(_))
    });
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::SessionCompactionComplete(env) = evt {
        let tokens = env.data.compaction_tokens_used.as_ref().unwrap();
        assert_eq!(tokens.cached_input, Some(0));
        assert_eq!(tokens.input, Some(109_298));
        assert_eq!(tokens.output, Some(3_742));
    } else {
        panic!("expected SessionCompactionComplete");
    }
}

#[test]
fn system_notification_round_trips() {
    let line = r#"{"type":"system.notification","data":{"content":"<system_notification>...</system_notification>","kind":{"agentId":"spec-review-task-02","agentType":"general-purpose","description":"Spec review Task 2","prompt":"You are reviewing ...","status":"completed","type":"agent_completed"}},"id":"e-sn-1","timestamp":"2026-05-18T03:22:20.667Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::SystemNotification(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::SystemNotification(env) = evt {
        assert_eq!(env.data.kind["type"], "agent_completed");
        assert_eq!(env.data.kind["status"], "completed");
    } else {
        panic!("expected SystemNotification");
    }
}

#[test]
fn permission_requested_round_trips() {
    let line = r#"{"type":"permission.requested","data":{"permissionRequest":{"canOfferSessionApproval":true,"fileName":"/tmp/a.tex","kind":"write","toolCallId":"toolu_vrtx_x"},"promptRequest":{"canOfferSessionApproval":true,"kind":"write","toolCallId":"toolu_vrtx_x"},"requestId":"150ee914-bab6-40ea-8418-c08acea6438b"},"id":"e-pr-1","timestamp":"2026-05-04T06:14:56.428Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::PermissionRequested(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::PermissionRequested(env) = evt {
        assert_eq!(
            env.data.request_id.as_deref(),
            Some("150ee914-bab6-40ea-8418-c08acea6438b"),
        );
        assert!(env.data.permission_request.is_some());
        assert!(env.data.prompt_request.is_some());
    } else {
        panic!("expected PermissionRequested");
    }
}

#[test]
fn permission_completed_round_trips() {
    let line = r#"{"type":"permission.completed","data":{"requestId":"84544f98-7e67-4033-9832-066a997648a7","result":{"kind":"approved"},"toolCallId":"toolu_vrtx_016MLeADTkFmg9QFBk76Pekc"},"id":"e-pc-1","timestamp":"2026-05-04T06:03:59.182Z","parentId":"p1"}"#;
    assert_round_trips(line, |e| matches!(e, CopilotEvent::PermissionCompleted(_)));
    let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    if let CopilotEvent::PermissionCompleted(env) = evt {
        assert_eq!(env.data.result.kind, "approved");
        assert_eq!(
            env.data.tool_call_id.as_deref(),
            Some("toolu_vrtx_016MLeADTkFmg9QFBk76Pekc"),
        );
    } else {
        panic!("expected PermissionCompleted");
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

#[test]
fn with_mcp_waste_fixture_extracts_expected_loaded_set() {
    use agentprof_adapters::copilot::{
        tools_changed::extract_loaded_set_from_session, CopilotAdapter,
    };
    use agentprof_core::adapter::Adapter;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/copilot");
    let adapter = CopilotAdapter;
    let sessions = adapter.discover_sessions(&root).expect("discover");
    let sref = sessions
        .iter()
        .find(|s| s.path.parent().unwrap().file_name().unwrap() == "with-mcp-waste")
        .expect("with-mcp-waste fixture present");
    let raw = adapter.load_session(sref).expect("load");
    let loaded = extract_loaded_set_from_session(&raw.events);
    assert_eq!(loaded.len(), 3, "3 MCP tools advertised in fixture");
    assert!(loaded.contains("mcp__github__search_issues"));
    assert!(loaded.contains("mcp__github__create_issue"));
    assert!(loaded.contains("mcp__filesystem__read_file"));
}

#[test]
fn copilot_user_message_with_mcp_load_extracts_tool_names() {
    // Locks the M2.1 T5.2.5 `Event::payload_loaded_mcp_tools()` trait
    // contract: a CopilotEvent::UserMessage whose `transformedContent`
    // embeds a <tools_changed_notice> block must surface the MCP tool
    // names (and only the MCP ones — bash/edit/skill__ etc. are filtered).
    //
    // This exercises the per-event trait override
    // (CopilotEvent → payload_loaded_mcp_tools) rather than the
    // session-level wrapper extract_loaded_set_from_session.
    use agentprof_core::adapter::Event;

    // Real Copilot user.message wire shape with an embedded
    // <tools_changed_notice> block listing MCP + builtin + skill tools;
    // only the mcp__ prefixed ones must come back from the trait method.
    let line = r#"{
        "type":"user.message",
        "data":{
            "content":"Continue.",
            "transformedContent":"<context>noise</context>\n<tools_changed_notice>\nNew tools available: mcp__github__search_issues, mcp__github__create_issue, bash, edit, skill__telemetry__report\n</tools_changed_notice>",
            "source":"cli",
            "attachments":[],
            "interactionId":"int-7"
        },
        "id":"e-load","timestamp":"2026-05-26T11:00:00Z","parentId":"e0"
    }"#;
    let evt: CopilotEvent = serde_json::from_str(line).expect("parse");

    let loaded = evt.payload_loaded_mcp_tools();
    assert_eq!(
        loaded.len(),
        2,
        "only the two mcp__ entries should pass the filter, got {loaded:?}"
    );
    assert!(loaded.contains("mcp__github__search_issues"));
    assert!(loaded.contains("mcp__github__create_issue"));
    // Negative checks — non-MCP tools must NOT appear.
    assert!(!loaded.contains("bash"));
    assert!(!loaded.contains("edit"));
    assert!(!loaded.contains("skill__telemetry__report"));
}

#[test]
fn copilot_non_user_message_event_yields_empty_loaded_set() {
    // Defensive companion to the above: a non-UserMessage event (here:
    // session.model_change) must return an empty BTreeSet — the trait
    // default — regardless of any other payload data.
    use agentprof_core::adapter::Event;
    let line = r#"{"type":"session.model_change","data":{"newModel":"claude-opus-4.7"},"id":"e4","timestamp":"2026-05-26T10:06:00Z","parentId":"e1"}"#;
    let evt: CopilotEvent = serde_json::from_str(line).expect("parse");
    assert!(evt.payload_loaded_mcp_tools().is_empty());
}
