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
