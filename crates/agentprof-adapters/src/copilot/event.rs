//! `CopilotEvent` enum — 1:1 mapping to `events.jsonl` wire format.

use std::collections::BTreeMap;

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
/// Carries session-lifecycle variants plus an [`CopilotEvent::Unknown`]
/// forward-compatibility fallback. Subsequent tasks add user/assistant/tool
/// variants.
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
    /// Session lifecycle (start of recording).
    #[serde(rename = "session.start")]
    SessionStart(WithEnvelope<SessionStartData>),
    /// Session info/status message.
    #[serde(rename = "session.info")]
    SessionInfo(WithEnvelope<SessionInfoData>),
    /// Mode transition (interactive/plan/autopilot).
    #[serde(rename = "session.mode_changed")]
    ModeChanged(WithEnvelope<ModeChangeData>),
    /// Model switched mid-session.
    #[serde(rename = "session.model_change")]
    ModelChange(WithEnvelope<ModelChangeData>),
    /// Plan document modified.
    #[serde(rename = "session.plan_changed")]
    PlanChanged(WithEnvelope<PlanChangeData>),
    /// Session terminated; carries shutdown summary.
    #[serde(rename = "session.shutdown")]
    Shutdown(WithEnvelope<ShutdownData>),
    /// Forward-compat fallback for unrecognized event types.
    #[serde(other)]
    Unknown,
}

// -- session.* payloads --

/// Payload for `session.start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionStartData {
    /// Stable session UUID.
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// Wire-format version.
    pub version: u32,
    /// Producer identifier (e.g. `copilot-agent`).
    pub producer: String,
    /// Copilot CLI version (e.g. `1.0.54`).
    #[serde(rename = "copilotVersion")]
    pub copilot_version: String,
    /// When the session started.
    #[serde(rename = "startTime")]
    pub start_time: DateTime<Utc>,
    /// Workspace context at session start.
    pub context: SessionContext,
    /// Whether the session ID was already in use when this start was emitted.
    #[serde(rename = "alreadyInUse")]
    pub already_in_use: bool,
}

/// Workspace context recorded at session start.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionContext {
    /// Working directory.
    pub cwd: String,
    /// Git repository root, if inside a repo.
    #[serde(rename = "gitRoot", default, skip_serializing_if = "Option::is_none")]
    pub git_root: Option<String>,
    /// Active git branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// HEAD commit SHA.
    #[serde(
        rename = "headCommit",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub head_commit: Option<String>,
    /// Repository identifier (e.g. `owner/repo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Host type (e.g. `github`).
    #[serde(rename = "hostType", default, skip_serializing_if = "Option::is_none")]
    pub host_type: Option<String>,
}

/// Payload for `session.info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionInfoData {
    /// Information kind (e.g. `folder_trust`).
    #[serde(rename = "infoType")]
    pub info_type: String,
    /// Human-readable message.
    pub message: String,
}

/// Payload for `session.mode_changed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModeChangeData {
    /// Previous mode value.
    #[serde(rename = "previousMode")]
    pub previous_mode: String,
    /// New mode value.
    #[serde(rename = "newMode")]
    pub new_mode: String,
}

/// Payload for `session.model_change`.
///
/// Note: only `newModel` is present in observed data; derive previous from
/// prior `ModelChange` / `AssistantMessage.model` if needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModelChangeData {
    /// New model identifier.
    #[serde(rename = "newModel")]
    pub new_model: String,
}

/// Payload for `session.plan_changed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PlanChangeData {
    /// Operation kind (e.g. `update`).
    pub operation: String,
}

/// Payload for `session.shutdown`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ShutdownData {
    /// Shutdown kind (e.g. `normal`).
    #[serde(rename = "shutdownType")]
    pub shutdown_type: String,
    /// Number of premium model requests.
    #[serde(rename = "totalPremiumRequests", default)]
    pub total_premium_requests: u32,
    /// Total time spent in API calls.
    #[serde(rename = "totalApiDurationMs", default)]
    pub total_api_duration_ms: u64,
    /// Unix epoch ms when the session started (mirrors `SessionStart.startTime`).
    #[serde(rename = "sessionStartTime", default)]
    pub session_start_time: u64,
    /// Aggregate code-change stats.
    #[serde(rename = "codeChanges")]
    pub code_changes: CodeChanges,
    /// Per-model usage rollup; shape is model-specific so stays as `Value`.
    #[serde(rename = "modelMetrics", default)]
    pub model_metrics: BTreeMap<String, serde_json::Value>,
    /// The model active at shutdown.
    #[serde(rename = "currentModel", default)]
    pub current_model: String,
}

/// Lines added/removed/files-touched aggregate from `session.shutdown`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CodeChanges {
    /// Lines added across all file edits.
    #[serde(rename = "linesAdded", default)]
    pub lines_added: u32,
    /// Lines removed across all file edits.
    #[serde(rename = "linesRemoved", default)]
    pub lines_removed: u32,
    /// Paths of files touched.
    #[serde(rename = "filesModified", default)]
    pub files_modified: Vec<String>,
}
