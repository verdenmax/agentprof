//! Data-quality warnings emitted by `derive_episodes`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::adapter::EventKind;

/// Non-fatal data-quality observation from `derive_episodes`.
///
/// See `docs/internals/adr-0004-episode-derivation.md` for taxonomy rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeriveWarning {
    /// Saw an `End`-shaped event without a matching open `Start`; the algorithm
    /// synthesized a zero-duration Start at the End's timestamp.
    SynthesizedStart {
        /// Kind of the End-shaped event that triggered synthesis.
        kind: EventKind,
        /// Id of the End event.
        end_event_id: String,
    },
    /// Saw a `Start`-shaped event with no matching `End` by end of events;
    /// algorithm clamped `span.ended_at` to the last event timestamp.
    OpenAtEndOfSession {
        /// Kind of the Start-shaped event left open.
        kind: EventKind,
        /// Id of the Start event.
        start_event_id: String,
    },
    /// `abort` event arrived but no Turn / `ToolCall` / `HookCall` was open;
    /// pushed to `Episodes.aborts` without attribution.
    AbortWithoutOpenElement {
        /// Reason carried by the `abort` event.
        reason: String,
        /// Timestamp of the `abort` event.
        at: DateTime<Utc>,
    },
    /// `ev.timestamp() < prev_ts`. Algorithm did not reorder; consumers may want to.
    NonMonotonicTimestamp {
        /// Id of the offending event.
        event_id: String,
        /// Previously-observed timestamp.
        prev_at: DateTime<Utc>,
        /// Timestamp of this event (lesser than `prev_at`).
        this_at: DateTime<Utc>,
    },
    /// Adapter's [`Event::payload_name`](crate::adapter::Event::payload_name)
    /// returned `None` for an event whose `EventKind` indicates it SHOULD
    /// carry a payload-defined name (`ToolExecStart`, `HookStart`,
    /// `SkillInvoked`). `derive_episodes` falls back to the event id, which
    /// works for snapshot stability but produces per-event UUIDs as
    /// [`ToolEpisode`](crate::episode::ToolEpisode) /
    /// [`HookEpisode`](crate::episode::HookEpisode) /
    /// [`SkillEpisode`](crate::episode::SkillEpisode) keys — defeating the
    /// purpose of the per-name aggregation.
    ///
    /// This warning is the signal that an adapter author forgot to override
    /// `Event::payload_name` for a relevant variant (a real risk for
    /// upcoming Claude / Codex adapters in Phase 2 / 3). It is **not** an
    /// indictment of the data: when `CopilotEvent` is fully implemented,
    /// this warning never fires on real Copilot sessions. The
    /// orphan-complete case for `ToolExecComplete` is handled separately by
    /// the [`ORPHAN_TOOL_SENTINEL`](crate::episode::ORPHAN_TOOL_SENTINEL)
    /// aggregation and does NOT emit this warning.
    PayloadNameMissing {
        /// Kind of the event whose `payload_name()` returned `None`.
        kind: EventKind,
        /// Id of the offending event (for adapter debugging).
        event_id: String,
    },
}

impl std::fmt::Display for DeriveWarning {
    /// Human-readable rendering for report surfaces (markdown / HTML).
    ///
    /// One-line description per variant; reports embed this directly
    /// rather than the enum's `Debug` representation so rendered text is
    /// stable across future variant refactors.
    ///
    /// Sub-fields of type [`EventKind`] are rendered via their `Debug`
    /// repr (e.g. `"ToolExecStart"`). `EventKind` is a `#[non_exhaustive]`
    /// unit-variant enum, so this naming is stable across additions.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::EventKind;
    /// use agentprof_core::episode::DeriveWarning;
    /// let w = DeriveWarning::SynthesizedStart {
    ///     kind: EventKind::ToolExecComplete,
    ///     end_event_id: "evt-1".into(),
    /// };
    /// assert!(w.to_string().contains("evt-1"));
    /// assert!(w.to_string().contains("ToolExecComplete"));
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SynthesizedStart { kind, end_event_id } => write!(
                f,
                "synthesized zero-duration start for {kind:?} (end event {end_event_id})"
            ),
            Self::OpenAtEndOfSession {
                kind,
                start_event_id,
            } => write!(
                f,
                "{kind:?} (start event {start_event_id}) was still open at end of session; \
                 clamped to last event timestamp"
            ),
            Self::AbortWithoutOpenElement { reason, at } => write!(
                f,
                "abort at {at} with no open turn/tool/hook (reason: {reason})"
            ),
            Self::NonMonotonicTimestamp {
                event_id,
                prev_at,
                this_at,
            } => write!(
                f,
                "non-monotonic timestamp at event {event_id}: previous {prev_at}, this {this_at}"
            ),
            Self::PayloadNameMissing { kind, event_id } => write!(
                f,
                "{kind:?} event {event_id} has no payload_name; \
                 episode aggregation will fall back to event id"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_human_readable_and_avoids_struct_debug_syntax() {
        let w = DeriveWarning::SynthesizedStart {
            kind: EventKind::ToolExecComplete,
            end_event_id: "evt-1".into(),
        };
        let s = w.to_string();
        assert!(s.contains("evt-1"));
        assert!(s.contains("ToolExecComplete"));
        // Should not look like the derived `Debug` struct repr.
        assert!(!s.contains("end_event_id:"));
        assert!(!s.contains(" { "));
    }
}
