# B1 — Wire the `success` bit from Copilot payload into Episodes

**Date**: 2026-06-06
**Topic ID**: `b1-failure-bit`
**Pipeline stage**: brainstorming → spec → ADR-0013 → implementation
**Backlog item**: `m1.6.2-followup-copilot-failure-bit` (UPSTREAM)

> **TL;DR** — `derive.rs:383` has been hardcoding `ToolCallStatus::Success`
> (and `:490` / `:504` have been hardcoding `HookCall.success: true`)
> since M1.2. The wire payload (`ToolResultData.success` + `.error.message`,
> `HookEndData.success`) is fully present but never consumed. This silently
> neutralizes F1.13 (RoiView Red/Yellow Tool cell), F1.16 (By Hook OK%
> color), and the F2.3-just-shipped "failure wins over pending" composition
> rule. The fix extends `Event` trait with 2 default-`None` methods,
> overrides them in `CopilotEvent`, and rewires the 3 hardcoded sites in
> `derive.rs`. Existing 4 fixtures with `success:false` events provide
> free end-to-end coverage.

---

## 1. The bug

### 1.1 Where it lives

**Three hardcoded sites in `crates/agentprof-core/src/episode/derive.rs`:**

```rust
// derive.rs:383 — on_tool_complete
let call = ToolCall {
    span,
    turn_id: open.turn_id,
    status: ToolCallStatus::Success, // Task 10b will read actual success bit
    user_requested: open.user_requested,
    arguments,
};
```

```rust
// derive.rs:487-490 — on_hook_end (paired path)
let call = HookCall {
    span,
    turn_id: open.turn_id,
    success: true,
    ...
};
```

```rust
// derive.rs:500-504 — on_hook_end (orphan / synthesized path)
let call = HookCall {
    span: Span::instant(ts),
    turn_id: self.open_turn_idx.map(|i| self.turns[i].id.clone()),
    success: true,
    ...
};
```

All three ignore the wire `success: bool`.

### 1.2 Why it's been silent

`commit_tool_call` (line 433) reads:

```rust
let is_failure = matches!(call.status, ToolCallStatus::Failure { .. });
```

Since `status` is always `Success`, `is_failure` is always `false`, so
`ep.failure_count` stays at 0. Same shape on the hook side (`failed = !call.success`
in `commit_hook_call:517` is always `false`).

### 1.3 Downstream features the bug silently breaks

Three already-shipped UX features depend on `failure_count`:

| Feature | Code path | Symptom on real data |
|---|---|---|
| **F1.13** RoiView Tool cell Red/Yellow | `views::roi::failure_severity_color(call_count, failure_count)` | Always returns `None` → Tool cell never colored |
| **F1.16** By Hook `OK%` + Hook cell color | `views::aggregate` reuses `failure_severity_color` | `OK%` always 100% / Hook cell never colored |
| **F2.3** `compose_tool_cell_style(failure, is_pending)` | Failure-wins-over-pending precedence | `failure` always `None` → pending always wins → spec §3.3 table never exercised |

All three look fine in unit tests (which feed synthetic `Failure` variants) but
lie to the user on real Copilot data.

### 1.4 Wire data is already present

Per ADR-0002 (Copilot event schema):

- `ToolExecComplete` → `ToolResultData { success: bool, error: Option<ToolError>, ... }`
- `ToolError { message: String }`
- `HookEnd` → `HookEndData { success: bool, ... }` (no error message field)

The fix is pure pipe-wiring — no schema changes, no new wire formats.

### 1.5 Fixture coverage already exists

```
crates/agentprof-adapters/tests/fixtures/copilot/
  with-mcp-calls/events.jsonl       1× tool.execution_complete success=false
  with-aborts/events.jsonl          1× hook.end success=false
  with-hooks-heavy/events.jsonl     2× hook.end success=false
  multi-sess-c/events.jsonl         1× (tool or hook — confirmed during impl)
```

5 events across 4 fixtures. End-to-end coverage is "free" — no synthetic
fixture needed.

---

## 2. Design decisions (Q1 – Q4 answered during brainstorm)

| ID | Decision | Rationale |
|---|---|---|
| **Q1** scope | **Both**: count failures AND populate `ToolCallStatus::Failure { message }` | The struct already has the field. Wiring it once now unlocks future UX (RoiView detail hover, TurnDetail error display) without re-touching the parser. ~3 extra lines. |
| **Q2** hook semantics | **All** wire `success: false` count as hook failures | Matches schema; "the hook said no" IS the hook's job, whether the cause was a crash or a deliberate `PreToolUse` block. Distinguishing kinds = new field = over-engineering for current UX. |
| **Q3** Event trait | **Two narrow methods**: `payload_success() -> Option<bool>` + `payload_error_message() -> Option<&str>` | Mirrors existing `payload_name` / `payload_model` / `tool_call_id` / `payload_tool_requests` pattern. Adapter-author code path stays simpler than a new public `EventOutcome` struct just to carry 2 fields. Codified in **ADR-0013**. |
| **Q4** None handling | **Default to Success silently** when `payload_success()` returns `None` | Preserves backward-compat for older Copilot 1.0.x payloads / adapters that don't yet override. The `MissingSuccessBit` warning alternative was rejected as too noisy for an Option-typed method whose whole point is "this event has no concept". |

---

## 3. Architecture

```
  Wire JSON                  Event trait                  derive.rs
 ┌──────────────┐         ┌────────────────────────┐    ┌──────────────────┐
 │ ToolResult   │   ───→  │ payload_success        │ →  │ on_tool_complete │
 │   .success   │         │   () -> Option<bool>   │    │   matches bit →  │
 │   .error     │   ───→  │ payload_error_message  │ →  │   ToolCallStatus │
 │     .message │         │   () -> Option<&str>   │    │   ::Success /    │
 └──────────────┘         └────────────────────────┘    │   Failure { msg }│
 ┌──────────────┐                                       │                  │
 │ HookEndData  │         (reuses payload_success)      │ on_hook_end ×2   │
 │   .success   │   ─────────────────→                  │   sets           │
 └──────────────┘                                       │   HookCall.succ  │
                                                        └──────────────────┘
```

Three layers change in this order:

1. **`Event` trait** (`crates/agentprof-core/src/adapter.rs`) — 2 new default-`None` methods
2. **`CopilotEvent`** (`crates/agentprof-adapters/src/copilot/event.rs`) — override both for `ToolExecComplete` + `HookEnd`
3. **`derive_episodes`** (`crates/agentprof-core/src/episode/derive.rs`) — `on_tool_complete` + `on_hook_end` consume them

---

## 4. Layer 1: Event trait extension

In `crates/agentprof-core/src/adapter.rs`, append after `payload_tool_requests`
(keep alphabetical-ish ordering by domain: name → model → tool_requests →
tool_call_id → success → error_message → token_usage):

```rust
/// Adapter-specific success bit for events that report it
/// (`tool.execution_complete`, `hook.end`). Returns `None` for
/// events without the concept, and for adapters / payload schemas
/// that don't carry the bit (forward-compat).
///
/// Consumed by [`crate::episode::derive_episodes`] to populate
/// [`crate::episode::tool::ToolCallStatus::Failure`] (tools) and
/// [`crate::episode::hook::HookCall::success`] (hooks). `None` is
/// treated as success — preserves existing behaviour for older
/// Copilot CLI 1.0.x payloads or adapters that don't yet override.
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
///     // payload_success() inherits the default `None` impl.
/// }
/// assert_eq!(StubEvent.payload_success(), None);
/// ```
fn payload_success(&self) -> Option<bool> {
    None
}

/// Adapter-specific error message for failure events
/// (`tool.execution_complete` with `success: false`). Returns
/// `None` for non-failure events, for adapters whose payload
/// schema doesn't carry one (e.g. Copilot's `hook.end`), or
/// when the payload simply omitted the message (`error: null`).
///
/// Consumed by [`crate::episode::derive_episodes`] to populate
/// the `ToolCallStatus::Failure { message: Option<String> }`
/// field — currently surfaced nowhere in UI but future-ready
/// for RoiView detail / TurnDetail error display.
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
/// assert_eq!(StubEvent.payload_error_message(), None);
/// ```
fn payload_error_message(&self) -> Option<&str> {
    None
}
```

**Forward-compat note**: every method has a default impl, so adding more
in this style is **non-breaking** for adapter authors — existing
implementors compile unchanged and get sensible `None` defaults.

---

## 5. Layer 2: `CopilotEvent` overrides

In `crates/agentprof-adapters/src/copilot/event.rs`, inside the existing
`impl Event for CopilotEvent { ... }` block (next to the other `payload_*`
overrides):

```rust
fn payload_success(&self) -> Option<bool> {
    match self {
        CopilotEvent::ToolExecComplete(WithEnvelope { payload, .. }) => Some(payload.success),
        CopilotEvent::HookEnd(WithEnvelope { payload, .. })          => Some(payload.success),
        _ => None,
    }
}

fn payload_error_message(&self) -> Option<&str> {
    match self {
        CopilotEvent::ToolExecComplete(WithEnvelope { payload, .. }) => {
            payload.error.as_ref().map(|e| e.message.as_str())
        }
        // hook.end has no error.message field on the wire (ADR-0002 line 93).
        _ => None,
    }
}
```

---

## 6. Layer 3: `derive.rs` consumers

### 6.1 `on_tool_complete` (line ~380)

```rust
let status = match ev.payload_success() {
    Some(false) => ToolCallStatus::Failure {
        message: ev.payload_error_message().map(str::to_owned),
    },
    Some(true) | None => ToolCallStatus::Success, // None = forward-compat default
};
let call = ToolCall {
    span,
    turn_id: open.turn_id,
    status,
    user_requested: open.user_requested,
    arguments,
};
```

Delete the `// Task 10b will read actual success bit` comment — done.

### 6.2 `on_hook_end` — two sites (line ~487 and ~501)

Both:

```rust
let success = ev.payload_success().unwrap_or(true); // None = forward-compat default
let call = HookCall { span, turn_id, success, ... };
```

### 6.3 Orphan abort path (line ~607) — unchanged

```rust
let call = HookCall { ..., success: false, ... };
```

Stays hardcoded. "Session ended while hook was still open" IS a failure
semantically (the hook never reached its end event), regardless of any
wire bit. No behavior change.

---

## 7. Test strategy

### 7.1 Trait-level unit tests

In `agentprof-core` (likely `episode/derive.rs::tests` mod), build a stub
Event impl that returns `Some(false)` + `Some("disk full")` for a tool
complete event. Run through `derive_episodes` end-to-end and assert:

- `episodes.tools["bash"].calls[0].status == ToolCallStatus::Failure { message: Some("disk full".into()) }`
- `episodes.tools["bash"].failure_count == 1`

Mirror tests for:
- Stub returning `Some(true)` → `Success` + `failure_count == 0`
- Stub returning `None` → `Success` + `failure_count == 0` (forward-compat path)
- Same matrix for hook events (`HookCall.success` instead of `ToolCallStatus`)
- Tool failure with `Some(false)` + `None` error message → `Failure { message: None }`

### 7.2 Adapter-level unit tests

In `agentprof-adapters/src/copilot/event.rs::tests`, construct synthetic
`CopilotEvent::ToolExecComplete` + `CopilotEvent::HookEnd` instances with
all 4 corners:
- tool success=true
- tool success=false + error=Some
- tool success=false + error=None
- hook success=true / false

Assert `payload_success()` + `payload_error_message()` return the wire data.

Also assert `payload_success()` returns `None` for unrelated variants
(e.g. `SessionStart`, `UserMessage`).

### 7.3 End-to-end fixture assertions

Add new test cases to `agentprof-adapters/tests/analyzer_on_fixtures.rs`
(or a new sibling `failure_count_on_fixtures.rs` if it's cleaner):

- `with-mcp-calls`: assert `episodes.tools` has ≥1 episode with
  `failure_count >= 1`
- `with-hooks-heavy`: assert `episodes.hooks` has ≥1 episode with
  `failure_count >= 1` (where the failures sum is 2 from spec)
- `with-aborts`: assert ≥1 hook failure
- `multi-sess-c`: assert ≥1 tool OR hook failure (confirm which during impl)

These tests **would have caught the bug** if they existed in M1.2; they
double as permanent regression guards.

### 7.4 Snapshot regenerations

Existing snapshot suites likely affected (confirm during impl):

- `analyzer_on_fixtures.rs` (4 fixtures)
- `aggregate_on_fixtures.rs` (any aggregator that touches failure_count)
- `export_on_fixtures.rs` (markdown / JSON / HTML serializing
  `failure_count`)
- TUI view snapshot tests for RoiView / By Hook on the 4 fixtures
  (Tool cell color may render differently → buffer-extracted snapshots
  may change for the colored cells)

Inspection protocol per snapshot diff:
1. Read the diff
2. Confirm changes are ONLY `failure_count: 0 → N` and downstream
   computed fields (failure_pct, color cells)
3. ANY OTHER change → halt, investigate
4. `cargo insta accept` only when (2) holds

---

## 8. Risk & rollback

**Snapshot blast radius**: 4 fixtures. The 16 other fixtures have no
`success:false` events → `payload_success()` returns `Some(true)` →
matches the existing always-Success path → no diff. Safe.

**Wire-format compatibility**: zero change. We're reading existing fields
in the existing payload schema.

**Adapter authors**: zero breakage. Default `None` impls mean any external
adapter compiles unchanged and gets the existing always-Success behavior.

**Rollback**: if a snapshot regen reveals an unexpected cascade, the
3-layer architecture means we can revert any layer independently. Most
conservative rollback: revert just `derive.rs` to keep the trait + adapter
layers (small surface, no behavior change).

---

## 9. Commits (per §5.7 granularity)

1. **`docs(spec): B1 wire success bit design`** — this file
2. **`docs(adr): ADR-0013 — Event trait two-narrow-methods for payload success/error`** — codifies Q3 design choice (§5.5 mandates ADR for new public trait API)
3. **`fix(core): B1 — wire wire-format success bit into ToolCall/HookCall (closes silent F1.13/F1.16/F2.3 misfire)`** — single squashed commit: Event trait extension + CopilotEvent overrides + derive.rs consumers + tests + snapshot regen + delete the `// Task 10b` TODO + CHANGELOG entry

---

## 10. Documentation updates (per §4.2 trigger table)

| File | Update |
|---|---|
| `docs/architecture.md` | Add `payload_success` + `payload_error_message` to the Event trait surface listing (§ adapter trait) |
| `docs/internals/adr-0013-event-success-bit.md` | NEW — codify the two-narrow-methods choice (Q3 alternative considered) |
| `crates/agentprof-core/README.md` | Add the 2 methods to the Adapter trait surface table |
| `crates/agentprof-adapters/README.md` | Note that `CopilotEvent` overrides them for `ToolExecComplete` + `HookEnd` |
| L3 rustdoc | Trait methods (per §4) + override sites + the `derive.rs` change-point comment |
| `CHANGELOG.md` | `### Fixed` entry: "(M1.2 regression) failure_count was always 0..." with refs to F1.13 / F1.16 / F2.3 |

---

## 11. Out of scope (explicit YAGNI)

- **RoiView "show error message" UX** — `Failure { message }` field gets populated, but no view consumes it yet. Deferred to future ticket (it's now "free" — UI work only, no parser touch).
- **TurnDetail error display** — same deferral.
- **Per-tool failure rate trend (sparklines)** — F1.13 is point-in-time, no historical comparison. Future ticket.
- **Distinguishing hook "crash" vs "deliberate block"** — Q2 decision: lump them. Future ticket if user complaint surfaces.
- **`MissingSuccessBit` derive warning** — Q4 decision: silent default. Future ticket if a downstream report wants to surface adapter coverage gaps.
- **Hook error message field** — wire doesn't have one (ADR-0002 line 93). Future Copilot CLI version that adds it would extend `payload_error_message` for hooks, no API change needed.

---

## 12. Success criteria

- [ ] `cargo test --workspace --all-features` green (777+ tests, new ones added)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace` clean
- [ ] `cargo fmt --all --check` clean
- [ ] All 4 affected fixtures produce non-zero `failure_count` end-to-end
- [ ] Snapshot diffs reviewed and contain only the expected `failure_count` flips
- [ ] ADR-0013 written and linked from `docs/architecture.md`
- [ ] CHANGELOG `### Fixed` entry mentions all 3 affected shipped features (F1.13, F1.16, F2.3)
- [ ] Manual sanity check: run `cargo run -p agentprof-cli -- analyze --agent copilot --path crates/agentprof-adapters/tests/fixtures/copilot/with-mcp-calls --export md` and confirm the failure shows up in the rendered output

---

## 13. References

- ADR-0002 — Copilot event schema (wire fields)
- ADR-0004 — Episodes serde unit convention (`ms_duration` helpers)
- ADR-0011 D-3 — orphan tool args lookup
- ADR-0012 D-4/D-6 — `model_metrics` last-wins semantics (template for new trait methods)
- F1.13 — RoiView Tool cell failure-severity color
- F1.16 — By Hook OK% + color
- F2.3 — `compose_tool_cell_style` failure-wins-over-pending
- Backlog: `m1.6.2-followup-copilot-failure-bit`
