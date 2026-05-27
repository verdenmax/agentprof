//! Adapter registry — dispatch from [`AgentKind`] to a concrete adapter.
//!
//! Today only [`AgentKind::Copilot`] is wired up; Claude and Codex return
//! [`None`] until Phase 2/3 lands. Higher-level code (CLI, TUI) should
//! prefer this module over hard-coding adapter constructors.

use agentprof_core::adapter::AgentKind;

use crate::copilot::CopilotAdapter;

/// Return the adapter implementation for `kind`, or [`None`] if no adapter
/// has been wired up yet for that agent.
///
/// # Stability
///
/// The return type is intentionally `Option<CopilotAdapter>` for M1.2 while
/// Copilot is the only wired-up agent. Once a second adapter ships (Phase 2:
/// Claude), this signature **must** change — `CopilotAdapter` cannot
/// represent another adapter. The likely shape is either
/// `Option<AnyAdapter>` with `enum AnyAdapter { Copilot(_), Claude(_), … }`,
/// or trait-object erasure once the `Event` associated type is stabilized
/// behind a runtime-friendly facade. Track this decision in the next
/// adapter-layer ADR (planned for M1.3+); for now callers should expect to
/// adapt at that point.
///
/// # Examples
///
/// ```
/// use agentprof_adapters::registry::adapter_for;
/// use agentprof_core::adapter::AgentKind;
///
/// assert!(adapter_for(AgentKind::Copilot).is_some());
/// assert!(adapter_for(AgentKind::Claude).is_none());
/// ```
#[must_use]
pub const fn adapter_for(kind: AgentKind) -> Option<CopilotAdapter> {
    match kind {
        AgentKind::Copilot => Some(CopilotAdapter),
        // `AgentKind` is `#[non_exhaustive]`; Claude / Codex / future variants
        // all fall through to `None` until their adapters are wired up.
        // TODO(phase-2): once a second adapter ships, replace this signature
        // — see `# Stability` above.
        _ => None,
    }
}

/// List of [`AgentKind`] values for which an adapter is currently available.
///
/// # Examples
///
/// ```
/// use agentprof_adapters::registry::supported_agents;
/// use agentprof_core::adapter::AgentKind;
///
/// assert_eq!(supported_agents(), &[AgentKind::Copilot]);
/// ```
#[must_use]
pub const fn supported_agents() -> &'static [AgentKind] {
    &[AgentKind::Copilot]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_for_copilot_is_some() {
        assert!(adapter_for(AgentKind::Copilot).is_some());
    }

    #[test]
    fn adapter_for_claude_and_codex_are_none() {
        assert!(adapter_for(AgentKind::Claude).is_none());
        assert!(adapter_for(AgentKind::Codex).is_none());
        assert_eq!(supported_agents(), &[AgentKind::Copilot]);
    }
}
