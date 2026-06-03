//! Turn aggregation types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::episode::call_ref::CallRef;
use crate::episode::mode_segment::Mode;

/// One assistant turn — the user-input → assistant-response cycle.
///
/// Bounded by the `assistant.turn_start` / `assistant.turn_end` pair when
/// present. Cross-references into `Episodes.tools[name].calls[idx]` and
/// `Episodes.hooks[name].calls[idx]` via the `tool_calls` / `hook_calls`
/// index vectors.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::{Turn, TurnStatus};
/// use chrono::{TimeZone, Utc};
/// let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
/// let turn = Turn::new("turn-1".into(), t);
/// assert_eq!(turn.id, "turn-1");
/// assert_eq!(turn.status, TurnStatus::Open);
/// assert!(turn.tool_calls.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Turn {
    /// Stable turn identifier (typically the `assistant.turn_start` event id).
    pub id: String,
    /// Timestamp of the `assistant.turn_start` event opening this turn.
    pub started_at: DateTime<Utc>,
    /// Timestamp of the matching `assistant.turn_end`; `None` while open.
    pub ended_at: Option<DateTime<Utc>>,
    /// Model identifier active at turn start, if reported by the adapter.
    pub model: Option<String>,
    /// Mode active at turn start, if known.
    pub mode: Option<Mode>,
    /// Output tokens reported on `turn_end`, when available.
    pub output_tokens: Option<u32>,
    /// Terminal status of this turn (open / completed / aborted).
    pub status: TurnStatus,
    /// Name-qualified back-references into `Episodes.tools[name].calls` for tool calls in this turn.
    pub tool_calls: Vec<CallRef>,
    /// Name-qualified back-references into `Episodes.hooks[name].calls` for hook calls in this turn.
    pub hook_calls: Vec<CallRef>,
    /// Name-qualified back-references into `Episodes.skills[name].invocations` for skills in this turn.
    pub skill_calls: Vec<CallRef>,
}

impl Turn {
    /// Create an `Open` Turn at the given start time.
    #[must_use]
    pub const fn new(id: String, started_at: DateTime<Utc>) -> Self {
        Self {
            id,
            started_at,
            ended_at: None,
            model: None,
            mode: None,
            output_tokens: None,
            status: TurnStatus::Open,
            tool_calls: Vec::new(),
            hook_calls: Vec::new(),
            skill_calls: Vec::new(),
        }
    }

    /// Returns `true` if any tool call in this turn is a user-blocking tool
    /// (i.e. its name appears in
    /// [`crate::analyzer::tool_rank::USER_BLOCKING_TOOLS`]).
    ///
    /// User-blocking tools (e.g. `ask_user`) wait on human input; their
    /// wall-time is dominated by user thinking, not by agent or machine
    /// work. Renderers that scale visual elements by turn duration should
    /// exclude these turns from "max duration" calculations to avoid
    /// squashing the visualization (see ADR-0005 §6 for the same split
    /// applied to the Tool Rank table; this method extends the same fix
    /// to the `FlamegraphView` as an M1.6.4 follow-up).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::episode::{CallRef, Turn};
    /// use chrono::{TimeZone, Utc};
    ///
    /// let t0 = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
    /// let mut turn = Turn::new("t1".into(), t0);
    /// assert!(!turn.is_user_blocking());
    ///
    /// turn.tool_calls.push(CallRef::new("ask_user".into(), 0));
    /// assert!(turn.is_user_blocking());
    /// ```
    #[must_use]
    pub fn is_user_blocking(&self) -> bool {
        self.tool_calls
            .iter()
            .any(|c| crate::analyzer::tool_rank::USER_BLOCKING_TOOLS.contains(&c.name.as_str()))
    }
}

/// Terminal status of a `Turn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TurnStatus {
    /// Saw `turn_start`; haven't seen `turn_end` yet (live session or truncated).
    Open,
    /// Saw matching `turn_end`.
    Completed,
    /// Saw an `abort` event while this turn was open.
    Aborted(AbortInfo),
}

/// Detail captured from an `abort` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AbortInfo {
    /// Reason string carried by the `abort` event (e.g. `"user_cancel"`).
    pub reason: String,
    /// Timestamp of the `abort` event.
    pub at: DateTime<Utc>,
}

impl AbortInfo {
    /// Construct from reason and timestamp.
    #[must_use]
    pub const fn new(reason: String, at: DateTime<Utc>) -> Self {
        Self { reason, at }
    }
}

/// Half-open time interval covering a single tool/hook/skill call.
///
/// For orphan-synthesized starts, `started_at == ended_at` and span duration is zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Span {
    /// Inclusive lower bound of the interval.
    pub started_at: DateTime<Utc>,
    /// Exclusive upper bound of the interval; equal to `started_at` for instants.
    pub ended_at: DateTime<Utc>,
}

impl Span {
    /// Construct from explicit start and end.
    #[must_use]
    pub const fn new(started_at: DateTime<Utc>, ended_at: DateTime<Utc>) -> Self {
        Self {
            started_at,
            ended_at,
        }
    }

    /// Zero-duration span at a single instant — used for orphan synthesis.
    #[must_use]
    pub const fn instant(at: DateTime<Utc>) -> Self {
        Self::new(at, at)
    }

    /// Duration of the span.
    #[must_use]
    pub fn duration(&self) -> chrono::Duration {
        self.ended_at - self.started_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    use crate::episode::call_ref::CallRef;

    #[test]
    fn span_instant_is_zero_duration() {
        let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
        let s = Span::instant(t);
        assert_eq!(s.duration(), chrono::Duration::zero());
    }

    #[test]
    fn turn_new_starts_open_and_empty() {
        let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
        let turn = Turn::new("x".into(), t);
        assert_eq!(turn.status, TurnStatus::Open);
        assert_eq!(turn.ended_at, None);
        assert!(turn.tool_calls.is_empty());
    }

    #[test]
    fn empty_turn_is_not_user_blocking() {
        let t = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        assert!(!Turn::new("t".into(), t).is_user_blocking());
    }

    #[test]
    fn turn_with_only_non_blocking_tools_is_not_user_blocking() {
        let t = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("t".into(), t);
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        turn.tool_calls.push(CallRef::new("edit".into(), 1));
        assert!(!turn.is_user_blocking());
    }

    #[test]
    fn turn_with_ask_user_is_user_blocking() {
        let t = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("t".into(), t);
        turn.tool_calls.push(CallRef::new("ask_user".into(), 0));
        assert!(turn.is_user_blocking());
    }

    #[test]
    fn turn_with_mix_is_user_blocking_when_any_tool_blocks() {
        let t = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("t".into(), t);
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        turn.tool_calls.push(CallRef::new("ask_user".into(), 1));
        turn.tool_calls.push(CallRef::new("edit".into(), 2));
        assert!(turn.is_user_blocking());
    }
}
