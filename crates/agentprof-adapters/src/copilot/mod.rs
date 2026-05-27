//! GitHub Copilot CLI adapter.
//!
//! Reads session telemetry from `~/.copilot/session-state/<uuid>/events.jsonl`
//! into [`CopilotEvent`] values.
//!
//! See `docs/internals/adr-0002-copilot-event-schema.md` for the wire format
//! reference.

mod event;
// `parser`, `paths`, `adapter` added in later tasks.

pub use event::{CopilotEvent, WithEnvelope};
