# Turn Metadata Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Populate `Turn.model` / `Turn.mode` / `Turn.output_tokens` from real wire-format data so `agentprof analyze` shows real values today and M1.5 ROI computations have inputs to operate on.

**Architecture:** 3 new `Event` trait methods (`payload_model` / `payload_output_tokens` / `payload_mode`) with default `None`, mirroring ADR-0005 D-1. `CopilotEvent` overrides for `AssistantMessage` + `ModeChanged`. `derive_episodes` gains a new `on_assistant_message` handler (sums output_tokens, last-wins for model), threads `current_mode` through `DeriveState` (updated by `on_mode_event`, captured at `on_turn_start`).

**Tech Stack:** Rust 2021, `chrono::Duration`, `serde` derive (no new deps).

**Spec:** `docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md`

---

## File structure

| File | Action | Responsibility |
|---|---|---|
| `crates/agentprof-core/src/adapter.rs` | Modify (extend `Event` trait) | Add 3 new methods with default `None` |
| `crates/agentprof-adapters/src/copilot/event.rs` | Modify (extend `CopilotEvent` impl + trait impl + tests) | Override 3 methods for `AssistantMessage` / `ModeChanged` |
| `crates/agentprof-core/src/episode/derive.rs` | Modify (add handler + state field + 4 sites) | New `on_assistant_message`; update `on_mode_event` + `on_turn_start`; add `current_mode` state field; add dispatch arm |
| `crates/agentprof-adapters/tests/snapshots/episode_derive__*.snap` | Re-accept (mechanical, ~10 files) | `model` / `output_tokens` / `mode` columns flip from `null` to real values |
| `crates/agentprof-adapters/tests/snapshots/analyzer_on_fixtures__*.snap` | Re-accept (mechanical, ~10 files) | Same — downstream of episode snapshots |
| `crates/agentprof-cli/tests/cli.rs` | Append 1 new integration test | Lock the `minimal` fixture's `output_tokens == 10` end-to-end |
| `docs/internals/adr-0005-analyzer-and-payload-name.md` | Modify (append Update §5) | Document trait extension for these 3 fields |
| `CHANGELOG.md` | Modify (append to `### Fixed` in `[Unreleased]`) | User-facing summary |

---

## Task 1: Extend `Event` trait with 3 new methods

**Files:**
- Modify: `crates/agentprof-core/src/adapter.rs`

- [ ] **Step 1.1: Add the 3 new trait methods after `payload_name`**

Open `crates/agentprof-core/src/adapter.rs`. Find the `pub trait Event { ... }` block. After the `payload_name` method (which currently ends with the closing `}` before the trait's own closing `}`), add three new methods.

The current end of the trait looks like:
```rust
    fn payload_name(&self) -> Option<&str> {
        None
    }
}
```

Replace that closing `}` of `payload_name` and the trait with:
```rust
    fn payload_name(&self) -> Option<&str> {
        None
    }

    /// Adapter-specific model identifier for the AI provider that produced
    /// this event. Returns `Some` for variants whose payload carries a
    /// model name (e.g. `AssistantMessage` in `CopilotEvent`), `None`
    /// otherwise.
    ///
    /// Used by `derive_episodes` to populate `Turn.model` (last-wins
    /// across assistant messages within a turn). M1.5 ROI computations
    /// will use this for per-token price lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    ///     // payload_model() inherits the default `None` impl.
    /// }
    /// assert_eq!(StubEvent.payload_model(), None);
    /// ```
    fn payload_model(&self) -> Option<&str> {
        None
    }

    /// Adapter-specific output token count for events that report it
    /// (e.g. `AssistantMessage` in `CopilotEvent`). Returns `None` for
    /// other variants.
    ///
    /// Used by `derive_episodes` to populate `Turn.output_tokens`
    /// (saturating sum across assistant messages within a turn). M1.5 ROI
    /// computations will use this for per-message cost calculation.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    /// }
    /// assert_eq!(StubEvent.payload_output_tokens(), None);
    /// ```
    fn payload_output_tokens(&self) -> Option<u32> {
        None
    }

    /// Adapter-specific new mode string for mode-transition events
    /// (e.g. `ModeChanged` in `CopilotEvent`). Returns `None` for variants
    /// without a mode payload.
    ///
    /// Used by `derive_episodes` to track the active session mode and
    /// attribute it to subsequently-opened turns. The string is converted
    /// to [`crate::episode::Mode`] via `Mode::from_wire`.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::{Event, EventKind};
    /// use chrono::Utc;
    ///
    /// struct StubEvent;
    /// impl Event for StubEvent {
    ///     fn id(&self) -> &str { "x" }
    ///     fn kind(&self) -> EventKind { EventKind::Unknown }
    ///     fn timestamp(&self) -> chrono::DateTime<Utc> { Utc::now() }
    ///     fn parent_id(&self) -> Option<&str> { None }
    /// }
    /// assert_eq!(StubEvent.payload_mode(), None);
    /// ```
    fn payload_mode(&self) -> Option<&str> {
        None
    }
}
```

- [ ] **Step 1.2: Add unit tests for the 3 default `None` implementations**

In `crates/agentprof-core/src/adapter.rs`, find the existing `#[cfg(test)] mod tests` block. The end of the existing block has a test like `default_payload_name_is_none` for the trait's `payload_name` default. After that test (before the closing `}` of the `mod tests`), add:

```rust
    #[test]
    fn default_payload_model_is_none() {
        struct DefaultPayloadModelEvent;
        impl Event for DefaultPayloadModelEvent {
            fn id(&self) -> &'static str {
                "e"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> DateTime<Utc> {
                Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(DefaultPayloadModelEvent.payload_model(), None);
    }

    #[test]
    fn default_payload_output_tokens_is_none() {
        struct DefaultPayloadTokensEvent;
        impl Event for DefaultPayloadTokensEvent {
            fn id(&self) -> &'static str {
                "e"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> DateTime<Utc> {
                Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(DefaultPayloadTokensEvent.payload_output_tokens(), None);
    }

    #[test]
    fn default_payload_mode_is_none() {
        struct DefaultPayloadModeEvent;
        impl Event for DefaultPayloadModeEvent {
            fn id(&self) -> &'static str {
                "e"
            }
            fn kind(&self) -> EventKind {
                EventKind::Unknown
            }
            fn timestamp(&self) -> DateTime<Utc> {
                Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap()
            }
            fn parent_id(&self) -> Option<&str> {
                None
            }
        }
        assert_eq!(DefaultPayloadModeEvent.payload_mode(), None);
    }
```

> **Note**: The `&'static str` return on `id()` for these stubs is required by clippy's `unnecessary_literal_bound` lint when returning a `&str` literal directly (a known pattern from Task 1 of M1.4).

- [ ] **Step 1.3: Run tests + gates**

```bash
cd /path/to/agentprof
cargo fmt --all
cargo test -p agentprof-core --lib adapter 2>&1 | tail -10
cargo test -p agentprof-core --doc 2>&1 | tail -5
cargo clippy --workspace --all-features --all-targets -- -D warnings 2>&1 | tail -5
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-core 2>&1 | tail -3
```

Expected:
- 3 new unit tests pass (`default_payload_model_is_none`, `default_payload_output_tokens_is_none`, `default_payload_mode_is_none`)
- 3 new doctests pass (one per trait method)
- clippy clean
- rustdoc clean

- [ ] **Step 1.4: Commit Task 1**

```bash
git add crates/agentprof-core/src/adapter.rs
git commit -m "feat(core): extend Event trait with payload_model / output_tokens / mode

Phase 1 of turn-metadata-extraction. Three new trait methods, all with
default None, mirroring the D-1 pattern established in ADR-0005:

- payload_model() -> Option<&str>
- payload_output_tokens() -> Option<u32>
- payload_mode() -> Option<&str>

These are the inputs derive_episodes needs to populate Turn.model /
Turn.output_tokens / Turn.mode (currently always None). M1.5 ROI
computations consume the same trio (price lookup by model, total cost
from output_tokens, ROI interpretation context from mode).

Default None lets future Codex/Claude adapters compile unchanged;
they get None-valued cells until they override. Per the audit decision
locked in the spec, no DeriveWarning::PayloadXxxMissing variants are
added — unlike payload_name (where missing → opaque-UUID episodes),
these three fields' absence only produces None cells, which is itself
a clear user signal.

3 new doctests + 3 new unit tests verify the default behavior. No
behavior change yet — CopilotEvent overrides land in Task 2.

Refs:
- docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md FR-1, FR-2, FR-3
- docs/internals/adr-0005-analyzer-and-payload-name.md D-1 (pattern this follows)

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 2: Implement the 3 methods on `CopilotEvent`

**Files:**
- Modify: `crates/agentprof-adapters/src/copilot/event.rs`

- [ ] **Step 2.1: Add inherent `payload_model` / `payload_output_tokens` / `payload_mode` methods after `payload_name`**

Find the inherent `payload_name` method on `CopilotEvent` (around line 1180). Immediately after the closing `}` of `payload_name`, BEFORE the closing `}` of the inherent `impl CopilotEvent` block, add:

```rust
    /// Returns the model identifier from the payload for variants that
    /// have one:
    /// - `AssistantMessage` → `data.model`
    /// - All other variants → `None`
    #[must_use]
    pub fn payload_model(&self) -> Option<&str> {
        match self {
            Self::AssistantMessage(env) => Some(env.data.model.as_str()),
            _ => None,
        }
    }

    /// Returns the output token count from the payload for variants that
    /// report it:
    /// - `AssistantMessage` → `data.output_tokens`
    /// - All other variants → `None`
    #[must_use]
    pub fn payload_output_tokens(&self) -> Option<u32> {
        match self {
            Self::AssistantMessage(env) => Some(env.data.output_tokens),
            _ => None,
        }
    }

    /// Returns the new mode string from the payload for mode-transition
    /// variants:
    /// - `ModeChanged` → `data.new_mode`
    /// - All other variants → `None`
    ///
    /// `derive_episodes` converts this string into a
    /// [`agentprof_core::episode::Mode`] via `Mode::from_wire`.
    #[must_use]
    pub fn payload_mode(&self) -> Option<&str> {
        match self {
            Self::ModeChanged(env) => Some(env.data.new_mode.as_str()),
            _ => None,
        }
    }
```

- [ ] **Step 2.2: Add trait impl delegation**

Find the `impl agentprof_core::adapter::Event for CopilotEvent { ... }` block (around line 1205). After the existing `fn payload_name(&self) -> Option<&str> { self.payload_name() }` method, BEFORE the closing `}` of the trait impl, add:

```rust
    fn payload_model(&self) -> Option<&str> {
        self.payload_model()
    }
    fn payload_output_tokens(&self) -> Option<u32> {
        self.payload_output_tokens()
    }
    fn payload_mode(&self) -> Option<&str> {
        self.payload_mode()
    }
```

- [ ] **Step 2.3: Add unit tests for the new payload methods**

At the bottom of `crates/agentprof-adapters/src/copilot/event.rs`, find the existing `#[cfg(test)] mod payload_name_tests` block. After it (before the file's end), add a new test module:

```rust
#[cfg(test)]
mod payload_metadata_tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn envelope<D>(data: D) -> WithEnvelope<D> {
        WithEnvelope {
            id: "e".into(),
            timestamp: Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap(),
            parent_id: None,
            ephemeral: None,
            agent_id: None,
            data,
        }
    }

    fn assistant_message(model: &str, output_tokens: u32) -> CopilotEvent {
        CopilotEvent::AssistantMessage(envelope(AssistantMessageData {
            message_id: "m".into(),
            model: model.into(),
            content: String::new(),
            tool_requests: Vec::new(),
            interaction_id: "i".into(),
            turn_id: "0".into(),
            reasoning_opaque: None,
            reasoning_text: None,
            encrypted_content: None,
            output_tokens,
            request_id: None,
            service_request_id: None,
        }))
    }

    #[test]
    fn assistant_message_returns_model() {
        let ev = assistant_message("claude-opus-4.7", 412);
        assert_eq!(ev.payload_model(), Some("claude-opus-4.7"));
    }

    #[test]
    fn assistant_message_returns_output_tokens() {
        let ev = assistant_message("gpt-5-mini", 88);
        assert_eq!(ev.payload_output_tokens(), Some(88));
    }

    #[test]
    fn mode_changed_returns_new_mode() {
        let ev = CopilotEvent::ModeChanged(envelope(ModeChangeData {
            previous_mode: "ask".into(),
            new_mode: "auto".into(),
        }));
        assert_eq!(ev.payload_mode(), Some("auto"));
    }

    #[test]
    fn non_assistant_message_has_no_model_or_tokens() {
        let ev = CopilotEvent::Unknown;
        assert_eq!(ev.payload_model(), None);
        assert_eq!(ev.payload_output_tokens(), None);
        assert_eq!(ev.payload_mode(), None);
    }

    #[test]
    fn mode_unchanged_payloads_return_none_for_mode() {
        // ModelChange has a payload but it's not a mode-transition event.
        let ev = CopilotEvent::ModelChange(envelope(ModelChangeData {
            new_model: "gpt-5".into(),
        }));
        assert_eq!(ev.payload_mode(), None);
        // ModelChange ALSO doesn't carry payload_model — only
        // AssistantMessage does. ModelChange just announces a switch.
        assert_eq!(ev.payload_model(), None);
    }
}
```

- [ ] **Step 2.4: Run tests + gates**

```bash
cd /path/to/agentprof
cargo fmt --all
cargo test -p agentprof-adapters --lib payload_metadata 2>&1 | tail -10
cargo clippy --workspace --all-features --all-targets -- -D warnings 2>&1 | tail -5
cargo test --workspace --all-features 2>&1 | tail -10
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace 2>&1 | tail -3
```

Expected:
- 5 new unit tests pass in `payload_metadata_tests` module
- all existing tests still pass
- clippy clean
- rustdoc clean

> **Note**: This task should NOT touch any snapshot. `derive_episodes` still ignores these new methods until Task 3 wires them in. The Event trait surface change is purely additive.

- [ ] **Step 2.5: Commit Task 2**

```bash
git add crates/agentprof-adapters/src/copilot/event.rs
git commit -m "feat(adapters): implement payload_model/output_tokens/mode for CopilotEvent

Phase 2 of turn-metadata-extraction. CopilotEvent now reads three
metadata fields from its wire payloads:

- AssistantMessage.data.model       → payload_model()
- AssistantMessage.data.output_tokens → payload_output_tokens()
- ModeChanged.data.new_mode         → payload_mode()

All other variants return None (inherited from Event trait defaults).
ModelChange explicitly returns None for payload_mode (it announces a
model switch, not a mode switch) and None for payload_model (only
AssistantMessage attributes a per-message model — ModelChange just
advertises 'we will use X going forward', no message-level cost).

5 unit tests cover: positive cases (3 fields), Unknown variant
baseline, and the subtle ModelChange-vs-ModeChange disambiguation.

derive_episodes still doesn't read these methods — that wiring lands
in Task 3. This commit is a pure trait-surface extension; no
behavior change in analyze output yet.

Refs:
- docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md §4.2

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 3: Wire `derive_episodes` to populate `Turn.model` / `output_tokens` / `mode`

**Files:**
- Modify: `crates/agentprof-core/src/episode/derive.rs`

- [ ] **Step 3.1: Add `current_mode` field to `DeriveState`**

Find the `struct DeriveState { ... }` declaration around line 91. Add a new field after `mode_segments`:

Current:
```rust
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
```

Change to:
```rust
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
    /// Active session mode tracked across the event stream. Updated by
    /// [`Self::on_mode_event`] when it sees `ModeChanged` events with a
    /// non-None [`Event::payload_mode`]. Captured at [`Self::on_turn_start`]
    /// and written into the new `Turn.mode` field. `None` until the first
    /// mode_changed event arrives.
    current_mode: Option<Mode>,
    aborts: Vec<AbortInfo>,
    warnings: Vec<DeriveWarning>,
}
```

- [ ] **Step 3.2: Initialize `current_mode` to `None` in `DeriveState::new`**

Find `impl DeriveState { fn new(meta: &SessionMeta) -> Self { ... } }` around line 141. Add `current_mode: None,` in the struct literal after `mode_segments`:

Current:
```rust
            mode_segments: vec![ModeSegment::new(
                Mode::Unknown("default".into()),
                meta.started_at,
            )],
            aborts: Vec::new(),
            warnings: Vec::new(),
```

Change to:
```rust
            mode_segments: vec![ModeSegment::new(
                Mode::Unknown("default".into()),
                meta.started_at,
            )],
            current_mode: None,
            aborts: Vec::new(),
            warnings: Vec::new(),
```

- [ ] **Step 3.3: Update `on_turn_start` to capture `current_mode`**

Find `fn on_turn_start<E: Event>(&mut self, ev: &E)` (around line 206). Replace:

```rust
    fn on_turn_start<E: Event>(&mut self, ev: &E) {
        let turn = Turn::new(ev.id().to_string(), ev.timestamp());
        self.turns.push(turn);
        self.open_turn_idx = Some(self.turns.len() - 1);
    }
```

With:

```rust
    fn on_turn_start<E: Event>(&mut self, ev: &E) {
        let mut turn = Turn::new(ev.id().to_string(), ev.timestamp());
        // Attribute the currently-active mode to this turn. If no
        // session.mode_changed event has been seen yet, mode stays None
        // (per FR-7). Mode-changes that happen mid-turn DON'T retroactively
        // update this turn's mode — only subsequent turns see the new
        // mode. Matches user intuition: 'this turn was started in X mode'.
        turn.mode = self.current_mode.clone();
        self.turns.push(turn);
        self.open_turn_idx = Some(self.turns.len() - 1);
    }
```

- [ ] **Step 3.4: Add new `on_assistant_message` handler**

Find `fn on_turn_end<E: Event>(&mut self, ev: &E)` (around line 212). Immediately BEFORE `on_turn_end`, add a new handler:

```rust
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
            turn.output_tokens = Some(
                turn.output_tokens
                    .unwrap_or(0)
                    .saturating_add(tokens),
            );
        }
    }
```

- [ ] **Step 3.5: Replace `on_mode_event` body to read real mode value**

Find `fn on_mode_event<E: Event>(&mut self, ev: &E)` around line 428. Replace the entire body:

Current:
```rust
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
```

With:
```rust
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
```

- [ ] **Step 3.6: Add dispatch arm for `EventKind::AssistantMessage`**

Find the dispatch `match ev.kind()` block in `derive_episodes` (around line 86). Add a new arm before `EventKind::ModeChanged | EventKind::ModelChange`:

Current:
```rust
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
```

Change to:
```rust
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
```

> **Note**: The comment changes too — `*Message` is no longer accurate since `AssistantMessage` now has a handler. Specify the remaining ones: `UserMessage`, `SystemMessage`.

- [ ] **Step 3.7: Add 4 unit tests in the existing `mod tests` block**

Find the existing `#[cfg(test)] mod tests` block at the bottom of `derive.rs`. After the last existing test (currently `payload_name_missing_warning_fires_when_adapter_returns_none`), before the closing `}` of `mod tests`, add 4 new tests.

The existing `E` test stub (a unit struct that implements `Event` with default `payload_*` impls) won't help here because we need to override `payload_model` / `payload_output_tokens` / `payload_mode` per test. Define a richer stub inline:

```rust
    /// Richer test stub that lets each test customize the payload methods.
    /// (The simpler `E` struct above doesn't override payload methods —
    /// all default to None.)
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
        // Sequence: mode→ask, turn-A opens, mode→auto mid-turn, turn-A
        // ends, turn-B opens, turn-B ends. Expected:
        //   turn-A.mode = Some(Ask)   (captured at turn_start; not retroactively updated)
        //   turn-B.mode = Some(Auto)  (captures the new current_mode)
        let events = vec![
            mode_change("ask", 1),
            turn_start("t-A", 2),
            mode_change("auto", 3),
            turn_end(4),
            turn_start("t-B", 5),
            turn_end(6),
        ];
        let ep = derive_episodes(&events, &meta());
        assert_eq!(ep.turns.len(), 2);
        assert_eq!(ep.turns[0].mode, Some(Mode::Ask));
        assert_eq!(ep.turns[1].mode, Some(Mode::Auto));
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
```

- [ ] **Step 3.8: Run unit tests + gates**

```bash
cd /path/to/agentprof
cargo fmt --all
cargo test -p agentprof-core --lib episode::derive::tests 2>&1 | tail -15
cargo clippy --workspace --all-features --all-targets -- -D warnings 2>&1 | tail -5
```

Expected:
- 4 new unit tests pass: `assistant_message_populates_model_and_output_tokens`, `multiple_assistant_messages_sum_output_tokens_and_last_wins_model`, `mode_change_attributes_to_next_turn_not_current`, `turn_without_assistant_message_has_none_model_and_tokens`
- existing 8+ derive tests still pass
- clippy clean

**Snapshot tests in `agentprof-adapters` will FAIL** at this point — that's expected. Task 4 re-accepts them.

```bash
cargo test --workspace --all-features 2>&1 | grep -E "FAILED|test result:" | head -20
```

Expected: `agentprof-adapters` episode_derive + analyzer_on_fixtures snapshot tests fail (model/mode/output_tokens columns now have real values where snapshots expected null). All other tests pass.

- [ ] **Step 3.9: Commit Task 3**

```bash
git add crates/agentprof-core/src/episode/derive.rs
git commit -m "feat(core): populate Turn.model / output_tokens / mode in derive_episodes

Phase 3 of turn-metadata-extraction — the moment 'agentprof analyze'
starts showing real values in the Model / Mode / Out-Tokens columns
instead of '—'.

Four coupled changes to derive.rs:

1. DeriveState gains a current_mode: Option<Mode> field, tracking the
   active session mode across the event stream. Initialized to None;
   updated by on_mode_event when a ModeChanged event carries a non-None
   payload_mode().

2. New on_assistant_message handler reads payload_model + payload_output_tokens
   from each AssistantMessage event and writes them to the current open
   turn:
   - turn.model: last-wins (mid-turn model switch is rare; final
     message's model is the effective one)
   - turn.output_tokens: saturating sum (M1.5 ROI cost formula needs
     turn total across all messages)
   If no turn is open (data anomaly — assistant.message before
   turn_start), silently ignore.

3. on_mode_event now reads ev.payload_mode() instead of pushing a hard-coded
   Mode::Unknown('changed') segment. The PLACEHOLDER for 'Task 10b will
   read actual mode value' is now resolved. ModelChange events still close
   the previous segment but don't push a new one (they don't change the
   mode, only the active model).

4. on_turn_start now captures self.current_mode.clone() into turn.mode.
   Mode-changes mid-turn do NOT retroactively update the current turn —
   only subsequent turns see the new mode (matches user intuition:
   'this turn was started in X mode').

Dispatch table gains: EventKind::AssistantMessage => state.on_assistant_message(ev)

4 new unit tests cover: single-message attribution, multi-message sum +
last-wins model, mode-change-mid-turn semantics, turn-without-assistant-message
defensive None.

Snapshot tests in agentprof-adapters NOW FAIL — model/mode/output_tokens
fields flip from null to real values for any fixture that has
AssistantMessage / ModeChanged events. Task 4 re-accepts them.

Refs:
- docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md FR-4..FR-8
- ADR-0005 Update §5 to be written in Task 6

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 4: Re-accept snapshots

**Files:**
- Modify: `crates/agentprof-adapters/tests/snapshots/episode_derive__*.snap` (~10 files; mechanical)
- Modify: `crates/agentprof-adapters/tests/snapshots/analyzer_on_fixtures__*.snap` (~10 files; mechanical)

- [ ] **Step 4.1: Run snapshot tests with `INSTA_UPDATE=always` to regenerate**

```bash
cd /path/to/agentprof
INSTA_UPDATE=always cargo test -p agentprof-adapters 2>&1 | tail -20
```

Expected: all tests pass. Snapshot files updated in place.

- [ ] **Step 4.2: Verify no `.snap.new` residuals**

```bash
find crates/agentprof-adapters/tests/snapshots -name "*.snap.new"
```

Expected: zero output. If there are leftovers, delete them:
```bash
find crates/agentprof-adapters/tests/snapshots -name "*.snap.new" -delete
```

- [ ] **Step 4.3: Spot-check the `minimal` fixture's snapshot to verify model + output_tokens populated**

```bash
grep -E '"model":|"output_tokens":' \
  crates/agentprof-adapters/tests/snapshots/episode_derive__episode__minimal.snap \
  | head -5
```

Expected output (the values for the single turn in `minimal/`):
```
      "model": "gpt-5-mini",
      "output_tokens": 10,
```

Note: was previously `"model": null` and `"output_tokens": null`.

- [ ] **Step 4.4: Spot-check the `with-mode-transitions` fixture for non-None mode values**

```bash
grep -B 1 -A 3 '"mode":' \
  crates/agentprof-adapters/tests/snapshots/episode_derive__episode__with-mode-transitions.snap \
  | head -20
```

Expected: at least one turn has `"mode": "Ask"`, `"mode": "Auto"`, or `"mode": {"Unknown": "..."}` (non-null) — depending on what mode_changed events the fixture has.

- [ ] **Step 4.5: Spot-check the analyzer snapshot (downstream propagation)**

```bash
grep -E '"model":|"output_tokens":' \
  crates/agentprof-adapters/tests/snapshots/analyzer_on_fixtures__analysis__minimal.snap \
  | head -5
```

Expected: same values as the episode snapshot (model + output_tokens propagate through analyze()).

- [ ] **Step 4.6: Re-run the workspace test suite to confirm everything green**

```bash
cargo test --workspace --all-features 2>&1 | grep "test result:" \
  | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
```

Expected: passed: 220+ (was 214 before this task; +6 from Tasks 1-3); failed: 0.

- [ ] **Step 4.7: Commit Task 4**

```bash
git add crates/agentprof-adapters/tests/snapshots/
git commit -m "test(adapters): re-accept snapshots with populated turn metadata

Phase 4 of turn-metadata-extraction. Mechanical snapshot re-acceptance
after Task 3 wired Turn.model / Turn.output_tokens / Turn.mode in
derive_episodes.

Per-fixture changes (model and output_tokens columns flipping from null
to real values for turns containing assistant.message events):
- minimal: model='gpt-5-mini', output_tokens=10 (was both null)
- builtin-tools-only, with-mcp-calls, with-skill-invoked,
  with-hooks-heavy, with-aborts, with-mode-transitions, live-truncated,
  cross-turn-tool, orphan-events: each turn now reflects its actual
  assistant.message model + summed output_tokens.
- with-mode-transitions: mode column also populated (Ask/Auto/Expert)
  from session.mode_changed events.

Both episode_derive__*.snap and analyzer_on_fixtures__*.snap snapshots
updated (analyzer rollups propagate the new turn metadata into
turn_summary rows).

Hand-verified:
- minimal: model + output_tokens visible in both episode + analyzer snaps
- with-mode-transitions: mode values populated across turns
- No spurious changes elsewhere

Refs:
- docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md FR-10..FR-12

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 5: CLI integration test for `minimal` fixture's output_tokens

**Files:**
- Modify: `crates/agentprof-cli/tests/cli.rs` (append 1 test)

- [ ] **Step 5.1: Add integration test asserting end-to-end value flow**

Open `crates/agentprof-cli/tests/cli.rs`. Find the last test in the file (currently `analyze_unsupported_agent_exits_with_friendly_message`). After its closing `}`, append:

```rust
#[test]
fn analyze_minimal_fixture_populates_turn_metadata_in_json() {
    // Regression test for turn-metadata-extraction: the `minimal` fixture
    // has an assistant.message with model='gpt-5-mini' + outputTokens=10.
    // Before turn-metadata-extraction, Turn.model and Turn.output_tokens
    // were always None (despite the data being in the wire format).
    //
    // This locks the end-to-end pipeline: parser reads the field →
    // CopilotEvent::payload_model/output_tokens returns Some →
    // derive_episodes' on_assistant_message writes to Turn →
    // analyzer's turn_summary copies into TurnSummaryRow →
    // json renderer serializes → JSON contains the real values.
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(fixtures_root().join("minimal"))
        .args(["--export", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&s).expect("output must be valid JSON");

    let turns = parsed["turn_summary"].as_array().unwrap();
    assert_eq!(turns.len(), 1, "minimal has exactly 1 turn");
    assert_eq!(
        turns[0]["model"], "gpt-5-mini",
        "Turn.model must come from assistant.message.data.model"
    );
    assert_eq!(
        turns[0]["output_tokens"], 10,
        "Turn.output_tokens must come from assistant.message.data.output_tokens"
    );
}
```

- [ ] **Step 5.2: Run the new test**

```bash
cd /path/to/agentprof
cargo test -p agentprof-cli --test cli analyze_minimal_fixture_populates 2>&1 | tail -8
```

Expected: 1 test passes.

- [ ] **Step 5.3: Run the full CLI test file to confirm no regression**

```bash
cargo test -p agentprof-cli --test cli 2>&1 | tail -15
```

Expected: 10 tests pass (was 9 before this task; +1 new).

- [ ] **Step 5.4: Run all gates**

```bash
cargo fmt --all
cargo clippy --workspace --all-features --all-targets -- -D warnings 2>&1 | tail -5
cargo test --workspace --all-features 2>&1 | grep "test result:" | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace 2>&1 | tail -3
```

Expected: 221+ passed / 0 failed; clippy + rustdoc clean.

- [ ] **Step 5.5: Commit Task 5**

```bash
git add crates/agentprof-cli/tests/cli.rs
git commit -m "test(cli): minimal fixture turn_summary[0] has model + output_tokens

Phase 5 of turn-metadata-extraction. End-to-end regression guard for
the full pipeline:

  jsonl parser → CopilotEvent::payload_* → derive_episodes' Turn fields
  → analyzer's TurnSummaryRow → json renderer → user-visible output

Asserts the minimal fixture's single turn now exposes:
  model = 'gpt-5-mini'
  output_tokens = 10

Before this milestone these were both null in the JSON output despite
the assistant.message event in minimal/events.jsonl line 4 carrying the
data. The single assertion catches regressions in any of the 5 layers.

10 cli integration tests now (was 9 + 1 new).

Refs:
- docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md §6 (Testing)

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 6: Docs sync — ADR-0005 Update §5 + CHANGELOG

**Files:**
- Modify: `docs/internals/adr-0005-analyzer-and-payload-name.md` (append Update §5)
- Modify: `CHANGELOG.md` (append to `[Unreleased]` `### Fixed`)

- [ ] **Step 6.1: Append Update §5 to ADR-0005**

Open `docs/internals/adr-0005-analyzer-and-payload-name.md`. The file currently ends with Update §4 about percentile precision. Append at the very end (after the closing line of Update §4):

```markdown

### Update §5: Turn metadata extraction (`payload_model` / `payload_output_tokens` / `payload_mode`)

**Context.** The M1.4 audit (Part 4) verified that `Turn.model` / `Turn.mode` / `Turn.output_tokens` fields exist with correct types per spec FR-2.2, but did not check whether they're actually populated from real wire data. User-facing inspection (2026-05-29) revealed all three columns show `—` in the `agentprof analyze` Markdown table for every fixture and every real Copilot session — the `derive_episodes` algorithm never wrote to these fields after `Turn::new()` initialized them to `None`.

The M1.5 milestone deliverables (per-tool ROI scoring, waste-estimate-USD, cross-session aggregation) all consume these three fields:
- `model` → price-per-token lookup + tokenizer choice
- `output_tokens` → `session_total_cost = Σ output_tokens × output_price[model]`
- `mode` → ROI interpretation context (`auto` vs `ask` modes have different ROI thresholds)

Shipping M1.4 without populating these would have forced M1.5 to do a foundational data-plumbing pass first.

**Decision.** Extend the `Event` trait with three new methods, all with default `None`, following the same pattern as D-1 (`payload_name`):

```rust
fn payload_model(&self) -> Option<&str> { None }
fn payload_output_tokens(&self) -> Option<u32> { None }
fn payload_mode(&self) -> Option<&str> { None }
```

`CopilotEvent` overrides:
- `AssistantMessage(env)` → `payload_model = Some(env.data.model)`, `payload_output_tokens = Some(env.data.output_tokens)`
- `ModeChanged(env)` → `payload_mode = Some(env.data.new_mode)`
- All other variants → trait defaults (`None`)

**Aggregation strategies in `derive_episodes`** (locked by this Update):
- `Turn.output_tokens`: **saturating sum** across all `assistant.message` events in a turn. M1.5 ROI formula requires turn total.
- `Turn.model`: **last-wins** across messages. Mid-turn model switches are rare but possible; the final message's model is the effective one.
- `Turn.mode`: **captured at `turn_start`** from `DeriveState.current_mode` (which tracks the latest `session.mode_changed` event). Mode changes mid-turn don't retroactively update the current turn — only subsequent turns see the new mode.

**Implementation.** A new `DeriveState.current_mode: Option<Mode>` field threads through the algorithm. `on_mode_event` now reads the actual mode from `ev.payload_mode()` (replacing the M1.3 PLACEHOLDER `Mode::Unknown("changed")`). `on_turn_start` clones `current_mode` into the new turn. A new `on_assistant_message` handler writes `Turn.model` + `Turn.output_tokens`. Dispatch table gains `EventKind::AssistantMessage => state.on_assistant_message(ev)`.

**No new `DeriveWarning` variants.** Unlike `PayloadNameMissing` (where missing → opaque-UUID `ToolEpisode` keys, a real degradation), missing `model` / `output_tokens` / `mode` only produces `None`-valued cells — a clear user signal on its own. Adding `PayloadXxxMissing` for every metadata field would inflate the warning vocabulary without proportional value.

**Tests.** 3 trait-default unit tests (`adapter.rs`), 5 CopilotEvent override tests (`event.rs`), 4 `derive.rs` integration tests covering sum / last-wins / mode-mid-turn / defensive-no-message scenarios, snapshot re-acceptance for ~20 affected `.snap` files (episode + analyzer layers), 1 CLI end-to-end regression test asserting `minimal` fixture's `turn_summary[0].output_tokens == 10`.

**Status.** Spec `docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md` (Approved). This Update is the architectural record; the spec is the implementation contract.
```

- [ ] **Step 6.2: Add entry to CHANGELOG.md `[Unreleased]` → `### Fixed`**

Open `CHANGELOG.md`. Find the `### Fixed` subsection under `## [Unreleased]`. The existing entry is `#### M1.4 audit followups (fix/m1.4-audit-followups)`. Add a new entry IMMEDIATELY AFTER that block (before the `### Added` subsection):

```markdown

#### Turn metadata extraction (`feat/turn-metadata-extraction`)

Discovered while validating the M1.4 audit fixes by running `agentprof analyze` against the `minimal` fixture and a real local Copilot session. The Markdown report's **Model / Mode / Out-Tokens** columns were all `—` for every turn, despite the wire data carrying these fields (`AssistantMessageData.model`, `AssistantMessageData.output_tokens`, `ModeChangeData.new_mode`). Root cause: `derive_episodes` never read these payload fields — the existing `Turn` struct fields were initialized to `None` by `Turn::new()` and never written to. Spec FR-2.2 required only "fields exist and correctly typed", which the M1.4 audit verified as compliant — the audit had no obligation to check "fields populated with real data". This was a real audit / spec blind spot that surfaced immediately on first user inspection.

**`agentprof-core`:**
- `Event` trait extended with 3 new methods, all with default `None` (mirroring ADR-0005 D-1): `payload_model() -> Option<&str>`, `payload_output_tokens() -> Option<u32>`, `payload_mode() -> Option<&str>`.
- `DeriveState` gains a `current_mode: Option<Mode>` field tracking the active session mode across the event stream.
- New `on_assistant_message` handler populates `Turn.model` (last-wins across messages in a turn) and `Turn.output_tokens` (saturating sum). M1.5 ROI computations consume both.
- `on_mode_event` now reads `ev.payload_mode()` instead of pushing a hard-coded `Mode::Unknown("changed")` segment — the M1.3 PLACEHOLDER for "Task 10b will read actual mode value" is now resolved.
- `on_turn_start` captures `current_mode.clone()` into `turn.mode`. Mid-turn mode changes don't retroactively update the current turn (matches user intuition: "this turn was started in X mode").
- Dispatch table gains `EventKind::AssistantMessage => state.on_assistant_message(ev)`.

**`agentprof-adapters`:**
- `CopilotEvent` overrides the 3 new trait methods for `AssistantMessage` and `ModeChanged` variants. `ModelChange` deliberately returns `None` for both `payload_model` and `payload_mode` (it announces a model switch, not a per-message model or a mode change).

**Snapshots:**
- ~10 `episode_derive__*.snap` + ~10 `analyzer_on_fixtures__*.snap` re-accepted. `minimal` fixture now shows `model: "gpt-5-mini"`, `output_tokens: 10` (was both null). `with-mode-transitions` fixture shows populated `mode` values.

**Tests:**
- 3 unit tests for trait default `None` (adapter.rs)
- 5 unit tests for CopilotEvent overrides + ModelChange-vs-ModeChange disambiguation (event.rs)
- 4 unit tests for `derive.rs` aggregation semantics: single-message attribution, sum + last-wins, mode-mid-turn semantics, defensive no-message
- 1 CLI integration test asserting `minimal` fixture's `turn_summary[0].output_tokens == 10` end-to-end

**Out of scope (M1.5 deliverables):**
- Cost / ROI computation logic (price tables, per-model tokenizers, `--with-cost` flag)
- `agentprof aggregate` cross-session rollups
- This commit only provides the **inputs** M1.5 will consume.

**Test count delta:** 214 → ~226 (+12: 3 + 5 + 4 + 1, plus snapshot diffs which don't change count).
```

- [ ] **Step 6.3: Run rustdoc + final gates**

```bash
cd /path/to/agentprof
cargo fmt --all
cargo clippy --workspace --all-features --all-targets -- -D warnings 2>&1 | tail -5
cargo test --workspace --all-features 2>&1 | grep "test result:" | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace 2>&1 | tail -3
```

Expected: 221+ passed / 0 failed; clippy + rustdoc clean.

- [ ] **Step 6.4: Commit Task 6**

```bash
git add docs/internals/adr-0005-analyzer-and-payload-name.md CHANGELOG.md
git commit -m "docs: ADR-0005 Update §5 + CHANGELOG for turn metadata extraction

Phase 6 (final) of turn-metadata-extraction.

ADR-0005 gains 'Update §5: Turn metadata extraction (payload_model /
payload_output_tokens / payload_mode)' documenting the trait extension
+ aggregation strategy decisions:
- output_tokens: saturating sum (M1.5 ROI needs turn total)
- model: last-wins (effective model = final message's model)
- mode: captured at turn_start (mode changes don't retroactively
  update current turn)
- No new DeriveWarning variants (missing field = None cell is sufficient
  signal; not a degradation like payload_name was)

ADR Status stays 'Accepted' — this is an additive elaboration of D-1
(trait extension pattern), not a reversal.

CHANGELOG [Unreleased] '### Fixed' adds 'Turn metadata extraction'
block documenting:
- The audit / spec blind spot that caused this (FR-2.2 only required
  'fields exist + correctly typed', no population requirement)
- Core changes (trait extension + derive handler)
- Adapter changes (CopilotEvent overrides for AssistantMessage +
  ModeChanged)
- Snapshot re-acceptance scope
- Test count delta (214 → 226+)
- Out-of-scope items deferred to M1.5

After this commit feat/turn-metadata-extraction is feature-complete.
Ready for final review + finishing-a-development-branch.

Refs:
- docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md
- ADR-0005 Update §5

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Final Self-Review

Run this checklist after Task 6 commits. **Stop and fix anything that fails.**

### Spec FR coverage

| Spec FR | Task | Verified |
|---|---|---|
| FR-1 Event::payload_model default | Task 1 | Step 1.1 (method body) + Step 1.2 (default test) |
| FR-2 Event::payload_output_tokens default | Task 1 | Step 1.1 + Step 1.2 |
| FR-3 Event::payload_mode default | Task 1 | Step 1.1 + Step 1.2 |
| FR-4 derive populates Turn.model (last-wins) | Task 3 | Step 3.4 (handler) + Step 3.7 (test) |
| FR-5 derive populates Turn.output_tokens (sum) | Task 3 | Step 3.4 + Step 3.7 |
| FR-6 derive populates Turn.mode at turn_start | Task 3 | Step 3.3 (turn_start) + Step 3.5 (on_mode_event) + Step 3.7 (test) |
| FR-7 current_mode updates on session.mode_changed | Task 3 | Step 3.5 |
| FR-8 on_assistant_message ignores no-open-turn | Task 3 | Step 3.4 (early-return) |
| FR-9 No new DeriveWarning variants | Task 3 | No warning emission in on_assistant_message body |
| FR-10 Snapshots re-accepted | Task 4 | All 7 steps |
| FR-11 minimal fixture shows model + output_tokens | Task 4 (spot-check) + Task 5 (assert) | Step 4.3 + Step 5.1 |
| FR-12 with-mode-transitions has non-None mode | Task 4 | Step 4.4 |

All 12 FRs covered.

### Placeholder scan

```bash
grep -nE 'TBD|TODO|XXX|FIXME|implement later|fill in details|similar to Task' \
    docs/superpowers/plans/2026-05-29-turn-metadata-extraction.md
```

Expected: zero matches.

### Type consistency check

| Type/Symbol | Defined in | Used in |
|---|---|---|
| `Event::payload_model()` | Task 1 (trait) | Task 2 (CopilotEvent impl), Task 3 (derive handler) |
| `Event::payload_output_tokens()` | Task 1 | Task 2, Task 3 |
| `Event::payload_mode()` | Task 1 | Task 2, Task 3 |
| `DeriveState.current_mode: Option<Mode>` | Task 3 Step 3.1 | Task 3 Step 3.3, 3.5 |
| `MetadataE` test stub | Task 3 Step 3.7 | Task 3 Step 3.7 tests only |
| `Turn.model` / `Turn.output_tokens` / `Turn.mode` | (already exists in `episode/turn.rs`) | Task 3 writers, Task 4 snapshot diffs, Task 5 test |
| `assistant.message.data.model` (`AssistantMessageData.model`) | (already in `event.rs:369-422`) | Task 2 |
| `assistant.message.data.output_tokens` (`AssistantMessageData.output_tokens: u32`) | (already in `event.rs:369-422`) | Task 2 |
| `ModeChangeData.new_mode` | (already in `event.rs:231-244`) | Task 2 |
| `Mode::from_wire(s)` | (already in `episode/mode_segment.rs`) | Task 3 Step 3.5 |

All references resolve.

### Final acceptance gates

```bash
cd /path/to/agentprof
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -D warnings 2>&1 | tail -5
cargo test --workspace --all-features 2>&1 | grep "test result:" | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace 2>&1 | tail -3
git --no-pager log --oneline main..HEAD | wc -l   # expect 6
git status --short                                  # expect empty
```

All gates clean. Branch ready for merge.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-29-turn-metadata-extraction.md`.

Two execution options:

**1. Subagent-Driven (recommended)** — Fresh subagent per task, two-stage review after each, fast iteration. Same model (`claude-opus-4.7-1m-internal`) throughout per user preference. Same workflow as M1.4.

**2. Inline Execution** — Execute tasks in this session using `executing-plans`, batch execution with manual review checkpoints.

**Notes for either approach:**

- Tasks 1, 2, 3 are sequentially dependent (each builds on the trait/types established by the previous). Tasks 4, 5, 6 are independent of each other but depend on Task 3.
- Task 3 is the largest (3 sub-steps in one file, 4 new tests). Worth its own subagent invocation; reviewer should focus on the `on_mode_event` semantic change + the mode-mid-turn invariant.
- Task 4 (snapshot re-acceptance) is mechanical but high-volume. Subagent should hand-verify at least the `minimal` and `with-mode-transitions` snapshots before bulk-accepting the rest.
- Estimated total: ~150-200 lines code + ~80 lines tests + ~20 snapshot file diffs (mechanical) + ~80 lines docs = ~6 commits.
