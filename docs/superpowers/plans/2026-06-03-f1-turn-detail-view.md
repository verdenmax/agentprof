# F1 — TurnDetailView + tool arguments plumbing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a full-screen `TurnDetailView` in the TUI that opens with `Enter` on a selected turn in `FlamegraphView`. Detail view lists every tool call in the turn (sorted by duration desc) with name, duration, ✓/✗ status, source-colored badge, and a one-line `args` preview (truncated to 80 chars + `…`); selected tool call's `args` toggle expanded JSON on `Enter`. To make this feasible, plumb the `arguments: serde_json::Value` parsed by `agentprof-adapters::copilot::event::ToolRequest` (currently discarded) through a new `Event::payload_tool_requests` extension method into a new `ToolCall.arguments: Option<serde_json::Value>` field, populated by a new PASS 0 args-map step in `agentprof-core::episode::derive::derive_episodes`.

**Architecture:** Three-layer change matching the data flow already established by ADR-0005 (`Event::payload_*` extension methods). Layer 1 `agentprof-core::adapter` adds a `fn payload_tool_requests(&self) -> Vec<(String, Value)>` trait method with `Vec::new()` default — symmetric with the four existing `payload_name`/`payload_model`/`payload_output_tokens`/`payload_mode` methods. Layer 2 `agentprof-adapters::copilot::event::CopilotEvent` implements it on the two payload-bearing variants (`AssistantMessage` → multi pairs from `tool_requests`, `ToolUserRequested` → single pair via `serde_json::to_value(&data.arguments)`). Layer 2.5 `agentprof-core::episode::tool::ToolCall` gains `pub arguments: Option<serde_json::Value>` (default `None`; `#[serde(skip_serializing_if = "Option::is_none")]`); `derive_episodes` walks events once to build `BTreeMap<tool_call_id, Value>` (PASS 0), then the existing PASS 1 state machine stamps each `ToolCall.arguments` on close via `args_by_call_id.get(&call_id).cloned()`. Layer 3 `agentprof-tui::views::turn_detail` is a new module hosting `TurnDetailState` + `render_turn_detail(&AppState)` + pure formatting helpers; `AppState<'a>` gains `pub detail_view: Option<TurnDetailState>` and `dispatch()` learns the recursive "`Enter` = drill deeper" rule (Flamegraph + valid selection + `Enter` → enter detail; in detail: `Esc` → exit, `Enter` → toggle args expand, `j`/`k`/`G`/`gg` → navigate tool calls, `1`/`2`/`3` → pop detail + switch view); `WatchRunner` round-trips `detail_view` via a new `WatchViewState.detail_view` field (same pattern as existing `pending_gg` / `help_overlay`) and `do_reload()` drops the field + red-banner footers if `turn_id` vanished.

**Tech Stack:** Rust 2021 (MSRV 1.78); existing `serde_json` (already a workspace dep used by every crate); existing `tracing` (D-4 conflict logging); existing `ratatui` 0.29 + `crossterm` (TUI rendering + key dispatch); existing `insta` for snapshots; **no new workspace dependencies**.

**Spec:** [`docs/superpowers/specs/2026-06-03-turn-detail-view-design.md`](../specs/2026-06-03-turn-detail-view-design.md) (commit `46063f4` + amend `9911e94`)

**ADR:** [`docs/internals/adr-0011-turn-detail-and-args-plumbing.md`](../../internals/adr-0011-turn-detail-and-args-plumbing.md) (commit `949201e` + amend `9911e94`)

---

## Pre-flight facts (locked — do NOT re-discover)

1. **Workspace branch model**: direct-to-`main`, per M1.6.4 follow-up wave convention (this F1 is the wave's Phase 2 feature). No long-lived feature branch. Each task lands as one commit with conventional-commit subject + `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>` trailer.

2. **AppState already borrows Episodes**: `crates/agentprof-tui/src/app/state.rs:88-90` shows `pub struct AppState<'a> { ..., pub report: &'a AnalysisReport, pub episodes: &'a Episodes }`. Constructor at `state.rs:114` is `pub fn new(report: &'a AnalysisReport, episodes: &'a Episodes) -> Self`. `render_turn_detail(&AppState<'_>)` reuses this dual-borrow; **no `AnalysisReport.episodes` field needed** (D-6 of ADR-0011 superseded — see commit `9911e94`).

3. **CallRef resolution pattern**: `Turn.tool_calls: Vec<CallRef>` where `CallRef { name: String, index: usize }` (defined at `crates/agentprof-core/src/episode/call_ref.rs:24`); resolve via `episodes.tools.get(&call_ref.name).and_then(|e| e.calls.get(call_ref.index))`. Already used by FlamegraphView; do NOT invent a new lookup helper.

4. **`Event::payload_*` precedent**: `crates/agentprof-core/src/adapter.rs:209-292` defines four existing extension methods (`payload_name`/`payload_model`/`payload_output_tokens`/`payload_mode`), each with a `Vec::new()` / `None` / `0` default + a per-variant override in `crates/agentprof-adapters/src/copilot/event.rs:1228-1304`. The new `payload_tool_requests` follows this exact pattern.

5. **CopilotEvent payload-bearing variants** for tool requests are exactly two:
   - `Self::AssistantMessage(env)` — `env.data.tool_requests: Vec<ToolRequest>` (defined at `event.rs:390`); each `ToolRequest` has `tool_call_id: String` (rename `toolCallId`, line 360) + `arguments: serde_json::Value` (line 364).
   - `Self::ToolUserRequested(env)` — `env.data.tool_call_id: String` (line 610) + `env.data.arguments: ToolUserArgs` (line 615). `ToolUserArgs` is a normal struct → `serde_json::to_value(&args).unwrap_or(Value::Null)` is total (the unwrap_or is dead-code defensive).

6. **`ToolCall` is `#[non_exhaustive]`** (`crates/agentprof-core/src/episode/tool.rs:42`); adding `pub arguments: Option<serde_json::Value>` is a non-breaking field add. The constructor `ToolCall::new(span: Span) -> Self` at line 56 defaults the new field to `None`.

7. **`derive_episodes` is pure + total + snapshot-stable** (`crates/agentprof-core/src/episode/derive.rs:82`). PASS 0 walk is a tiny linear scan that materializes `BTreeMap<String, serde_json::Value>`; PASS 1 is the existing state-machine walk modified to consult the map on `on_tool_complete`. Total complexity still `O(N_events × max_requests_per_event)` ⊆ `O(N_events²)` worst-case but ⊆ `O(N_events)` typical.

8. **Conflict-on-duplicate-id is "first wins"** per D-4 of ADR-0011: use `args_by_call_id.entry(call_id).or_insert(args)`. If the entry-occupied path runs (duplicate `tool_call_id` seen), emit a `tracing::debug!(target = "derive", tool_call_id = %id, "duplicate tool_call_id args ignored")` at the BTreeMap collection site. Cheap detection; the existing project uses `tracing` extensively (ADR-0010).

9. **No new dependencies**: `serde_json` is already a workspace dep used by every crate. `ratatui` + `crossterm` + `insta` + `tracing` all already in `agentprof-tui`. No `Cargo.toml` `[dependencies]` table edits needed — only feature additions / version bumps would require touching workspace dep table.

10. **Snapshot-test pattern**: `crates/agentprof-tui/tests/views.rs:11-78` is the canonical TUI snapshot harness using `TestBackend::new(100, 30)` + `Terminal::new` + `runner.draw_frame` + `buffer_to_symbol_grid`. The helper `buffer_to_symbol_grid` is **color-blind** by design (uses `cell.symbol()` only). Color assertions live in inline unit tests inside `crates/agentprof-tui/src/views/*.rs` `#[cfg(test)] mod tests`.

11. **`buffer_to_symbol_grid` is private to `views.rs`** — copy the helper into the new `tests/turn_detail.rs` file (DRY rule waived for test fixtures; comment with `// Copied from tests/views.rs — see L57-71 there`).

12. **`AppState::dispatch` already implements `pending_gg` / `j-k` / `G-gg` vim keys** at `crates/agentprof-tui/src/app/state.rs:172-211`. The detail-view key dispatch should follow the same shape (clear-and-execute on non-`g` key, two-key sequence for `gg`).

13. **`AppState::dispatch` is currently free-fn** (not a method on `AppState`), takes `&mut AppState<'_>, event: Event` → `Action`. Detail-view dispatch goes **inside** this same fn (early return after detail-view handles; fallthrough to existing dispatch for view-switching keys).

14. **`WatchViewState` already has `pending_gg`** (`crates/agentprof-tui/src/watch.rs:172`), proving the round-trip pattern. Adding `detail_view: Option<TurnDetailState>` follows the same model.

15. **`WatchRunner` reconstructs transient `AppState` on every frame** (`watch.rs:398, 457`): `let mut transient = AppState::new(report, episodes); transient.help_open = self.view_state.help_overlay;`. Each transient ALSO needs `transient.detail_view = self.view_state.detail_view.clone();` (and the post-dispatch `self.view_state.detail_view = transient.detail_view;` write-back).

16. **`WatchViewState.do_reload`** at `watch.rs:478-491` swaps `self.data` on success. Add a follow-up check: if `self.view_state.detail_view.is_some()` AND new `episodes.turns` doesn't contain the cached `turn_id`, drop the field and set `self.last_error = Some("turn {id} disappeared after reload".into())`.

17. **Workspace lints** (`Cargo.toml` `[workspace.lints.clippy]`): `unwrap_used = "deny"` (use `expect()` only in `#[cfg(test)]` or `main.rs`); `missing_docs = "warn"` + workspace `-D warnings` (all `pub` items in lib crates need rustdoc + `# Examples`; bin crates are link-local so doctests must use ` ```text ` not ` ```rust `); captured-id format strings only (`format!("{var}")` not `format!("{}", var)`).

18. **Help overlay** is `fn draw_help_overlay(frame: &mut Frame<'_>, full: Rect)` at `crates/agentprof-tui/src/app/mod.rs:134`. Current height is 22 rows (per the M1.5 + follow-up adds). Adding 5 detail-view rows → 27 rows total. The overlay's `Rect` math at line 134-ish uses a `min` clamp against terminal height; ensure 27 still fits a typical 30-row TestBackend.

19. **CI gates (failure = redo)**: `cargo fmt --all --check` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo test --workspace --all-features` / `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace`. Run all four at the end of each Task (after the test step, before the commit step).

20. **Cross-crate test scope**: Tests for Layer 1 (`Event::payload_tool_requests` default) live in `agentprof-core/src/adapter.rs` `#[cfg(test)]`. Tests for Layer 2 (`CopilotEvent::payload_tool_requests` impl) live in `agentprof-adapters/src/copilot/event.rs` `mod payload_name_tests` (same module as existing impl tests). Tests for Layer 2.5 (`ToolCall.arguments` field + `derive_episodes` stamping) live in `agentprof-core/src/episode/tool.rs` `#[cfg(test)]` and `agentprof-core/src/episode/derive.rs` `#[cfg(test)]`. Tests for Layer 3 (TurnDetailView) live in `agentprof-tui/src/views/turn_detail.rs` `#[cfg(test)]` (helpers) + `agentprof-tui/tests/turn_detail.rs` (snapshot + dispatch + reload).

21. **CHANGELOG location**: `CHANGELOG.md` at workspace root; `[Unreleased]` section; F1 entries go under `### Added` (3 items: trait method, struct field, TurnDetailView) + `### Changed` (1 item: JSON export adds `arguments` field).

22. **Privacy doc location**: `docs/features/privacy.md`; the new §8 documents the "args data is passed through as-is from adapter, no redaction in v1" posture. Don't conflate with `AGENTPROF_LOG_FULL_PATHS` (that's logging fields, not payload data).

23. **Adapter doc location**: `docs/adapters.md`; add a section under "Required trait impls" noting that `payload_tool_requests` is **recommended-but-optional** — adapters that don't implement it ship with the silent-fallback "args = `(not captured)`" badge in TurnDetailView.

---

## Task 0: Branch setup + baseline gates

**Files:** No source changes; verify branch state.

- [ ] **Step 1: Confirm branch + clean tree + HEAD**

```bash
cd /home/verden/pfind/2026-spring/code/agentprof
git status                  # expect: On branch main / nothing to commit
git log --oneline -3        # HEAD should be 9911e94 (spec/ADR amend) or newer
```

- [ ] **Step 2: Establish baseline test count**

```bash
cargo test --workspace --all-features 2>&1 | grep -E '^test result' | \
  awk '{passed+=$4} END {print "baseline tests:", passed}'
```

Expected: around **533** (this is the floor — `+31` target means ~564 after F1).

- [ ] **Step 3: Establish baseline gates green**

```bash
cargo fmt --all --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace
```

Expected: all three succeed silently (or with deprecation notes only).

---

## Task 1: Core — `Event::payload_tool_requests` trait method (default impl)

**Files:**
- Modify: `crates/agentprof-core/src/adapter.rs:209-292` (insert new method after `payload_mode`)
- Test: `crates/agentprof-core/src/adapter.rs` `#[cfg(test)]` block at bottom

**Why first:** Layer 1 of the data plumbing. Adding the trait method first means Layer 2 (adapter impl) compiles cleanly against a real method (not a "TODO" comment).

- [ ] **Step 1: Write the failing test**

Append inside the existing `#[cfg(test)]` test module at the bottom of `crates/agentprof-core/src/adapter.rs` (search for `mod test` near the end of the file):

```rust
#[test]
fn payload_tool_requests_default_returns_empty() {
    struct StubEvent;
    impl Event for StubEvent {
        fn id(&self) -> &str { "stub" }
        fn kind(&self) -> EventKind { EventKind::Unknown }
        fn timestamp(&self) -> chrono::DateTime<chrono::Utc> { chrono::Utc::now() }
        fn parent_id(&self) -> Option<&str> { None }
        // payload_tool_requests inherits the default impl
    }
    assert_eq!(StubEvent.payload_tool_requests().len(), 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p agentprof-core --lib payload_tool_requests_default 2>&1 | tail -10
```

Expected: `error[E0599]: no method named 'payload_tool_requests' found for struct 'StubEvent'`.

- [ ] **Step 3: Add the trait method with default impl**

Insert after the existing `fn payload_mode` method (around `crates/agentprof-core/src/adapter.rs:292`):

```rust
    /// Adapter-specific `(tool_call_id, arguments)` pairs declared by this
    /// event. Returns empty for events without tool-request payloads.
    ///
    /// Used by [`crate::episode::derive_episodes`] to populate
    /// [`crate::episode::ToolCall::arguments`] — the args data point
    /// lives separately from the span on the wire (Copilot:
    /// `assistant.message.tool_requests[*]` and
    /// `tool.user_requested.arguments`), so the derive function needs
    /// a first-pass map keyed by `tool_call_id` before it can attach
    /// args to the matching span on close.
    ///
    /// Default returns empty `Vec`; adapters override for relevant
    /// payload-bearing variants. See ADR-0011 D-1 / D-2.
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
    ///     // payload_tool_requests() inherits the default `Vec::new()` impl.
    /// }
    /// assert!(StubEvent.payload_tool_requests().is_empty());
    /// ```
    fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
        Vec::new()
    }
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p agentprof-core --lib payload_tool_requests_default 2>&1 | tail -5
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Run all four gates**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-core --all-targets -- -D warnings && \
cargo test -p agentprof-core --all-features && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-core
```

Expected: all green (doctest in the new `# Examples` also runs and passes).

- [ ] **Step 6: Commit**

```bash
git add crates/agentprof-core/src/adapter.rs
git -c commit.gpgsign=false commit -m "feat(core): Event::payload_tool_requests trait method (default empty)

New extension-method on the Event trait, symmetric with the existing
four payload_name / payload_model / payload_output_tokens / payload_mode
methods (cf. ADR-0005). Returns Vec<(tool_call_id, arguments)> pairs.

Default impl returns empty Vec so existing adapter trait impls compile
unchanged (non-breaking change). Adapters opt into rich TurnDetailView
UX by overriding for variants that carry tool_call_id + arguments
(e.g. Copilot AssistantMessage.tool_requests[*] +
ToolUserRequested.arguments — landed in Task 2 of this plan).

Spec §3.2, ADR-0011 D-1 + D-2.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 2: Adapter — `CopilotEvent::payload_tool_requests` impl

**Files:**
- Modify: `crates/agentprof-adapters/src/copilot/event.rs:1228-1304` (insert new inherent method + trait override)
- Test: `crates/agentprof-adapters/src/copilot/event.rs` `mod payload_name_tests` (search for line ~1306)

**Why second:** Layer 2 of the data plumbing. Implements the method on the only real adapter we have so the new `ToolCall.arguments` field (Task 3) has a real data source to populate from.

- [ ] **Step 1: Write the failing tests (3 cases)**

Append inside the existing `mod payload_name_tests` (around `crates/agentprof-adapters/src/copilot/event.rs:1306`):

```rust
#[test]
fn payload_tool_requests_assistant_message_multi() {
    let env = envelope(AssistantMessageData {
        message_id: "m1".into(),
        model: "claude-sonnet-4.6".into(),
        turn_id: "t1".into(),
        output_tokens: 100,
        tool_requests: vec![
            ToolRequest {
                tool_call_id: "tc-1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({"command": "ls -la"}),
                call_type: "function".into(),
                intention_summary: None,
            },
            ToolRequest {
                tool_call_id: "tc-2".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "/etc/hosts"}),
                call_type: "function".into(),
                intention_summary: None,
            },
        ],
    });
    let ev = CopilotEvent::AssistantMessage(env);
    let pairs = ev.payload_tool_requests();
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].0, "tc-1");
    assert_eq!(pairs[0].1, serde_json::json!({"command": "ls -la"}));
    assert_eq!(pairs[1].0, "tc-2");
    assert_eq!(pairs[1].1, serde_json::json!({"path": "/etc/hosts"}));
}

#[test]
fn payload_tool_requests_tool_user_requested_single() {
    let env = envelope(ToolUserRequestedData {
        tool_call_id: "tc-9".into(),
        tool_name: "ask_user".into(),
        arguments: ToolUserArgs {
            prompt: "What's your favorite color?".into(),
            choices: vec!["red".into(), "blue".into()],
            allow_freeform: true,
        },
    });
    let ev = CopilotEvent::ToolUserRequested(env);
    let pairs = ev.payload_tool_requests();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0, "tc-9");
    // serde-roundtrip semantics: ToolUserArgs serializes deterministically
    let v = &pairs[0].1;
    assert_eq!(v["prompt"], "What's your favorite color?");
    assert_eq!(v["choices"][0], "red");
    assert_eq!(v["allow_freeform"], true);
}

#[test]
fn payload_tool_requests_other_variants_empty() {
    let env = envelope(SessionStartData {
        session_id: "s".into(),
        version: 1,
        producer: "test".into(),
        copilot_version: "1.0.0".into(),
        start_time: chrono::Utc::now(),
        context: Default::default(),
        already_in_use: false,
    });
    let ev = CopilotEvent::SessionStart(env);
    assert_eq!(ev.payload_tool_requests().len(), 0);
}
```

> **NOTE on field names** in the `ToolRequest { ... }` literal: verify the exact field set with `grep -n 'pub struct ToolRequest' -A 25 crates/agentprof-adapters/src/copilot/event.rs` before pasting — the struct may have added fields by the time you implement this. Same for `AssistantMessageData`, `ToolUserRequestedData`, `ToolUserArgs`, `SessionStartData`. If serde rejects, look at the `envelope()` helper at line 1311 for the wrapping shape.

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p agentprof-adapters --lib payload_tool_requests 2>&1 | tail -10
```

Expected: `error[E0599]: no method named 'payload_tool_requests' found` (for all 3 tests).

- [ ] **Step 3: Add the inherent impl method**

Insert after the existing inherent `pub fn payload_mode` (around line 1280 — right before the `impl agentprof_core::adapter::Event for CopilotEvent` block):

```rust
    /// Returns `(tool_call_id, arguments)` pairs for variants that carry them:
    /// - [`Self::AssistantMessage`] → one pair per
    ///   [`AssistantMessageData::tool_requests`] entry (multi).
    /// - [`Self::ToolUserRequested`] → one pair via
    ///   `serde_json::to_value(&data.arguments)` (single).
    /// - All other variants → empty `Vec`.
    ///
    /// Consumed by [`agentprof_core::episode::derive_episodes`] to populate
    /// [`agentprof_core::episode::ToolCall::arguments`].
    #[must_use]
    pub fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
        match self {
            Self::AssistantMessage(env) => env
                .data
                .tool_requests
                .iter()
                .map(|tr| (tr.tool_call_id.clone(), tr.arguments.clone()))
                .collect(),
            Self::ToolUserRequested(env) => {
                // ToolUserArgs is a plain struct → to_value is total; the
                // unwrap_or is dead-code defensive (debug_assert documents
                // the invariant in tests).
                let v = serde_json::to_value(&env.data.arguments)
                    .unwrap_or(serde_json::Value::Null);
                vec![(env.data.tool_call_id.clone(), v)]
            }
            _ => Vec::new(),
        }
    }
```

- [ ] **Step 4: Add the trait override**

Append inside the `impl agentprof_core::adapter::Event for CopilotEvent { ... }` block (search for `fn payload_mode(&self)` around line 1301):

```rust
    fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
        self.payload_tool_requests()
    }
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p agentprof-adapters --lib payload_tool_requests 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 6: Run all four gates**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-adapters --all-targets -- -D warnings && \
cargo test -p agentprof-adapters --all-features && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-adapters
```

Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add crates/agentprof-adapters/src/copilot/event.rs
git -c commit.gpgsign=false commit -m "feat(adapters): CopilotEvent::payload_tool_requests impl

Override the new core trait method on the two payload-bearing
CopilotEvent variants:
- AssistantMessage → emits one pair per tool_requests[] entry
  (tool_call_id, arguments) — multi.
- ToolUserRequested → emits a single pair via to_value(&arguments).

All other variants inherit the empty-Vec default. ToolUserArgs is a
plain struct so to_value is total; the unwrap_or(Value::Null) is
dead-code defensive (documented + tested via roundtrip).

3 new tests in mod payload_name_tests cover the multi / single /
other-variant cases.

Spec §3.3, ADR-0011 D-2.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 3: Core — `ToolCall.arguments` field

**Files:**
- Modify: `crates/agentprof-core/src/episode/tool.rs:42-66` (struct + ctor)
- Test: `crates/agentprof-core/src/episode/tool.rs` `#[cfg(test)]` (append)

**Why third:** Layer 2.5. The field must exist before `derive_episodes` (Task 4) can populate it.

- [ ] **Step 1: Write the failing tests**

Append to (or create) `#[cfg(test)] mod tests` at the bottom of `crates/agentprof-core/src/episode/tool.rs`:

```rust
#[cfg(test)]
mod arguments_field_tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn one_sec_span() -> Span {
        Span::new(
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 1).unwrap(),
        )
    }

    #[test]
    fn tool_call_default_arguments_is_none() {
        let tc = ToolCall::new(one_sec_span());
        assert!(tc.arguments.is_none());
    }

    #[test]
    fn tool_call_serde_roundtrip_with_arguments() {
        let mut tc = ToolCall::new(one_sec_span());
        tc.arguments = Some(serde_json::json!({"cmd": "ls", "verbose": true}));
        let json = serde_json::to_string(&tc).unwrap();
        let back: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(back.arguments, tc.arguments);
    }

    #[test]
    fn tool_call_arguments_skipped_when_none_in_json() {
        let tc = ToolCall::new(one_sec_span());
        let json = serde_json::to_string(&tc).unwrap();
        assert!(
            !json.contains("\"arguments\""),
            "None arguments should be skipped: {json}"
        );
    }

    #[test]
    fn tool_call_arguments_present_when_some_in_json() {
        let mut tc = ToolCall::new(one_sec_span());
        tc.arguments = Some(serde_json::json!({"x": 1}));
        let json = serde_json::to_string(&tc).unwrap();
        assert!(
            json.contains("\"arguments\""),
            "Some arguments should serialize: {json}"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p agentprof-core --lib arguments_field 2>&1 | tail -10
```

Expected: `error[E0560]: struct 'ToolCall' has no field named 'arguments'`.

- [ ] **Step 3: Add the field + update constructor**

Modify `crates/agentprof-core/src/episode/tool.rs:42-65`:

```rust
/// One invocation of a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCall {
    /// Time interval covering the call (start → end).
    pub span: Span,
    /// Owning turn id, when the call was attributable to an open turn.
    pub turn_id: Option<String>,
    /// Terminal status of the call.
    pub status: ToolCallStatus,
    /// `true` if the call originated from `ToolUserRequested` (manual approval).
    pub user_requested: bool,
    /// Tool arguments JSON value, when the adapter captured and emitted
    /// it via [`crate::adapter::Event::payload_tool_requests`]. `None`
    /// when either (a) the adapter did not implement that method for
    /// the relevant variant, or (b) the `tool_call_id` had no matching
    /// tool-request event in the session (e.g. orphan completes,
    /// mid-session resume).
    ///
    /// Skipped in JSON output when `None` to keep the schema clean for
    /// archives produced by adapters that don't yet plumb args.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

impl ToolCall {
    /// Construct with status `Success` by default — adjust before pushing.
    #[must_use]
    pub const fn new(span: Span) -> Self {
        Self {
            span,
            turn_id: None,
            status: ToolCallStatus::Success,
            user_requested: false,
            arguments: None,
        }
    }
}
```

> **NOTE**: `serde_json::Value` is not `const`-constructible in stable Rust, but `Option::None` is — the `pub const fn new` continues to compile because we default to `None`.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p agentprof-core --lib arguments_field 2>&1 | tail -10
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 5: Run full core test suite to catch unintended snapshot breaks**

```bash
cargo test -p agentprof-core --all-features 2>&1 | grep -E '^test result'
```

Expected: all green. No snapshot tests in `agentprof-core` reference `ToolCall` JSON output directly, but if any do (unlikely), they'd fail with the new optional field. If failures appear, inspect the diff — if it's purely additive (a new `"arguments": ...` key in some fixture), accept the snapshot. Otherwise debug.

- [ ] **Step 6: Run gates + commit**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-core --all-targets -- -D warnings && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-core

git add crates/agentprof-core/src/episode/tool.rs
git -c commit.gpgsign=false commit -m "feat(core): ToolCall.arguments Option<serde_json::Value> field

Add per-call args field on ToolCall, defaulting to None.
#[serde(skip_serializing_if = \"Option::is_none\")] keeps the JSON
schema clean for adapters that don't yet plumb args.

ToolCall is #[non_exhaustive] so the field add is non-breaking.
The pub const fn new(span) ctor stays const — Option::None is
const-constructible.

4 new tests in arguments_field_tests cover default-None,
serde roundtrip, skip-when-none, and present-when-some.

Spec §3.2, ADR-0011 D-5.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 4: Core — `derive_episodes` PASS 0 args-map + stamp on close

**Files:**
- Modify: `crates/agentprof-core/src/episode/derive.rs:82+` (add PASS 0 + thread map through state)
- Test: `crates/agentprof-core/src/episode/derive.rs` `#[cfg(test)]` (append)

**Why fourth:** Layer 2.5 wiring. Connects Layer 1 (`Event::payload_tool_requests`) + Layer 2 (`CopilotEvent` impl) to Layer 2.5 (`ToolCall.arguments` field). After this Task, end-to-end data flow from raw events → `ToolCall.arguments` works — the TUI (Layer 3) can consume it.

- [ ] **Step 1: Write the failing tests**

Append inside the existing `#[cfg(test)] mod tests` at the bottom of `crates/agentprof-core/src/episode/derive.rs` (search for `mod tests` near end):

```rust
#[cfg(test)]
mod args_plumbing_tests {
    use super::*;
    use crate::adapter::{AgentKind, EventKind};
    use crate::model::SessionMeta;
    use chrono::{TimeZone, Utc};

    /// Minimal event variants for testing the args-attachment flow.
    /// Mirrors the shape that `derive_episodes` walks.
    enum E {
        TurnStart { id: String, ts: chrono::DateTime<Utc> },
        AssistantMsg { id: String, ts: chrono::DateTime<Utc>, requests: Vec<(String, serde_json::Value)> },
        ToolStart { id: String, ts: chrono::DateTime<Utc>, tool_call_id: String, tool_name: String },
        ToolEnd { id: String, ts: chrono::DateTime<Utc>, tool_call_id: String, success: bool },
        TurnEnd { id: String, ts: chrono::DateTime<Utc> },
    }

    impl Event for E {
        fn id(&self) -> &str {
            match self {
                E::TurnStart { id, .. }
                | E::AssistantMsg { id, .. }
                | E::ToolStart { id, .. }
                | E::ToolEnd { id, .. }
                | E::TurnEnd { id, .. } => id,
            }
        }
        fn kind(&self) -> EventKind {
            match self {
                E::TurnStart { .. } => EventKind::TurnStart,
                E::AssistantMsg { .. } => EventKind::AssistantMessage,
                E::ToolStart { .. } => EventKind::ToolExecStart,
                E::ToolEnd { .. } => EventKind::ToolExecComplete,
                E::TurnEnd { .. } => EventKind::TurnEnd,
            }
        }
        fn timestamp(&self) -> chrono::DateTime<Utc> {
            match self {
                E::TurnStart { ts, .. }
                | E::AssistantMsg { ts, .. }
                | E::ToolStart { ts, .. }
                | E::ToolEnd { ts, .. }
                | E::TurnEnd { ts, .. } => *ts,
            }
        }
        fn parent_id(&self) -> Option<&str> { None }
        fn payload_name(&self) -> Option<&str> {
            match self {
                E::ToolStart { tool_name, .. } => Some(tool_name.as_str()),
                _ => None,
            }
        }
        fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
            match self {
                E::AssistantMsg { requests, .. } => requests.clone(),
                _ => Vec::new(),
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
            E::TurnStart { id: "t".into(), ts: t(0) },
            E::AssistantMsg {
                id: "m".into(),
                ts: t(1),
                requests: vec![("tc-1".into(), serde_json::json!({"command": "ls"}))],
            },
            E::ToolStart {
                id: "s".into(), ts: t(2),
                tool_call_id: "tc-1".into(), tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(), ts: t(3),
                tool_call_id: "tc-1".into(), success: true,
            },
            E::TurnEnd { id: "te".into(), ts: t(4) },
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
            E::TurnStart { id: "t".into(), ts: t(0) },
            E::ToolStart {
                id: "s".into(), ts: t(1),
                tool_call_id: "tc-orphan".into(), tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(), ts: t(2),
                tool_call_id: "tc-orphan".into(), success: true,
            },
            E::TurnEnd { id: "te".into(), ts: t(3) },
        ];
        let ep = derive_episodes(&events, &meta());
        let bash = ep.tools.get("bash").expect("bash episode present");
        assert!(bash.calls[0].arguments.is_none());
    }

    #[test]
    fn args_first_occurrence_wins_on_duplicate_call_id() {
        let events = vec![
            E::TurnStart { id: "t".into(), ts: t(0) },
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
                id: "s".into(), ts: t(3),
                tool_call_id: "tc-dup".into(), tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(), ts: t(4),
                tool_call_id: "tc-dup".into(), success: true,
            },
            E::TurnEnd { id: "te".into(), ts: t(5) },
        ];
        let ep = derive_episodes(&events, &meta());
        let bash = ep.tools.get("bash").unwrap();
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
            E::TurnStart { id: "t".into(), ts: t(0) },
            E::ToolStart {
                id: "s".into(), ts: t(1),
                tool_call_id: "tc-late".into(), tool_name: "bash".into(),
            },
            E::ToolEnd {
                id: "e".into(), ts: t(2),
                tool_call_id: "tc-late".into(), success: true,
            },
            E::AssistantMsg {
                id: "m".into(),
                ts: t(3),
                requests: vec![("tc-late".into(), serde_json::json!({"late": true}))],
            },
            E::TurnEnd { id: "te".into(), ts: t(4) },
        ];
        let ep = derive_episodes(&events, &meta());
        let bash = ep.tools.get("bash").unwrap();
        assert_eq!(
            bash.calls[0].arguments,
            Some(serde_json::json!({"late": true})),
            "PASS 0 must collect args before PASS 1 walks state machine"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p agentprof-core --lib args_plumbing 2>&1 | tail -15
```

Expected: 4 tests fail with `assertion failed: bash.calls[0].arguments == Some(...)` (or similar — the `ToolCall.arguments` field defaults to `None` since Task 3, but `derive_episodes` never stamps it).

- [ ] **Step 3: Add PASS 0 to derive_episodes**

In `crates/agentprof-core/src/episode/derive.rs`, modify the body of `pub fn derive_episodes`. Search for the `let mut state = State::default();` line near the top of the fn body.

INSERT before that line:

```rust
    // PASS 0: collect (tool_call_id → arguments) map by walking events once
    // before the state machine. ToolCall.arguments is then attached in
    // on_tool_complete via args_by_call_id.get(&call_id).cloned().
    // First-occurrence-wins on duplicate ids (D-4).
    let mut args_by_call_id: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    for ev in events {
        for (call_id, args) in ev.payload_tool_requests() {
            // entry(...).or_insert(...) returns the existing value on
            // duplicate id — log at debug level for diagnosability.
            let new_id = !args_by_call_id.contains_key(&call_id);
            args_by_call_id.entry(call_id.clone()).or_insert(args);
            if !new_id {
                tracing::debug!(
                    target: "derive",
                    tool_call_id = %call_id,
                    "duplicate tool_call_id args ignored (first-wins)"
                );
            }
        }
    }
```

Then thread `args_by_call_id` into `State`. INSIDE `struct State<'a>` (search for `struct State`), add a field:

```rust
    /// PASS 0 map: tool_call_id → args. Populated before state-machine
    /// walk; consulted on `on_tool_complete` to stamp ToolCall.arguments.
    args_by_call_id: std::collections::BTreeMap<String, serde_json::Value>,
```

If `State` is `#[derive(Default)]`, this works automatically (`BTreeMap` defaults to empty). If `State` has a manual `Default` impl, add `args_by_call_id: BTreeMap::new()` to the default body.

If `State::new(...)` exists and takes args, add the new field there too.

Initialize from PASS 0 in `derive_episodes`:

```rust
    let mut state = State {
        args_by_call_id,
        ..State::default()
    };
```

> **NOTE**: This relies on `State` being a struct with `#[derive(Default)]` or compatible field-update-shorthand support. If `State` uses a custom `Default` or a `pub fn new()` ctor, modify the initialization accordingly. INSPECT the existing `let mut state = ...` line and adapt.

- [ ] **Step 4: Stamp arguments on tool_complete**

Search for `fn on_tool_complete` in `derive.rs`. Find the line where the `ToolCall` is finalized and pushed into `tools[name].calls`. It currently looks roughly like:

```rust
            let mut tool_call = ToolCall::new(span);
            tool_call.turn_id = Some(turn_id);
            tool_call.status = status;
            tool_call.user_requested = user_requested;
            // … push into tools[name].calls
```

INSERT immediately before the `push`:

```rust
            tool_call.arguments = self.args_by_call_id.get(&tool_call_id).cloned();
```

> **NOTE**: The variable name for the tool_call_id is likely `tool_call_id` or `id` — check the local scope. The lookup is `get(&...)` then `cloned()` because the map owns `Value`s and we move-clone into the `ToolCall`. This is `O(log N_unique_call_ids)` per close.

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p agentprof-core --lib args_plumbing 2>&1 | tail -15
```

Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 6: Run full core suite + check snapshot impact**

```bash
cargo test -p agentprof-core --all-features 2>&1 | grep -E '^test result|FAILED'
```

Expected: all green. If `*.snap` files churn (some derive snapshot tests serialize Episodes JSON), inspect:

```bash
find crates/agentprof-core -name '*.snap.new' 2>/dev/null
```

If new snapshots have only **additive** `"arguments": ...` entries, accept them:

```bash
find crates/agentprof-core -name '*.snap.new' -exec sh -c \
  'echo "=== $1 ===" && diff -u "${1%.new}" "$1" | head -20' _ {} \;
# If additive only:
find crates/agentprof-core -name '*.snap.new' -exec sh -c 'mv "$1" "${1%.new}"' _ {} \;
```

- [ ] **Step 7: Run gates + commit**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-core --all-targets -- -D warnings && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-core

git add crates/agentprof-core/src/episode/derive.rs $(find crates/agentprof-core -name '*.snap' 2>/dev/null)
git -c commit.gpgsign=false commit -m "feat(core): derive_episodes — PASS 0 args map + stamp ToolCall.arguments

Add a small PASS 0 walk to derive_episodes that builds a
BTreeMap<tool_call_id, serde_json::Value> from
Event::payload_tool_requests() across all events. PASS 1 (the
existing state machine) then stamps ToolCall.arguments on
on_tool_complete via args_by_call_id.get(&tool_call_id).cloned().

First-occurrence-wins on duplicate tool_call_id (D-4); duplicates
trigger a tracing::debug! at target 'derive' for diagnosability.

End-to-end data plumbing complete:
  ToolRequest.arguments (adapter) ─▶ payload_tool_requests (trait)
                                  ─▶ PASS 0 args_by_call_id (derive)
                                  ─▶ ToolCall.arguments (episode model)

Total complexity stays O(N_events × max_requests_per_event) ⊆ O(N_events²)
worst-case, O(N_events) typical. Derive remains pure + total +
snapshot-stable.

4 new tests in args_plumbing_tests cover: args attached when
AssistantMsg precedes ToolEnd, args None when no matching request,
first-wins on duplicate, args attached when AssistantMsg arrives
after ToolEnd (PASS 0 ordering-independent).

Spec §3.2, ADR-0011 D-3 + D-4.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 5: TUI — `views/turn_detail.rs` pure helpers + state struct

**Files:**
- Create: `crates/agentprof-tui/src/views/turn_detail.rs`
- Modify: `crates/agentprof-tui/src/views/mod.rs` (add `pub mod turn_detail;` line)

**Why fifth:** Layer 3 begins. Pure helpers + state struct first (no rendering yet); render fn lands in Task 6. Following TDD: state machine + pure formatters first, then composed render fn.

- [ ] **Step 1: Create the file scaffold + state struct + tests**

Create `crates/agentprof-tui/src/views/turn_detail.rs`:

```rust
//! Full-screen detail view for a single turn — opens when the user
//! presses `Enter` on a selected turn in [`crate::views::flamegraph`].
//!
//! Renders one block per tool call in the turn (sorted by duration desc):
//! tool name (colored by [`agentprof_core::model::ToolSource`]), duration,
//! ✓/✗ status, source badge, and a one-line `args` preview (truncated to
//! 80 chars + `…`). Selecting a tool call (`↑`/`↓`/`j`/`k`/`G`/`gg`) and
//! pressing `Enter` toggles the call's `args` between truncated and
//! fully-expanded JSON. `Esc` returns to flamegraph; `1`/`2`/`3` pop
//! detail and switch views.
//!
//! See [`docs/superpowers/specs/2026-06-03-turn-detail-view-design.md`]
//! and ADR-0011 for the design rationale.

use std::collections::HashSet;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use agentprof_core::episode::ToolCallStatus;
use agentprof_core::model::ToolSource;

/// Per-detail-view persistent state. Lives on
/// [`crate::app::state::AppState::detail_view`] and (in watch mode)
/// on `WatchViewState::detail_view`.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::turn_detail::TurnDetailState;
/// let mut s = TurnDetailState::new("T3");
/// assert_eq!(s.turn_id, "T3");
/// assert_eq!(s.selected_tool_idx, 0);
/// s.move_down(5);
/// assert_eq!(s.selected_tool_idx, 1);
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TurnDetailState {
    /// Identifier of the turn being shown
    /// ([`agentprof_core::episode::Turn::id`]).
    pub turn_id: String,
    /// Currently selected tool_call index in the per-turn duration-sorted
    /// list. `0` when the turn has no tool calls.
    pub selected_tool_idx: usize,
    /// Tool-call indices whose args row is currently expanded
    /// (toggled by `Enter`).
    pub expanded_tools: HashSet<usize>,
    /// Vertical viewport offset (reserved for scroll past the visible
    /// rect; updated in render fn).
    pub viewport_top: u16,
    /// Vim-style `gg` two-key sequence in-progress flag, mirroring
    /// [`crate::app::state::AppState::pending_gg`] but scoped to the
    /// detail view.
    pub pending_gg: bool,
}

impl TurnDetailState {
    /// Construct a detail-view state pointing at the given turn id with
    /// the first tool call selected.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::views::turn_detail::TurnDetailState;
    /// let s = TurnDetailState::new("turn-abc");
    /// assert!(!s.pending_gg);
    /// assert!(s.expanded_tools.is_empty());
    /// ```
    #[must_use]
    pub fn new(turn_id: impl Into<String>) -> Self {
        Self {
            turn_id: turn_id.into(),
            selected_tool_idx: 0,
            expanded_tools: HashSet::new(),
            viewport_top: 0,
            pending_gg: false,
        }
    }

    /// Move selection up by one (saturating at 0).
    pub fn move_up(&mut self) {
        self.selected_tool_idx = self.selected_tool_idx.saturating_sub(1);
        self.pending_gg = false;
    }

    /// Move selection down by one (clamped to `max.saturating_sub(1)`).
    /// `max` is the number of tool calls in the turn.
    pub fn move_down(&mut self, max: usize) {
        if max > 0 && self.selected_tool_idx + 1 < max {
            self.selected_tool_idx += 1;
        }
        self.pending_gg = false;
    }

    /// Jump to first tool_call (`gg`).
    pub fn jump_first(&mut self) {
        self.selected_tool_idx = 0;
        self.pending_gg = false;
    }

    /// Jump to last tool_call (`G`). `max` is the number of tool calls.
    pub fn jump_last(&mut self, max: usize) {
        self.selected_tool_idx = max.saturating_sub(1);
        self.pending_gg = false;
    }

    /// Toggle args expansion for the selected tool_call.
    pub fn toggle_expand(&mut self) {
        if !self.expanded_tools.insert(self.selected_tool_idx) {
            self.expanded_tools.remove(&self.selected_tool_idx);
        }
        self.pending_gg = false;
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn new_initializes_first_selected() {
        let s = TurnDetailState::new("t1");
        assert_eq!(s.turn_id, "t1");
        assert_eq!(s.selected_tool_idx, 0);
        assert!(s.expanded_tools.is_empty());
        assert_eq!(s.viewport_top, 0);
        assert!(!s.pending_gg);
    }

    #[test]
    fn move_up_saturates_at_zero() {
        let mut s = TurnDetailState::new("t");
        s.move_up();
        assert_eq!(s.selected_tool_idx, 0);
    }

    #[test]
    fn move_down_clamps_at_max() {
        let mut s = TurnDetailState::new("t");
        s.selected_tool_idx = 2;
        s.move_down(3);
        assert_eq!(s.selected_tool_idx, 2, "already at last");
        s.move_down(4);
        assert_eq!(s.selected_tool_idx, 3, "advances when room");
    }

    #[test]
    fn move_down_zero_max_no_panic() {
        let mut s = TurnDetailState::new("t");
        s.move_down(0);
        assert_eq!(s.selected_tool_idx, 0);
    }

    #[test]
    fn jump_last_handles_empty() {
        let mut s = TurnDetailState::new("t");
        s.jump_last(0);
        assert_eq!(s.selected_tool_idx, 0);
        s.jump_last(7);
        assert_eq!(s.selected_tool_idx, 6);
    }

    #[test]
    fn toggle_expand_flips() {
        let mut s = TurnDetailState::new("t");
        s.toggle_expand();
        assert!(s.expanded_tools.contains(&0));
        s.toggle_expand();
        assert!(!s.expanded_tools.contains(&0));
    }

    #[test]
    fn movement_clears_pending_gg() {
        let mut s = TurnDetailState::new("t");
        s.pending_gg = true;
        s.move_down(2);
        assert!(!s.pending_gg);
        s.pending_gg = true;
        s.jump_first();
        assert!(!s.pending_gg);
    }
}

// === Pure formatters (rendering helpers) ==================================

/// Format a `Some(args)` JSON value as a single-line preview, truncated to
/// `max_chars` characters with a trailing `…` when over budget. `None`
/// yields the dim-rendered `(not captured)` placeholder text.
///
/// Uses `chars().count()` not byte length; CJK/wide-glyph widths are
/// approximate (off by ±1 cell). Documented limitation per ADR-0011 D-11.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::turn_detail::format_args_preview;
/// use serde_json::json;
///
/// let v = json!({"command": "ls -la"});
/// let s = format_args_preview(Some(&v), 80);
/// assert!(s.starts_with("{"));
///
/// assert_eq!(format_args_preview(None, 80), "(not captured)");
///
/// let big = json!({"x": "a".repeat(200)});
/// let s = format_args_preview(Some(&big), 30);
/// assert!(s.ends_with('…'));
/// assert!(s.chars().count() <= 30);
/// ```
#[must_use]
pub fn format_args_preview(args: Option<&serde_json::Value>, max_chars: usize) -> String {
    let Some(v) = args else {
        return "(not captured)".to_string();
    };
    // serde_json::to_string emits a single-line compact form.
    let s = serde_json::to_string(v).unwrap_or_else(|_| "<unserializable>".to_string());
    if s.chars().count() <= max_chars {
        s
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

/// Format `Some(args)` as multi-line pretty JSON, wrapped to `width`
/// columns. `None` yields a single `(not captured)` line.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::turn_detail::wrap_args_full;
/// use serde_json::json;
///
/// let v = json!({"a": 1, "b": [2, 3]});
/// let lines = wrap_args_full(Some(&v), 40);
/// assert!(!lines.is_empty());
/// assert!(lines.iter().all(|l| l.chars().count() <= 40));
///
/// let lines = wrap_args_full(None, 40);
/// assert_eq!(lines, vec!["(not captured)".to_string()]);
/// ```
#[must_use]
pub fn wrap_args_full(args: Option<&serde_json::Value>, width: usize) -> Vec<String> {
    let Some(v) = args else {
        return vec!["(not captured)".to_string()];
    };
    let pretty = serde_json::to_string_pretty(v).unwrap_or_else(|_| "<unserializable>".to_string());
    let mut out = Vec::new();
    let width = width.max(1);
    for raw_line in pretty.lines() {
        if raw_line.chars().count() <= width {
            out.push(raw_line.to_string());
        } else {
            // Word-wrap on whitespace; fallback to char-chunk for long
            // tokens (e.g. long strings without spaces).
            let mut cur = String::new();
            for word in raw_line.split_whitespace() {
                let candidate_len = cur.chars().count()
                    + (!cur.is_empty() as usize)
                    + word.chars().count();
                if candidate_len <= width {
                    if !cur.is_empty() {
                        cur.push(' ');
                    }
                    cur.push_str(word);
                } else {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                    // Word itself longer than width → char-chunk it.
                    if word.chars().count() > width {
                        let mut chunk = String::new();
                        for c in word.chars() {
                            if chunk.chars().count() == width {
                                out.push(std::mem::take(&mut chunk));
                            }
                            chunk.push(c);
                        }
                        cur = chunk;
                    } else {
                        cur = word.to_string();
                    }
                }
            }
            if !cur.is_empty() {
                out.push(cur);
            }
        }
    }
    out
}

/// Status sigil for the per-call status badge.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::ToolCallStatus;
/// use agentprof_tui::views::turn_detail::status_sigil;
/// assert_eq!(status_sigil(&ToolCallStatus::Success), "✓");
/// assert_eq!(
///     status_sigil(&ToolCallStatus::Failure { message: None }),
///     "✗"
/// );
/// ```
#[must_use]
pub fn status_sigil(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Success => "✓",
        ToolCallStatus::Failure { .. } => "✗",
        ToolCallStatus::OrphanEnd { .. } => "?",
    }
}

#[cfg(test)]
mod formatter_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_args_preview_exact_80_chars_no_truncation() {
        let s_len80 = "a".repeat(76); // ←"{\"k\":\""(6) + 76 + "\"}"(2) = 84... need <= 80
        let v = json!({"k": s_len80});
        let s = format_args_preview(Some(&v), 80);
        // Truncation: actual JSON is longer than 80, so should end with …
        assert!(s.chars().count() <= 80);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn format_args_preview_short_no_truncation() {
        let v = json!({"x": 1});
        let s = format_args_preview(Some(&v), 80);
        assert_eq!(s, r#"{"x":1}"#);
    }

    #[test]
    fn format_args_preview_none_yields_not_captured() {
        assert_eq!(format_args_preview(None, 80), "(not captured)");
    }

    #[test]
    fn format_args_preview_zero_max_yields_ellipsis_only_or_empty() {
        let v = json!({"x": 1});
        let s = format_args_preview(Some(&v), 0);
        // saturating_sub(1) → take(0) → "" + "…" = "…", char count == 1 > 0.
        // We tolerate the ellipsis-only single-char string at boundary.
        assert!(s.chars().count() <= 1);
    }

    #[test]
    fn wrap_args_full_pretty_short_fits() {
        let v = json!({"a": 1});
        let lines = wrap_args_full(Some(&v), 80);
        assert!(lines.iter().all(|l| l.chars().count() <= 80));
        // pretty JSON of {"a":1} is "{\n  \"a\": 1\n}" → 3 lines.
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn wrap_args_full_none_yields_not_captured() {
        assert_eq!(
            wrap_args_full(None, 80),
            vec!["(not captured)".to_string()]
        );
    }

    #[test]
    fn wrap_args_full_zero_width_no_panic() {
        let v = json!({"a": 1});
        let _ = wrap_args_full(Some(&v), 0);
    }

    #[test]
    fn status_sigil_covers_all_variants() {
        assert_eq!(status_sigil(&ToolCallStatus::Success), "✓");
        assert_eq!(
            status_sigil(&ToolCallStatus::Failure { message: None }),
            "✗"
        );
        // OrphanEnd has an EventId-typed field; construct via the public
        // ctor or skip if the variant's exhaustive shape blocks.
    }
}

// render_turn_detail() lands in Task 6
```

- [ ] **Step 2: Register the module**

Modify `crates/agentprof-tui/src/views/mod.rs` — add `pub mod turn_detail;` to the existing list of `pub mod` declarations. Order alphabetically with the existing entries (`aggregate`, `flamegraph`, `format`, `roi`, then `turn_detail`).

- [ ] **Step 3: Run tests**

```bash
cargo test -p agentprof-tui --lib turn_detail 2>&1 | tail -15
```

Expected: ~17 tests pass (8 state tests + 8 formatter tests + 1 sigil test).

- [ ] **Step 4: Run gates + commit**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-tui --all-targets -- -D warnings && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-tui

git add crates/agentprof-tui/src/views/turn_detail.rs crates/agentprof-tui/src/views/mod.rs
git -c commit.gpgsign=false commit -m "feat(tui): views::turn_detail state struct + pure formatters

Layer 3 scaffold — TurnDetailState (turn_id, selected_tool_idx,
expanded_tools HashSet, viewport_top, pending_gg) + state-machine
helpers (new/move_up/move_down/jump_first/jump_last/toggle_expand)
+ pure formatters (format_args_preview, wrap_args_full, status_sigil).

No rendering yet — render_turn_detail lands in the next task. This
commit decomposes the view into:
- state machine (movement-only, no IO)
- pure pretty-printing helpers (testable in isolation)

so the eventual ratatui composition is a thin shell over tested units.

17 inline tests (8 state + 8 formatter + 1 sigil) cover boundary
cases for movement saturation, expand toggle, format truncation,
wrap zero-width, and not-captured placeholder.

Spec §3.4, ADR-0011 D-9 + D-10 + D-11.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 6: TUI — `render_turn_detail()` ratatui composition

**Files:**
- Modify: `crates/agentprof-tui/src/views/turn_detail.rs` (append `render_turn_detail`)
- Test: `crates/agentprof-tui/src/views/turn_detail.rs` `#[cfg(test)]` (render unit tests)

**Why sixth:** Compose the pure helpers from Task 5 into a frame-level render. The render fn is intentionally last because it depends on `AppState<'_>` (Task 5 didn't need it) and the test pattern requires snapshot infrastructure.

- [ ] **Step 1: Write a render unit test that drives the helper composition**

Append inside `crates/agentprof-tui/src/views/turn_detail.rs` `#[cfg(test)]` section:

```rust
#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::app::state::AppState;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::AnalysisReport;
    use agentprof_core::episode::{
        CallRef, Episodes, Span as EpSpan, ToolCall, ToolCallStatus, ToolEpisode, Turn,
    };
    use agentprof_core::model::{SessionMeta, ToolSource};
    use chrono::{Duration, TimeZone, Utc};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn fixture() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta.clone());

        let mut episodes = Episodes::new();
        // Add a "bash" tool episode with one call carrying args.
        let span = EpSpan::new(
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 1).unwrap(),
        );
        let mut tc = ToolCall::new(span);
        tc.status = ToolCallStatus::Success;
        tc.arguments = Some(serde_json::json!({"command": "ls -la"}));
        let mut tool_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        tool_ep.calls.push(tc);
        episodes.tools.insert("bash".into(), tool_ep);

        // Add a Turn referencing that call.
        let mut turn = Turn::new(
            "T1".into(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
        );
        turn.ended_at = Some(Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 2).unwrap());
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        episodes.turns.push(turn);

        (report, episodes)
    }

    fn buffer_to_symbol_grid(buffer: &ratatui::buffer::Buffer) -> String {
        let cells_per_row = buffer.area.width as usize;
        let mut text = String::with_capacity(buffer.content.len() + buffer.area.height as usize);
        for (i, cell) in buffer.content.iter().enumerate() {
            if i > 0 && i % cells_per_row == 0 {
                text.push('\n');
            }
            text.push_str(cell.symbol());
        }
        text
    }

    #[test]
    fn render_one_tool_turn_does_not_panic() {
        let (report, episodes) = fixture();
        let state = AppState::new(&report, &episodes);
        let detail = TurnDetailState::new("T1");

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_turn_detail(f, f.area(), &detail, &state))
            .unwrap();
        let grid = buffer_to_symbol_grid(terminal.backend().buffer());
        // Expect: tool name "bash" appears somewhere in the rendered grid.
        assert!(grid.contains("bash"), "rendered grid missing 'bash': {grid}");
    }

    #[test]
    fn render_missing_turn_shows_diagnostic() {
        let (report, episodes) = fixture();
        let state = AppState::new(&report, &episodes);
        let detail = TurnDetailState::new("nonexistent-turn");

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_turn_detail(f, f.area(), &detail, &state))
            .unwrap();
        let grid = buffer_to_symbol_grid(terminal.backend().buffer());
        // Expect a "(turn not found)" message.
        assert!(
            grid.contains("not found") || grid.contains("nonexistent"),
            "render_missing_turn: expected diagnostic message: {grid}"
        );
    }

    #[test]
    fn render_empty_tool_calls_shows_no_tool_calls() {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta.clone());
        let mut episodes = Episodes::new();
        let turn = Turn::new(
            "T-empty".into(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
        );
        episodes.turns.push(turn);

        let state = AppState::new(&report, &episodes);
        let detail = TurnDetailState::new("T-empty");

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_turn_detail(f, f.area(), &detail, &state))
            .unwrap();
        let grid = buffer_to_symbol_grid(terminal.backend().buffer());
        assert!(
            grid.contains("no tool calls"),
            "expected '(no tool calls)' placeholder: {grid}"
        );
    }
}
```

- [ ] **Step 2: Run the failing tests**

```bash
cargo test -p agentprof-tui --lib turn_detail::render_tests 2>&1 | tail -10
```

Expected: `error[E0425]: cannot find function 'render_turn_detail' in this scope`.

- [ ] **Step 3: Implement `render_turn_detail`**

Append to `crates/agentprof-tui/src/views/turn_detail.rs` (after the formatter helpers, before `#[cfg(test)]`):

```rust
use crate::theme;

/// Render the full-screen detail view for the turn referenced by
/// `state.turn_id`. If the turn id is not found in
/// `app_state.episodes.turns`, renders a `(turn not found)` diagnostic.
///
/// Tool calls are sorted by `Span::duration()` descending. Each call
/// emits a header line `<sigil> <name> <dur> <status> <source>` and a
/// `└ args:` follower line — single-line truncated preview by default,
/// or full pretty-printed wrap when the index is in
/// `state.expanded_tools`.
///
/// # Examples
///
/// ```no_run
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use agentprof_tui::app::state::AppState;
/// use agentprof_tui::views::turn_detail::{render_turn_detail, TurnDetailState};
/// use chrono::Utc;
/// use ratatui::backend::TestBackend;
/// use ratatui::Terminal;
///
/// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
/// let report = AnalysisReport::new(meta);
/// let episodes = Episodes::new();
/// let state = AppState::new(&report, &episodes);
/// let detail = TurnDetailState::new("any");
///
/// let backend = TestBackend::new(80, 20);
/// let mut terminal = Terminal::new(backend).unwrap();
/// terminal
///     .draw(|f| render_turn_detail(f, f.area(), &detail, &state))
///     .unwrap();
/// ```
pub fn render_turn_detail(
    f: &mut Frame<'_>,
    area: Rect,
    state: &TurnDetailState,
    app_state: &crate::app::state::AppState<'_>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Turn {} ", state.turn_id));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Look up the turn by id.
    let turn = app_state
        .episodes
        .turns
        .iter()
        .find(|t| t.id == state.turn_id);
    let Some(turn) = turn else {
        let p = Paragraph::new(format!("(turn {} not found)", state.turn_id))
            .style(Style::default().fg(Color::Red));
        f.render_widget(p, inner);
        return;
    };

    if turn.tool_calls.is_empty() {
        let p = Paragraph::new("(no tool calls)")
            .style(Style::default().add_modifier(Modifier::DIM));
        f.render_widget(p, inner);
        return;
    }

    // Resolve CallRef → (name, ToolSource, ToolCall) and sort by duration desc.
    let mut resolved: Vec<(String, ToolSource, &agentprof_core::episode::ToolCall)> = turn
        .tool_calls
        .iter()
        .filter_map(|cref| {
            app_state
                .episodes
                .tools
                .get(&cref.name)
                .and_then(|ep| ep.calls.get(cref.index).map(|c| (ep.name.clone(), ep.source, c)))
        })
        .collect();
    resolved.sort_by(|(_, _, a), (_, _, b)| b.span.duration().cmp(&a.span.duration()));

    // Compose lines. Header line + args follower per call, separated by blank.
    let mut lines: Vec<Line<'static>> = Vec::new();
    let width = inner.width as usize;

    // Turn-level header.
    let total_ms = turn
        .duration()
        .num_milliseconds()
        .max(0) as u64;
    let tool_count = turn.tool_calls.len();
    lines.push(Line::from(format!(
        "{}ms wall · {} tool calls",
        total_ms, tool_count
    )));
    lines.push(Line::from(""));

    for (idx, (name, source, call)) in resolved.iter().enumerate() {
        let is_selected = idx == state.selected_tool_idx;
        let is_expanded = state.expanded_tools.contains(&idx);
        let marker = if is_selected { "▶ " } else { "  " };
        let dur_ms = call.span.duration().num_milliseconds().max(0);
        let sigil = status_sigil(&call.status);
        let source_label = match source {
            ToolSource::Builtin => "builtin",
            ToolSource::Mcp => "mcp",
            ToolSource::Skill => "skill",
            _ => "unknown",
        };
        let color = theme::tool_source_color(*source);
        let mut header_spans: Vec<Span<'static>> = Vec::new();
        header_spans.push(Span::raw(marker.to_string()));
        header_spans.push(Span::styled(name.clone(), Style::default().fg(color)));
        header_spans.push(Span::raw(format!("  {dur_ms}ms  {sigil}  {source_label}")));
        lines.push(Line::from(header_spans));

        if is_expanded {
            for raw in wrap_args_full(call.arguments.as_ref(), width.saturating_sub(8)) {
                lines.push(Line::from(format!("       {raw}")));
            }
        } else {
            let preview = format_args_preview(call.arguments.as_ref(), 80);
            let args_line = format!("   └ args: {preview}");
            lines.push(Line::from(Span::styled(
                args_line,
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        lines.push(Line::from(""));
    }

    // Footer hint line (rendered as the last line if room permits).
    let mut all_lines = lines;
    let selected_name = resolved
        .get(state.selected_tool_idx)
        .map(|(n, _, _)| n.as_str())
        .unwrap_or("");
    let expand_label = if state.expanded_tools.contains(&state.selected_tool_idx) {
        "Enter collapse"
    } else {
        "Enter expand"
    };
    all_lines.push(Line::from(format!(
        "selected: {selected_name} · {expand_label} · Esc return · j/k G/gg navigate"
    )));

    let p = Paragraph::new(all_lines).scroll((state.viewport_top, 0));
    f.render_widget(p, inner);
}
```

- [ ] **Step 4: Run render tests**

```bash
cargo test -p agentprof-tui --lib turn_detail::render_tests 2>&1 | tail -15
```

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 5: Run gates + commit**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-tui --all-targets -- -D warnings && \
cargo test -p agentprof-tui --all-features 2>&1 | grep -E '^test result|FAILED' && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-tui

git add crates/agentprof-tui/src/views/turn_detail.rs
git -c commit.gpgsign=false commit -m "feat(tui): render_turn_detail full-screen view

Compose the Task-5 pure helpers (format_args_preview, wrap_args_full,
status_sigil) + state machine into a ratatui frame-level render.

Looks up turn by id in app_state.episodes.turns; renders:
- (turn not found) red diagnostic when id missing
- (no tool calls) dim placeholder when turn has no tool_calls
- otherwise: one block per tool call sorted by duration desc, each
  block being header line (\"▶ name  dur  ✓  source\" with name in
  source-color) + args follower (single-line truncated preview or
  multi-line pretty-wrapped when call idx is in expanded_tools)

Footer hint shows selected tool name + Enter expand/collapse hint
+ Esc return + j/k G/gg navigate.

3 render tests cover: one-tool turn renders 'bash', missing turn
diagnostic, empty tool_calls placeholder.

AppState integration + key dispatch lands in Task 7.

Spec §2.1 + §3.4 + §4, ADR-0011 D-7 + D-8 + D-10.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 7: TUI — `AppState.detail_view` field + key dispatch

**Files:**
- Modify: `crates/agentprof-tui/src/app/state.rs:54-91` (add field) + `:147-240` (dispatch) + `:113-128` (ctor default)
- Modify: `crates/agentprof-tui/src/app/mod.rs:121-128` (render fork)
- Test: `crates/agentprof-tui/src/app/state.rs` `#[cfg(test)]`

**Why seventh:** Wires Task 5+6 into the existing single-session `AppRunner`. After this Task, `analyze --export tui` users can press Enter and see detail.

- [ ] **Step 1: Write failing dispatch tests**

Append to (or create) `#[cfg(test)] mod dispatch_tests` at the bottom of `crates/agentprof-tui/src/app/state.rs`:

```rust
#[cfg(test)]
mod detail_view_dispatch_tests {
    use super::*;
    use crate::app::Event;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::AnalysisReport;
    use agentprof_core::episode::{
        CallRef, Episodes, Span as EpSpan, ToolCall, ToolCallStatus, ToolEpisode, Turn,
    };
    use agentprof_core::model::{SessionMeta, ToolSource};
    use chrono::{TimeZone, Utc};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn build_state_with_turn() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta.clone());

        let mut episodes = Episodes::new();
        let span = EpSpan::new(
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 1).unwrap(),
        );
        let mut tc = ToolCall::new(span);
        tc.status = ToolCallStatus::Success;
        let mut tool_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        tool_ep.calls.push(tc);
        episodes.tools.insert("bash".into(), tool_ep);

        let mut turn = Turn::new(
            "T1".into(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
        );
        turn.ended_at = Some(Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 2).unwrap());
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        episodes.turns.push(turn);

        (report, episodes)
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::empty()))
    }

    #[test]
    fn detail_view_starts_none() {
        let (r, e) = build_state_with_turn();
        let s = AppState::new(&r, &e);
        assert!(s.detail_view.is_none());
    }

    #[test]
    fn enter_on_flamegraph_with_valid_selection_opens_detail() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        s.flame_selected = 0;
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(s.detail_view.is_some());
        assert_eq!(s.detail_view.as_ref().unwrap().turn_id, "T1");
    }

    #[test]
    fn esc_in_detail_closes_detail_preserves_flame_selected() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.flame_selected = 0;
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Esc));
        assert!(s.detail_view.is_none());
        assert_eq!(s.flame_selected, 0);
    }

    #[test]
    fn enter_in_detail_toggles_expansion() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(s.detail_view.as_ref().unwrap().expanded_tools.contains(&0));
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(!s.detail_view.as_ref().unwrap().expanded_tools.contains(&0));
    }

    #[test]
    fn jk_in_detail_navigate_tool_calls() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        // Build a detail state with 2 calls (need to extend fixture briefly).
        let mut detail = crate::views::turn_detail::TurnDetailState::new("T1");
        detail.selected_tool_idx = 0;
        s.detail_view = Some(detail);
        // Only 1 call in this fixture, so move_down is no-op.
        let _ = dispatch(&mut s, key(KeyCode::Char('j')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 0);
        // move_up from 0 is also no-op.
        let _ = dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 0);
    }

    #[test]
    fn number_keys_in_detail_pop_then_switch_view() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Char('2')));
        assert!(s.detail_view.is_none(), "1/2/3 pops detail");
        assert_eq!(s.view, View::Roi, "and switches view");
    }

    #[test]
    fn enter_on_flamegraph_invalid_selection_no_panic() {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta);
        let episodes = Episodes::new(); // no turns
        let mut s = AppState::new(&report, &episodes);
        s.view = View::Flamegraph;
        s.flame_selected = 0;
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(s.detail_view.is_none(), "no-op when no turn at index");
    }

    #[test]
    fn q_quits_even_in_detail_view() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let act = dispatch(&mut s, key(KeyCode::Char('q')));
        assert!(matches!(act, Action::Quit));
    }
}
```

- [ ] **Step 2: Add the `detail_view` field**

Modify `crates/agentprof-tui/src/app/state.rs` `pub struct AppState<'a>` (around line 54-91). Add at the end of the field list (before `report` / `episodes`):

```rust
    /// Optional full-screen detail view for the currently-selected turn.
    /// `Some` after the user presses `Enter` on a turn row in
    /// [`crate::views::flamegraph`]; cleared by `Esc` or `1`/`2`/`3`.
    pub detail_view: Option<crate::views::turn_detail::TurnDetailState>,
```

Modify `pub fn new` (around line 113-128) to default the new field:

```rust
    #[must_use]
    pub fn new(report: &'a AnalysisReport, episodes: &'a Episodes) -> Self {
        Self {
            view: View::Flamegraph,
            scroll: HashMap::new(),
            roi_sort: SortKey::default(),
            roi_selected: 0,
            flame_selected: 0,
            help_open: false,
            flame_viewport_top: std::cell::Cell::new(0),
            roi_viewport_top: std::cell::Cell::new(0),
            pending_gg: false,
            detail_view: None,
            report,
            episodes,
        }
    }
```

- [ ] **Step 3: Add the detail-view dispatch branch to `dispatch()`**

Modify `crates/agentprof-tui/src/app/state.rs` `pub fn dispatch` (currently starting at line 147). After the global `q`/Ctrl-C check (around line 170, right BEFORE the vim G/gg block), insert the detail-view dispatch:

```rust
    // Detail-view dispatch — when detail_view is Some, certain keys go
    // exclusively to the detail state; 1/2/3 pop detail then fall through
    // to top-level view-switch dispatch; q/? stay global (handled above
    // for q, fall through below for ?).
    if let Some(detail) = state.detail_view.as_mut() {
        let count = state
            .episodes
            .turns
            .iter()
            .find(|t| t.id == detail.turn_id)
            .map(|t| t.tool_calls.len())
            .unwrap_or(0);
        match k.code {
            KeyCode::Esc => {
                state.detail_view = None;
                state.pending_gg = false;
                return Action::None;
            }
            KeyCode::Enter => {
                detail.toggle_expand();
                return Action::None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                detail.move_up();
                return Action::None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                detail.move_down(count);
                return Action::None;
            }
            KeyCode::Char('G') => {
                detail.jump_last(count);
                return Action::None;
            }
            KeyCode::Char('g') if !k.modifiers.contains(KeyModifiers::SHIFT) => {
                if detail.pending_gg {
                    detail.jump_first();
                } else {
                    detail.pending_gg = true;
                }
                return Action::None;
            }
            KeyCode::Char('g') if k.modifiers.contains(KeyModifiers::SHIFT) => {
                detail.jump_last(count);
                return Action::None;
            }
            KeyCode::Char('1' | '2' | '3') => {
                // Pop detail + fall through to top-level view-switch
                // dispatch (below).
                state.detail_view = None;
                state.pending_gg = false;
                // (fall through)
            }
            KeyCode::Char('?') => {
                // Fall through to existing help_open toggle.
                detail.pending_gg = false;
            }
            _ => {
                // Swallow unknown key; clear pending_gg defensively.
                detail.pending_gg = false;
                return Action::None;
            }
        }
    }
```

Also: add a NEW branch right after the existing letter-keys-for-Roi block (around line 226) and BEFORE the `match k.code` Tab/help/scroll block — to handle the `Enter` key from Flamegraph view opening the detail:

```rust
    // Open detail view: Enter on Flamegraph with a valid turn selection.
    if state.view == View::Flamegraph
        && state.detail_view.is_none()
        && matches!(k.code, KeyCode::Enter)
    {
        let idx = state.flame_selected;
        if let Some(turn) = state.episodes.turns.get(idx) {
            state.detail_view = Some(crate::views::turn_detail::TurnDetailState::new(
                turn.id.clone(),
            ));
        }
        return Action::None;
    }
```

> **NOTE on key handling for vim `G` with SHIFT**: re-read line 175-181 of the existing code to see how `is_capital_g` / `is_lowercase_g` are computed — the existing convention is to accept both `KeyCode::Char('G')` AND `KeyCode::Char('g')` + SHIFT modifier. Mirror this in the detail-view branch above for correctness on all terminal emulators.

- [ ] **Step 4: Wire the render fork in `app/mod.rs`**

Modify `crates/agentprof-tui/src/app/mod.rs` (around `pub fn draw_frame` or `fn draw` body, line 121-128 area). Find the body that currently looks like:

```rust
        match self.state.view {
            View::Flamegraph => flamegraph::render(frame, area, &self.state),
            View::Roi => roi::render(frame, area, &self.state),
            View::Aggregate => aggregate::render(frame, area, &self.state),
        }
        if self.state.help_open {
            draw_help_overlay(frame, area);
        }
```

Replace with:

```rust
        if let Some(detail) = self.state.detail_view.as_ref() {
            crate::views::turn_detail::render_turn_detail(frame, area, detail, &self.state);
        } else {
            match self.state.view {
                View::Flamegraph => flamegraph::render(frame, area, &self.state),
                View::Roi => roi::render(frame, area, &self.state),
                View::Aggregate => aggregate::render(frame, area, &self.state),
            }
        }
        if self.state.help_open {
            draw_help_overlay(frame, area);
        }
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p agentprof-tui --lib detail_view_dispatch 2>&1 | tail -15
```

Expected: 8 tests pass.

```bash
cargo test -p agentprof-tui --all-features 2>&1 | grep -E '^test result|FAILED'
```

Expected: all green; no snapshot churn (the snapshot tests in `tests/views.rs` never open detail, so they render the same paths).

- [ ] **Step 6: Run gates + commit**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-tui --all-targets -- -D warnings && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-tui

git add crates/agentprof-tui/src/app/state.rs crates/agentprof-tui/src/app/mod.rs
git -c commit.gpgsign=false commit -m "feat(tui): AppState.detail_view + Enter-to-open + dispatch wiring

Hook the Task 5+6 TurnDetailView into AppRunner:
- AppState gains pub detail_view: Option<TurnDetailState> field
  (#[non_exhaustive] preserved; new() ctor defaults to None).
- dispatch() learns: Enter on Flamegraph with valid flame_selected
  opens detail_view; in-detail keys Esc/Enter/j/k/↑/↓/G/gg route to
  TurnDetailState methods; 1/2/3 pop detail then fall through to
  top-level view-switch; q always quits; unknown keys swallowed.
- app/mod.rs render fork: when detail_view.is_some(), render
  turn_detail::render_turn_detail in place of the per-view dispatch.

8 dispatch tests cover: starts-None, Enter opens, Esc closes +
preserves flame_selected, Enter-in-detail toggles expand, j/k
saturating navigate, 1/2/3 pops-then-switches, Enter on empty
episodes no-ops, q quits even in detail.

WatchRunner integration lands in Task 8 (cross-render persistence
via WatchViewState.detail_view round-trip).

Spec §2.1 + §3.4, ADR-0011 D-9 + D-15.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 8: TUI — `WatchViewState.detail_view` + reload safety

**Files:**
- Modify: `crates/agentprof-tui/src/watch.rs:163-174` (WatchViewState field) + `:394-411` (render round-trip) + `:453-464` (dispatch round-trip) + `:478-491` (do_reload safety)
- Test: `crates/agentprof-tui/tests/watch_runner.rs` (append) or `crates/agentprof-tui/src/watch.rs` inline

**Why eighth:** Wires detail_view into WatchRunner so `watch ...` users have parity with `analyze --export tui`. WatchRunner reconstructs transient AppState every frame + every key dispatch — detail_view must round-trip through WatchViewState.

- [ ] **Step 1: Write the failing test**

Append to `crates/agentprof-tui/tests/watch_runner.rs` (look for an existing test fn pattern to mirror):

```rust
#[test]
fn watch_view_state_persists_detail_view_field() {
    use agentprof_tui::watch::WatchViewState;
    let s = WatchViewState::default();
    assert!(s.detail_view.is_none(), "WatchViewState defaults to detail_view = None");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p agentprof-tui --test watch_runner watch_view_state_persists_detail_view 2>&1 | tail -10
```

Expected: `error[E0609]: no field 'detail_view' on type 'WatchViewState'`.

- [ ] **Step 3: Add the field**

Modify `crates/agentprof-tui/src/watch.rs` `pub struct WatchViewState` (around line 163-174):

```rust
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct WatchViewState {
    pub agg_sort: AggSortKey,
    pub agg_selected: usize,
    pub help_overlay: bool,
    pub pending_gg: bool,
    /// Mirrors [`crate::app::state::AppState::detail_view`] across
    /// WatchRunner's transient-AppState reconstruction. Cleared by
    /// [`WatchRunner::do_reload`] when the cached `turn_id` is no longer
    /// present in the reloaded `Episodes`.
    pub detail_view: Option<crate::views::turn_detail::TurnDetailState>,
}
```

- [ ] **Step 4: Round-trip in render path**

Modify the WatchData::Single arm of `render_into` (around line 394-401):

```rust
            WatchData::Single {
                report, episodes, ..
            } => {
                let mut transient = AppState::new(report, episodes);
                transient.help_open = self.view_state.help_overlay;
                transient.detail_view = self.view_state.detail_view.clone();
                // Render through the app::mod path so render fork picks up
                // detail_view automatically.
                if let Some(detail) = transient.detail_view.as_ref() {
                    crate::views::turn_detail::render_turn_detail(
                        frame,
                        body_area,
                        detail,
                        &transient,
                    );
                } else {
                    views::aggregate::render(frame, body_area, &transient);
                }
            }
```

> **NOTE**: the existing render call was `views::aggregate::render` — verify by reading lines 395-401 first. If the existing code calls a different per-view dispatcher, replace just the detail-view branch around it (the goal is: detail_view present → render_turn_detail; detail_view absent → original behavior).

- [ ] **Step 5: Round-trip in dispatch path**

Modify the `Single` dispatch arm (around line 453-464):

```rust
                    if let WatchData::Single {
                        report, episodes, ..
                    } = &self.data
                    {
                        let mut transient = AppState::new(report, episodes);
                        transient.help_open = self.view_state.help_overlay;
                        transient.detail_view = self.view_state.detail_view.clone();
                        match dispatch(&mut transient, ev) {
                            Action::Quit => return Ok(()),
                            Action::None => {
                                self.view_state.help_overlay = transient.help_open;
                                self.view_state.detail_view = transient.detail_view;
                            }
                        }
                    }
```

- [ ] **Step 6: Reload safety in `do_reload`**

Modify `crates/agentprof-tui/src/watch.rs` `fn do_reload` (around line 478-491):

```rust
    fn do_reload(&mut self) {
        if let Some(cb) = self.reload.as_mut() {
            match cb() {
                Ok(new_data) => {
                    self.data = new_data;
                    self.refresh_count = self.refresh_count.saturating_add(1);
                    self.last_error = None;

                    // Drop detail_view if its cached turn_id no longer
                    // exists in the reloaded Episodes (Single mode only;
                    // Cross mode doesn't support detail view).
                    if let WatchData::Single { episodes, .. } = &self.data {
                        if let Some(dv) = self.view_state.detail_view.as_ref() {
                            let still_present = episodes
                                .turns
                                .iter()
                                .any(|t| t.id == dv.turn_id);
                            if !still_present {
                                let id = dv.turn_id.clone();
                                self.view_state.detail_view = None;
                                self.last_error = Some(format!(
                                    "turn {id} disappeared after reload"
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    self.last_error = Some(e.to_string());
                }
            }
        }
    }
```

- [ ] **Step 7: Add reload-drop integration test**

Append to `crates/agentprof-tui/tests/watch_runner.rs`:

```rust
#[test]
fn reload_drops_detail_view_when_turn_disappears() {
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::AnalysisReport;
    use agentprof_core::episode::{
        CallRef, Episodes, Span as EpSpan, ToolCall, ToolEpisode, Turn,
    };
    use agentprof_core::model::{SessionMeta, ToolSource};
    use agentprof_tui::watch::{WatchData, WatchRunner};
    use chrono::{TimeZone, Utc};

    fn fixture_with_t1() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta.clone());
        let mut episodes = Episodes::new();
        let span = EpSpan::new(
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 1).unwrap(),
        );
        let mut tc = ToolCall::new(span);
        tc.status = agentprof_core::episode::ToolCallStatus::Success;
        let mut tool_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        tool_ep.calls.push(tc);
        episodes.tools.insert("bash".into(), tool_ep);
        let mut turn = Turn::new(
            "T1".into(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
        );
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        episodes.turns.push(turn);
        (report, episodes)
    }

    fn fixture_empty() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta);
        (report, Episodes::new())
    }

    let (r1, e1) = fixture_with_t1();
    let initial = WatchData::Single { report: r1, episodes: e1 };
    let mut runner = WatchRunner::new_static(initial);
    runner.view_state_mut().detail_view = Some(
        agentprof_tui::views::turn_detail::TurnDetailState::new("T1"),
    );

    // Simulate a reload that returns the empty fixture.
    let reload_call = std::sync::Arc::new(std::sync::Mutex::new(0u32));
    let rc = reload_call.clone();
    runner = runner.with_reload(Box::new(move || {
        *rc.lock().unwrap() += 1;
        let (r, e) = fixture_empty();
        Ok(WatchData::Single { report: r, episodes: e })
    }));
    // Trigger a reload via the test-only iteration helper.
    runner.do_reload_for_test();
    assert!(runner.view_state().detail_view.is_none(),
        "turn-disappeared reload should drop detail_view");
    assert!(runner
        .last_error()
        .unwrap_or("")
        .contains("disappeared"));
}
```

> **NOTE**: this test depends on a few `WatchRunner` accessors that may need to be added if not already public: `view_state(&self) -> &WatchViewState`, `view_state_mut(&mut self) -> &mut WatchViewState`, `with_reload(self, cb) -> Self`, and `do_reload_for_test(&mut self)`. If they're missing, ADD them as `#[doc(hidden)] pub` methods in `watch.rs` (test-only access; the runtime entry point is still `run()` / `run_one_iteration_for_test`). Mirror the `step_for_test` style at line 617.

- [ ] **Step 8: Run tests**

```bash
cargo test -p agentprof-tui --test watch_runner 2>&1 | tail -10
```

Expected: 2 new tests pass, all existing tests still pass.

- [ ] **Step 9: Run gates + commit**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-tui --all-targets -- -D warnings && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps -p agentprof-tui

git add crates/agentprof-tui/src/watch.rs crates/agentprof-tui/tests/watch_runner.rs
git -c commit.gpgsign=false commit -m "feat(tui): WatchRunner — detail_view round-trip + reload safety

Persist TurnDetailState across WatchRunner's transient AppState
reconstruction (render + dispatch paths):
- WatchViewState gains pub detail_view: Option<TurnDetailState>
  field (Default::default → None).
- render_into and key-dispatch paths clone detail_view into the
  transient AppState before delegating; dispatch writes back.
- render_into delegates to render_turn_detail when detail_view
  is Some; falls through to existing per-view dispatch otherwise.

do_reload safety (M1.6.3 watch model):
- on successful reload of WatchData::Single, validate that the
  cached detail_view.turn_id is still present in fresh episodes.
- if not: drop detail_view + set last_error red-banner footer
  with 'turn <id> disappeared after reload' message.

Pattern mirrors existing pending_gg / help_overlay round-trip
(ADR-0009 D-13 red-banner footer convention extended to a new
reload-affected field).

2 new tests:
- watch_view_state_persists_detail_view_field (default None)
- reload_drops_detail_view_when_turn_disappears (integration)

Spec §3.4, ADR-0011 D-14.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 9: TUI — Help overlay rows + Flamegraph footer hint

**Files:**
- Modify: `crates/agentprof-tui/src/app/mod.rs:134` `fn draw_help_overlay` (5 new rows, height 22 → 27)
- Modify: `crates/agentprof-tui/src/views/flamegraph.rs` footer line (add `Enter detail` hint)

**Why ninth:** Discoverability. Without these hints, the Enter affordance is invisible.

- [ ] **Step 1: Update help overlay**

Modify `crates/agentprof-tui/src/app/mod.rs` `fn draw_help_overlay`. Find the constant `HELP_LINES` (or equivalent) and add 5 new lines BEFORE the "Cell legend" section (preserving the existing structure):

```
  Detail view (Flamegraph → Enter):
    Enter     toggle args expand
    Esc       return to flamegraph
    j/k G/gg  navigate tool calls
    1/2/3     pop detail + switch view
```

Bump the overlay height from `22` to `27` (find the literal height constant, likely `height = 22` somewhere in the centered-rect math).

- [ ] **Step 2: Update flamegraph footer**

Find the footer line composition in `crates/agentprof-tui/src/views/flamegraph.rs` (search for `selected:` text from the recent M1.6.4 follow-up wave commit `96bed91`). After the `... +K more` content, append `· Enter for detail`:

```rust
// roughly:
format!("T{idx} selected: {tools_preview} · Enter for detail")
```

Verify the footer doesn't overflow gantt_w (existing truncation logic handles this; the new ` · Enter for detail` adds 18 chars).

- [ ] **Step 3: Run gates**

```bash
cargo fmt --all --check && \
cargo clippy -p agentprof-tui --all-targets -- -D warnings && \
cargo test -p agentprof-tui --all-features 2>&1 | grep -E '^test result|FAILED'
```

Expected: all green. Snapshot tests that include the flamegraph footer line MAY churn (`tests/views.rs::snapshot_flamegraph_*`). If they do, the diff will show ` · Enter for detail` added at the end of the footer line — that's the intended change. Accept:

```bash
find crates/agentprof-tui -name '*.snap.new' 2>/dev/null
# Inspect each diff, then if intended:
find crates/agentprof-tui -name '*.snap.new' -exec sh -c 'mv "$1" "${1%.new}"' _ {} \;
```

- [ ] **Step 4: Commit**

```bash
git add crates/agentprof-tui/src/app/mod.rs crates/agentprof-tui/src/views/flamegraph.rs \
        crates/agentprof-tui/tests/snapshots/
git -c commit.gpgsign=false commit -m "docs(tui): help overlay + flamegraph footer — surface Enter-for-detail

Discoverability follow-up for the F1 TurnDetailView feature:

- Help overlay (?): 5 new lines under \"Detail view (Flamegraph →
  Enter)\" listing the Enter/Esc/j/k/G/gg/1-2-3 keys. Overlay
  height bumped 22 → 27 to accommodate.
- Flamegraph footer (M1.6.4 wave selected-turn line): append
  '· Enter for detail' so the affordance is visible the moment a
  user navigates to a turn row.

Snapshot churn: flamegraph footer snapshots gain '· Enter for
detail' at the end of the selected line; accepted as intended.

Spec §2.1.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 10: Docs — README + CHANGELOG + privacy + adapters

**Files:**
- Modify: `README.md` (TUI section — mention Enter-for-detail + args plumbing)
- Modify: `CHANGELOG.md` `[Unreleased]` (3 Added + 1 Changed)
- Modify: `docs/features/privacy.md` (new §8)
- Modify: `docs/adapters.md` (recommended-impl note)
- Modify: `crates/agentprof-core/README.md` + `crates/agentprof-tui/README.md` (L2 docs sync)

**Why tenth:** Per `.github/copilot-instructions.md` §4.2 — code changes must include same-PR doc updates. This is the last task; everything else is implementation that doesn't ship until docs are in sync.

- [ ] **Step 1: Update CHANGELOG.md `[Unreleased]`**

Edit `CHANGELOG.md`. Under `### Added` (prepend, so newest first):

```
- **TUI TurnDetailView (F1)**: pressing `Enter` on a selected turn in `FlamegraphView` opens a full-screen detail view listing every tool call in that turn (sorted by duration desc). Each row shows tool name colored by `ToolSource`, duration, ✓/✗/? status, source badge, and a single-line `args` preview truncated to 80 characters. In detail view: `Enter` toggles args expansion to fully-pretty-printed JSON; `j`/`k` (or `↑`/`↓`) navigate; `G` / `gg` jump to last/first; `Esc` returns to flamegraph preserving turn selection; `1`/`2`/`3` pop detail + switch view; `q` quits globally; `?` toggles help overlay (extended with 5 new rows). Works in both `analyze --export tui` and `watch ...` (single-session). WatchRunner reload safety drops the detail view + red-banner-footers \"turn <id> disappeared after reload\" if the cached turn id vanishes. See [spec](docs/superpowers/specs/2026-06-03-turn-detail-view-design.md) and [ADR-0011](docs/internals/adr-0011-turn-detail-and-args-plumbing.md).
- **`Event::payload_tool_requests` trait method (core)**: new extension method on `agentprof_core::adapter::Event`, symmetric with the existing four `payload_*` methods. Default impl returns empty `Vec`; adapters override to expose `(tool_call_id, arguments)` pairs from payload-bearing variants. `derive_episodes` consumes the output in a new PASS 0 args-map step.
- **`ToolCall.arguments` field (core)**: `agentprof_core::episode::ToolCall` gains `pub arguments: Option<serde_json::Value>` (`#[serde(skip_serializing_if = \"Option::is_none\")]`), populated by `derive_episodes` from `Event::payload_tool_requests`. `None` when the adapter doesn't implement the method or when the call's `tool_call_id` had no matching tool-request event. The struct remains `#[non_exhaustive]` so the field add is non-breaking.
```

Under `### Changed` (prepend):

```
- **JSON export schema (analyze --export json)**: per-tool-call payloads now include an optional `arguments` field (parsed JSON value passed through from the adapter). Schema-strict consumers should treat it as a forward-compatible addition. Omitted entirely (`#[serde(skip_serializing_if = \"Option::is_none\")]`) when the adapter did not capture args for a given call.
```

- [ ] **Step 2: Update README.md TUI section**

Edit `README.md`. Find the TUI section (currently mentions `█` colored / `░` thinking / `·` padding / vim keys / speedscope deep-dive hint). Add a new bullet under the existing 3-cell legend list:

```markdown
- **Enter** on a selected turn row → opens a full-screen **TurnDetailView**: every tool call in that turn with name, duration, ✓/✗ status, source badge, and a one-line `args` preview (80-char truncated). In detail view, `Enter` toggles the selected call's args between truncated and full pretty-printed JSON; `Esc` returns to flamegraph; `j/k G/gg` navigate; `1/2/3` pop detail and switch top-level view. Args are populated for Copilot CLI adapter; other adapters show `(not captured)` until they implement `Event::payload_tool_requests`.
```

- [ ] **Step 3: Create `docs/features/privacy.md` §8**

Append (or insert in numerical order if other §s exist):

```markdown
## 8. Tool arguments in `ToolCall.arguments` (M1.6.4 follow-up wave)

As of F1 (2026-06-03), `agentprof_core::episode::ToolCall` carries an
optional `arguments: serde_json::Value` field populated from
`Event::payload_tool_requests()`. For the Copilot CLI adapter this
includes the raw JSON args of every `tool_request` and `tool.user_requested`
event — e.g.:

- `bash` calls carry `{ "command": "rg pattern --type rust" }`
- `read_file` carries `{ "path": "/home/user/project/src/main.rs" }`
- `mcp:postgres::execute_query` carries `{ "query": "SELECT * FROM ..." }`
- `ask_user` carries the prompt + choice list shown to the user

**No redaction is performed in v1.** The args data is passed through
as-is to:

1. The TUI `TurnDetailView` (shown to anyone viewing the report).
2. The JSON export (`analyze --export json`) — args appear as a per-call
   field. Schema: `tool_call.arguments: Option<serde_json::Value>`,
   omitted when the adapter didn't capture args.

This matches the existing posture on tool names, raw event content, and
turn timing data: agentprof trusts whatever the adapter emits and does
not introspect payload contents to scrub sensitive substrings.

**Note**: the `AGENTPROF_LOG_FULL_PATHS` environment variable governs
*logging fields* (e.g. `session = %hash`), NOT payload data. It has no
effect on `ToolCall.arguments` rendering or serialization.

**Future**: a `--show-results` / args-redaction feature is reserved for
a future privacy RFC. Until then, users should be aware that:

- Sharing `analyze --export json` output with third parties may expose
  user paths, queries, file contents, and prompts.
- Recording a `watch` TUI session on screen captures args.
- HTML / Markdown / CSV exports do NOT include args (those format
  exports are tool-aggregated, not per-call — see ADR-0011 D-12).
```

- [ ] **Step 4: Update `docs/adapters.md`**

Find the section listing required / recommended `Event` trait methods (or the existing "to add a new adapter" instructions). Append:

```markdown
### Optional: `Event::payload_tool_requests` (M1.6.4 follow-up wave)

Adapters that want their users to benefit from the TUI
`TurnDetailView`'s args preview SHOULD implement this method. Return a
`Vec<(String, serde_json::Value)>` of `(tool_call_id, arguments)` pairs
declared by the event. Default impl returns empty `Vec`; adapters
without an override silently ship the `(not captured)` placeholder in
the TUI detail view.

Example (Copilot adapter): see `crates/agentprof-adapters/src/copilot/event.rs`
for the canonical impl across `AssistantMessage` (multi-pair) and
`ToolUserRequested` (single-pair) variants.

See ADR-0011 D-2 for rationale on the method shape (returns `Vec` not
`Option<...>` because some events carry multiple tool requests).
```

- [ ] **Step 5: Update L2 READMEs**

`crates/agentprof-core/README.md` — add to the "Public API surface" section:

```markdown
- `Event::payload_tool_requests` — opt-in `(tool_call_id, arguments)`
  pair extraction; consumed by `derive_episodes` PASS 0 to populate
  `ToolCall.arguments`.
- `ToolCall.arguments: Option<serde_json::Value>` — per-call args JSON,
  serde-skipped when `None`.
```

`crates/agentprof-tui/README.md` — add to the "Views" list and "Key
bindings" table:

```markdown
| View | Trigger |
|---|---|
| `TurnDetailView` | `Enter` on a selected row in `FlamegraphView` |

| Key | View | Action |
|---|---|---|
| `Enter` | Flamegraph | Open TurnDetailView for selected turn |
| `Enter` | TurnDetail | Toggle args expansion for selected tool call |
| `Esc` | TurnDetail | Return to FlamegraphView (preserves selection) |
| `j/k ↑/↓` | TurnDetail | Navigate tool calls |
| `G/gg` | TurnDetail | Jump to last/first tool call |
| `1/2/3` | TurnDetail | Pop detail + switch to view 1/2/3 |
```

- [ ] **Step 6: Run gates (docs only — no test impact expected)**

```bash
cargo fmt --all --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings && \
cargo test --workspace --all-features 2>&1 | grep -E '^test result|FAILED' && \
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace
```

Expected: all green. Final aggregate test count should be **~564** (533 baseline + 31 new), exceeding the spec §5.5 target.

- [ ] **Step 7: Commit**

```bash
git add CHANGELOG.md README.md docs/features/privacy.md docs/adapters.md \
        crates/agentprof-core/README.md crates/agentprof-tui/README.md
git -c commit.gpgsign=false commit -m "docs(f1): README + CHANGELOG + privacy + adapters + L2 READMEs

Same-PR doc sync for the F1 TurnDetailView + args plumbing feature
(per .github/copilot-instructions.md §4.2).

CHANGELOG.md [Unreleased]:
- Added: 3 entries (TUI TurnDetailView, Event::payload_tool_requests
  trait method, ToolCall.arguments field).
- Changed: 1 entry (JSON export schema gains optional arguments field).

README.md: TUI section gains the Enter-for-detail bullet under the
existing 3-cell legend.

docs/features/privacy.md: new §8 'Tool arguments in
ToolCall.arguments' documents the no-redaction posture, JSON export
implications, and AGENTPROF_LOG_FULL_PATHS scope clarification.

docs/adapters.md: new 'Optional: Event::payload_tool_requests'
section documents the recommended-but-optional impl and the silent
(not captured) fallback.

L2 READMEs:
- agentprof-core: Public API surface gains
  payload_tool_requests + ToolCall.arguments rows.
- agentprof-tui: Views list gains TurnDetailView; key bindings
  table gains 6 detail-view rows.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---
