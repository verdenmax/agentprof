//! Per-session metadata extracted from session-lifecycle events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::adapter::AgentKind;

/// Normalized per-session metadata.
///
/// # Examples
///
/// `SessionMeta` is `#[non_exhaustive]`, so external code constructs it by
/// deserializing rather than via a struct literal:
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::model::SessionMeta;
///
/// let meta: SessionMeta = serde_json::from_str(r#"{
///     "id": "abc-123",
///     "agent": "copilot",
///     "started_at": "2026-05-26T10:00:00Z",
///     "is_live": false
/// }"#).unwrap();
/// assert_eq!(meta.agent, AgentKind::Copilot);
/// assert_eq!(meta.id, "abc-123");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionMeta {
    /// Stable session ID (typically a UUID; for Copilot, the directory name).
    pub id: String,
    /// Which agent produced this session.
    pub agent: AgentKind,
    /// `producer` field from `session.start`; may be absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    /// Agent CLI version reported at start; may be absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
    /// When the session began.
    pub started_at: DateTime<Utc>,
    /// Working directory at session start; may be absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Git repository identifier; may be absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Git branch at session start; may be absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// `true` when an `inuse.*.lock` indicates the session is still live.
    pub is_live: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn meta_round_trip_serde() {
        let meta = SessionMeta {
            id: "abc-123".to_owned(),
            agent: crate::adapter::AgentKind::Copilot,
            producer: Some("copilot-agent".to_owned()),
            agent_version: Some("1.0.54".to_owned()),
            started_at: chrono::Utc.with_ymd_and_hms(2026, 5, 26, 10, 0, 0).unwrap(),
            cwd: Some("/tmp/proj".to_owned()),
            repository: Some("owner/repo".to_owned()),
            branch: Some("main".to_owned()),
            is_live: false,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: SessionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, meta.id);
        assert_eq!(back.agent, meta.agent);
    }
}
