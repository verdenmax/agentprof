//! Skill episode aggregation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Per-skill-name aggregation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SkillEpisode {
    /// Skill name (the key under which this episode is stored).
    pub name: String,
    /// All invocations of this skill, in event order.
    pub invocations: Vec<SkillInvocation>,
    /// Sum across `invocations` of how many tool calls landed in each
    /// invocation's triggered-tools window.
    pub subsequent_tool_calls: u32,
}

impl SkillEpisode {
    /// Construct an empty `SkillEpisode` for the given skill name.
    #[must_use]
    pub const fn new(name: String) -> Self {
        Self {
            name,
            invocations: Vec::new(),
            subsequent_tool_calls: 0,
        }
    }
}

/// One `skill.invoked` event with its triggered-tools window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SkillInvocation {
    /// Timestamp of the `skill.invoked` event.
    pub at: DateTime<Utc>,
    /// Owning turn id, when attributable to an open turn.
    pub turn_id: Option<String>,
    /// Indices into the relevant `ToolEpisode.calls` vector for tool invocations
    /// that occurred within the trailing K-event window after this skill invocation.
    pub triggered_tools: Vec<usize>,
}

impl SkillInvocation {
    /// Construct with an empty triggered-tools list.
    #[must_use]
    pub const fn new(at: DateTime<Utc>) -> Self {
        Self {
            at,
            turn_id: None,
            triggered_tools: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn skill_episode_new_starts_empty() {
        let ep = SkillEpisode::new("brainstorming".into());
        assert!(ep.invocations.is_empty());
        assert_eq!(ep.subsequent_tool_calls, 0);
    }

    #[test]
    fn skill_invocation_new_empty_window() {
        let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
        let inv = SkillInvocation::new(t);
        assert!(inv.triggered_tools.is_empty());
    }
}
