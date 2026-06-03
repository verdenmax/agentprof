//! Tool episode aggregation.

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::episode::turn::Span;
use crate::model::ToolSource;

/// Per-tool-name aggregation across all calls in a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolEpisode {
    /// Tool name (the key under which this episode is stored).
    pub name: String,
    /// Origin of the tool (builtin / MCP / unknown).
    pub source: ToolSource,
    /// All invocations of this tool, in event order.
    pub calls: Vec<ToolCall>,
    /// Sum of `call.span.duration()` across `calls`.
    pub total_duration: Duration,
    /// Number of calls whose status is `Failure { .. }`.
    pub fail_count: u32,
}

impl ToolEpisode {
    /// Construct an empty `ToolEpisode` for the given tool name + source.
    #[must_use]
    pub const fn new(name: String, source: ToolSource) -> Self {
        Self {
            name,
            source,
            calls: Vec::new(),
            total_duration: Duration::zero(),
            fail_count: 0,
        }
    }
}

/// One invocation of a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCall {
    /// Time interval covering the call (start → end).
    pub span: Span,
    /// Owning turn id, when the call was attributable to an open turn.
    pub turn_id: Option<String>,
    /// Terminal status of the call.
    pub status: ToolCallStatus,
    /// `true` if the call originated from `ToolUserRequested` (manual approval).
    pub user_requested: bool,
    /// Tool arguments JSON value, when the adapter captured and emitted
    /// it via [`crate::adapter::Event::payload_tool_requests`]. `None`
    /// when either (a) the adapter did not implement that method for
    /// the relevant variant, or (b) no tool-request event with a
    /// matching `tool_call_id` was found in the session. Case (b)
    /// covers orphan completes, mid-session resumes, and (in principle)
    /// any non-orphan call whose request event was lost or filtered —
    /// `derive_episodes` will still attempt the lookup for orphan
    /// completes (the args-map collection in PASS 0 is independent of
    /// the tool start/complete pairing).
    ///
    /// Skipped in JSON output when `None` to keep the schema clean for
    /// archives produced by adapters that don't yet plumb args.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

impl ToolCall {
    /// Construct with status `Success` by default — adjust before pushing.
    #[must_use]
    pub const fn new(span: Span) -> Self {
        Self {
            span,
            turn_id: None,
            status: ToolCallStatus::Success,
            user_requested: false,
            arguments: None,
        }
    }
}

/// Terminal status of a tool call.
///
/// See `docs/internals/adr-0004-episode-derivation.md` D-2 for orphan semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolCallStatus {
    /// `tool.execution_complete` arrived with `success = true`.
    Success,
    /// `tool.execution_complete` arrived with `success = false`.
    Failure {
        /// Optional error message captured from the wire payload.
        message: Option<String>,
    },
    /// `tool.execution_complete` arrived without preceding `tool.execution_start`;
    /// algorithm synthesized a zero-duration Start at the End's timestamp.
    OrphanSynthesizedStart,
    /// `tool.execution_start` arrived but no matching End by end of events;
    /// `span.ended_at` was clamped to the last event timestamp.
    OpenAtEndOfSession,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn episode_new_starts_empty() {
        let ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        assert!(ep.calls.is_empty());
        assert_eq!(ep.fail_count, 0);
        assert_eq!(ep.total_duration, Duration::zero());
    }

    #[test]
    fn tool_call_new_defaults_to_success() {
        let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
        let call = ToolCall::new(Span::instant(t));
        assert_eq!(call.status, ToolCallStatus::Success);
        assert!(!call.user_requested);
        assert_eq!(call.turn_id, None);
    }
}

#[cfg(test)]
mod arguments_field_tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn one_sec_span() -> Span {
        Span::new(
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 1).unwrap(),
        )
    }

    #[test]
    fn tool_call_default_arguments_is_none() {
        let tc = ToolCall::new(one_sec_span());
        assert!(tc.arguments.is_none());
    }

    #[test]
    fn tool_call_serde_roundtrip_with_arguments() {
        let mut tc = ToolCall::new(one_sec_span());
        tc.arguments = Some(serde_json::json!({"cmd": "ls", "verbose": true}));
        let json = serde_json::to_string(&tc).unwrap();
        let back: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(back.arguments, tc.arguments);
    }

    #[test]
    fn tool_call_arguments_skipped_when_none_in_json() {
        let tc = ToolCall::new(one_sec_span());
        let json = serde_json::to_string(&tc).unwrap();
        assert!(
            !json.contains("\"arguments\""),
            "None arguments should be skipped: {json}"
        );
    }

    #[test]
    fn tool_call_arguments_present_when_some_in_json() {
        let mut tc = ToolCall::new(one_sec_span());
        tc.arguments = Some(serde_json::json!({"x": 1}));
        let json = serde_json::to_string(&tc).unwrap();
        assert!(
            json.contains("\"arguments\""),
            "Some arguments should serialize: {json}"
        );
    }

    #[test]
    fn tool_call_deserializes_legacy_json_without_arguments_field() {
        // Pre-F1 archives (written before ToolCall.arguments existed) must
        // still deserialize cleanly. This locks the `#[serde(default)]`
        // attribute against accidental removal.
        let legacy = r#"{
            "span": {"started_at":"2026-06-03T00:00:00Z","ended_at":"2026-06-03T00:00:01Z"},
            "turn_id": null,
            "status": "Success",
            "user_requested": false
        }"#;
        let tc: ToolCall = serde_json::from_str(legacy).unwrap();
        assert!(tc.arguments.is_none());
    }
}
