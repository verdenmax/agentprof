//! Top-level container for all derived episodes from a single session.

use std::collections::{BTreeMap, BTreeSet};

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
    #[serde(default)]
    pub turns: Vec<Turn>,
    /// Per-tool-name aggregation, keyed by tool name.
    #[serde(default)]
    pub tools: BTreeMap<String, ToolEpisode>,
    /// Per-hook-name aggregation, keyed by hook name.
    #[serde(default)]
    pub hooks: BTreeMap<String, HookEpisode>,
    /// Per-skill-name aggregation, keyed by skill name.
    #[serde(default)]
    pub skills: BTreeMap<String, SkillEpisode>,
    /// Mode-segment timeline, in event order.
    #[serde(default)]
    pub mode_segments: Vec<ModeSegment>,
    /// `abort` events that could not be attributed to an open Turn.
    #[serde(default)]
    pub aborts: Vec<AbortInfo>,
    /// Data-quality observations made while deriving.
    #[serde(default)]
    pub warnings: Vec<DeriveWarning>,
    /// Per-model token-usage rollup, populated from
    /// [`crate::adapter::Event::payload_model_metrics`] during the
    /// [`crate::episode::derive_episodes`] walk. `None` when no
    /// event provided the data (e.g. session without
    /// `EventKind::Shutdown`).
    ///
    /// Map key is the model identifier as reported by the adapter.
    /// Cloned into `AnalysisReport::model_metrics` by `analyze()`
    /// (Task 6 of F1.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_metrics: Option<BTreeMap<String, crate::analyzer::ModelUsage>>,
    /// Set of MCP tool names ever loaded into this session's tool catalog,
    /// accumulated from per-event [`crate::adapter::Event::payload_loaded_mcp_tools`]
    /// during the [`crate::episode::derive_episodes`] walk.
    ///
    /// "Ever loaded" semantics per ADR-0015 D-2: once a tool name has
    /// been announced, it stays in this set for the remainder of the
    /// session even if a later "Tools no longer available:" notice
    /// removes it. Cloned into
    /// [`crate::analyzer::AnalysisReport::loaded_mcp_tools`] by
    /// `analyze()`. Empty when no event reported tool loads (e.g.
    /// non-Copilot adapters that don't expose tool-loading events, or
    /// sessions that never installed any MCP server).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub loaded_mcp_tools: BTreeSet<String>,
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
        assert!(e.model_metrics.is_none(), "model_metrics defaults to None");
        assert!(
            e.loaded_mcp_tools.is_empty(),
            "loaded_mcp_tools defaults to empty"
        );
    }
}
