//! Top-level container for all derived episodes from a single session.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::episode::{
    hook::HookEpisode,
    mode_segment::ModeSegment,
    skill::SkillEpisode,
    tool::ToolEpisode,
    turn::{AbortInfo, Turn},
    warning::DeriveWarning,
};

/// All episodes derived from a single `RawSession<E>`.
///
/// Constructed by `derive_episodes`. Snapshot-stable: `BTreeMap` ensures
/// deterministic key ordering across runs / platforms.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::Episodes;
/// let e = Episodes::default();
/// assert!(e.turns.is_empty());
/// assert!(e.warnings.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Episodes {
    /// All assistant turns in event order.
    pub turns: Vec<Turn>,
    /// Per-tool-name aggregation, keyed by tool name.
    pub tools: BTreeMap<String, ToolEpisode>,
    /// Per-hook-name aggregation, keyed by hook name.
    pub hooks: BTreeMap<String, HookEpisode>,
    /// Per-skill-name aggregation, keyed by skill name.
    pub skills: BTreeMap<String, SkillEpisode>,
    /// Mode-segment timeline, in event order.
    pub mode_segments: Vec<ModeSegment>,
    /// `abort` events that could not be attributed to an open Turn.
    pub aborts: Vec<AbortInfo>,
    /// Data-quality observations made while deriving.
    pub warnings: Vec<DeriveWarning>,
}

impl Episodes {
    /// Construct an empty `Episodes`. All collections start empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_default_is_empty() {
        let e = Episodes::new();
        assert!(e.turns.is_empty());
        assert!(e.tools.is_empty());
        assert!(e.hooks.is_empty());
        assert!(e.skills.is_empty());
        assert!(e.mode_segments.is_empty());
        assert!(e.aborts.is_empty());
        assert!(e.warnings.is_empty());
    }
}
