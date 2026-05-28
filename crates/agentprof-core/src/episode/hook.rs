//! Hook episode aggregation.

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::episode::turn::Span;

/// Per-hook-name aggregation across all calls in a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookEpisode {
    /// Hook name (the key under which this episode is stored).
    pub name: String,
    /// All invocations of this hook, in event order.
    pub calls: Vec<HookCall>,
    /// Sum of `call.span.duration()` across `calls`.
    pub total_duration: Duration,
    /// Number of calls whose `success` is false.
    pub failure_count: u32,
}

impl HookEpisode {
    /// Construct an empty `HookEpisode` for the given hook name.
    #[must_use]
    pub const fn new(name: String) -> Self {
        Self {
            name,
            calls: Vec::new(),
            total_duration: Duration::zero(),
            failure_count: 0,
        }
    }
}

/// One invocation of a hook.
///
/// `synthesized_start = true` when `hook.end` arrived without a matching
/// `hook.start`. See ADR-0004 D-2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HookCall {
    /// Time interval covering the call (start → end).
    pub span: Span,
    /// Owning turn id, when the call was attributable to an open turn.
    pub turn_id: Option<String>,
    /// Whether the hook completed successfully.
    pub success: bool,
    /// `true` if the start was synthesized from an orphan end.
    pub synthesized_start: bool,
}

impl HookCall {
    /// Construct a successful `HookCall` with the given span (not synthesized).
    #[must_use]
    pub const fn new(span: Span) -> Self {
        Self {
            span,
            turn_id: None,
            success: true,
            synthesized_start: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn episode_new_starts_empty() {
        let ep = HookEpisode::new("pre-tool".into());
        assert!(ep.calls.is_empty());
        assert_eq!(ep.failure_count, 0);
    }

    #[test]
    fn hook_call_new_defaults_to_success_not_synthesized() {
        let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
        let c = HookCall::new(Span::instant(t));
        assert!(c.success);
        assert!(!c.synthesized_start);
    }
}
