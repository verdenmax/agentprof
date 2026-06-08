//! GitHub Copilot CLI adapter.
//!
//! Reads session telemetry from `~/.copilot/session-state/<uuid>/events.jsonl`
//! into [`CopilotEvent`] values.
//!
//! See `docs/internals/adr-0002-copilot-event-schema.md` for the wire format
//! reference.

pub mod adapter;
mod event;
pub mod mcp_config;
pub mod parser;
pub mod paths;
pub mod tools_changed;

pub use adapter::CopilotAdapter;
pub use event::{
    AbortData, AssistantMessageData, CodeChanges, CompactionTokensUsed, CopilotEvent, HookEndData,
    HookInput, HookOutput, HookStartData, ModeChangeData, ModelChangeData, PermissionCompletedData,
    PermissionRequestedData, PermissionResult, PlanChangeData, SessionCompactionCompleteData,
    SessionCompactionStartData, SessionContext, SessionInfoData, SessionResumeData,
    SessionStartData, SessionWarningData, ShutdownData, SkillData, SubagentCompletedData,
    SubagentFailedData, SubagentStartedData, SystemMessageData, SystemNotificationData, ToolError,
    ToolExecData, ToolRequest, ToolResult, ToolResultData, ToolTelemetry, ToolUserArgs,
    ToolUserRequestedData, TurnRefData, UserMessageData, WithEnvelope,
};
pub use mcp_config::{load_mcp_config, ParsedMcpConfig, ServerInfo};
pub use tools_changed::extract_loaded_set_from_session;
