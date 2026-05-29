---
title: "Turn metadata extraction — model / mode / output_tokens"
status: "Approved"
date: "2026-05-29"
milestone: "M1.4 post-audit P1 fix (ROI prep for M1.5)"
authors: "@verdenmax (project owner), AI assistant (Copilot CLI)"
tags: ["spec", "agentprof-core", "agentprof-adapters", "derive_episodes", "Event-trait", "ROI"]
---

# Turn metadata extraction — `model` / `mode` / `output_tokens`

## 1. Problem statement

Run `agentprof analyze` against any committed fixture or real Copilot session and you'll see this in the Turn Summary table:

```
| # | Turn ID | Status    | Duration | Model | Mode | Tools | Hooks | Skills | Out-Tokens |
| 1 | turn-A  | Completed | 2.00s    | —     | —    | 1     | 0     | 0      | —          |
```

The **Model / Mode / Out-Tokens columns are always `—`**. Not because the data is missing — the wire format carries it (`AssistantMessageData.model`, `AssistantMessageData.output_tokens`, `ModeChangeData.new_mode` are all `required` fields per `crates/agentprof-adapters/src/copilot/event.rs`). The columns are blank because `derive_episodes` **never reads these fields**.

### Confirmation
```bash
grep -n '\.model =\|\.mode =\|\.output_tokens =' crates/agentprof-core/src/episode/derive.rs
# (zero matches — these Turn fields are never assigned after Turn::new() initializes them to None)
```

### Why this matters

The 3 missing fields are the **arithmetic inputs** to every M1.5 deliverable:

| Field | M1.5 dependency |
|---|---|
| `model` | Price-per-token lookup (`gpt-5` vs `claude-opus-4` differ ~5×) + tokenizer choice (cl100k_base vs anthropic-cl100k) |
| `output_tokens` | `session_total_cost = Σ output_tokens × output_price[model]` |
| `mode` | ROI interpretation context — same `schema_utilization=30%` is **waste** in `auto` mode (auto-approval inflates calls) but may be **fine** in `ask` mode |

Without these, `agentprof analyze` is "a cash register that counts transactions but doesn't know prices" — M1.5's per-tool ROI scoring and waste-estimate-USD computations both require this data.

### Why the M1.4 audit missed it

4 audit subagents reviewed M1.4 against spec FR-2.2 ("turn_summary fields present and correctly typed"). The spec didn't say "fields must be **populated from real data**", so default `None` shipping was technically spec-compliant. None of the audits ran `analyze --section turn-summary` against `minimal/` fixture (which has `outputTokens: 10` in line 4) to compare expected vs actual. This is a spec / audit blind spot, not a code bug per se — but the user impact is the same: core analyzer output looks broken.

## 2. Goal

Populate `Turn.model`, `Turn.mode`, `Turn.output_tokens` from the actual wire-format data, so `agentprof analyze` shows real values today, and M1.5 ROI computations have inputs to operate on.

**Non-goals (out of scope for this spec):**
- Cost / ROI computation logic itself (M1.5)
- Tokenizer integration (M1.5)
- Price tables per model (M1.5)
- `mode_segments` global timeline restructure (already works for its own purpose)
- Reasoning text extraction (`reasoning_opaque` / `reasoning_text` — orthogonal)

## 3. Scope

### Modifies

- `crates/agentprof-core/src/adapter.rs` — extend `Event` trait with 3 new methods (default `None`)
- `crates/agentprof-adapters/src/copilot/event.rs` — implement the 3 methods on `CopilotEvent` for `AssistantMessage` / `ModeChanged` variants
- `crates/agentprof-core/src/episode/derive.rs` — add `on_assistant_message` handler; thread `current_mode` through `DeriveState`; update `on_turn_start` to capture mode; update `on_mode_event` to update both `current_mode` and `ModeSegment` timeline

### Does not touch

- `Turn` struct (fields already exist — `model: Option<String>`, `mode: Option<Mode>`, `output_tokens: Option<u32>`)
- `TurnSummaryRow` (mapping is already in place; `turn_summary.rs` will just see non-None values pass through)
- CLI / md / json renderers (already render `Option::map_or` correctly)
- Existing analyzer rollups (`tool_rank`, `hook_rank` don't use these fields)
- ADR-0005 (will get an Update §5 documenting this extension; not a new ADR — Event trait already established by D-1)

## 4. Design

### 4.1 Event trait extension

Three new methods on `agentprof_core::adapter::Event`, all with default `None`:

```rust
pub trait Event {
    // Existing methods (M1.2 + M1.4 D-1):
    fn id(&self) -> &str;
    fn kind(&self) -> EventKind;
    fn timestamp(&self) -> DateTime<Utc>;
    fn parent_id(&self) -> Option<&str>;
    fn payload_name(&self) -> Option<&str> { None }

    // NEW (this spec):

    /// Returns the **model identifier** for the AI provider that produced
    /// this event. Implementations should return `Some` for variants whose
    /// payload carries a model name (e.g., `AssistantMessage` in CopilotEvent),
    /// `None` otherwise.
    fn payload_model(&self) -> Option<&str> { None }

    /// Returns the **output token count** reported by the model for this
    /// event. Implementations should return `Some` for variants whose
    /// payload carries `outputTokens` (e.g., `AssistantMessage`), `None`
    /// otherwise.
    fn payload_output_tokens(&self) -> Option<u32> { None }

    /// Returns the **new mode string** for mode-transition events. Used by
    /// `derive_episodes` to track the active session mode and attribute it
    /// to subsequently-opened turns. Implementations should return `Some`
    /// for variants like `ModeChanged`, `None` otherwise.
    fn payload_mode(&self) -> Option<&str> { None }
}
```

### 4.2 CopilotEvent overrides

```rust
// In CopilotEvent's inherent impl + delegate from trait impl:

pub fn payload_model(&self) -> Option<&str> {
    match self {
        Self::AssistantMessage(env) => Some(env.data.model.as_str()),
        _ => None,
    }
}

pub fn payload_output_tokens(&self) -> Option<u32> {
    match self {
        Self::AssistantMessage(env) => Some(env.data.output_tokens),
        _ => None,
    }
}

pub fn payload_mode(&self) -> Option<&str> {
    match self {
        Self::ModeChanged(env) => Some(env.data.new_mode.as_str()),
        _ => None,
    }
}
```

### 4.3 `derive_episodes` changes

**State extension**: `DeriveState` gains a `current_mode: Option<Mode>` field, defaulting to `None`. It tracks the active mode as the event stream advances.

**New handler `on_assistant_message`**:
```rust
fn on_assistant_message<E: Event>(&mut self, ev: &E) {
    if let Some(idx) = self.open_turn_idx {
        if let Some(turn) = self.turns.get_mut(idx) {
            // Last-wins model: if mid-turn the model changes, the final
            // assistant.message reflects the model that actually produced output.
            if let Some(model) = ev.payload_model() {
                turn.model = Some(model.to_string());
            }
            // Sum output_tokens: total turn output across all messages.
            if let Some(tokens) = ev.payload_output_tokens() {
                turn.output_tokens =
                    Some(turn.output_tokens.unwrap_or(0).saturating_add(tokens));
            }
        }
    }
    // If no open turn (e.g. assistant.message arrives before turn_start —
    // shouldn't happen but isn't a panic), silently ignore. The data is
    // still in the wire stream; we just don't have a Turn to attribute it to.
}
```

**Updated `on_mode_event`** (currently only manages `ModeSegment` timeline):
```rust
fn on_mode_event<E: Event>(&mut self, ev: &E) {
    let ts = ev.timestamp();
    // 1. Close out the previous ModeSegment (existing behavior).
    if let Some(seg) = self.mode_segments.last_mut() {
        seg.ended_at = Some(ts);
    }
    // 2. Read the new mode from payload + update state + start new segment.
    if let Some(new_mode_str) = ev.payload_mode() {
        let new_mode = Mode::from_wire(new_mode_str);
        self.current_mode = Some(new_mode.clone());
        self.mode_segments.push(ModeSegment::new(new_mode, ts));
    }
    // No `else` branch — if no mode value (e.g. ModelChange event with no
    // mode payload), we close the previous segment but don't start a new
    // one. This matches existing behavior.
}
```

**Updated `on_turn_start`** (currently constructs `Turn::new()` with default `None` mode):
```rust
fn on_turn_start<E: Event>(&mut self, ev: &E) {
    let mut turn = Turn::new(ev.id().to_string(), ev.timestamp());
    // Attribute the currently-active mode to this turn. If no ModeChange
    // has been seen yet (session start before first mode_changed event),
    // mode stays None.
    turn.mode = self.current_mode.clone();
    self.turns.push(turn);
    self.open_turn_idx = Some(self.turns.len() - 1);
}
```

**Dispatch table** gains one new match arm:
```rust
match ev.kind() {
    // ... existing arms ...
    EventKind::AssistantMessage => state.on_assistant_message(ev),  // NEW
    EventKind::ModeChanged | EventKind::ModelChange => state.on_mode_event(ev),
    EventKind::Abort => state.on_abort(ev),
    _ => {} // metadata-only events with no Turn impact
}
```

### 4.4 Data flow

```
session.start
  └── (no mode info yet — current_mode = None)

session.mode_changed { newMode: "ask" }
  └── on_mode_event: current_mode = Some(Ask)
                     + ModeSegment.push(Ask)

assistant.turn_start
  └── on_turn_start: turn.mode = current_mode.clone() = Some(Ask)

assistant.message { model: "claude-opus-4.7", output_tokens: 412 }
  └── on_assistant_message: turn.model = Some("claude-opus-4.7")
                            turn.output_tokens = Some(412)

assistant.message { model: "claude-opus-4.7", output_tokens: 88 }
  └── on_assistant_message: turn.model = Some("claude-opus-4.7")  (last-wins, same)
                            turn.output_tokens = Some(500)        (sum: 412+88)

session.mode_changed { newMode: "auto" }
  └── on_mode_event: current_mode = Some(Auto)
                     + ModeSegment.close + push(Auto)
  └── (this turn's mode stays Ask; next turn will pick up Auto)

assistant.turn_end
  └── on_turn_end: turn.status = Completed (existing)
                   (model / mode / output_tokens already populated)
```

### 4.5 Aggregation semantics (recap)

| Field | Strategy | Rationale |
|---|---|---|
| `output_tokens` | Sum across all `assistant.message` in turn | M1.5 ROI requires turn total. Saturating-add prevents u32 overflow on pathological sessions. |
| `model` | Last-wins across messages | Mid-turn model switch is rare but possible; final message's model is the effective one. |
| `mode` | Captured at `turn_start` from `current_mode` | Mode-changes mid-turn DON'T retroactively change `turn.mode` — only subsequent turns see the new mode. Matches user intuition ("this turn was started in `ask` mode"). |

## 5. Functional requirements

- **FR-1**: `Event::payload_model() -> Option<&str>` exists on the trait with default `None`. CopilotEvent overrides for `AssistantMessage`.
- **FR-2**: `Event::payload_output_tokens() -> Option<u32>` exists on the trait with default `None`. CopilotEvent overrides for `AssistantMessage`.
- **FR-3**: `Event::payload_mode() -> Option<&str>` exists on the trait with default `None`. CopilotEvent overrides for `ModeChanged`.
- **FR-4**: `derive_episodes` populates `Turn.model` from `assistant.message.data.model` (last-wins).
- **FR-5**: `derive_episodes` populates `Turn.output_tokens` from `assistant.message.data.output_tokens` (sum across messages).
- **FR-6**: `derive_episodes` populates `Turn.mode` at `turn_start` from `current_mode` (captured from latest `session.mode_changed.data.new_mode`).
- **FR-7**: `current_mode` updates on every `session.mode_changed` event. Initial value is `None` (sessions without explicit mode events get `Turn.mode = None`).
- **FR-8**: `on_assistant_message` silently ignores events arriving with no `open_turn_idx` (defensive — should not happen in well-formed data).
- **FR-9**: No new `DeriveWarning` variants. Missing payload fields produce `None`-valued cells, which is itself a clear user signal.
- **FR-10**: Existing snapshots in `crates/agentprof-adapters/tests/snapshots/` will need re-acceptance (model / mode / output_tokens columns flip from `null` to real values for fixtures that have AssistantMessage / ModeChanged events).
- **FR-11**: `minimal/` fixture now shows `model: "gpt-5-mini"` and `output_tokens: 10` in its turn snapshot (was `null`).
- **FR-12**: `with-mode-transitions/` fixture shows non-`None` mode values across its turns.

## 6. Testing

- 3 new CopilotEvent unit tests (1 per `payload_model` / `payload_output_tokens` / `payload_mode`, similar to existing payload_name tests).
- 4 new `derive.rs` unit tests:
  - `assistant_message_populates_model_and_output_tokens` (single message)
  - `multiple_assistant_messages_sum_output_tokens_and_last_wins_model` (model switch + 2 messages)
  - `mode_change_attributes_to_next_turn_not_current` (mode-change mid-turn doesn't retroactively update)
  - `turn_with_no_assistant_message_has_none_model` (defensive — turn open + close with no message in between)
- Snapshot re-acceptance: `episode_derive__episode__*.snap` + `analyzer_on_fixtures__analysis__*.snap` for fixtures touching these events.
- CLI integration test (`tests/cli.rs`): assert `analyze --session minimal --export json | jq '.turn_summary[0].output_tokens'` returns 10.

## 7. Risks & mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `assistant.message` arrives outside open turn (data anomaly) | Low | Silent data loss for that message | FR-8: defensive ignore + future ADR can add `OrphanAssistantMessage` warning if observed |
| Sum overflow for output_tokens (u32 ≈ 4.3B; pathological session w/ many huge messages) | Negligible (real sessions ≪ 10M tokens) | Saturating instead of panicking | Use `.saturating_add(...)` |
| Snapshot churn cascades to many `.snap` files | High | High review burden but no behavior risk | Run `INSTA_UPDATE=always cargo test` once; hand-verify 1-2 snapshots; commit batch |
| `current_mode` initial state ambiguity (sessions starting before any mode_changed event) | Medium | Initial turns get `mode = None` | Accept as documented FR-7; future: read `session.start.data.initial_mode` if Copilot adds it |
| Mid-turn model switch causes ambiguous `turn.model` | Low | Last-wins might differ from "first impression" | Document last-wins in FR-4; M1.5 ROI computation should sum cost per-message if precision matters |

## 8. Commit plan

| # | Commit | Files |
|---|---|---|
| 1 | `feat(core): extend Event trait with payload_model / output_tokens / mode` | adapter.rs + unit test |
| 2 | `feat(adapters): implement payload_model/output_tokens/mode for CopilotEvent` | copilot/event.rs + 3 unit tests |
| 3 | `feat(core): populate Turn.model / output_tokens / mode in derive_episodes` | derive.rs (new on_assistant_message + on_mode_event update + on_turn_start update + state field) + 4 unit tests |
| 4 | `test(adapters): re-accept snapshots with populated turn metadata` | snapshots/ batch update |
| 5 | `test(cli): minimal fixture turn_summary[0].output_tokens == 10 + model populated` | tests/cli.rs |
| 6 | `docs: ADR-0005 Update §5 (turn metadata extraction) + CHANGELOG` | adr-0005 + CHANGELOG |

Estimated total: ~150-200 lines of code + ~80 lines of new tests + N snapshot diffs (mechanical).

## 9. References

- `docs/architecture.md` §7.1 (ROI algorithm formulas)
- `docs/internals/adr-0005-analyzer-and-payload-name.md` D-1 (Event trait extension pattern this spec follows)
- `docs/superpowers/specs/2026-05-29-m1.4-cli-and-analyzer-design.md` FR-2.2 (the under-specified field requirement that caused this)
- M1.4 audit summary (this session, 2026-05-29)
