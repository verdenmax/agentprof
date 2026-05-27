//! `CopilotEvent` enum — 1:1 mapping to `events.jsonl` wire format.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Common per-event envelope fields wrapped around every payload variant.
///
/// # Examples
///
/// ```
/// use agentprof_adapters::copilot::WithEnvelope;
///
/// #[derive(serde::Deserialize, Debug)]
/// struct Empty {}
///
/// let json = r#"{"id":"e1","timestamp":"2026-05-26T10:00:00Z","parentId":null,"data":{}}"#;
/// let env: WithEnvelope<Empty> = serde_json::from_str(json).unwrap();
/// assert_eq!(env.id, "e1");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WithEnvelope<D> {
    /// UUID identifying this event.
    pub id: String,
    /// ISO-8601 timestamp.
    pub timestamp: DateTime<Utc>,
    /// Parent event ID; `None` at session start.
    #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// `true` for transient events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    /// Variant-specific payload.
    pub data: D,
}

/// One line from `events.jsonl`, discriminated by the wire-format `type` field.
///
/// Subsequent tasks (5-9) add the named variants; this skeleton supports only
/// the [`CopilotEvent::Unknown`] forward-compatibility fallback.
///
/// # Examples
///
/// ```
/// use agentprof_adapters::copilot::CopilotEvent;
///
/// let json = r#"{"type":"some.future.event","data":{},"id":"e","timestamp":"2026-05-26T10:00:00Z","parentId":null}"#;
/// let evt: CopilotEvent = serde_json::from_str(json).unwrap();
/// assert!(matches!(evt, CopilotEvent::Unknown));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum CopilotEvent {
    /// Forward-compat fallback for unrecognized event types.
    #[serde(other)]
    Unknown,
}
