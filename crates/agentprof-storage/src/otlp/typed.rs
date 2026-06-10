//! Intermediate representation between raw OTLP wire types and persistable
//! storage rows.
//!
//! OTLP `LogRecord`, `Metric`, and `Span` are heterogeneous, verbose, and
//! protobuf-shaped. The pipeline converts each incoming signal at the
//! boundary into a uniform internal [`TypedEvent`] enum so the rest of the
//! OTLP subsystem (mapper output, [`crate::otlp`] router, session buffer
//! flush) can pattern-match on a small, stable surface.
//!
//! This module is **pure data**: no I/O, no async, no protobuf or tonic
//! dependencies. The mapper (M2.2 T5.2) is the only producer; the
//! session router (M2.2 T6.1) and persistable conversion (M2.2 T7.1) are
//! the only consumers.
//!
//! The variant set mirrors the seven Claude Code OTLP signals we
//! currently understand (spec §1.1, §3.2). New agents add variants here;
//! [`TypedEvent`], [`SignalKind`], and [`TokenDirection`] are all
//! `#[non_exhaustive]` so adding a variant is non-breaking.

use std::path::PathBuf;

use agentprof_core::adapter::AgentKind;
use agentprof_core::episode::tool::ToolCallStatus;
use agentprof_core::model::tool_source::ToolSource;
use chrono::{DateTime, Utc};

/// Uniform internal representation of one decoded OTLP signal.
///
/// Each variant corresponds to one `claude_code.*` event name (spec §1.1)
/// or one OTLP data-point flavor we care about. Variants are
/// `#[non_exhaustive]` at the enum level so future agents can extend the
/// set without a breaking change.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::typed::TypedEvent;
/// use agentprof_core::adapter::AgentKind;
/// use chrono::Utc;
///
/// let ev = TypedEvent::SessionStart {
///     session_id: "abc-123".into(),
///     agent: AgentKind::Claude,
///     started_at: Utc::now(),
///     model: Some("claude-sonnet-4.6".into()),
///     cwd: None,
/// };
/// assert_eq!(ev.session_id(), Some("abc-123"));
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TypedEvent {
    /// `claude_code.session.start` log → opens a `SessionBuffer`.
    SessionStart {
        /// `OTel` resource attribute `session.id` (spec §5.2 fallback chain).
        session_id: String,
        /// Agent producing the signal, derived from `service.name` /
        /// `agent.kind` (spec §5.2).
        agent: AgentKind,
        /// Wall-clock start time (from `LogRecord.time_unix_nano`).
        started_at: DateTime<Utc>,
        /// Model identifier when announced at session start.
        model: Option<String>,
        /// Working directory of the agent process when available.
        cwd: Option<PathBuf>,
    },
    /// `claude_code.user_prompt` log → contributes one entry to
    /// `Episodes.turns[*]`.
    UserPrompt {
        /// `OTel` resource attribute `session.id`.
        session_id: String,
        /// Per-turn identifier (string to accommodate uuids and ints alike).
        turn_id: String,
        /// Wall-clock time of the prompt.
        timestamp: DateTime<Utc>,
        /// Size of the prompt in bytes when reported by the emitter.
        prompt_size_bytes: Option<u64>,
    },
    /// `claude_code.tool_decision` log → opens a `ToolCall` entry in
    /// `Episodes.tools`.
    ToolDecisionStart {
        /// `OTel` resource attribute `session.id`.
        session_id: String,
        /// Turn this tool call belongs to (optional — some emitters
        /// don't tag the surrounding turn).
        turn_id: Option<String>,
        /// Tool identifier (e.g. `bash`, `mcp__github__list_issues`).
        tool_name: String,
        /// Where the tool came from (builtin / MCP server / skill).
        source: ToolSource,
        /// Wall-clock time the decision was logged.
        timestamp: DateTime<Utc>,
        /// `true` when the user explicitly approved this call.
        user_approved: bool,
    },
    /// `claude_code.tool_result` log → closes a previously-opened
    /// `ToolCall`. Pairing rules live in spec §5.4.
    ToolResult {
        /// `OTel` resource attribute `session.id`.
        session_id: String,
        /// Same `turn_id` as the matching `ToolDecisionStart`, when
        /// available.
        turn_id: Option<String>,
        /// Same `tool_name` as the matching `ToolDecisionStart`.
        tool_name: String,
        /// Wall-clock time the result was logged.
        timestamp: DateTime<Utc>,
        /// Terminal status of the tool call.
        status: ToolCallStatus,
    },
    /// `claude_code.token.usage` metric data point → rolls up into
    /// `AnalysisReport.model_metrics`.
    TokenUsage {
        /// `OTel` resource attribute `session.id`.
        session_id: String,
        /// Model the tokens were billed to (e.g. `claude-sonnet-4.6`).
        model: String,
        /// Which counter on the model this point increments.
        direction: TokenDirection,
        /// Token count for this data point.
        value: u64,
        /// Wall-clock time of the metric data point.
        timestamp: DateTime<Utc>,
    },
    /// `claude_code.session.end` log → flushes the `SessionBuffer`.
    SessionEnd {
        /// `OTel` resource attribute `session.id`.
        session_id: String,
        /// Wall-clock end time (from `LogRecord.time_unix_nano`).
        ended_at: DateTime<Utc>,
    },
    /// Catch-all for signals we don't yet understand. The mapper emits
    /// this so the router can `tracing::debug!` once and drop, instead
    /// of silently swallowing unknown events.
    Unrecognized {
        /// Which OTLP signal carried the unrecognized payload.
        signal: SignalKind,
        /// Free-form identity string (event name, metric name, span
        /// name) used for the debug log.
        identity: String,
    },
}

impl TypedEvent {
    /// Returns the `session.id` this event belongs to, or `None` for
    /// [`TypedEvent::Unrecognized`].
    ///
    /// The router (M2.2 T6.1) uses this to key events into per-session
    /// buffers; events without a session id are dropped with a debug log
    /// per spec §5.2 / §5.5.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::typed::{TypedEvent, SignalKind};
    ///
    /// let ev = TypedEvent::Unrecognized {
    ///     signal: SignalKind::Log,
    ///     identity: "claude_code.future_event".into(),
    /// };
    /// assert_eq!(ev.session_id(), None);
    /// ```
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::SessionStart { session_id, .. }
            | Self::UserPrompt { session_id, .. }
            | Self::ToolDecisionStart { session_id, .. }
            | Self::ToolResult { session_id, .. }
            | Self::TokenUsage { session_id, .. }
            | Self::SessionEnd { session_id, .. } => Some(session_id.as_str()),
            Self::Unrecognized { .. } => None,
        }
    }
}

/// Which OTLP signal a payload arrived on.
///
/// Used by [`TypedEvent::Unrecognized`] to disambiguate logs / metrics /
/// traces when reporting an unrecognized identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SignalKind {
    /// OTLP Logs (`ExportLogsServiceRequest`).
    Log,
    /// OTLP Metrics (`ExportMetricsServiceRequest`).
    Metric,
    /// OTLP Traces (`ExportTraceServiceRequest`).
    Trace,
}

/// Which counter on a model a [`TypedEvent::TokenUsage`] point updates.
///
/// Mirrors Anthropic's billing categories (input / output / cache read /
/// cache creation). Future providers extend this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenDirection {
    /// Prompt / input tokens billed to the model.
    Input,
    /// Completion / output tokens billed to the model.
    Output,
    /// Cache-read tokens (Anthropic prompt cache hit).
    CacheRead,
    /// Cache-creation tokens (Anthropic prompt cache write).
    CacheCreation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap()
    }

    #[test]
    fn typed_event_variants_constructible() {
        let events = [
            TypedEvent::SessionStart {
                session_id: "s1".into(),
                agent: AgentKind::Claude,
                started_at: ts(),
                model: Some("claude-sonnet-4.6".into()),
                cwd: Some(PathBuf::from("/tmp/proj")),
            },
            TypedEvent::UserPrompt {
                session_id: "s1".into(),
                turn_id: "t1".into(),
                timestamp: ts(),
                prompt_size_bytes: Some(42),
            },
            TypedEvent::ToolDecisionStart {
                session_id: "s1".into(),
                turn_id: Some("t1".into()),
                tool_name: "bash".into(),
                source: ToolSource::Builtin,
                timestamp: ts(),
                user_approved: true,
            },
            TypedEvent::ToolResult {
                session_id: "s1".into(),
                turn_id: Some("t1".into()),
                tool_name: "bash".into(),
                timestamp: ts(),
                status: ToolCallStatus::Success,
            },
            TypedEvent::TokenUsage {
                session_id: "s1".into(),
                model: "claude-sonnet-4.6".into(),
                direction: TokenDirection::Input,
                value: 1234,
                timestamp: ts(),
            },
            TypedEvent::SessionEnd {
                session_id: "s1".into(),
                ended_at: ts(),
            },
            TypedEvent::Unrecognized {
                signal: SignalKind::Log,
                identity: "claude_code.future".into(),
            },
        ];

        assert_eq!(events.len(), 7);
        for ev in &events[..6] {
            assert_eq!(ev.session_id(), Some("s1"));
        }
        assert_eq!(events[6].session_id(), None);
    }

    #[test]
    fn signal_kind_hash_eq() {
        assert_eq!(SignalKind::Log, SignalKind::Log);
        assert_ne!(SignalKind::Log, SignalKind::Metric);

        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        SignalKind::Trace.hash(&mut h1);
        SignalKind::Trace.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn token_direction_distinct_and_copy() {
        let a = TokenDirection::Input;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(TokenDirection::Input, TokenDirection::CacheRead);
        assert_ne!(TokenDirection::Output, TokenDirection::CacheCreation);
    }
}
