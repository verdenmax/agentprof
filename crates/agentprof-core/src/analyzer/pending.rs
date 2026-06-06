//! Pending-call detection (F2) — derived "is this call stuck right
//! now?" property without schema change.
//!
//! User pain: when Copilot CLI invokes `ask_user`, the wire emits a
//! `tool.execution_start` and then BLOCKS waiting for user input in
//! the terminal. If the user is AFK, the session sits stalled with no
//! further events. In `agentprof watch` this manifests as "latest
//! turn never progresses" with no visible signal that *the agent is
//! waiting on you*, not stuck doing work.
//!
//! This module re-uses the existing
//! [`crate::episode::tool::ToolCallStatus::OpenAtEndOfSession`]
//! status (already meaning "started but no complete by last-event")
//! and turns it into a render-time pending indicator by comparing
//! `now - call.span.started_at` against a tool-class-specific
//! threshold:
//!
//! - `ask_user` (and any future entry in
//!   [`crate::analyzer::tool_rank::USER_BLOCKING_TOOLS`]) — 30 s.
//! - Any other tool — 5 minutes (long `bash`, MCP server stuck, etc).
//!
//! Designed for two callers:
//!
//! - **watch mode** (live): pass `Utc::now()` each render frame.
//! - **postmortem** (`analyze --export tui`): pass
//!   `meta.ended_at` (the historical "now" when the session ended)
//!   so pending status freezes.

use chrono::{DateTime, Duration, Utc};

use crate::analyzer::tool_rank::USER_BLOCKING_TOOLS;
use crate::episode::tool::{ToolCall, ToolCallStatus};
use crate::episode::Episodes;

/// Time an `ask_user` (or any tool in
/// [`crate::analyzer::tool_rank::USER_BLOCKING_TOOLS`]) can be
/// open before pending detection fires.
///
/// 30 seconds is comfortably longer than "I'm answering AI's prompt"
/// (typically < 10 s) and short enough that 30 s of silence is a
/// clear signal the user wandered off.
pub const ASK_USER_THRESHOLD: Duration = Duration::seconds(30);

/// Time any non-`USER_BLOCKING_TOOLS` tool can be open before
/// pending detection fires.
///
/// 5 minutes is the smallest "obviously stuck" boundary that
/// doesn't fire on legitimate long-running tools (a `cargo test`
/// suite or a build is plausibly 30+ minutes).
pub const DEFAULT_THRESHOLD: Duration = Duration::minutes(5);

/// Returns the pending threshold for a given tool name.
///
/// `USER_BLOCKING_TOOLS` members → [`ASK_USER_THRESHOLD`].
/// Everything else → [`DEFAULT_THRESHOLD`]. See
/// [`crate::analyzer::tool_rank::USER_BLOCKING_TOOLS`] for the
/// canonical list.
///
/// # Examples
///
/// ```
/// use chrono::Duration;
/// use agentprof_core::analyzer::pending::{
///     threshold_for, ASK_USER_THRESHOLD, DEFAULT_THRESHOLD,
/// };
/// assert_eq!(threshold_for("ask_user"), ASK_USER_THRESHOLD);
/// assert_eq!(threshold_for("bash"), DEFAULT_THRESHOLD);
/// assert_eq!(threshold_for("anything-else"), DEFAULT_THRESHOLD);
/// # let _ = Duration::seconds(0);
/// ```
#[must_use]
pub fn threshold_for(tool_name: &str) -> Duration {
    if USER_BLOCKING_TOOLS.contains(&tool_name) {
        ASK_USER_THRESHOLD
    } else {
        DEFAULT_THRESHOLD
    }
}

/// Is this call currently pending?
///
/// A call is pending when:
/// 1. Its status is `ToolCallStatus::OpenAtEndOfSession` (no
///    `tool.execution_complete` arrived before the session's
///    last-event timestamp), AND
/// 2. `now - call.span.started_at >= threshold_for(tool_name)`.
///
/// `now` is an explicit parameter so the two callers can pass
/// different "now":
/// - **watch mode**: `Utc::now()` — pending state re-evaluates each
///   frame.
/// - **postmortem**: `meta.ended_at` (or the last event timestamp).
///
/// Defensive: if `now < call.span.started_at` (clock skew), elapsed
/// is negative and the comparison returns false.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::pending::is_pending;
/// use agentprof_core::episode::tool::{ToolCall, ToolCallStatus};
/// use agentprof_core::episode::turn::Span;
/// use chrono::{Duration, TimeZone, Utc};
///
/// let started = Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap();
/// let mut call = ToolCall::new(Span::new(started, started));
/// call.status = ToolCallStatus::OpenAtEndOfSession;
///
/// // 30s elapsed — at the ask_user threshold (>=), pending.
/// let now = started + Duration::seconds(30);
/// assert!(is_pending(&call, "ask_user", now));
///
/// // 29.999s elapsed — below threshold.
/// let just_under = started + Duration::milliseconds(29_999);
/// assert!(!is_pending(&call, "ask_user", just_under));
///
/// // Success status — never pending.
/// let mut completed = ToolCall::new(Span::new(started, started));
/// completed.status = ToolCallStatus::Success;
/// let way_later = started + Duration::hours(30);
/// assert!(!is_pending(&completed, "ask_user", way_later));
/// ```
#[must_use]
pub fn is_pending(call: &ToolCall, tool_name: &str, now: DateTime<Utc>) -> bool {
    if !matches!(call.status, ToolCallStatus::OpenAtEndOfSession) {
        return false;
    }
    let elapsed = now.signed_duration_since(call.span.started_at);
    elapsed >= threshold_for(tool_name)
}

/// Identified pending call with rendering metadata.
///
/// Borrows from the source [`Episodes`] for `tool_name` and `turn_id`
/// so the result is a cheap view.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PendingCall<'a> {
    /// Tool name as keyed in `Episodes.tools`. e.g. `"ask_user"`.
    pub tool_name: &'a str,
    /// Turn this call was attributed to, if any.
    pub turn_id: Option<&'a str>,
    /// When `tool.execution_start` arrived.
    pub started_at: DateTime<Utc>,
    /// `now - started_at`, computed once at scan time.
    pub elapsed: Duration,
    /// True iff `tool_name` is in
    /// [`crate::analyzer::tool_rank::USER_BLOCKING_TOOLS`].
    pub is_user_blocking: bool,
}

/// Scan all open calls across `episodes.tools` for pending ones.
///
/// Returns a deterministically-ordered vector:
/// 1. `is_user_blocking` descending (user-blocking first).
/// 2. `tool_name` ascending (stable across reloads).
/// 3. `started_at` ascending (oldest pending first).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::pending::pending_calls;
/// use agentprof_core::episode::Episodes;
/// use chrono::Utc;
///
/// assert!(pending_calls(&Episodes::default(), Utc::now()).is_empty());
/// ```
#[must_use]
pub fn pending_calls<'a>(episodes: &'a Episodes, now: DateTime<Utc>) -> Vec<PendingCall<'a>> {
    let mut out: Vec<PendingCall<'a>> = Vec::new();
    for (tool_name, ep) in &episodes.tools {
        let is_user_blocking = USER_BLOCKING_TOOLS.contains(&tool_name.as_str());
        for call in &ep.calls {
            if is_pending(call, tool_name, now) {
                out.push(PendingCall {
                    tool_name: tool_name.as_str(),
                    turn_id: call.turn_id.as_deref(),
                    started_at: call.span.started_at,
                    elapsed: now.signed_duration_since(call.span.started_at),
                    is_user_blocking,
                });
            }
        }
    }
    // Deterministic sort.
    out.sort_by(|a, b| {
        std::cmp::Reverse(a.is_user_blocking)
            .cmp(&std::cmp::Reverse(b.is_user_blocking))
            .then_with(|| a.tool_name.cmp(b.tool_name))
            .then_with(|| a.started_at.cmp(&b.started_at))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::episode::tool::ToolEpisode;
    use crate::episode::turn::Span;
    use crate::model::ToolSource;
    use chrono::TimeZone;

    fn at(s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap() + Duration::seconds(i64::from(s))
    }

    fn open_call(started_offset_s: u32) -> ToolCall {
        let started = at(started_offset_s);
        let mut c = ToolCall::new(Span::new(started, started));
        c.status = ToolCallStatus::OpenAtEndOfSession;
        c
    }

    #[test]
    fn threshold_for_ask_user_is_30s() {
        assert_eq!(threshold_for("ask_user"), Duration::seconds(30));
    }

    #[test]
    fn threshold_for_other_tools_is_5min() {
        for name in ["bash", "read_file", "skill__code-reviewer__run", ""] {
            assert_eq!(threshold_for(name), Duration::minutes(5));
        }
    }

    #[test]
    fn is_pending_user_blocking_threshold_exact() {
        let call = open_call(0);
        // 30s elapsed == ASK_USER_THRESHOLD. Uses `>=`.
        assert!(is_pending(&call, "ask_user", at(30)));
    }

    #[test]
    fn is_pending_user_blocking_threshold_just_under() {
        let call = open_call(0);
        let now = at(0) + Duration::milliseconds(29_999);
        assert!(!is_pending(&call, "ask_user", now));
    }

    #[test]
    fn is_pending_default_threshold_exact() {
        let call = open_call(0);
        assert!(is_pending(&call, "bash", at(300)));
    }

    #[test]
    fn is_pending_default_threshold_just_under() {
        let call = open_call(0);
        assert!(!is_pending(&call, "bash", at(299)));
    }

    #[test]
    fn is_pending_non_open_status_returns_false() {
        let started = at(0);
        let way_later = started + Duration::hours(30);
        for status in [
            ToolCallStatus::Success,
            ToolCallStatus::Failure { message: None },
            ToolCallStatus::OrphanSynthesizedStart,
        ] {
            let mut c = ToolCall::new(Span::new(started, started));
            c.status = status.clone();
            assert!(
                !is_pending(&c, "ask_user", way_later),
                "{status:?} must never be pending"
            );
        }
    }

    #[test]
    fn is_pending_now_before_start_returns_false() {
        // Defensive: clock skew between wire-event emitter and our
        // local Utc::now() should not crash + should not claim pending.
        let call = open_call(30);
        // started_at = at(30); now = at(0) → elapsed = -30s.
        assert!(!is_pending(&call, "ask_user", at(0)));
    }

    #[test]
    fn pending_calls_empty_episodes_returns_empty() {
        let eps = Episodes::default();
        assert!(pending_calls(&eps, at(99_999)).is_empty());
    }

    #[test]
    fn pending_calls_sorts_user_blocking_first_then_name_then_started() {
        let mut eps = Episodes::default();
        // Two bash calls (one started at 0s, one at 10s) + one ask_user (5s).
        // All three should be pending at now=at(600) (way past both thresholds).
        let mut bash_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        bash_ep.calls.push(open_call(0));
        bash_ep.calls.push(open_call(10));
        eps.tools.insert("bash".into(), bash_ep);

        let mut ask_ep = ToolEpisode::new("ask_user".into(), ToolSource::Builtin);
        ask_ep.calls.push(open_call(5));
        eps.tools.insert("ask_user".into(), ask_ep);

        let pending = pending_calls(&eps, at(600));
        assert_eq!(pending.len(), 3, "all 3 should be pending: {pending:?}");
        // ask_user first (user-blocking).
        assert_eq!(pending[0].tool_name, "ask_user");
        assert!(pending[0].is_user_blocking);
        // Then bash by started_at ascending (oldest first).
        assert_eq!(pending[1].tool_name, "bash");
        assert_eq!(pending[1].started_at, at(0));
        assert_eq!(pending[2].tool_name, "bash");
        assert_eq!(pending[2].started_at, at(10));
    }

    #[test]
    fn pending_calls_omits_non_pending_within_threshold() {
        let mut eps = Episodes::default();
        let mut ask_ep = ToolEpisode::new("ask_user".into(), ToolSource::Builtin);
        ask_ep.calls.push(open_call(0)); // started at 0
        eps.tools.insert("ask_user".into(), ask_ep);
        // now = at(15) → elapsed = 15s < ASK_USER_THRESHOLD (30s).
        assert!(pending_calls(&eps, at(15)).is_empty());
    }
}
