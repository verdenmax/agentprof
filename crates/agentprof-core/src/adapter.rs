//! Adapter trait and supporting types.
//!
//! This module defines the contract that every per-agent adapter implements
//! ([`Adapter`]), the [`Event`] trait that adapter-specific event types must
//! satisfy, and auxiliary types used at the boundary ([`AgentKind`],
//! [`EventKind`], [`SessionRef`], [`AdapterError`]).
//!
//! Concrete adapter implementations live in the `agentprof-adapters` crate.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::session::RawSession;

/// Which AI agent's session log this adapter targets.
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
///
/// assert_eq!("copilot".parse::<AgentKind>().unwrap(), AgentKind::Copilot);
/// assert_eq!(AgentKind::Copilot.to_string(), "copilot");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
#[cfg_attr(feature = "clap-derive", derive(clap::ValueEnum))]
pub enum AgentKind {
    /// GitHub Copilot CLI (`~/.copilot/session-state/<uuid>/events.jsonl`).
    Copilot,
    /// Anthropic Claude Code (`~/.claude/projects/**/*.jsonl`). Reserved for Phase 2.
    Claude,
    /// `OpenAI` Codex CLI. Reserved for Phase 3.
    Codex,
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Copilot => f.write_str("copilot"),
            Self::Claude => f.write_str("claude"),
            Self::Codex => f.write_str("codex"),
        }
    }
}

impl FromStr for AgentKind {
    type Err = ParseAgentKindError;

    /// Parse a lowercase agent name (`"copilot"`, `"claude"`, `"codex"`) into an [`AgentKind`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseAgentKindError`] when the input is not one of the
    /// three recognized lowercase strings. Casing is significant —
    /// `"Copilot"` does NOT parse; use lowercase.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use std::str::FromStr;
    /// assert_eq!(AgentKind::from_str("copilot").unwrap(), AgentKind::Copilot);
    /// assert!(AgentKind::from_str("Copilot").is_err());
    /// assert!(AgentKind::from_str("nope").is_err());
    /// ```
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "copilot" => Ok(Self::Copilot),
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            other => Err(ParseAgentKindError(other.to_owned())),
        }
    }
}

/// Returned by [`AgentKind::from_str`] when the input does not match a known agent.
#[derive(Debug, thiserror::Error)]
#[error("unknown agent kind: {0:?}; expected one of: copilot, claude, codex")]
pub struct ParseAgentKindError(String);

/// Coarse classification of an event for cheap pattern matching by analyzers
/// that don't care about per-payload details.
///
/// Variants mirror the 28 canonical event categories observed in the
/// `events.jsonl` wire format (per `docs/internals/adr-0002-copilot-event-schema.md`
/// plus the M1.3 schema-audit expansion) plus an [`EventKind::Unknown`]
/// forward-compat sentinel — 29 total.
/// Adapters that don't emit a given event type simply never produce that variant.
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::EventKind;
/// let k = EventKind::ToolExecStart;
/// fn is_tool(k: EventKind) -> bool {
///     matches!(k, EventKind::ToolExecStart | EventKind::ToolExecComplete | EventKind::ToolUserRequested)
/// }
/// assert!(is_tool(k));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventKind {
    /// Session lifecycle (start of recording).
    SessionStart,
    /// Session info/status message.
    SessionInfo,
    /// Mode transition (interactive/plan/autopilot).
    ModeChanged,
    /// Model switched mid-session.
    ModelChange,
    /// Plan document modified (Copilot-specific).
    PlanChanged,
    /// Session terminated; carries shutdown summary.
    Shutdown,
    /// User-authored message.
    UserMessage,
    /// Assistant turn opened.
    TurnStart,
    /// Assistant-authored message (may include tool call requests).
    AssistantMessage,
    /// Assistant turn closed.
    TurnEnd,
    /// Tool execution begin.
    ToolExecStart,
    /// Tool execution completion (success or error).
    ToolExecComplete,
    /// User-requested tool execution (manual approval path).
    ToolUserRequested,
    /// Lifecycle hook begin (Copilot-specific).
    HookStart,
    /// Lifecycle hook end.
    HookEnd,
    /// Skill invocation (Copilot-specific).
    SkillInvoked,
    /// System-emitted message (e.g. system prompt injection).
    SystemMessage,
    /// System-emitted structured notification (e.g. agent completion banner).
    SystemNotification,
    /// Session received a non-fatal warning (e.g. MCP server unresponsive).
    SessionWarning,
    /// Existing session resumed from on-disk state.
    SessionResume,
    /// Conversation context compaction begun.
    SessionCompactionStart,
    /// Conversation context compaction finished.
    SessionCompactionComplete,
    /// Permission request awaiting user / policy decision.
    PermissionRequested,
    /// Permission request resolved (approved / denied / cancelled).
    PermissionCompleted,
    /// Subagent invocation started.
    SubagentStarted,
    /// Subagent finished successfully.
    SubagentCompleted,
    /// Subagent failed before completion.
    SubagentFailed,
    /// Turn aborted (user cancel or internal failure).
    Abort,
    /// Forward-compat: event type the parser didn't recognize.
    Unknown,
}

/// Adapter-side event trait.
///
/// Implementations of [`Adapter`] produce a stream of values of an associated
/// type that satisfies this trait. Analyzers in `agentprof-core::episode`
/// (M1.3) consume only this trait, not the concrete event enum.
pub trait Event {
    /// Stable per-event identifier (typically the wire-format UUID).
    /// Returns `""` for forward-compat sentinel variants (e.g. `CopilotEvent::Unknown`).
    fn id(&self) -> &str;
    /// Coarse kind for cheap pattern matching.
    fn kind(&self) -> EventKind;
    /// Event timestamp, normalized to UTC.
    fn timestamp(&self) -> DateTime<Utc>;
    /// Parent event ID (forms a DAG mirroring the trace tree); `None` at session start.
    fn parent_id(&self) -> Option<&str>;

    /// Adapter-specific payload-defined name for the event (e.g. tool name,
    /// hook name/type, skill name). Returns `None` for events without such a
    /// concept (`session.start`, `user.message`, etc).
    ///
    /// Used by `derive_episodes` to key tools/hooks/skills by their real
    /// payload names instead of opaque event IDs. Default returns `None`;
    /// adapters override for payload-bearing variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    ///     // payload_name() inherits the default `None` impl.
    /// }
    /// assert_eq!(StubEvent.payload_name(), None);
    /// ```
    fn payload_name(&self) -> Option<&str> {
        None
    }

    /// Adapter-specific model identifier for the AI provider that produced
    /// this event. Returns `Some` for variants whose payload carries a
    /// model name (e.g. `AssistantMessage` in `CopilotEvent`), `None`
    /// otherwise.
    ///
    /// Used by `derive_episodes` to populate `Turn.model` (last-wins
    /// across assistant messages within a turn). M1.5 ROI computations
    /// will use this for per-token price lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    ///     // payload_model() inherits the default `None` impl.
    /// }
    /// assert_eq!(StubEvent.payload_model(), None);
    /// ```
    fn payload_model(&self) -> Option<&str> {
        None
    }

    /// Adapter-specific output token count for events that report it
    /// (e.g. `AssistantMessage` in `CopilotEvent`). Returns `None` for
    /// other variants.
    ///
    /// Used by `derive_episodes` to populate `Turn.output_tokens`
    /// (saturating sum across assistant messages within a turn). M1.5 ROI
    /// computations will use this for per-message cost calculation.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    /// }
    /// assert_eq!(StubEvent.payload_output_tokens(), None);
    /// ```
    fn payload_output_tokens(&self) -> Option<u32> {
        None
    }

    /// Adapter-specific new mode string for mode-transition events
    /// (e.g. `ModeChanged` in `CopilotEvent`). Returns `None` for variants
    /// without a mode payload.
    ///
    /// Used by `derive_episodes` to track the active session mode and
    /// attribute it to subsequently-opened turns. The string is converted
    /// to [`crate::episode::Mode`] via `Mode::from_wire`.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    /// }
    /// assert_eq!(StubEvent.payload_mode(), None);
    /// ```
    fn payload_mode(&self) -> Option<&str> {
        None
    }

    /// Adapter-specific `(tool_call_id, arguments)` pairs declared by this
    /// event. Returns empty for events without tool-request payloads.
    ///
    /// Used by [`crate::episode::derive_episodes`] to populate
    /// `ToolCall::arguments` — the args data point
    /// lives separately from the span on the wire (Copilot:
    /// `assistant.message.tool_requests[*]` and
    /// `tool.user_requested.arguments`), so the derive function needs
    /// a first-pass map keyed by `tool_call_id` before it can attach
    /// args to the matching span on close.
    ///
    /// Default returns empty `Vec`; adapters override for relevant
    /// payload-bearing variants. See ADR-0011 D-1 / D-2.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    ///     // payload_tool_requests() inherits the default `Vec::new()` impl.
    /// }
    /// assert!(StubEvent.payload_tool_requests().is_empty());
    /// ```
    fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
        Vec::new()
    }

    /// Adapter-specific `tool_call_id` for events that carry one
    /// (e.g. `ToolExecStart`, `ToolExecComplete`, `ToolUserRequested`).
    /// Returns `None` for events without the concept.
    ///
    /// Used by [`crate::episode::derive_episodes`] to look up
    /// `(tool_call_id → arguments)` pairs collected in PASS 0 from
    /// [`Self::payload_tool_requests`].
    ///
    /// Default returns `None`; adapters override for variants whose
    /// payload carries `tool_call_id`.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    /// }
    /// assert_eq!(StubEvent.tool_call_id(), None);
    /// ```
    fn tool_call_id(&self) -> Option<&str> {
        None
    }

    /// Adapter-specific per-model token-usage rollup, when the event
    /// reports it (e.g. Copilot CLI's `session.shutdown`). Returns
    /// `None` for events without the data.
    ///
    /// Used by `derive_episodes` to populate `Episodes::model_metrics`,
    /// which `analyze()` then clones into
    /// `AnalysisReport::model_metrics`. (Those fields are added in
    /// later F1.7 tasks; backticked here as forward refs.)
    ///
    /// Singular semantics — multiple events emitting non-None values
    /// in the same session (unusual but possible) follow last-wins by
    /// event order (matches existing [`crate::episode::Turn::model`]
    /// semantics). See ADR-0012 D-4 + D-6.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    /// }
    /// assert!(StubEvent.payload_model_metrics().is_none());
    /// ```
    fn payload_model_metrics(
        &self,
    ) -> Option<std::collections::BTreeMap<String, crate::analyzer::ModelUsage>> {
        None
    }
}

/// Reference to a single discoverable session.
///
/// Produced by [`Adapter::discover_sessions`]; consumed by [`Adapter::load_session`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SessionRef {
    /// Stable session identifier (typically a UUID).
    pub id: String,
    /// Which agent produced this session.
    pub agent: AgentKind,
    /// Filesystem path to the canonical events log (e.g. `events.jsonl`).
    pub path: PathBuf,
    /// Last modification time of `path`.
    pub modified_at: SystemTime,
    /// Size in bytes of `path`.
    pub size_bytes: u64,
    /// `true` if the session is currently being written (detected via an
    /// `inuse.<pid>.lock` file or equivalent).
    pub is_live: bool,
}

impl SessionRef {
    /// Construct a [`SessionRef`] from its raw components.
    ///
    /// Adapter implementations use this rather than struct-literal syntax
    /// because [`SessionRef`] is `#[non_exhaustive]` and therefore cannot
    /// be built from outside `agentprof-core`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use std::time::SystemTime;
    /// use agentprof_core::adapter::{AgentKind, SessionRef};
    ///
    /// let sref = SessionRef::new(
    ///     "abc".to_owned(),
    ///     AgentKind::Copilot,
    ///     PathBuf::from("/tmp/events.jsonl"),
    ///     SystemTime::UNIX_EPOCH,
    ///     0,
    ///     false,
    /// );
    /// assert_eq!(sref.id, "abc");
    /// ```
    #[must_use]
    pub const fn new(
        id: String,
        agent: AgentKind,
        path: PathBuf,
        modified_at: SystemTime,
        size_bytes: u64,
        is_live: bool,
    ) -> Self {
        Self {
            id,
            agent,
            path,
            modified_at,
            size_bytes,
            is_live,
        }
    }
}

/// Errors returned by [`Adapter`] implementations.
///
/// Single-line parse failures (e.g. one bad JSONL line) accumulate as
/// [`crate::error::ParseWarning`] inside the returned [`RawSession`] rather
/// than producing `AdapterError`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AdapterError {
    /// The discovered session root does not exist or is not a directory.
    ///
    /// Repair: verify the path exists, is a directory, and is readable; for
    /// the default Copilot location ensure `$HOME/.copilot/session-state`
    /// is present (it is created by Copilot CLI on first use).
    #[error(
        "session root not found: {path}; verify the path exists, is a directory, and is readable"
    )]
    RootNotFound {
        /// Offending path.
        path: PathBuf,
    },

    /// An I/O failure occurred while reading a session file.
    #[error("I/O error reading {path}: {source}")]
    Io {
        /// Offending path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The session log lacks a required `session.start`-equivalent event.
    #[error("session.start event missing in {path}")]
    MissingSessionStart {
        /// Offending path.
        path: PathBuf,
    },

    /// The session log declares a wire-format version newer than what this
    /// adapter supports.
    #[error("unsupported events.jsonl version: {version}, max supported: {max_supported}")]
    UnsupportedVersion {
        /// Version field encountered.
        version: u32,
        /// Highest version this adapter knows how to parse.
        max_supported: u32,
    },
}

/// Contract implemented by every per-agent adapter.
pub trait Adapter: Send + Sync {
    /// Adapter-specific event enum.
    type Event: Event + serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug;

    /// Which agent this adapter targets.
    fn agent_kind(&self) -> AgentKind;

    /// Conventional on-disk location of session logs for this agent.
    fn default_session_root(&self) -> Option<PathBuf>;

    /// Walk `root` and return references to every discoverable session.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::RootNotFound`] if `root` does not exist or is
    /// not a directory, or [`AdapterError::Io`] for filesystem read failures.
    fn discover_sessions(&self, root: &Path) -> Result<Vec<SessionRef>, AdapterError>;

    /// Read and parse a single session into a [`RawSession`].
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Io`] on read failures, or
    /// [`AdapterError::MissingSessionStart`] / [`AdapterError::UnsupportedVersion`]
    /// when the session log is structurally invalid.
    fn load_session(&self, sref: &SessionRef) -> Result<RawSession<Self::Event>, AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_kind_round_trip() {
        assert_eq!("copilot".parse::<AgentKind>().unwrap(), AgentKind::Copilot);
        assert_eq!(AgentKind::Copilot.to_string(), "copilot");
        assert_eq!("claude".parse::<AgentKind>().unwrap(), AgentKind::Claude);
        assert!("nope".parse::<AgentKind>().is_err());
    }

    #[test]
    fn agent_kind_serde_round_trip() {
        let agent = AgentKind::Copilot;
        let s = serde_json::to_string(&agent).unwrap();
        assert_eq!(s, "\"copilot\"");
        let back: AgentKind = serde_json::from_str(&s).unwrap();
        assert_eq!(back, agent);
    }

    #[test]
    fn event_kind_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<EventKind>();
    }

    #[test]
    fn session_ref_can_be_constructed() {
        let sref = SessionRef {
            id: "abc".into(),
            agent: AgentKind::Copilot,
            path: PathBuf::from("/tmp/x"),
            modified_at: SystemTime::UNIX_EPOCH,
            size_bytes: 0,
            is_live: false,
        };
        assert_eq!(sref.agent, AgentKind::Copilot);
    }

    struct DefaultPayloadNameEvent;
    impl Event for DefaultPayloadNameEvent {
        fn id(&self) -> &'static str {
            "default"
        }
        fn kind(&self) -> EventKind {
            EventKind::Unknown
        }
        fn timestamp(&self) -> chrono::DateTime<Utc> {
            Utc::now()
        }
        fn parent_id(&self) -> Option<&str> {
            None
        }
    }

    #[test]
    fn default_payload_name_is_none() {
        assert!(DefaultPayloadNameEvent.payload_name().is_none());
    }

    #[test]
    fn default_payload_model_is_none() {
        use chrono::TimeZone;
        struct DefaultPayloadModelEvent;
        impl Event for DefaultPayloadModelEvent {
            fn id(&self) -> &'static str {
                "e"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> DateTime<Utc> {
                Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(DefaultPayloadModelEvent.payload_model(), None);
    }

    #[test]
    fn default_payload_output_tokens_is_none() {
        use chrono::TimeZone;
        struct DefaultPayloadTokensEvent;
        impl Event for DefaultPayloadTokensEvent {
            fn id(&self) -> &'static str {
                "e"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> DateTime<Utc> {
                Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(DefaultPayloadTokensEvent.payload_output_tokens(), None);
    }

    #[test]
    fn default_payload_mode_is_none() {
        use chrono::TimeZone;
        struct DefaultPayloadModeEvent;
        impl Event for DefaultPayloadModeEvent {
            fn id(&self) -> &'static str {
                "e"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> DateTime<Utc> {
                Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(DefaultPayloadModeEvent.payload_mode(), None);
    }

    #[test]
    fn payload_tool_requests_default_returns_empty() {
        struct StubEvent;
        impl Event for StubEvent {
            fn id(&self) -> &'static str {
                "stub"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
                chrono::Utc::now()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(StubEvent.payload_tool_requests().len(), 0);
    }

    #[test]
    fn tool_call_id_default_returns_none() {
        struct StubEvent;
        impl Event for StubEvent {
            fn id(&self) -> &'static str {
                "stub"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
                chrono::Utc::now()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(StubEvent.tool_call_id(), None);
    }

    #[test]
    fn payload_model_metrics_default_returns_none() {
        struct StubEvent;
        impl Event for StubEvent {
            fn id(&self) -> &'static str {
                "stub"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
                chrono::Utc::now()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert!(StubEvent.payload_model_metrics().is_none());
    }
}
