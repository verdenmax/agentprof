//! `derive_episodes` — the pure aggregation function.
//!
//! See `docs/internals/adr-0004-episode-derivation.md` for the algorithm
//! rationale and `docs/superpowers/specs/2026-05-27-...-design.md` §7 for
//! the state-machine pseudocode.

use crate::adapter::Event;
use crate::episode::Episodes;
use crate::model::SessionMeta;

/// Derive episodes from a slice of events and session metadata.
///
/// This is a pure function: same input always produces same output. It
/// never panics, never does I/O, never consults the clock. Data quality
/// issues collect into `Episodes.warnings` rather than returning `Err`.
///
/// **Stub:** Task 10 wires the real state machine. Until then this returns
/// `Episodes::default()` so callers can compile against the final signature.
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::{AgentKind, Event, EventKind};
/// use agentprof_core::episode::{derive_episodes, Episodes};
/// use agentprof_core::model::SessionMeta;
/// use chrono::Utc;
///
/// // A tiny doctest Event type to satisfy the type bound.
/// struct StubEvent;
/// impl Event for StubEvent {
///     fn id(&self) -> &str { "stub" }
///     fn kind(&self) -> EventKind { EventKind::Unknown }
///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
///     fn parent_id(&self) -> Option<&str> { None }
/// }
///
/// let meta = SessionMeta::new(
///     "abc".into(),
///     AgentKind::Copilot,
///     Utc::now(),
///     false,
/// );
/// let events: Vec<StubEvent> = Vec::new();
/// let episodes: Episodes = derive_episodes(&events, &meta);
/// assert!(episodes.turns.is_empty());
/// ```
#[must_use]
pub fn derive_episodes<E: Event>(_events: &[E], _meta: &SessionMeta) -> Episodes {
    Episodes::new()
}
