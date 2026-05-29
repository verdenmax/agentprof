//! `derive_episodes` — the pure aggregation function.
//!
//! See `docs/internals/adr-0004-episode-derivation.md` for the algorithm
//! rationale and `docs/superpowers/specs/2026-05-27-...-design.md` §7 for
//! the state-machine pseudocode.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::adapter::{Event, EventKind};
use crate::episode::{
    call_ref::CallRef,
    hook::{HookCall, HookEpisode},
    mode_segment::{Mode, ModeSegment},
    skill::{SkillEpisode, SkillInvocation},
    tool::{ToolCall, ToolCallStatus, ToolEpisode},
    turn::{AbortInfo, Span, Turn, TurnStatus},
    warning::DeriveWarning,
    Episodes,
};
use crate::model::{SessionMeta, ToolSource};

/// Maximum events to look ahead for skill `triggered_tools` attribution.
/// See ADR-0004 NEG-003 + IMP-003 for rationale.
const SKILL_TRIGGER_WINDOW: usize = 50;

/// Sentinel `ToolEpisode` name used when `tool.execution_complete` arrives
/// without a matching `tool.execution_start` (orphan-synthesized start).
///
/// The wire payload of `tool.execution_complete` ([`ToolResultData`]) carries
/// only `tool_call_id`, not `tool_name`. With nothing meaningful to key on,
/// `derive_episodes` aggregates all such orphan calls under this single
/// sentinel name rather than emitting a separate `ToolEpisode` per opaque
/// event id. The accompanying [`DeriveWarning::SynthesizedStart`] records
/// the original event id for per-call accountability.
///
/// The angle-bracket form is deliberately not a valid tool name in any
/// agent's wire format, so it cannot collide with a real tool. Downstream
/// consumers (markdown / JSON renderers, future TUI) may special-case this
/// key for display, e.g. "(synthesized: 3 orphan completes)".
///
/// [`ToolResultData`]: crate::episode::tool::ToolCall
pub const ORPHAN_TOOL_SENTINEL: &str = "<orphan>";

/// Derive episodes from a slice of events and session metadata.
///
/// **Pure.** Same input → same output, byte-for-byte. Snapshot-stable.
/// **Total.** Never returns `Err`. Data-quality issues land in `Episodes.warnings`.
/// **Single-pass.** `O(N_events)` time, `O(N_episodes + N_warnings)` space.
///
/// See `docs/internals/adr-0004-episode-derivation.md` for the full rationale.
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::{AgentKind, Event, EventKind};
/// use agentprof_core::episode::{derive_episodes, Episodes};
/// use agentprof_core::model::SessionMeta;
/// use chrono::Utc;
///
/// struct StubEvent;
/// impl Event for StubEvent {
///     fn id(&self) -> &str { "stub" }
///     fn kind(&self) -> EventKind { EventKind::Unknown }
///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
///     fn parent_id(&self) -> Option<&str> { None }
/// }
///
/// let meta = SessionMeta::new(
///     "abc".into(),
///     AgentKind::Copilot,
///     Utc::now(),
///     false,
/// );
/// let events: Vec<StubEvent> = Vec::new();
/// let episodes: Episodes = derive_episodes(&events, &meta);
/// assert!(episodes.turns.is_empty());
/// ```
#[must_use]
pub fn derive_episodes<E: Event>(events: &[E], meta: &SessionMeta) -> Episodes {
    let mut state = DeriveState::new(meta);
    for (idx, ev) in events.iter().enumerate() {
        state.observe_timestamp(ev);
        match ev.kind() {
            EventKind::TurnStart => state.on_turn_start(ev),
            EventKind::TurnEnd => state.on_turn_end(ev),
            EventKind::ToolExecStart => state.on_tool_start(ev),
            EventKind::ToolExecComplete => state.on_tool_complete(ev),
            EventKind::ToolUserRequested => state.on_tool_user_requested(ev),
            EventKind::HookStart => state.on_hook_start(ev),
            EventKind::HookEnd => state.on_hook_end(ev),
            EventKind::SkillInvoked => state.on_skill_invoked(ev),
            EventKind::ModeChanged | EventKind::ModelChange => state.on_mode_event(ev),
            EventKind::Abort => state.on_abort(ev),
            _ => {} // metadata-only events (Session*, *Message, Shutdown, Unknown): no-op for derive
        }
        state.bump_skill_windows(idx, ev);
    }
    state.finalize()
}

// ---------- Internal state machine ----------

struct DeriveState {
    last_event_ts: Option<DateTime<Utc>>,
    prev_ts: Option<DateTime<Utc>>,
    turns: Vec<Turn>,
    open_turn_idx: Option<usize>,
    open_tool_calls: Vec<OpenToolCall>,
    open_hook_calls: Vec<OpenHookCall>,
    open_skills: Vec<OpenSkill>,
    tools: BTreeMap<String, ToolEpisode>,
    hooks: BTreeMap<String, HookEpisode>,
    skills: BTreeMap<String, SkillEpisode>,
    mode_segments: Vec<ModeSegment>,
    aborts: Vec<AbortInfo>,
    warnings: Vec<DeriveWarning>,
}

struct OpenToolCall {
    name: String,
    source: ToolSource,
    started_at: DateTime<Utc>,
    turn_id: Option<String>,
    user_requested: bool,
}

struct OpenHookCall {
    name: String,
    started_at: DateTime<Utc>,
    turn_id: Option<String>,
}

struct OpenSkill {
    name: String,
    invocation_idx: usize, // index into skills[name].invocations
    window_remaining: usize,
}

impl DeriveState {
    fn new(meta: &SessionMeta) -> Self {
        Self {
            last_event_ts: None,
            prev_ts: None,
            turns: Vec::new(),
            open_turn_idx: None,
            open_tool_calls: Vec::new(),
            open_hook_calls: Vec::new(),
            open_skills: Vec::new(),
            tools: BTreeMap::new(),
            hooks: BTreeMap::new(),
            skills: BTreeMap::new(),
            mode_segments: vec![ModeSegment::new(
                Mode::Unknown("default".into()),
                meta.started_at,
            )],
            aborts: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn observe_timestamp<E: Event>(&mut self, ev: &E) {
        let ts = ev.timestamp();
        if let Some(prev) = self.prev_ts {
            if ts < prev {
                self.warnings.push(DeriveWarning::NonMonotonicTimestamp {
                    event_id: ev.id().to_string(),
                    prev_at: prev,
                    this_at: ts,
                });
            }
        }
        self.prev_ts = Some(ts);
        self.last_event_ts = Some(ts);
    }

    /// Extract the payload-defined name from an event, with adapter-quality
    /// instrumentation.
    ///
    /// Returns `event.payload_name().to_string()` when the adapter override
    /// provides a name, otherwise falls back to `event.id().to_string()` AND
    /// emits a [`DeriveWarning::PayloadNameMissing`] so downstream consumers
    /// can flag the adapter as misconfigured. This is the defense-in-depth
    /// signal for ADR-0005 D-1's "silent failure" risk identified by the
    /// M1.4 audit: future Codex/Claude adapter authors who forget to override
    /// `Event::payload_name` for a name-bearing variant will see the warning
    /// instead of silently degrading into per-event-UUID groupings.
    ///
    /// `expected_kind` is the kind the caller dispatched on (one of
    /// `ToolExecStart` / `HookStart` / `HookEnd` / `SkillInvoked`). It is
    /// reported as the warning's `kind` field rather than `ev.kind()` so the
    /// signal stays stable even if the underlying adapter mislabels the
    /// event.
    fn resolve_payload_name<E: Event>(&mut self, ev: &E, expected_kind: EventKind) -> String {
        if let Some(name) = ev.payload_name() {
            return name.to_string();
        }
        self.warnings.push(DeriveWarning::PayloadNameMissing {
            kind: expected_kind,
            event_id: ev.id().to_string(),
        });
        ev.id().to_string()
    }

    fn on_turn_start<E: Event>(&mut self, ev: &E) {
        let turn = Turn::new(ev.id().to_string(), ev.timestamp());
        self.turns.push(turn);
        self.open_turn_idx = Some(self.turns.len() - 1);
    }

    fn on_turn_end<E: Event>(&mut self, ev: &E) {
        if let Some(idx) = self.open_turn_idx.take() {
            if let Some(turn) = self.turns.get_mut(idx) {
                turn.ended_at = Some(ev.timestamp());
                if !matches!(turn.status, TurnStatus::Aborted(_)) {
                    turn.status = TurnStatus::Completed;
                }
            }
        } else {
            // Orphan TurnEnd: synthesize a zero-duration Turn.
            let mut synth = Turn::new(format!("synthesized-{}", ev.id()), ev.timestamp());
            synth.ended_at = Some(ev.timestamp());
            synth.status = TurnStatus::Completed;
            self.turns.push(synth);
            self.warnings.push(DeriveWarning::SynthesizedStart {
                kind: EventKind::TurnStart,
                end_event_id: ev.id().to_string(),
            });
        }
    }

    fn on_tool_start<E: Event>(&mut self, ev: &E) {
        // Use real payload-defined name (e.g. "bash", "PreToolUse", "brainstorming")
        // via Event::payload_name(); fall back to event.id() as a safety net for
        // adapters that haven't implemented payload_name for a relevant variant.
        // See ADR-0005 D-1 + IMP-003 + the M1.4 audit Update section.
        let name = self.resolve_payload_name(ev, EventKind::ToolExecStart);
        let source = ToolSource::infer(&name);
        let turn_id = self.open_turn_idx.map(|i| self.turns[i].id.clone());
        self.open_tool_calls.push(OpenToolCall {
            name,
            source,
            started_at: ev.timestamp(),
            turn_id,
            user_requested: false,
        });
    }

    fn on_tool_complete<E: Event>(&mut self, ev: &E) {
        let ts = ev.timestamp();
        if let Some(open) = self.open_tool_calls.pop() {
            let span = Span::new(open.started_at, ts);
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status: ToolCallStatus::Success, // Task 10b will read actual success bit
                user_requested: open.user_requested,
            };
            self.commit_tool_call(&open.name, &open.source, call);
        } else {
            // Orphan complete → synthesize 0ms Start.
            //
            // ToolExecComplete's payload (ToolResultData) has no tool_name
            // field, so payload_name() returns None for the only adapter
            // wired today (CopilotEvent). Rather than fall back to ev.id()
            // — a per-event UUID that would create a fresh ToolEpisode per
            // orphan event and pollute tool_rank output with opaque keys —
            // aggregate all orphan completes under a single sentinel name.
            // The per-call SynthesizedStart warning below preserves the
            // original event id for accountability. See ADR-0005 Update
            // section (M1.4 audit followups) for the design rationale.
            let span = Span::instant(ts);
            let name = ORPHAN_TOOL_SENTINEL.to_string();
            let source = ToolSource::infer(&name);
            let call = ToolCall {
                span,
                turn_id: self.open_turn_idx.map(|i| self.turns[i].id.clone()),
                status: ToolCallStatus::OrphanSynthesizedStart,
                user_requested: false,
            };
            self.commit_tool_call(&name, &source, call);
            self.warnings.push(DeriveWarning::SynthesizedStart {
                kind: EventKind::ToolExecStart,
                end_event_id: ev.id().to_string(),
            });
        }
    }

    fn on_tool_user_requested<E: Event>(&mut self, _ev: &E) {
        // User-requested marker; flag the most-recent OpenToolCall if any.
        // Simplification: in real Copilot data, ToolUserRequested arrives
        // BEFORE the related ToolExecStart. Task 10b will pair properly.
        if let Some(call) = self.open_tool_calls.last_mut() {
            call.user_requested = true;
        }
    }

    fn commit_tool_call(&mut self, name: &str, source: &ToolSource, call: ToolCall) {
        let dur = call.span.duration();
        let is_failure = matches!(call.status, ToolCallStatus::Failure { .. });
        let start_turn_id = call.turn_id.clone();
        let ep = self
            .tools
            .entry(name.to_string())
            .or_insert_with(|| ToolEpisode::new(name.to_string(), source.clone()));
        let new_idx = ep.calls.len();
        ep.calls.push(call);
        ep.total_duration += dur;
        if is_failure {
            ep.fail_count = ep.fail_count.saturating_add(1);
        }

        // Self-describing back-reference for both turns and skill windows.
        let tool_ref = CallRef::new(name.to_string(), new_idx);

        // Attribute this committed tool call to every currently-open skill
        // window. Moved here (from `bump_skill_windows`) so the encoding is
        // by (tool-name, per-name index) instead of a meaningless cumulative
        // sum across all tool episodes. See ADR-0004 + the CallRef section.
        for s in &self.open_skills {
            if let Some(ep) = self.skills.get_mut(&s.name) {
                if let Some(inv) = ep.invocations.get_mut(s.invocation_idx) {
                    inv.triggered_tools.push(tool_ref.clone());
                }
                ep.subsequent_tool_calls = ep.subsequent_tool_calls.saturating_add(1);
            }
        }

        // Attribute back-reference to the Turn open AT START time (call.turn_id),
        // not the Turn open at commit time. Fixes commit-call-turn-divergence —
        // a tool whose span crosses turn boundaries belongs to its start turn.
        // See ADR-0005 D-2.
        if let Some(turn_id) = start_turn_id.as_ref() {
            if let Some(turn_idx) = self.turns.iter().rposition(|t| &t.id == turn_id) {
                self.turns[turn_idx].tool_calls.push(tool_ref);
            }
        }
    }

    fn on_hook_start<E: Event>(&mut self, ev: &E) {
        let name = self.resolve_payload_name(ev, EventKind::HookStart);
        let turn_id = self.open_turn_idx.map(|i| self.turns[i].id.clone());
        self.open_hook_calls.push(OpenHookCall {
            name,
            started_at: ev.timestamp(),
            turn_id,
        });
    }

    fn on_hook_end<E: Event>(&mut self, ev: &E) {
        let ts = ev.timestamp();
        if let Some(open) = self.open_hook_calls.pop() {
            let span = Span::new(open.started_at, ts);
            let call = HookCall {
                span,
                turn_id: open.turn_id,
                success: true,
                synthesized_start: false,
            };
            self.commit_hook_call(&open.name, call);
        } else {
            // Orphan hook.end: HookEnd payload carries hook_type so
            // payload_name() returns Some for CopilotEvent. For future
            // adapters that haven't implemented payload_name on HookEnd,
            // resolve_payload_name will emit PayloadNameMissing (using
            // HookEnd as the kind, since that's the actual event seen).
            let name = self.resolve_payload_name(ev, EventKind::HookEnd);
            let call = HookCall {
                span: Span::instant(ts),
                turn_id: self.open_turn_idx.map(|i| self.turns[i].id.clone()),
                success: true,
                synthesized_start: true,
            };
            self.commit_hook_call(&name, call);
            self.warnings.push(DeriveWarning::SynthesizedStart {
                kind: EventKind::HookStart,
                end_event_id: ev.id().to_string(),
            });
        }
    }

    fn commit_hook_call(&mut self, name: &str, call: HookCall) {
        let dur = call.span.duration();
        let failed = !call.success;
        let start_turn_id = call.turn_id.clone();
        let ep = self
            .hooks
            .entry(name.to_string())
            .or_insert_with(|| HookEpisode::new(name.to_string()));
        let new_idx = ep.calls.len();
        ep.calls.push(call);
        ep.total_duration += dur;
        if failed {
            ep.failure_count = ep.failure_count.saturating_add(1);
        }
        // See commit_tool_call (ADR-0005 D-2) for rationale.
        if let Some(turn_id) = start_turn_id.as_ref() {
            if let Some(turn_idx) = self.turns.iter().rposition(|t| &t.id == turn_id) {
                self.turns[turn_idx]
                    .hook_calls
                    .push(CallRef::new(name.to_string(), new_idx));
            }
        }
    }

    fn on_skill_invoked<E: Event>(&mut self, ev: &E) {
        let name = self.resolve_payload_name(ev, EventKind::SkillInvoked);
        let inv = SkillInvocation::new(ev.timestamp());
        let ep = self
            .skills
            .entry(name.clone())
            .or_insert_with(|| SkillEpisode::new(name.clone()));
        let new_idx = ep.invocations.len();
        ep.invocations.push(inv);
        if let Some(turn_idx) = self.open_turn_idx {
            self.turns[turn_idx]
                .skill_calls
                .push(CallRef::new(name.clone(), new_idx));
        }
        self.open_skills.push(OpenSkill {
            name,
            invocation_idx: new_idx,
            window_remaining: SKILL_TRIGGER_WINDOW,
        });
    }

    fn on_mode_event<E: Event>(&mut self, ev: &E) {
        let ts = ev.timestamp();
        if let Some(seg) = self.mode_segments.last_mut() {
            seg.ended_at = Some(ts);
        }
        // PLACEHOLDER: Mode value extraction is payload-specific.
        // Task 10b can refine to read the actual mode value.
        self.mode_segments
            .push(ModeSegment::new(Mode::Unknown("changed".into()), ts));
    }

    fn on_abort<E: Event>(&mut self, ev: &E) {
        let info = AbortInfo::new("abort".to_string(), ev.timestamp());

        // Attach to most-recently-opened: try open tool/hook first (more specific), then turn.
        if let Some(open) = self.open_tool_calls.pop() {
            let span = Span::new(open.started_at, ev.timestamp());
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status: ToolCallStatus::Failure {
                    message: Some("aborted".to_string()),
                },
                user_requested: open.user_requested,
            };
            self.commit_tool_call(&open.name, &open.source, call);
            return;
        }
        if let Some(open) = self.open_hook_calls.pop() {
            let span = Span::new(open.started_at, ev.timestamp());
            let call = HookCall {
                span,
                turn_id: open.turn_id,
                success: false,
                synthesized_start: false,
            };
            self.commit_hook_call(&open.name, call);
            return;
        }
        if let Some(turn_idx) = self.open_turn_idx {
            if let Some(turn) = self.turns.get_mut(turn_idx) {
                turn.status = TurnStatus::Aborted(info);
                turn.ended_at = Some(ev.timestamp());
            }
            self.open_turn_idx = None;
            return;
        }
        // Nothing open — push to aborts + warn.
        self.aborts.push(info);
        self.warnings.push(DeriveWarning::AbortWithoutOpenElement {
            reason: "abort".to_string(),
            at: ev.timestamp(),
        });
    }

    fn bump_skill_windows<E: Event>(&mut self, _idx: usize, _ev: &E) {
        // Tool-call attribution moved into `commit_tool_call` (which knows
        // the actual tool name + per-name index, enabling self-describing
        // `CallRef` back-references). Here we only decrement window timers.
        self.open_skills.retain_mut(|s| {
            s.window_remaining = s.window_remaining.saturating_sub(1);
            s.window_remaining > 0
        });
    }

    fn finalize(mut self) -> Episodes {
        let last_ts = self.last_event_ts;

        // Open Turn left as TurnStatus::Open with ended_at = None (ADR-0004 §7.2).
        // The take() simply releases the index; no field mutation needed.
        let _ = self.open_turn_idx.take();

        // Close any open ToolCall as OpenAtEndOfSession.
        for open in std::mem::take(&mut self.open_tool_calls) {
            let end = last_ts.unwrap_or(open.started_at);
            let span = Span::new(open.started_at, end);
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status: ToolCallStatus::OpenAtEndOfSession,
                user_requested: open.user_requested,
            };
            let start_event_id = format!("open-tool-{}", open.name);
            self.commit_tool_call(&open.name, &open.source, call);
            self.warnings.push(DeriveWarning::OpenAtEndOfSession {
                kind: EventKind::ToolExecStart,
                start_event_id,
            });
        }

        // Close any open HookCall similarly.
        for open in std::mem::take(&mut self.open_hook_calls) {
            let end = last_ts.unwrap_or(open.started_at);
            let span = Span::new(open.started_at, end);
            let call = HookCall {
                span,
                turn_id: open.turn_id,
                success: true,
                synthesized_start: false,
            };
            let start_event_id = format!("open-hook-{}", open.name);
            self.commit_hook_call(&open.name, call);
            self.warnings.push(DeriveWarning::OpenAtEndOfSession {
                kind: EventKind::HookStart,
                start_event_id,
            });
        }

        // Close the last open mode segment.
        if let Some(seg) = self.mode_segments.last_mut() {
            if seg.ended_at.is_none() {
                if let Some(ts) = last_ts {
                    seg.ended_at = Some(ts);
                }
            }
        }

        Episodes {
            turns: self.turns,
            tools: self.tools,
            hooks: self.hooks,
            skills: self.skills,
            mode_segments: self.mode_segments,
            aborts: self.aborts,
            warnings: self.warnings,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::adapter::AgentKind;
    use chrono::TimeZone;

    fn meta() -> SessionMeta {
        SessionMeta::new(
            "s1".into(),
            AgentKind::Copilot,
            Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap(),
            false,
        )
    }

    struct E {
        id: &'static str,
        kind: EventKind,
        ts: DateTime<Utc>,
    }
    impl Event for E {
        fn id(&self) -> &str {
            self.id
        }
        fn kind(&self) -> EventKind {
            self.kind
        }
        fn timestamp(&self) -> DateTime<Utc> {
            self.ts
        }
        fn parent_id(&self) -> Option<&str> {
            None
        }
    }

    fn at(secs: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, secs).unwrap()
    }

    #[test]
    fn empty_events_produce_empty_episodes_with_default_mode_segment() {
        let events: Vec<E> = Vec::new();
        let ep = derive_episodes(&events, &meta());
        assert!(ep.turns.is_empty());
        assert!(ep.tools.is_empty());
        assert_eq!(ep.mode_segments.len(), 1);
    }

    #[test]
    fn paired_turn_start_end_completes() {
        let events = vec![
            E {
                id: "t1",
                kind: EventKind::TurnStart,
                ts: at(1),
            },
            E {
                id: "t1-end",
                kind: EventKind::TurnEnd,
                ts: at(5),
            },
        ];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 1);
        assert_eq!(ep.turns[0].status, TurnStatus::Completed);
        assert_eq!(ep.turns[0].ended_at, Some(at(5)));
    }

    #[test]
    fn orphan_turn_end_synthesizes_warning() {
        let events = vec![E {
            id: "orphan-end",
            kind: EventKind::TurnEnd,
            ts: at(3),
        }];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 1);
        assert!(ep.warnings.iter().any(|w| matches!(
            w,
            DeriveWarning::SynthesizedStart {
                kind: EventKind::TurnStart,
                ..
            }
        )));
    }

    #[test]
    fn open_tool_at_end_emits_open_warning() {
        let events = vec![E {
            id: "tool-start",
            kind: EventKind::ToolExecStart,
            ts: at(2),
        }];
        let ep = derive_episodes(&events, &meta());
        // The placeholder uses event.id() as tool name → "tool-start" key.
        let entry = ep.tools.get("tool-start").expect("tool present");
        assert_eq!(entry.calls.len(), 1);
        assert_eq!(entry.calls[0].status, ToolCallStatus::OpenAtEndOfSession);
        assert!(ep.warnings.iter().any(|w| matches!(
            w,
            DeriveWarning::OpenAtEndOfSession {
                kind: EventKind::ToolExecStart,
                ..
            }
        )));
    }

    #[test]
    fn abort_with_no_open_element_warns_and_pushes_to_aborts() {
        let events = vec![E {
            id: "abort1",
            kind: EventKind::Abort,
            ts: at(2),
        }];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.aborts.len(), 1);
        assert!(ep
            .warnings
            .iter()
            .any(|w| matches!(w, DeriveWarning::AbortWithoutOpenElement { .. })));
    }

    #[test]
    fn non_monotonic_timestamp_warns_without_reordering() {
        let events = vec![
            E {
                id: "a",
                kind: EventKind::Unknown,
                ts: at(5),
            },
            E {
                id: "b",
                kind: EventKind::Unknown,
                ts: at(3),
            },
        ];
        let ep = derive_episodes(&events, &meta());
        assert!(ep
            .warnings
            .iter()
            .any(|w| matches!(w, DeriveWarning::NonMonotonicTimestamp { .. })));
    }

    #[test]
    fn cross_turn_tool_attributes_to_start_time_turn() {
        let events = vec![
            E {
                id: "turn-A",
                kind: EventKind::TurnStart,
                ts: at(1),
            },
            E {
                id: "tool-X-start",
                kind: EventKind::ToolExecStart,
                ts: at(2),
            },
            E {
                id: "turn-A-end",
                kind: EventKind::TurnEnd,
                ts: at(3),
            },
            E {
                id: "turn-B",
                kind: EventKind::TurnStart,
                ts: at(4),
            },
            E {
                id: "tool-X-end",
                kind: EventKind::ToolExecComplete,
                ts: at(5),
            },
            E {
                id: "turn-B-end",
                kind: EventKind::TurnEnd,
                ts: at(6),
            },
        ];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 2);
        // The tool started during turn-A; the back-reference must land on turn-A,
        // even though the ToolExecComplete event arrives during turn-B.
        assert_eq!(
            ep.turns[0].tool_calls.len(),
            1,
            "tool should attribute to start-time turn (turn-A)"
        );
        assert_eq!(
            ep.turns[1].tool_calls.len(),
            0,
            "turn-B should not own a tool it didn't start"
        );
        // The committed ToolCall.turn_id must agree.
        let tool_ep = ep
            .tools
            .values()
            .next()
            .expect("exactly one tool episode expected");
        assert_eq!(tool_ep.calls[0].turn_id.as_deref(), Some("turn-A"));
    }

    #[test]
    fn orphan_tool_complete_aggregates_under_sentinel_not_event_id() {
        // Three orphan tool.execution_complete events with no matching
        // tool.execution_start. Each has a distinct event id. They should
        // ALL aggregate under ORPHAN_TOOL_SENTINEL ("<orphan>") and each
        // emit a SynthesizedStart warning carrying the original id.
        //
        // Before the fix they each created a separate ToolEpisode keyed by
        // the event UUID, polluting tool_rank with opaque entries.
        let events = vec![
            E {
                id: "t1",
                kind: EventKind::TurnStart,
                ts: at(1),
            },
            E {
                id: "orphan-A",
                kind: EventKind::ToolExecComplete,
                ts: at(2),
            },
            E {
                id: "orphan-B",
                kind: EventKind::ToolExecComplete,
                ts: at(3),
            },
            E {
                id: "orphan-C",
                kind: EventKind::ToolExecComplete,
                ts: at(4),
            },
            E {
                id: "t1-end",
                kind: EventKind::TurnEnd,
                ts: at(5),
            },
        ];
        let ep = derive_episodes(&events, &meta());

        // Exactly ONE ToolEpisode keyed by the sentinel.
        assert_eq!(
            ep.tools.len(),
            1,
            "all orphans should aggregate to one episode"
        );
        assert!(
            ep.tools.contains_key(ORPHAN_TOOL_SENTINEL),
            "expected key '{ORPHAN_TOOL_SENTINEL}', got {:?}",
            ep.tools.keys().collect::<Vec<_>>()
        );

        let orphan_ep = &ep.tools[ORPHAN_TOOL_SENTINEL];
        assert_eq!(orphan_ep.name, ORPHAN_TOOL_SENTINEL);
        assert_eq!(orphan_ep.calls.len(), 3, "3 orphan completes → 3 calls");
        for call in &orphan_ep.calls {
            assert!(matches!(
                call.status,
                ToolCallStatus::OrphanSynthesizedStart
            ));
            assert_eq!(call.turn_id.as_deref(), Some("t1"));
        }

        // Per-call accountability via warnings (carry original event ids).
        let synth: Vec<&str> = ep
            .warnings
            .iter()
            .filter_map(|w| match w {
                DeriveWarning::SynthesizedStart {
                    kind: EventKind::ToolExecStart,
                    end_event_id,
                } => Some(end_event_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(synth, ["orphan-A", "orphan-B", "orphan-C"]);

        // Back-reference also lives on the turn.
        assert_eq!(ep.turns[0].tool_calls.len(), 3);
        for tref in &ep.turns[0].tool_calls {
            assert_eq!(tref.name, ORPHAN_TOOL_SENTINEL);
        }
    }

    #[test]
    fn payload_name_missing_warning_fires_when_adapter_returns_none() {
        // The stub Event `E` does NOT override `payload_name`, so the
        // trait's default `None` is returned. Feeding it ToolExecStart /
        // HookStart / SkillInvoked kinds simulates a future adapter that
        // forgot to override payload_name. Each event should produce one
        // PayloadNameMissing warning carrying the original kind + id, AND
        // the name should fall back to event.id() so derive_episodes still
        // produces output (graceful degradation).
        let events = vec![
            E {
                id: "turn",
                kind: EventKind::TurnStart,
                ts: at(1),
            },
            E {
                id: "tool-evt",
                kind: EventKind::ToolExecStart,
                ts: at(2),
            },
            E {
                id: "hook-evt",
                kind: EventKind::HookStart,
                ts: at(3),
            },
            E {
                id: "skill-evt",
                kind: EventKind::SkillInvoked,
                ts: at(4),
            },
            E {
                id: "tool-end",
                kind: EventKind::ToolExecComplete,
                ts: at(5),
            },
            E {
                id: "hook-end",
                kind: EventKind::HookEnd,
                ts: at(6),
            },
            E {
                id: "turn-end",
                kind: EventKind::TurnEnd,
                ts: at(7),
            },
        ];
        let ep = derive_episodes(&events, &meta());

        // Collect just the PayloadNameMissing warnings.
        let missing: Vec<(EventKind, &str)> = ep
            .warnings
            .iter()
            .filter_map(|w| match w {
                DeriveWarning::PayloadNameMissing { kind, event_id } => {
                    Some((*kind, event_id.as_str()))
                }
                _ => None,
            })
            .collect();

        // ToolExecStart + HookStart + SkillInvoked each emit one warning
        // (3 total). HookEnd happy path (matched) does NOT emit because the
        // name comes from the OpenHookCall, not the End event.
        assert_eq!(missing.len(), 3, "got: {missing:?}");
        assert!(missing.contains(&(EventKind::ToolExecStart, "tool-evt")));
        assert!(missing.contains(&(EventKind::HookStart, "hook-evt")));
        assert!(missing.contains(&(EventKind::SkillInvoked, "skill-evt")));

        // Graceful degradation: event.id() fallback produced ToolEpisode /
        // HookEpisode / SkillEpisode keys.
        assert!(ep.tools.contains_key("tool-evt"));
        assert!(ep.hooks.contains_key("hook-evt"));
        assert!(ep.skills.contains_key("skill-evt"));
    }
}
