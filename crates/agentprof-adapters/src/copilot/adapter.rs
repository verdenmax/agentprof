//! [`CopilotAdapter`] — the [`Adapter`] implementation for GitHub Copilot CLI.

use std::path::{Path, PathBuf};

use agentprof_core::adapter::{Adapter, AdapterError, AgentKind, SessionRef};
use agentprof_core::model::session::RawSession;

use crate::copilot::event::CopilotEvent;
use crate::copilot::{parser, paths};

/// Zero-sized adapter that reads Copilot CLI session logs.
///
/// Construct via [`CopilotAdapter::default`] (or the unit struct literal
/// `CopilotAdapter`) and pass to any code that takes `&dyn Adapter` or a
/// generic `A: Adapter`.
///
/// # Examples
///
/// ```
/// use agentprof_adapters::copilot::CopilotAdapter;
/// use agentprof_core::adapter::{Adapter, AgentKind};
///
/// let adapter = CopilotAdapter;
/// assert_eq!(adapter.agent_kind(), AgentKind::Copilot);
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct CopilotAdapter;

impl Adapter for CopilotAdapter {
    type Event = CopilotEvent;

    fn agent_kind(&self) -> AgentKind {
        AgentKind::Copilot
    }

    fn default_session_root(&self) -> Option<PathBuf> {
        paths::default_session_root()
    }

    fn discover_sessions(&self, root: &Path) -> Result<Vec<SessionRef>, AdapterError> {
        paths::discover_sessions(root)
    }

    fn load_session(&self, sref: &SessionRef) -> Result<RawSession<Self::Event>, AdapterError> {
        parser::parse_events_jsonl(&sref.path, sref.is_live)
    }
}
