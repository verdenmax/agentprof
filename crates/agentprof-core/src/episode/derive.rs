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
/// **Two-pass.** PASS 0 walks events once to collect
/// `(tool_call_id → arguments)` into a `BTreeMap` from each event's
/// [`Event::payload_tool_requests`] output. PASS 1 runs the
/// state machine; on every `ToolCall` close (normal, orphan,
/// abort, end-of-session) it looks up args via the End event's
/// [`Event::tool_call_id`] (with fallback to the Start-captured
/// id when the End event doesn't carry one). Total complexity
/// stays `O(N_events × max_requests_per_event)` — `O(N_events)`
/// typical, `O(N_episodes + N_warnings)` space. See ADR-0011
/// D-3 + D-4 for the rationale.
///
/// See `docs/internals/adr-0004-episode-derivation.md` for the full rationale.
///
/// # Adapter contract for `ToolCall.arguments`
///
/// Adapters that wish to populate `ToolCall.arguments` must
/// implement BOTH [`Event::payload_tool_requests`] (to emit
/// `(tool_call_id, args)` pairs at request time) AND
/// [`Event::tool_call_id`] (to expose the id on close-event
/// variants for lookup). Implementing only one silently
/// no-ops: PASS 0 will collect args that PASS 1 cannot find,
/// or PASS 1 will lookup ids that PASS 0 never collected.
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
#[tracing::instrument(name = "analyzer.derive_episodes", skip_all, fields(events = events.len()))]
pub fn derive_episodes<E: Event>(events: &[E], meta: &SessionMeta) -> Episodes {
    // PASS 0: collect (tool_call_id → arguments) map by walking events once
    // before the state machine. ToolCall.arguments is then attached on tool
    // close via args_by_call_id.get(&call_id).cloned().
    // First-occurrence-wins on duplicate ids (ADR-0011 D-4).
    let mut args_by_call_id: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for ev in events {
        for (call_id, args) in ev.payload_tool_requests() {
            if args_by_call_id.contains_key(&call_id) {
                tracing::debug!(
                    target: "derive",
                    tool_call_id = %call_id,
                    "duplicate tool_call_id args ignored (first-wins)"
                );
                continue;
            }
            args_by_call_id.insert(call_id, args);
        }
    }
    let mut state = DeriveState::new(meta, args_by_call_id);
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
            EventKind::AssistantMessage => state.on_assistant_message(ev),
            EventKind::ModeChanged | EventKind::ModelChange => state.on_mode_event(ev),
            EventKind::Abort => state.on_abort(ev),
            _ => {} // metadata-only events (Session*, UserMessage, SystemMessage, Shutdown, Unknown): no-op for derive
        }
        state.bump_skill_windows(idx, ev);
    }
    let episodes = state.finalize();
    tracing::debug!(
        turns = episodes.turns.len(),
        tool_calls = episodes
            .tools
            .values()
            .map(|t| t.calls.len())
            .sum::<usize>(),
        hooks = episodes.hooks.len(),
        warnings = episodes.warnings.len(),
        "derived episodes"
    );
    episodes
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
    /// Active session mode tracked across the event stream. Initialized
    /// to `Some(Mode::Interactive)` (Copilot CLI's implicit default;
    /// see `DeriveState::new`). Updated by [`Self::on_mode_event`] when
    /// it sees `ModeChanged` events with a non-None [`Event::payload_mode`].
    /// Captured at [`Self::on_turn_start`] and written into the new
    /// `Turn.mode` field.
    current_mode: Option<Mode>,
    aborts: Vec<AbortInfo>,
    warnings: Vec<DeriveWarning>,
    /// PASS 0 map: `tool_call_id` → args. Populated before the state-machine
    /// walk; consulted at tool-close (normal, orphan, abort, end-of-session)
    /// to stamp `ToolCall.arguments`. See ADR-0011 D-3 + D-4.
    args_by_call_id: BTreeMap<String, serde_json::Value>,
}

struct OpenToolCall {
    name: String,
    source: ToolSource,
    started_at: DateTime<Utc>,
    turn_id: Option<String>,
    user_requested: bool,
    /// Adapter-supplied `tool_call_id` captured at Start, used to look up
    /// args from `DeriveState::args_by_call_id` at close time (esp. when
    /// the close path is abort / end-of-session and doesn't carry an
    /// `Event` with `tool_call_id()` of its own).
    tool_call_id: Option<String>,
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
    fn new(meta: &SessionMeta, args_by_call_id: BTreeMap<String, serde_json::Value>) -> Self {
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
                // Copilot CLI's implicit default mode at session start.
                // Verified against 73 real session.mode_changed events:
                // every `previousMode → newMode` transition that opens
                // the session uses 'interactive' as previousMode (sessions
                // without any mode_changed events run entirely in
                // interactive). Was Mode::Unknown("default") in M1.3
                // before this vocabulary was discovered.
                Mode::Interactive,
                meta.started_at,
            )],
            // Initial active mode matches the initial ModeSegment.
            current_mode: Some(Mode::Interactive),
            aborts: Vec::new(),
            warnings: Vec::new(),
            args_by_call_id,
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
        let mut turn = Turn::new(ev.id().to_string(), ev.timestamp());
        // Attribute the currently-active mode to this turn. If no
        // session.mode_changed event has been seen yet, mode stays None
        // (per spec FR-7). Mode-changes that happen mid-turn DON'T
        // retroactively update this turn's mode — only subsequent turns
        // see the new mode. Matches user intuition: 'this turn was
        // started in X mode'.
        turn.mode.clone_from(&self.current_mode);
        self.turns.push(turn);
        self.open_turn_idx = Some(self.turns.len() - 1);
    }

    fn on_assistant_message<E: Event>(&mut self, ev: &E) {
        // Populate Turn.model (last-wins across messages) and Turn.output_tokens
        // (saturating sum across messages). Per spec FR-4, FR-5.
        //
        // If no turn is open (data anomaly — assistant.message arriving
        // before turn_start), silently ignore: the data is still in the
        // event stream, we just don't have a Turn to attribute it to.
        // Per spec FR-8.
        let Some(idx) = self.open_turn_idx else {
            return;
        };
        let Some(turn) = self.turns.get_mut(idx) else {
            return;
        };
        if let Some(model) = ev.payload_model() {
            turn.model = Some(model.to_string());
        }
        if let Some(tokens) = ev.payload_output_tokens() {
            turn.output_tokens = Some(turn.output_tokens.unwrap_or(0).saturating_add(tokens));
        }
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
            tool_call_id: ev.tool_call_id().map(str::to_owned),
        });
    }

    fn on_tool_complete<E: Event>(&mut self, ev: &E) {
        let ts = ev.timestamp();
        // End event's tool_call_id is the authoritative key for normal pairs
        // (stack-order pairing per ADR-0004 — see file comment); for orphans
        // the same End event id is still the only key we have to look up
        // PASS 0 args (ADR-0011 D-3 + M2 follow-up: orphan should still try).
        let end_call_id = ev.tool_call_id();
        if let Some(open) = self.open_tool_calls.pop() {
            let span = Span::new(open.started_at, ts);
            // Prefer End-event id (always present in Copilot); fall back
            // to Start-captured id. The fallback is load-bearing for
            // future adapters whose Complete variant doesn't carry the
            // id (i.e. their `Event::tool_call_id()` returns None for
            // ToolExecComplete).
            let lookup_id = end_call_id.or(open.tool_call_id.as_deref());
            let arguments = lookup_id.and_then(|id| self.args_by_call_id.get(id).cloned());
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status: ToolCallStatus::Success, // Task 10b will read actual success bit
                user_requested: open.user_requested,
                arguments,
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
            // Even orphan completes can carry a tool_call_id whose args were
            // declared earlier by an assistant.message — keep the lookup so
            // M2 of the Task 3 review is satisfied (ADR-0011 D-3).
            let arguments = end_call_id.and_then(|id| self.args_by_call_id.get(id).cloned());
            let call = ToolCall {
                span,
                turn_id: self.open_turn_idx.map(|i| self.turns[i].id.clone()),
                status: ToolCallStatus::OrphanSynthesizedStart,
                user_requested: false,
                arguments,
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
        // Close the previous ModeSegment regardless of whether we have a
        // new value (existing behavior preserved).
        if let Some(seg) = self.mode_segments.last_mut() {
            seg.ended_at = Some(ts);
        }
        // Read the actual mode from the payload. Per spec FR-6 + FR-7:
        // - ModeChanged events carry data.new_mode → Some
        // - ModelChange events do NOT carry mode → None (we still close
        //   the previous segment but don't push a new one, since the
        //   active mode is unchanged)
        if let Some(new_mode_str) = ev.payload_mode() {
            let new_mode = Mode::from_wire(new_mode_str);
            self.current_mode = Some(new_mode.clone());
            self.mode_segments.push(ModeSegment::new(new_mode, ts));
        }
    }

    fn on_abort<E: Event>(&mut self, ev: &E) {
        let info = AbortInfo::new("abort".to_string(), ev.timestamp());

        // Attach to most-recently-opened: try open tool/hook first (more specific), then turn.
        if let Some(open) = self.open_tool_calls.pop() {
            let span = Span::new(open.started_at, ev.timestamp());
            // Abort event itself has no tool_call_id; use the Start-captured id.
            let arguments = open
                .tool_call_id
                .as_deref()
                .and_then(|id| self.args_by_call_id.get(id).cloned());
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status: ToolCallStatus::Failure {
                    message: Some("aborted".to_string()),
                },
                user_requested: open.user_requested,
                arguments,
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
            // End-of-session close: rely on Start-captured tool_call_id since
            // there is no triggering Event here.
            let arguments = open
                .tool_call_id
                .as_deref()
                .and_then(|id| self.args_by_call_id.get(id).cloned());
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status: ToolCallStatus::OpenAtEndOfSession,
                user_requested: open.user_requested,
                arguments,
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

    /// Richer test stub that lets each test customize the payload methods.
    /// The simpler `E` struct above (defined earlier in this `mod tests`)
    /// doesn't override payload methods — all default to None.
    struct MetadataE {
        id: &'static str,
        kind: EventKind,
        ts: DateTime<Utc>,
        model: Option<&'static str>,
        output_tokens: Option<u32>,
        mode: Option<&'static str>,
    }
    impl Event for MetadataE {
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
        fn payload_model(&self) -> Option<&str> {
            self.model
        }
        fn payload_output_tokens(&self) -> Option<u32> {
            self.output_tokens
        }
        fn payload_mode(&self) -> Option<&str> {
            self.mode
        }
    }

    fn turn_start(id: &'static str, secs: u32) -> MetadataE {
        MetadataE {
            id,
            kind: EventKind::TurnStart,
            ts: at(secs),
            model: None,
            output_tokens: None,
            mode: None,
        }
    }
    fn turn_end(secs: u32) -> MetadataE {
        MetadataE {
            id: "te",
            kind: EventKind::TurnEnd,
            ts: at(secs),
            model: None,
            output_tokens: None,
            mode: None,
        }
    }
    fn assistant_msg(model: &'static str, tokens: u32, secs: u32) -> MetadataE {
        MetadataE {
            id: "am",
            kind: EventKind::AssistantMessage,
            ts: at(secs),
            model: Some(model),
            output_tokens: Some(tokens),
            mode: None,
        }
    }
    fn mode_change(mode: &'static str, secs: u32) -> MetadataE {
        MetadataE {
            id: "mc",
            kind: EventKind::ModeChanged,
            ts: at(secs),
            model: None,
            output_tokens: None,
            mode: Some(mode),
        }
    }

    #[test]
    fn assistant_message_populates_model_and_output_tokens() {
        let events = vec![
            turn_start("t1", 1),
            assistant_msg("claude-opus-4.7", 412, 2),
            turn_end(3),
        ];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 1);
        assert_eq!(ep.turns[0].model.as_deref(), Some("claude-opus-4.7"));
        assert_eq!(ep.turns[0].output_tokens, Some(412));
    }

    #[test]
    fn multiple_assistant_messages_sum_output_tokens_and_last_wins_model() {
        // Two messages in same turn: model changes mid-turn (rare but
        // possible), tokens should sum, model should be last-wins.
        let events = vec![
            turn_start("t1", 1),
            assistant_msg("gpt-5-mini", 100, 2),
            assistant_msg("claude-opus-4.7", 250, 3),
            turn_end(4),
        ];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 1);
        // Sum: 100 + 250 = 350
        assert_eq!(ep.turns[0].output_tokens, Some(350));
        // Last-wins: the second message's model.
        assert_eq!(ep.turns[0].model.as_deref(), Some("claude-opus-4.7"));
    }

    #[test]
    fn mode_change_attributes_to_next_turn_not_current() {
        // Sequence: mode→interactive, turn-A opens, mode→autopilot mid-turn,
        // turn-A ends, turn-B opens, turn-B ends. Expected:
        //   turn-A.mode = Some(Interactive)  (captured at turn_start; not retroactively updated)
        //   turn-B.mode = Some(Autopilot)    (captures the new current_mode)
        let events = vec![
            mode_change("interactive", 1),
            turn_start("t-A", 2),
            mode_change("autopilot", 3),
            turn_end(4),
            turn_start("t-B", 5),
            turn_end(6),
        ];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 2);
        assert_eq!(ep.turns[0].mode, Some(Mode::Interactive));
        assert_eq!(ep.turns[1].mode, Some(Mode::Autopilot));
    }

    #[test]
    fn turn_without_assistant_message_has_none_model_and_tokens() {
        // Defensive: a turn that opens and closes with no assistant.message
        // in between (atypical but possible) keeps model/output_tokens
        // at None — and that's the user-facing signal.
        let events = vec![turn_start("t1", 1), turn_end(2)];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 1);
        assert_eq!(ep.turns[0].model, None);
        assert_eq!(ep.turns[0].output_tokens, None);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod args_plumbing_tests {
    use super::*;
    use crate::adapter::{AgentKind, EventKind};
    use crate::model::SessionMeta;
    use chrono::{TimeZone, Utc};

    /// Minimal event variants for testing the args-attachment flow.
    /// Mirrors the shape that `derive_episodes` walks.
    enum E {
        TurnStart {
            id: String,
            ts: chrono::DateTime<Utc>,
        },
        AssistantMsg {
            id: String,
            ts: chrono::DateTime<Utc>,
            requests: Vec<(String, serde_json::Value)>,
        },
        ToolStart {
            id: String,
            ts: chrono::DateTime<Utc>,
            tool_call_id: String,
            tool_name: String,
        },
        ToolEnd {
            id: String,
            ts: chrono::DateTime<Utc>,
            tool_call_id: String,
        },
        TurnEnd {
            id: String,
            ts: chrono::DateTime<Utc>,
        },
    }

    impl Event for E {
        fn id(&self) -> &str {
            match self {
                Self::TurnStart { id, .. }
                | Self::AssistantMsg { id, .. }
                | Self::ToolStart { id, .. }
                | Self::ToolEnd { id, .. }
                | Self::TurnEnd { id, .. } => id,
            }
        }
        fn kind(&self) -> EventKind {
            match self {
                Self::TurnStart { .. } => EventKind::TurnStart,
                Self::AssistantMsg { .. } => EventKind::AssistantMessage,
                Self::ToolStart { .. } => EventKind::ToolExecStart,
                Self::ToolEnd { .. } => EventKind::ToolExecComplete,
                Self::TurnEnd { .. } => EventKind::TurnEnd,
            }
        }
        fn timestamp(&self) -> chrono::DateTime<Utc> {
            match self {
                Self::TurnStart { ts, .. }
                | Self::AssistantMsg { ts, .. }
                | Self::ToolStart { ts, .. }
                | Self::ToolEnd { ts, .. }
                | Self::TurnEnd { ts, .. } => *ts,
            }
        }
        fn parent_id(&self) -> Option<&str> {
            None
        }
        fn payload_name(&self) -> Option<&str> {
            match self {
                Self::ToolStart { tool_name, .. } => Some(tool_name.as_str()),
                _ => None,
            }
        }
        fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
            match self {
                Self::AssistantMsg { requests, .. } => requests.clone(),
                _ => Vec::new(),
            }
        }
        fn tool_call_id(&self) -> Option<&str> {
            match self {
                Self::ToolStart { tool_call_id, .. } | Self::ToolEnd { tool_call_id, .. } => {
                    Some(tool_call_id.as_str())
                }
                _ => None,
            }
        }
    }

    fn meta() -> SessionMeta {
        SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false)
    }

    fn t(s: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, s).unwrap()
    }

    #[test]
    fn args_attached_when_payload_tool_requests_seen_before_close() {
        let events = vec![
            E::TurnStart {
                id: "t".into(),
                ts: t(0),
            },
            E::AssistantMsg {
                id: "m".into(),
                ts: t(1),
                requests: vec![("tc-1".into(), serde_json::json!({"command": "ls"}))],
            },
            E::ToolStart {
                id: "s".into(),
                ts: t(2),
                tool_call_id: "tc-1".into(),
                tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(),
                ts: t(3),
                tool_call_id: "tc-1".into(),
            },
            E::TurnEnd {
                id: "te".into(),
                ts: t(4),
            },
        ];
        let ep = derive_episodes(&events, &meta());
        let bash = ep.tools.get("bash").expect("bash episode present");
        assert_eq!(bash.calls.len(), 1);
        assert_eq!(
            bash.calls[0].arguments,
            Some(serde_json::json!({"command": "ls"}))
        );
    }

    #[test]
    fn args_none_when_no_matching_tool_request_event() {
        let events = vec![
            E::TurnStart {
                id: "t".into(),
                ts: t(0),
            },
            E::ToolStart {
                id: "s".into(),
                ts: t(1),
                tool_call_id: "tc-orphan".into(),
                tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(),
                ts: t(2),
                tool_call_id: "tc-orphan".into(),
            },
            E::TurnEnd {
                id: "te".into(),
                ts: t(3),
            },
        ];
        let ep = derive_episodes(&events, &meta());
        let bash = ep.tools.get("bash").expect("bash episode present");
        assert!(bash.calls[0].arguments.is_none());
    }

    #[test]
    fn args_first_occurrence_wins_on_duplicate_call_id() {
        let events = vec![
            E::TurnStart {
                id: "t".into(),
                ts: t(0),
            },
            E::AssistantMsg {
                id: "m1".into(),
                ts: t(1),
                requests: vec![("tc-dup".into(), serde_json::json!({"v": "first"}))],
            },
            E::AssistantMsg {
                id: "m2".into(),
                ts: t(2),
                requests: vec![("tc-dup".into(), serde_json::json!({"v": "second"}))],
            },
            E::ToolStart {
                id: "s".into(),
                ts: t(3),
                tool_call_id: "tc-dup".into(),
                tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(),
                ts: t(4),
                tool_call_id: "tc-dup".into(),
            },
            E::TurnEnd {
                id: "te".into(),
                ts: t(5),
            },
        ];
        let ep = derive_episodes(&events, &meta());
        let bash = ep.tools.get("bash").expect("bash episode present");
        assert_eq!(
            bash.calls[0].arguments,
            Some(serde_json::json!({"v": "first"})),
            "first-wins on duplicate tool_call_id"
        );
    }

    #[test]
    fn args_attached_when_assistant_msg_arrives_after_tool_close() {
        // Defensive: derive PASS 0 walks ALL events first, so the ordering
        // of AssistantMsg vs ToolStart/ToolEnd should not matter.
        let events = vec![
            E::TurnStart {
                id: "t".into(),
                ts: t(0),
            },
            E::ToolStart {
                id: "s".into(),
                ts: t(1),
                tool_call_id: "tc-late".into(),
                tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(),
                ts: t(2),
                tool_call_id: "tc-late".into(),
            },
            E::AssistantMsg {
                id: "m".into(),
                ts: t(3),
                requests: vec![("tc-late".into(), serde_json::json!({"late": true}))],
            },
            E::TurnEnd {
                id: "te".into(),
                ts: t(4),
            },
        ];
        let ep = derive_episodes(&events, &meta());
        let bash = ep.tools.get("bash").expect("bash episode present");
        assert_eq!(
            bash.calls[0].arguments,
            Some(serde_json::json!({"late": true})),
            "PASS 0 must collect args before PASS 1 walks state machine"
        );
    }
}
