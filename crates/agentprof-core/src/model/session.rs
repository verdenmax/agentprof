//! Parser output: typed session data.

use serde::{Deserialize, Serialize};

use crate::error::ParseWarning;
use crate::model::meta::SessionMeta;

/// One parsed session: metadata + event stream + parse warnings.
///
/// `E` is the adapter's native event type. Analyzers in
/// `agentprof-core::episode` (M1.3) consume `&[E]` slices generic over
/// [`crate::adapter::Event`].
///
/// # Invariants
///
/// - `events` is in source-file order; parser never reorders.
/// - Broken/blank lines do NOT appear in `events`; broken lines accumulate
///   in `parse_warnings`.
/// - `parse_warnings.is_empty()` is the happy path; nonzero warnings don't
///   invalidate `events`.
///
/// # Examples
///
/// `RawSession` and `SessionMeta` are `#[non_exhaustive]`, so external code
/// constructs them via deserialization (the parser does the same, just from
/// adapter-native event types):
///
/// ```
/// use agentprof_core::model::session::RawSession;
/// use agentprof_core::adapter::{Event, EventKind};
/// use chrono::{DateTime, Utc, TimeZone};
///
/// #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
/// struct DocEvent;
/// impl Event for DocEvent {
///     fn id(&self) -> &str { "doc" }
///     fn kind(&self) -> EventKind { EventKind::Unknown }
///     fn timestamp(&self) -> DateTime<Utc> {
///         Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
///     }
///     fn parent_id(&self) -> Option<&str> { None }
/// }
///
/// let s: RawSession<DocEvent> = serde_json::from_str(r#"{
///     "meta": {
///         "id": "x",
///         "agent": "copilot",
///         "started_at": "2026-01-01T00:00:00Z",
///         "is_live": false
///     },
///     "events": [null],
///     "parse_warnings": []
/// }"#).unwrap();
/// assert_eq!(s.events.len(), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RawSession<E> {
    /// Session-level metadata.
    pub meta: SessionMeta,
    /// Events in file order.
    pub events: Vec<E>,
    /// Warnings about unparseable lines; never blocks output.
    #[serde(default)]
    pub parse_warnings: Vec<ParseWarning>,
}

impl<E> RawSession<E> {
    /// Construct a `RawSession` directly from parts.
    ///
    /// Provided so adapter crates (which cannot use a struct literal due to
    /// `#[non_exhaustive]`) can assemble parsed output. Adapters are
    /// expected to populate `parse_warnings` for any single-line failures
    /// rather than returning an `Err`.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{AgentKind, Event, EventKind};
    /// use agentprof_core::model::{RawSession, SessionMeta};
    /// use chrono::{DateTime, TimeZone, Utc};
    ///
    /// #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    /// struct E;
    /// impl Event for E {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> DateTime<Utc> {
    ///         Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
    ///     }
    ///     fn parent_id(&self) -> Option<&str> { None }
    /// }
    ///
    /// let meta = SessionMeta::new(
    ///     "id".into(),
    ///     AgentKind::Copilot,
    ///     Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    ///     false,
    /// );
    /// let s: RawSession<E> = RawSession::new(meta, vec![], vec![]);
    /// assert_eq!(s.events.len(), 0);
    /// ```
    #[must_use]
    pub const fn new(meta: SessionMeta, events: Vec<E>, parse_warnings: Vec<ParseWarning>) -> Self {
        Self {
            meta,
            events,
            parse_warnings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{AgentKind, Event, EventKind};
    use chrono::{DateTime, TimeZone, Utc};

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct StubEvent;
    impl Event for StubEvent {
        #[allow(clippy::unnecessary_literal_bound)] // trait-fixed return type
        fn id(&self) -> &str {
            "stub"
        }
        fn kind(&self) -> EventKind {
            EventKind::Unknown
        }
        fn timestamp(&self) -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2026, 5, 26, 0, 0, 0).unwrap()
        }
        fn parent_id(&self) -> Option<&str> {
            None
        }
    }

    #[test]
    fn raw_session_serializes_round_trip() {
        let session: RawSession<StubEvent> = RawSession {
            meta: crate::model::meta::SessionMeta {
                id: "s1".into(),
                agent: AgentKind::Copilot,
                producer: None,
                agent_version: None,
                started_at: Utc.with_ymd_and_hms(2026, 5, 26, 0, 0, 0).unwrap(),
                cwd: None,
                repository: None,
                branch: None,
                is_live: false,
            },
            events: vec![StubEvent],
            parse_warnings: vec![],
        };
        let json = serde_json::to_string(&session).unwrap();
        let back: RawSession<StubEvent> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.events.len(), 1);
        assert_eq!(back.parse_warnings.len(), 0);
    }
}
