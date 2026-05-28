//! `CopilotEvent` enum — 1:1 mapping to `events.jsonl` wire format.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Common per-event envelope fields wrapped around every payload variant.
///
/// The optional top-level `agent_id` is populated for events emitted by an
/// active subagent or tool call (notably `subagent.started`,
/// `subagent.completed`, and some `assistant.message` / `skill.invoked`
/// records). It is `None` for envelope-only events that omit the field on the
/// wire.
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
    /// Top-level subagent / tool-call identifier, when emitted by an active
    /// subagent. `None` for non-subagent events.
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Variant-specific payload.
    pub data: D,
}

/// One line from `events.jsonl`, discriminated by the wire-format `type` field.
///
/// Carries 21 named variants — session lifecycle (`session.*`), user/assistant
/// messaging (`user.message`, `assistant.message`, `assistant.turn_start`,
/// `assistant.turn_end`), system messages (`system.message`), tool
/// execution (`tool.execution_start`, `tool.execution_complete`,
/// `tool.user_requested`), subagent lifecycle (`subagent.started`,
/// `subagent.completed`, `subagent.failed`), hook lifecycle (`hook.start`,
/// `hook.end`), skill activation (`skill.invoked`), and user-initiated
/// cancellation (`abort`) — plus an [`CopilotEvent::Unknown`]
/// forward-compatibility fallback.
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
    /// User-authored message.
    #[serde(rename = "user.message")]
    UserMessage(WithEnvelope<UserMessageData>),
    /// Assistant turn opened.
    #[serde(rename = "assistant.turn_start")]
    TurnStart(WithEnvelope<TurnRefData>),
    /// Assistant-authored message.
    #[serde(rename = "assistant.message")]
    AssistantMessage(WithEnvelope<AssistantMessageData>),
    /// Assistant turn closed.
    #[serde(rename = "assistant.turn_end")]
    TurnEnd(WithEnvelope<TurnRefData>),
    /// System-emitted message.
    #[serde(rename = "system.message")]
    SystemMessage(WithEnvelope<SystemMessageData>),
    /// Tool execution begin.
    #[serde(rename = "tool.execution_start")]
    ToolExecStart(WithEnvelope<ToolExecData>),
    /// Tool execution completion (success or error).
    #[serde(rename = "tool.execution_complete")]
    ToolExecComplete(WithEnvelope<ToolResultData>),
    /// User-requested tool execution.
    #[serde(rename = "tool.user_requested")]
    ToolUserRequested(WithEnvelope<ToolUserRequestedData>),
    /// Hook invocation started.
    #[serde(rename = "hook.start")]
    HookStart(WithEnvelope<HookStartData>),
    /// Hook invocation ended (success or failure).
    #[serde(rename = "hook.end")]
    HookEnd(WithEnvelope<HookEndData>),
    /// Skill activated for this session.
    #[serde(rename = "skill.invoked")]
    SkillInvoked(WithEnvelope<SkillData>),
    /// Subagent (task delegated to a sub-LLM) invocation started.
    #[serde(rename = "subagent.started")]
    SubagentStarted(WithEnvelope<SubagentStartedData>),
    /// Subagent finished successfully.
    #[serde(rename = "subagent.completed")]
    SubagentCompleted(WithEnvelope<SubagentCompletedData>),
    /// Subagent failed (error returned before completion).
    #[serde(rename = "subagent.failed")]
    SubagentFailed(WithEnvelope<SubagentFailedData>),
    /// User-initiated session abort.
    #[serde(rename = "abort")]
    Abort(WithEnvelope<AbortData>),
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

// -- user / assistant / turn / system payloads --

/// Payload for `user.message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UserMessageData {
    /// Raw user content as typed.
    pub content: String,
    /// Content after Copilot CLI applies system-prompt-style transformations.
    #[serde(
        rename = "transformedContent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub transformed_content: Option<String>,
    /// Where the message came from (e.g. `"cli"`, `"sdk"`).
    pub source: String,
    /// File attachments accompanying the message.
    #[serde(default)]
    pub attachments: Vec<serde_json::Value>,
    /// Interaction ID linking this prompt to the subsequent assistant turn.
    #[serde(rename = "interactionId")]
    pub interaction_id: String,
}

/// Payload for `assistant.turn_start` and `assistant.turn_end`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TurnRefData {
    /// Turn identifier (typically a small integer encoded as string).
    #[serde(rename = "turnId")]
    pub turn_id: String,
    /// Interaction ID; absent on `turn_end`.
    #[serde(
        rename = "interactionId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub interaction_id: Option<String>,
}

/// One requested tool invocation embedded in an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolRequest {
    /// Stable ID for this tool call.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// Tool name (`mcp__` / `skill__` / builtin).
    pub name: String,
    /// Tool-specific argument object.
    pub arguments: serde_json::Value,
    /// Tool-call type (typically `"function"`).
    #[serde(rename = "type")]
    pub call_type: String,
    /// Optional short summary of intent.
    #[serde(
        rename = "intentionSummary",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub intention_summary: Option<String>,
}

/// Payload for `assistant.message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AssistantMessageData {
    /// Stable message ID.
    #[serde(rename = "messageId")]
    pub message_id: String,
    /// Model that produced this message.
    pub model: String,
    /// Assistant text content.
    pub content: String,
    /// Tool calls requested by this message.
    #[serde(rename = "toolRequests", default)]
    pub tool_requests: Vec<ToolRequest>,
    /// Interaction ID grouping prompt → response.
    #[serde(rename = "interactionId")]
    pub interaction_id: String,
    /// Turn ID this message belongs to.
    #[serde(rename = "turnId")]
    pub turn_id: String,
    /// GitHub-encrypted internal reasoning state (opaque blob).
    #[serde(
        rename = "reasoningOpaque",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_opaque: Option<String>,
    /// Plaintext reasoning trace (when emitted by the model).
    #[serde(
        rename = "reasoningText",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_text: Option<String>,
    /// GitHub-encrypted full content (opaque blob).
    #[serde(
        rename = "encryptedContent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub encrypted_content: Option<String>,
    /// Output token count reported by the model.
    #[serde(rename = "outputTokens")]
    pub output_tokens: u32,
    /// Optional upstream request ID.
    #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Optional service-side request ID.
    #[serde(
        rename = "serviceRequestId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub service_request_id: Option<String>,
}

/// Payload for `system.message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SystemMessageData {
    /// Role (typically `"system"`).
    pub role: String,
    /// System message content (e.g. system prompt injection).
    pub content: String,
}

// -- tool.* payloads --

/// Payload for `tool.execution_start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolExecData {
    /// Tool call ID to match against `tool.execution_complete`.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// Tool name (used by [`agentprof_core::model::ToolSource::infer`]).
    #[serde(rename = "toolName")]
    pub tool_name: String,
    /// Tool-specific argument object (shape varies; kept as Value).
    pub arguments: serde_json::Value,
}

/// Tool result returned alongside `tool.execution_complete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolResult {
    /// Short / summarized result content shown to the user.
    pub content: String,
    /// Verbose result content the model sees.
    #[serde(rename = "detailedContent")]
    pub detailed_content: String,
}

/// Telemetry record attached to a tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolTelemetry {
    /// Tool-author-defined string properties.
    #[serde(default)]
    pub properties: std::collections::BTreeMap<String, serde_json::Value>,
    /// Tool-author-defined numeric metrics (e.g. `resultLength`, `responseTokenLimit`).
    #[serde(default)]
    pub metrics: std::collections::BTreeMap<String, u64>,
    /// Sensitive properties scrubbed from analytics streams.
    #[serde(rename = "restrictedProperties", default)]
    pub restricted_properties: serde_json::Value,
}

/// Error sub-payload when a tool execution fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolError {
    /// Human-readable error message.
    pub message: String,
}

/// Payload for `tool.execution_complete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolResultData {
    /// Tool call ID matching the `tool.execution_start`.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// Model that requested this tool call.
    pub model: String,
    /// Interaction grouping (matches `AssistantMessage` / `ToolExecStart`).
    #[serde(rename = "interactionId")]
    pub interaction_id: String,
    /// Turn ID (absent on some user-requested calls).
    #[serde(rename = "turnId", default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Whether the tool succeeded.
    pub success: bool,
    /// Result payload.
    pub result: ToolResult,
    /// Telemetry counters/properties.
    #[serde(rename = "toolTelemetry")]
    pub tool_telemetry: ToolTelemetry,
    /// Error details when `success == false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ToolError>,
}

/// Arguments specific to user-requested tool invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolUserArgs {
    /// Shell-like command string the user authorized.
    pub command: String,
    /// User-supplied description.
    pub description: String,
}

/// Payload for `tool.user_requested`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolUserRequestedData {
    /// Tool call ID.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// Tool name.
    #[serde(rename = "toolName")]
    pub tool_name: String,
    /// User-supplied arguments.
    pub arguments: ToolUserArgs,
}

// -- hook.* payloads --

/// Hook input snapshot recorded when a hook is invoked.
///
/// Note: `timestamp` here is a Unix epoch millisecond count (integer), distinct
/// from the ISO-8601 [`WithEnvelope::timestamp`] on the enclosing envelope.
/// This matches Copilot CLI's wire format, where hook inputs carry their own
/// numeric timestamp for downstream tool consumption (see ADR-0002).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookInput {
    /// Session UUID at the time the hook fired.
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// Unix epoch milliseconds when the hook input was captured.
    pub timestamp: u64,
    /// Working directory at hook time.
    pub cwd: String,
    /// Origin label (e.g. `startup`, `tool_use`).
    pub source: String,
    /// The initial user prompt for the session, when present.
    #[serde(
        rename = "initialPrompt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub initial_prompt: Option<String>,
}

/// Payload for `hook.start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookStartData {
    /// Unique invocation ID for matching against the paired `hook.end`.
    #[serde(rename = "hookInvocationId")]
    pub hook_invocation_id: String,
    /// Hook kind (e.g. `SessionStart`, `PreToolUse`, `PostToolUse`).
    #[serde(rename = "hookType")]
    pub hook_type: String,
    /// Snapshot of input fed to the hook.
    pub input: HookInput,
}

/// Optional structured output emitted by a hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookOutput {
    /// Additional context the hook injected back into the session.
    #[serde(
        rename = "additionalContext",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_context: Option<String>,
}

/// Payload for `hook.end`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookEndData {
    /// Invocation ID matching the prior `hook.start`.
    #[serde(rename = "hookInvocationId")]
    pub hook_invocation_id: String,
    /// Hook kind (mirrors `HookStartData.hook_type`).
    #[serde(rename = "hookType")]
    pub hook_type: String,
    /// Hook output, when the hook produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<HookOutput>,
    /// Whether the hook completed successfully.
    pub success: bool,
}

// -- skill.* payloads --

/// Payload for `skill.invoked`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SkillData {
    /// Skill name (e.g. `using-superpowers`).
    pub name: String,
    /// Filesystem path to the skill definition.
    pub path: String,
    /// Skill body content loaded into context.
    pub content: String,
    /// Where the skill came from (e.g. `plugin`, `project`, `user`).
    pub source: String,
    /// Plugin name when the skill is plugin-sourced.
    #[serde(
        rename = "pluginName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub plugin_name: Option<String>,
    /// Plugin version when the skill is plugin-sourced.
    #[serde(
        rename = "pluginVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub plugin_version: Option<String>,
    /// One-line skill description (from frontmatter).
    pub description: String,
    /// Trigger that caused the skill to be invoked.
    pub trigger: String,
}

// -- abort payload --

/// Payload for `abort`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AbortData {
    /// Reason for the abort (e.g. `user_interrupt`).
    pub reason: String,
}

// -- subagent.* payloads --
//
// Note: `subagent.started` and `subagent.completed` carry `agentId` at the
// envelope level, not inside `data`. That is handled uniformly by
// [`WithEnvelope::agent_id`] (above) — these payload structs only model the
// fields that actually live inside `data`.

/// Payload for `subagent.started`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct SubagentStartedData {
    /// Long-form description of the subagent's capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_description: Option<String>,
    /// User-facing display name (e.g. `General Purpose Agent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_display_name: Option<String>,
    /// Stable agent identifier (e.g. `general-purpose`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Tool call id that triggered the subagent invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Payload for `subagent.completed`.
///
/// Metrics fields are emitted by newer CLI versions only and may be absent
/// in earlier samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct SubagentCompletedData {
    /// User-facing display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_display_name: Option<String>,
    /// Stable agent identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Wall-clock duration of the subagent run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Model executed inside the subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Originating tool call id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Total tokens consumed across the subagent run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    /// Total tool calls issued by the subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tool_calls: Option<u64>,
}

/// Payload for `subagent.failed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct SubagentFailedData {
    /// User-facing display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_display_name: Option<String>,
    /// Stable agent identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Wall-clock duration up to the failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Failure message (raw upstream error text).
    pub error: String,
    /// Originating tool call id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls issued before the failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tool_calls: Option<u64>,
}

// -- inherent accessors + Event trait impl --

impl CopilotEvent {
    /// Returns the stable per-event identifier from the wire-format `id` field.
    ///
    /// For the [`CopilotEvent::Unknown`] forward-compat sentinel, returns `""`
    /// (the envelope is not retained for unknown event types).
    #[must_use]
    #[allow(clippy::match_same_arms)] // tagged-union dispatch over heterogeneous WithEnvelope<T>
    pub fn id(&self) -> &str {
        match self {
            Self::SessionStart(env) => &env.id,
            Self::SessionInfo(env) => &env.id,
            Self::ModeChanged(env) => &env.id,
            Self::ModelChange(env) => &env.id,
            Self::PlanChanged(env) => &env.id,
            Self::Shutdown(env) => &env.id,
            Self::UserMessage(env) => &env.id,
            Self::TurnStart(env) => &env.id,
            Self::AssistantMessage(env) => &env.id,
            Self::TurnEnd(env) => &env.id,
            Self::SystemMessage(env) => &env.id,
            Self::ToolExecStart(env) => &env.id,
            Self::ToolExecComplete(env) => &env.id,
            Self::ToolUserRequested(env) => &env.id,
            Self::HookStart(env) => &env.id,
            Self::HookEnd(env) => &env.id,
            Self::SkillInvoked(env) => &env.id,
            Self::SubagentStarted(env) => &env.id,
            Self::SubagentCompleted(env) => &env.id,
            Self::SubagentFailed(env) => &env.id,
            Self::Abort(env) => &env.id,
            Self::Unknown => "",
        }
    }

    /// Returns the coarse [`agentprof_core::adapter::EventKind`] for this event,
    /// suitable for cheap pattern matching in analyzers.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_adapters::copilot::CopilotEvent;
    /// use agentprof_core::adapter::EventKind;
    ///
    /// let line = r#"{"type":"abort","data":{"reason":"user_interrupt"},"id":"e","timestamp":"2026-05-26T10:00:00Z","parentId":"p"}"#;
    /// let evt: CopilotEvent = serde_json::from_str(line).unwrap();
    /// assert_eq!(evt.kind(), EventKind::Abort);
    /// ```
    #[must_use]
    pub const fn kind(&self) -> agentprof_core::adapter::EventKind {
        use agentprof_core::adapter::EventKind as K;
        match self {
            Self::SessionStart(_) => K::SessionStart,
            Self::SessionInfo(_) => K::SessionInfo,
            Self::ModeChanged(_) => K::ModeChanged,
            Self::ModelChange(_) => K::ModelChange,
            Self::PlanChanged(_) => K::PlanChanged,
            Self::Shutdown(_) => K::Shutdown,
            Self::UserMessage(_) => K::UserMessage,
            Self::TurnStart(_) => K::TurnStart,
            Self::AssistantMessage(_) => K::AssistantMessage,
            Self::TurnEnd(_) => K::TurnEnd,
            Self::SystemMessage(_) => K::SystemMessage,
            Self::ToolExecStart(_) => K::ToolExecStart,
            Self::ToolExecComplete(_) => K::ToolExecComplete,
            Self::ToolUserRequested(_) => K::ToolUserRequested,
            Self::HookStart(_) => K::HookStart,
            Self::HookEnd(_) => K::HookEnd,
            Self::SkillInvoked(_) => K::SkillInvoked,
            Self::SubagentStarted(_) => K::SubagentStarted,
            Self::SubagentCompleted(_) => K::SubagentCompleted,
            Self::SubagentFailed(_) => K::SubagentFailed,
            Self::Abort(_) => K::Abort,
            Self::Unknown => K::Unknown,
        }
    }

    /// Returns the event timestamp normalized to UTC.
    ///
    /// For [`CopilotEvent::Unknown`], returns [`DateTime::<Utc>::UNIX_EPOCH`]
    /// as a sentinel (the envelope is not retained).
    #[must_use]
    #[allow(clippy::match_same_arms)] // tagged-union dispatch over heterogeneous WithEnvelope<T>
    pub const fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::SessionStart(env) => env.timestamp,
            Self::SessionInfo(env) => env.timestamp,
            Self::ModeChanged(env) => env.timestamp,
            Self::ModelChange(env) => env.timestamp,
            Self::PlanChanged(env) => env.timestamp,
            Self::Shutdown(env) => env.timestamp,
            Self::UserMessage(env) => env.timestamp,
            Self::TurnStart(env) => env.timestamp,
            Self::AssistantMessage(env) => env.timestamp,
            Self::TurnEnd(env) => env.timestamp,
            Self::SystemMessage(env) => env.timestamp,
            Self::ToolExecStart(env) => env.timestamp,
            Self::ToolExecComplete(env) => env.timestamp,
            Self::ToolUserRequested(env) => env.timestamp,
            Self::HookStart(env) => env.timestamp,
            Self::HookEnd(env) => env.timestamp,
            Self::SkillInvoked(env) => env.timestamp,
            Self::SubagentStarted(env) => env.timestamp,
            Self::SubagentCompleted(env) => env.timestamp,
            Self::SubagentFailed(env) => env.timestamp,
            Self::Abort(env) => env.timestamp,
            Self::Unknown => DateTime::<Utc>::UNIX_EPOCH,
        }
    }

    /// Returns the parent event ID, forming a DAG mirroring the trace tree.
    ///
    /// `None` at session start, for top-level events, or for the
    /// [`CopilotEvent::Unknown`] sentinel.
    #[must_use]
    #[allow(clippy::match_same_arms)] // tagged-union dispatch over heterogeneous WithEnvelope<T>
    pub fn parent_id(&self) -> Option<&str> {
        match self {
            Self::SessionStart(env) => env.parent_id.as_deref(),
            Self::SessionInfo(env) => env.parent_id.as_deref(),
            Self::ModeChanged(env) => env.parent_id.as_deref(),
            Self::ModelChange(env) => env.parent_id.as_deref(),
            Self::PlanChanged(env) => env.parent_id.as_deref(),
            Self::Shutdown(env) => env.parent_id.as_deref(),
            Self::UserMessage(env) => env.parent_id.as_deref(),
            Self::TurnStart(env) => env.parent_id.as_deref(),
            Self::AssistantMessage(env) => env.parent_id.as_deref(),
            Self::TurnEnd(env) => env.parent_id.as_deref(),
            Self::SystemMessage(env) => env.parent_id.as_deref(),
            Self::ToolExecStart(env) => env.parent_id.as_deref(),
            Self::ToolExecComplete(env) => env.parent_id.as_deref(),
            Self::ToolUserRequested(env) => env.parent_id.as_deref(),
            Self::HookStart(env) => env.parent_id.as_deref(),
            Self::HookEnd(env) => env.parent_id.as_deref(),
            Self::SkillInvoked(env) => env.parent_id.as_deref(),
            Self::SubagentStarted(env) => env.parent_id.as_deref(),
            Self::SubagentCompleted(env) => env.parent_id.as_deref(),
            Self::SubagentFailed(env) => env.parent_id.as_deref(),
            Self::Abort(env) => env.parent_id.as_deref(),
            Self::Unknown => None,
        }
    }
}

impl agentprof_core::adapter::Event for CopilotEvent {
    fn id(&self) -> &str {
        self.id()
    }
    fn kind(&self) -> agentprof_core::adapter::EventKind {
        self.kind()
    }
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp()
    }
    fn parent_id(&self) -> Option<&str> {
        self.parent_id()
    }
}
