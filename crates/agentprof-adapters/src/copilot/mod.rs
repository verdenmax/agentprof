//! GitHub Copilot CLI adapter.
//!
//! Reads session telemetry from `~/.copilot/session-state/<uuid>/events.jsonl`
//! into [`CopilotEvent`] values.
//!
//! See `docs/internals/adr-0002-copilot-event-schema.md` for the wire format
//! reference.

mod event;
pub mod parser;
// `paths`, `adapter` added in later tasks.

pub use event::{
    AbortData, AssistantMessageData, CodeChanges, CopilotEvent, HookEndData, HookInput, HookOutput,
    HookStartData, ModeChangeData, ModelChangeData, PlanChangeData, SessionContext,
    SessionInfoData, SessionStartData, ShutdownData, SkillData, SystemMessageData, ToolError,
    ToolExecData, ToolRequest, ToolResult, ToolResultData, ToolTelemetry, ToolUserArgs,
    ToolUserRequestedData, TurnRefData, UserMessageData, WithEnvelope,
};
