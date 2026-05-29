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
