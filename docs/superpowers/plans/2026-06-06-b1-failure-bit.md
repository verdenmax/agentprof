# B1 — Wire success bit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the wire-format `success` bit from Copilot `tool.execution_complete` + `hook.end` payloads through the `Event` trait into `ToolCall`/`HookCall`, closing the silent-misfire bug that has neutralized F1.13 / F1.16 / F2.3 on real Copilot data since M1.2.

**Architecture:** Three-layer change matching the existing `payload_*` extension pattern.
(1) `Event` trait grows 2 default-`None` methods (`payload_success`, `payload_error_message`).
(2) `CopilotEvent` overrides both via inherent method + trait forwarder for `ToolExecComplete` + `HookEnd`.
(3) `derive_episodes` consumes them in `on_tool_complete` + `on_hook_end` (×2 sites), defaulting `None` to Success for forward-compat.

**Tech Stack:** Rust 2021 / cargo workspace / `chrono` / `serde_json` (for free-form `ToolResultData` field access) / `insta` (snapshot regen) / no new dependencies.

**Spec:** [`docs/superpowers/specs/2026-06-06-b1-failure-bit-design.md`](../specs/2026-06-06-b1-failure-bit-design.md)
**ADR:** [`docs/internals/adr-0013-event-success-bit.md`](../../internals/adr-0013-event-success-bit.md)

**Commit strategy:** Each task ends with its own commit for clean review history. Optional: interactive rebase/squash to one `fix(core):` commit before merging if preferred (spec §9 originally suggested squashing; per-task commits are fine for solo development).

---

## Task 1: Add `payload_success` + `payload_error_message` to `Event` trait

**Files:**
- Modify: `crates/agentprof-core/src/adapter.rs` — append 2 new methods after `payload_tool_requests`

**Why this task first:** Default-`None` impls mean no caller behaviour changes; adapter authors are unaffected. Clean isolated commit, fully testable via doctest, sets up T2/T3.

- [ ] **Step 1: Locate insertion point**

```bash
cd /home/verden/pfind/2026-spring/code/agentprof
grep -n "fn payload_tool_requests\|fn tool_call_id" crates/agentprof-core/src/adapter.rs
```

Expected: lines around 326 (`fn payload_tool_requests`) and 356 (`fn tool_call_id`). Insert the 2 new methods **between** these two methods (keep tool_call_id at its current location so PASS-0 lookup pairing stays grouped).

- [ ] **Step 2: Edit `crates/agentprof-core/src/adapter.rs`** — insert immediately after the closing `}` of `payload_tool_requests` (around line 328) and before the doctest of `tool_call_id` (around line 330):

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
    /// See ADR-0013 D-3.
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
    /// See ADR-0013 D-4 + D-6.
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

- [ ] **Step 3: Run doctest to verify it compiles + passes**

```bash
cargo test --doc -p agentprof-core --all-features adapter
```

Expected: PASS (≥2 new doctests, no failures).

- [ ] **Step 4: Run full workspace to confirm no regression**

```bash
cargo test --workspace --all-features 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6} END {printf "%d passed, %d failed\n", p, f}'
```

Expected: `779 passed, 0 failed` (777 baseline + 2 new doctests).

- [ ] **Step 5: Lint + rustdoc gates**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace 2>&1 | tail -3
```

Expected: clean (no warnings).

- [ ] **Step 6: Commit**

```bash
git add crates/agentprof-core/src/adapter.rs
git commit -m "feat(core): add Event::payload_success + payload_error_message (default None)

Two new default-None methods on Event trait, mirroring the existing
payload_name / payload_model / payload_tool_requests / tool_call_id
pattern. Adapter authors unaffected (default impls compile unchanged);
consumers in derive_episodes will be wired in a follow-up commit.

See ADR-0013 for two-narrow-methods vs bundled-struct rationale.
Spec: docs/superpowers/specs/2026-06-06-b1-failure-bit-design.md §4.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 2: Override `payload_success` + `payload_error_message` in `CopilotEvent`

**Files:**
- Modify: `crates/agentprof-adapters/src/copilot/event.rs` — add inherent methods + trait forwarders + tests

**Pattern (confirmed via `payload_model_metrics` precedent at line 1379):**
- `pub fn payload_X(&self) -> ...` inherent method with full logic + `# Examples` doctest
- `fn payload_X(&self) -> ...` inside `impl Event for CopilotEvent` block — one-line forwarder `self.payload_X()`

- [ ] **Step 1: Write failing inherent-method doctest first**

Locate the inherent `pub fn payload_model_metrics` definition (around line 1379 per recon). Insert the new inherent methods immediately after it (before `impl Event for CopilotEvent` block at ~line 1430):

```rust
    /// Wire-format success bit for `tool.execution_complete` + `hook.end`.
    ///
    /// Returns `Some(payload.success)` for those two variants, `None`
    /// for all other variants (matches the Event trait contract — `None`
    /// means "this event has no concept of success"). See ADR-0013.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_adapters::copilot::CopilotEvent;
    /// assert!(CopilotEvent::Unknown.payload_success().is_none());
    /// ```
    #[must_use]
    pub fn payload_success(&self) -> Option<bool> {
        match self {
            Self::ToolExecComplete(env) => Some(env.data.success),
            Self::HookEnd(env)          => Some(env.data.success),
            _ => None,
        }
    }

    /// Wire-format error message for `tool.execution_complete` failures.
    ///
    /// Returns `Some(&payload.error.message)` for `ToolExecComplete`
    /// variants whose payload carries an `error`, `None` otherwise.
    /// `HookEnd` always returns `None` here — the Copilot wire schema
    /// (ADR-0002 line 93) does not carry an error message for hooks.
    /// See ADR-0013 D-6.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_adapters::copilot::CopilotEvent;
    /// assert!(CopilotEvent::Unknown.payload_error_message().is_none());
    /// ```
    #[must_use]
    pub fn payload_error_message(&self) -> Option<&str> {
        match self {
            Self::ToolExecComplete(env) => {
                env.data.error.as_ref().map(|e| e.message.as_str())
            }
            _ => None,
        }
    }

```

- [ ] **Step 2: Add the trait-forwarder overrides inside `impl Event for CopilotEvent`**

Locate the block at ~line 1430. Find where `fn payload_model_metrics(&self) ... { self.payload_model_metrics() }` ends (around line 1448). Insert immediately after it (before the closing `}` of `impl Event`):

```rust
    fn payload_success(&self) -> Option<bool> {
        self.payload_success()
    }
    fn payload_error_message(&self) -> Option<&str> {
        self.payload_error_message()
    }
```

- [ ] **Step 3: Verify compile + run the new doctests**

```bash
cd /home/verden/pfind/2026-spring/code/agentprof
cargo test --doc -p agentprof-adapters --all-features payload_success
cargo test --doc -p agentprof-adapters --all-features payload_error_message
```

Expected: both PASS (2 new doctests).

- [ ] **Step 4: Add adapter-level unit tests** — append to `mod payload_name_tests` at ~line 1449 (don't create a new mod — reuse the `envelope<D>()` helper). Insert before the closing `}` of `mod payload_name_tests`:

```rust
    // ──────────────────────────────────────────────────────────────────
    // B1 — payload_success + payload_error_message
    // ──────────────────────────────────────────────────────────────────

    fn tool_complete_data(success: bool, error: Option<ToolError>) -> ToolResultData {
        ToolResultData {
            tool_call_id: "tc-1".into(),
            model: Some("claude-sonnet-4.6".into()),
            interaction_id: Some("i".into()),
            turn_id: Some("t1".into()),
            success,
            result: None,
            tool_telemetry: None,
            error,
        }
    }

    fn hook_end_data(success: bool) -> HookEndData {
        HookEndData {
            hook_invocation_id: "hi".into(),
            hook_type: "PreToolUse".into(),
            output: None,
            success,
        }
    }

    #[test]
    fn payload_success_tool_complete_success_true() {
        let ev = CopilotEvent::ToolExecComplete(envelope(tool_complete_data(true, None)));
        assert_eq!(ev.payload_success(), Some(true));
        assert_eq!(ev.payload_error_message(), None);
    }

    #[test]
    fn payload_success_tool_complete_failure_with_message() {
        let err = ToolError { message: "disk full".into() };
        let ev = CopilotEvent::ToolExecComplete(envelope(tool_complete_data(false, Some(err))));
        assert_eq!(ev.payload_success(), Some(false));
        assert_eq!(ev.payload_error_message(), Some("disk full"));
    }

    #[test]
    fn payload_success_tool_complete_failure_without_message() {
        // Wire payload may omit error object entirely; payload_error_message
        // returns None, payload_success still Some(false).
        let ev = CopilotEvent::ToolExecComplete(envelope(tool_complete_data(false, None)));
        assert_eq!(ev.payload_success(), Some(false));
        assert_eq!(ev.payload_error_message(), None);
    }

    #[test]
    fn payload_success_hook_end_true() {
        let ev = CopilotEvent::HookEnd(envelope(hook_end_data(true)));
        assert_eq!(ev.payload_success(), Some(true));
        // Hook schema (ADR-0002 line 93) has no error message field.
        assert_eq!(ev.payload_error_message(), None);
    }

    #[test]
    fn payload_success_hook_end_false() {
        let ev = CopilotEvent::HookEnd(envelope(hook_end_data(false)));
        assert_eq!(ev.payload_success(), Some(false));
        assert_eq!(ev.payload_error_message(), None);
    }

    #[test]
    fn payload_success_unrelated_variant_is_none() {
        // SessionStart has no success/failure concept.
        assert_eq!(CopilotEvent::Unknown.payload_success(), None);
        assert_eq!(CopilotEvent::Unknown.payload_error_message(), None);
    }

    #[test]
    fn payload_success_trait_forwarder_matches_inherent() {
        // Confirm the Event trait override forwards to the inherent method.
        use agentprof_core::adapter::Event as _;
        let ev = CopilotEvent::ToolExecComplete(envelope(tool_complete_data(false, None)));
        assert_eq!(Event::payload_success(&ev), ev.payload_success());
        assert_eq!(Event::payload_error_message(&ev), ev.payload_error_message());
    }
```

- [ ] **Step 5: Run new tests**

```bash
cargo test -p agentprof-adapters --all-features --lib payload_success
cargo test -p agentprof-adapters --all-features --lib payload_error_message
```

Expected: all 7 new tests PASS. If any test fails to compile because of missing imports (`HookEndData`, `ToolError`), add them to the `mod payload_name_tests { use super::*; ... }` use list — `super::*` should already pull them in but verify.

- [ ] **Step 6: Run full workspace gates**

```bash
cargo test --workspace --all-features 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6} END {printf "%d passed, %d failed\n", p, f}'
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
```

Expected: `786 passed, 0 failed` (779 baseline + 7 new tests + 2 new doctests already counted... actually: 777 + 2 trait doctests + 2 inherent doctests + 7 unit = 788. Adjust your read accordingly — the exact number matters less than "no failures").

- [ ] **Step 7: Commit**

```bash
git add crates/agentprof-adapters/src/copilot/event.rs
git commit -m "feat(adapters): CopilotEvent overrides payload_success + payload_error_message

ToolExecComplete reads ToolResultData.success + .error.message.
HookEnd reads HookEndData.success only (Copilot wire schema has no
hook error message field per ADR-0002 line 93).

Adds 7 unit tests + 2 inherent-method doctests covering all 4 corners:
- tool success=true / false × error=Some / None
- hook success=true / false
- unrelated variant returns None
- trait forwarder matches inherent method output

Dead code until next commit wires derive_episodes to consume them.
Behavior of derive_episodes (and downstream failure_count) unchanged
by this commit.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 3: Wire `on_tool_complete` + `on_hook_end` (×2 sites) in `derive.rs` + regen snapshots

**Files:**
- Modify: `crates/agentprof-core/src/episode/derive.rs` — 3 hardcoded sites
- Modify: snapshot files under `crates/agentprof-adapters/tests/snapshots/` (regenerated, not hand-edited)
- Modify: `crates/agentprof-core/src/episode/derive.rs::tests` — add unit tests using a stub Event

**This is the load-bearing task.** After this commit, `failure_count` on the 4 affected fixtures starts reflecting reality.

- [ ] **Step 1: Write a failing unit test FIRST (TDD red)** — append to `mod tests` at the bottom of `crates/agentprof-core/src/episode/derive.rs`:

```rust
    // ──────────────────────────────────────────────────────────────────
    // B1 — wire-format success bit consumption (closes
    // F1.13/F1.16/F2.3 silent misfire — see ADR-0013)
    // ──────────────────────────────────────────────────────────────────

    /// Stub Event that lets a test drive payload_success + payload_error_message.
    #[derive(Clone)]
    struct StubFailingTool {
        success: Option<bool>,
        error: Option<String>,
    }

    impl Event for StubFailingTool {
        fn id(&self) -> &str { "tc-end" }
        fn kind(&self) -> EventKind { EventKind::ToolExecComplete }
        fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc.with_ymd_and_hms(2026, 6, 6, 12, 0, 1).unwrap()
        }
        fn parent_id(&self) -> Option<&str> { None }
        fn payload_name(&self) -> Option<&str> { Some("bash") }
        fn tool_call_id(&self) -> Option<&str> { Some("tc-1") }
        fn payload_success(&self) -> Option<bool> { self.success }
        fn payload_error_message(&self) -> Option<&str> { self.error.as_deref() }
    }

    /// Stub start event paired with StubFailingTool.
    struct StubToolStart;
    impl Event for StubToolStart {
        fn id(&self) -> &str { "tc-start" }
        fn kind(&self) -> EventKind { EventKind::ToolExecStart }
        fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc.with_ymd_and_hms(2026, 6, 6, 12, 0, 0).unwrap()
        }
        fn parent_id(&self) -> Option<&str> { None }
        fn payload_name(&self) -> Option<&str> { Some("bash") }
        fn tool_call_id(&self) -> Option<&str> { Some("tc-1") }
    }

    #[test]
    fn b1_tool_complete_success_false_records_failure_with_message() {
        let start = StubToolStart;
        let end = StubFailingTool {
            success: Some(false),
            error: Some("disk full".into()),
        };
        let events: Vec<Box<dyn Event>> = vec![Box::new(start), Box::new(end)];
        let refs: Vec<&dyn Event> = events.iter().map(|b| b.as_ref()).collect();
        let episodes = derive_episodes(&refs);
        let ep = episodes.tools.get("bash").expect("bash episode exists");
        assert_eq!(ep.failure_count, 1, "failure_count must reflect wire success=false");
        assert_eq!(ep.calls.len(), 1);
        assert!(
            matches!(
                &ep.calls[0].status,
                ToolCallStatus::Failure { message: Some(m) } if m == "disk full"
            ),
            "expected Failure {{ message: Some(\"disk full\") }}, got {:?}",
            ep.calls[0].status
        );
    }

    #[test]
    fn b1_tool_complete_success_false_no_message_yields_failure_message_none() {
        let start = StubToolStart;
        let end = StubFailingTool {
            success: Some(false),
            error: None,
        };
        let events: Vec<Box<dyn Event>> = vec![Box::new(start), Box::new(end)];
        let refs: Vec<&dyn Event> = events.iter().map(|b| b.as_ref()).collect();
        let episodes = derive_episodes(&refs);
        let ep = episodes.tools.get("bash").unwrap();
        assert_eq!(ep.failure_count, 1);
        assert!(matches!(&ep.calls[0].status, ToolCallStatus::Failure { message: None }));
    }

    #[test]
    fn b1_tool_complete_payload_success_none_defaults_to_success() {
        // Forward-compat: an adapter that doesn't override payload_success
        // (returns None) preserves the existing always-Success behavior.
        // See ADR-0013 D-3 + Q4.
        let start = StubToolStart;
        let end = StubFailingTool { success: None, error: None };
        let events: Vec<Box<dyn Event>> = vec![Box::new(start), Box::new(end)];
        let refs: Vec<&dyn Event> = events.iter().map(|b| b.as_ref()).collect();
        let episodes = derive_episodes(&refs);
        let ep = episodes.tools.get("bash").unwrap();
        assert_eq!(ep.failure_count, 0, "None must default to Success");
        assert!(matches!(&ep.calls[0].status, ToolCallStatus::Success));
    }
```

Plus 2 more for the hook side (insert into the same mod):

```rust
    /// Stub Event for hook.end with overridable success bit.
    struct StubHookEnd { success: Option<bool> }
    impl Event for StubHookEnd {
        fn id(&self) -> &str { "h-end" }
        fn kind(&self) -> EventKind { EventKind::HookEnd }
        fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc.with_ymd_and_hms(2026, 6, 6, 12, 0, 1).unwrap()
        }
        fn parent_id(&self) -> Option<&str> { None }
        fn payload_name(&self) -> Option<&str> { Some("PreToolUse") }
        fn payload_success(&self) -> Option<bool> { self.success }
    }

    struct StubHookStart;
    impl Event for StubHookStart {
        fn id(&self) -> &str { "h-start" }
        fn kind(&self) -> EventKind { EventKind::HookStart }
        fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc.with_ymd_and_hms(2026, 6, 6, 12, 0, 0).unwrap()
        }
        fn parent_id(&self) -> Option<&str> { None }
        fn payload_name(&self) -> Option<&str> { Some("PreToolUse") }
    }

    #[test]
    fn b1_hook_end_success_false_records_failure() {
        let start = StubHookStart;
        let end = StubHookEnd { success: Some(false) };
        let events: Vec<Box<dyn Event>> = vec![Box::new(start), Box::new(end)];
        let refs: Vec<&dyn Event> = events.iter().map(|b| b.as_ref()).collect();
        let episodes = derive_episodes(&refs);
        let ep = episodes.hooks.get("PreToolUse").unwrap();
        assert_eq!(ep.failure_count, 1);
        assert!(!ep.calls[0].success);
    }

    #[test]
    fn b1_hook_end_payload_success_none_defaults_to_success() {
        // Forward-compat default.
        let start = StubHookStart;
        let end = StubHookEnd { success: None };
        let events: Vec<Box<dyn Event>> = vec![Box::new(start), Box::new(end)];
        let refs: Vec<&dyn Event> = events.iter().map(|b| b.as_ref()).collect();
        let episodes = derive_episodes(&refs);
        let ep = episodes.hooks.get("PreToolUse").unwrap();
        assert_eq!(ep.failure_count, 0);
        assert!(ep.calls[0].success);
    }
```

(If `chrono::TimeZone` isn't already imported in the test mod, add `use chrono::TimeZone;` at the mod top.)

- [ ] **Step 2: Run to verify RED**

```bash
cargo test -p agentprof-core --all-features --lib b1_ 2>&1 | tail -15
```

Expected: 5 FAILs — `failure_count` is still always 0 (matches current bug), Failure variants don't match, `success` is still always true. All 5 tests fail before the fix lands.

- [ ] **Step 3: Implement the fix — `on_tool_complete` at line ~380**

In `crates/agentprof-core/src/episode/derive.rs`, replace the existing block:

```rust
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status: ToolCallStatus::Success, // Task 10b will read actual success bit
                user_requested: open.user_requested,
                arguments,
            };
            self.commit_tool_call(&open.name, &open.source, call);
```

with:

```rust
            // B1: read wire-format success bit + error message via the
            // Event trait. `None` defaults to Success (forward-compat
            // for older Copilot 1.0.x / adapters that don't override).
            // See ADR-0013 D-3.
            let status = match ev.payload_success() {
                Some(false) => ToolCallStatus::Failure {
                    message: ev.payload_error_message().map(str::to_owned),
                },
                Some(true) | None => ToolCallStatus::Success,
            };
            let call = ToolCall {
                span,
                turn_id: open.turn_id,
                status,
                user_requested: open.user_requested,
                arguments,
            };
            self.commit_tool_call(&open.name, &open.source, call);
```

- [ ] **Step 4: Implement the fix — `on_hook_end` first site (~line 487)**

Replace:

```rust
            let call = HookCall {
                span,
                turn_id: open.turn_id,
                success: true,
```

with:

```rust
            // B1: read wire-format success bit; None → forward-compat
            // default to true. See ADR-0013 D-3.
            let call = HookCall {
                span,
                turn_id: open.turn_id,
                success: ev.payload_success().unwrap_or(true),
```

- [ ] **Step 5: Implement the fix — `on_hook_end` second site (~line 501, the orphan/synthesized path)**

Replace:

```rust
            let call = HookCall {
                span: Span::instant(ts),
                turn_id: self.open_turn_idx.map(|i| self.turns[i].id.clone()),
                success: true,
```

with:

```rust
            let call = HookCall {
                span: Span::instant(ts),
                turn_id: self.open_turn_idx.map(|i| self.turns[i].id.clone()),
                // B1: orphan path still reads the wire bit if present.
                success: ev.payload_success().unwrap_or(true),
```

**Do NOT touch line ~607** (the `Abort`-consumes-open-hook synthesis path with `success: false`). That path's `false` is semantically correct — the hook never reached its end event, so it's a failure regardless of any wire bit.

- [ ] **Step 6: Run the 5 new unit tests — verify GREEN**

```bash
cargo test -p agentprof-core --all-features --lib b1_ 2>&1 | tail -10
```

Expected: 5 PASS.

- [ ] **Step 7: Run the full test suite — expect snapshot failures on the 4 affected fixtures**

```bash
cargo test --workspace --all-features 2>&1 | grep -E "FAILED|^test result" | head -30
```

Expected: Several snapshot tests FAIL on `with-mcp-calls`, `with-aborts`, `with-hooks-heavy`, `multi-sess-c`. All other tests should still PASS. The failed tests will be in `analyzer_on_fixtures`, `aggregate_on_fixtures`, `export_on_fixtures`, and possibly TUI view snapshots.

- [ ] **Step 8: Inspect snapshot diffs BEFORE accepting**

```bash
cargo insta pending-snapshots 2>&1 | head -40
```

For each pending snapshot, run:

```bash
cargo insta show <snapshot-path>  # or open the .new file in editor
```

**Acceptance gate (spec §7.4):** ONLY accept diffs that show:
- `failure_count: 0` → `failure_count: N` (where N > 0) — the headline change
- Computed downstream fields: `failure_rate`, `success_count`, percentages, OK%, etc. — derived from `failure_count`
- TUI snapshot cells: any Red/Yellow color changes ONLY on the Tool / Hook cells of fixtures where failures are present (F1.13 / F1.16 finally firing)

REJECT (halt + investigate) any diff showing:
- New/removed episodes (would mean derive grouping changed — bug)
- Duration changes (would mean span computation drifted — bug)
- Turn count changes
- Non-failure-related cell color flips
- Schema-version-style metadata changes

- [ ] **Step 9: Accept verified snapshots**

```bash
cargo insta accept
```

Then re-run to confirm:

```bash
cargo test --workspace --all-features 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6} END {printf "%d passed, %d failed\n", p, f}'
```

Expected: all PASS. New baseline test count: 786 + 5 = ~791. Adjust your expectation but confirm 0 failures.

- [ ] **Step 10: Run lint + rustdoc gates**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 11: Commit**

```bash
git add crates/agentprof-core/src/episode/derive.rs \
        crates/agentprof-adapters/tests/snapshots/ \
        crates/agentprof-core/tests/snapshots/ 2>/dev/null
git status  # confirm only derive.rs + snapshot files are staged
git commit -m "fix(core): B1 — consume payload_success + payload_error_message in derive_episodes

Closes the silent failure_count=0 bug that has neutralized F1.13 (RoiView
Red/Yellow Tool cell), F1.16 (By Hook OK% + color), and F2.3 (compose_tool_cell_style
failure-wins-over-pending) on real Copilot data since M1.2 (commit c5716aa).

Three derive.rs sites change:
- on_tool_complete (line ~380): reads ev.payload_success() →
  ToolCallStatus::Failure { message: ev.payload_error_message().to_owned() }
  or Success (when Some(true) or None — forward-compat).
- on_hook_end paired path (line ~487): reads payload_success().unwrap_or(true).
- on_hook_end synthesized-start path (line ~501): same.

Orphan abort path (line ~607) intentionally retains hardcoded success: false
— the hook never reached its end event, which IS a failure regardless of wire.

Deletes the // Task 10b will read actual success bit TODO that's been
sitting in derive.rs:383 since M1.2.

Snapshot regen: 4 fixtures (with-mcp-calls, with-aborts, with-hooks-heavy,
multi-sess-c) now produce non-zero failure_count end-to-end. Inspected
diffs per spec §7.4 — only failure_count + derived percentages + correctly-
fired F1.13/F1.16 cell color changes.

Adds 5 unit tests (3 tool + 2 hook) covering Some(false)+message,
Some(false)+no_message, None forward-compat (×2), Some(true) implicit
via the existing always-Success tests.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 4: Add end-to-end fixture assertions (regression guards)

**Files:**
- Modify: `crates/agentprof-adapters/tests/analyzer_on_fixtures.rs` — add per-fixture failure_count assertions

**Why this task:** The snapshot tests in T3 verify the *output shape*. These end-to-end assertions verify the *semantic claim* — "this fixture HAS failures, and they show up." They would have caught the bug in M1.2 and serve as permanent regression guards.

- [ ] **Step 1: Locate insertion point**

```bash
tail -20 crates/agentprof-adapters/tests/analyzer_on_fixtures.rs
```

Identify where the existing per-fixture tests end (likely at the bottom of the file, before `}` for the test module if any, or just at EOF).

- [ ] **Step 2: Append end-to-end assertion tests**

```rust
// ──────────────────────────────────────────────────────────────────────
// B1 — end-to-end regression guards: fixtures with success:false events
// must produce non-zero failure_count downstream. Closes the silent bug
// that hid behind the always-zero failure_count between M1.2 and B1.
// See ADR-0013 + spec §7.3.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn b1_with_mcp_calls_has_tool_failure() {
    let report = load_and_analyze("with-mcp-calls");
    let total_tool_failures: u32 = report
        .tool_rank
        .iter()
        .map(|r| r.failure_count)
        .sum();
    assert!(
        total_tool_failures >= 1,
        "with-mcp-calls fixture has 1 tool.execution_complete success=false event; \
         expected >= 1 tool failure, got {total_tool_failures}. \
         Regression of the M1.2 always-zero bug?"
    );
}

#[test]
fn b1_multi_sess_c_has_tool_failure() {
    let report = load_and_analyze("multi-sess-c");
    let total_tool_failures: u32 = report
        .tool_rank
        .iter()
        .map(|r| r.failure_count)
        .sum();
    assert!(
        total_tool_failures >= 1,
        "multi-sess-c fixture has 1 tool.execution_complete success=false event; \
         expected >= 1 tool failure, got {total_tool_failures}."
    );
}

#[test]
fn b1_with_hooks_heavy_has_hook_failure() {
    let report = load_and_analyze("with-hooks-heavy");
    let total_hook_failures: u32 = report
        .hook_rank
        .iter()
        .map(|r| r.failure_count)
        .sum();
    assert!(
        total_hook_failures >= 1,
        "with-hooks-heavy fixture has 2 hook.end success=false events; \
         expected >= 1 hook failure, got {total_hook_failures}."
    );
}

#[test]
fn b1_with_aborts_has_hook_failure() {
    let report = load_and_analyze("with-aborts");
    let total_hook_failures: u32 = report
        .hook_rank
        .iter()
        .map(|r| r.failure_count)
        .sum();
    assert!(
        total_hook_failures >= 1,
        "with-aborts fixture has 1 hook.end success=false event; \
         expected >= 1 hook failure, got {total_hook_failures}."
    );
}
```

- [ ] **Step 3: Run new tests**

```bash
cargo test -p agentprof-adapters --all-features --test analyzer_on_fixtures b1_ 2>&1 | tail -10
```

Expected: all 4 PASS (T3 already wired the consumers, so failure_count is now non-zero).

- [ ] **Step 4: Confirm full suite still green**

```bash
cargo test --workspace --all-features 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6} END {printf "%d passed, %d failed\n", p, f}'
```

Expected: all PASS. Baseline +4.

- [ ] **Step 5: Lint gates**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/agentprof-adapters/tests/analyzer_on_fixtures.rs
git commit -m "test(adapters): B1 — end-to-end regression guards for failure_count

Adds 4 fixture-driven assertions verifying that the 4 fixtures known
to contain success:false events produce non-zero failure_count
end-to-end (2 tool fixtures: with-mcp-calls + multi-sess-c; 2 hook
fixtures: with-hooks-heavy + with-aborts).

These tests would have caught the always-zero bug in M1.2 if they had
existed. They now serve as permanent regression guards — any future
refactor that re-hardcodes Success or breaks the payload_success wiring
will trip a clearly-named test pointing back to ADR-0013.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 5: Documentation updates (L1 + L2 + CHANGELOG)

**Files:**
- Modify: `docs/architecture.md` — Event trait surface listing
- Modify: `crates/agentprof-core/README.md` — Adapter trait surface table
- Modify: `crates/agentprof-adapters/README.md` — Note CopilotEvent overrides
- Modify: `CHANGELOG.md` — `### Fixed` entry

Per `update-docs-on-code-change.instructions.md` + project §4.2 trigger table.

- [ ] **Step 1: Locate the Event trait listing in `docs/architecture.md`**

```bash
grep -n "payload_tool_requests\|payload_model_metrics" docs/architecture.md
```

Find the section listing the `payload_*` family of methods on the Event trait.

- [ ] **Step 2: Add `payload_success` + `payload_error_message` to that listing**

Concrete edit example (the exact section header will vary):

```markdown
| `payload_tool_requests` | `Vec<(String, serde_json::Value)>` | F1 D-4 — args plumbing | ADR-0011 |
| `payload_model_metrics` | `Option<BTreeMap<String, ModelUsage>>` | F1.7 — session token rollup | ADR-0012 |
| **`payload_success`** | **`Option<bool>`** | **B1 — wire-format success bit** | **ADR-0013** |
| **`payload_error_message`** | **`Option<&str>`** | **B1 — failure error message** | **ADR-0013** |
```

(Adapt to whatever the actual table looks like — preserve column structure, keep ordering after the most recent `payload_*` entry.)

- [ ] **Step 3: Add the 2 methods to `crates/agentprof-core/README.md`**

```bash
grep -n "payload_tool_requests\|payload_model_metrics\|Adapter trait" crates/agentprof-core/README.md
```

Find the L2 adapter-trait-surface listing and append the same 2 rows (or short bullets) describing the new methods, with links to ADR-0013.

- [ ] **Step 4: Add a note to `crates/agentprof-adapters/README.md`**

Find the CopilotEvent override surface section and add a short bullet:

```markdown
- `payload_success` / `payload_error_message` (B1, ADR-0013): overridden
  for `ToolExecComplete` (success + error.message) and `HookEnd`
  (success only — wire schema has no hook error message field).
```

- [ ] **Step 5: Update `CHANGELOG.md`** — locate `## [Unreleased]` and add a `### Fixed` entry:

```markdown
### Fixed

- **B1 — `failure_count` always 0 (M1.2 regression)** — `derive.rs:383` had
  been hardcoding `ToolCallStatus::Success` (with a TODO comment "Task 10b
  will read actual success bit") and `:490` / `:504` had been hardcoding
  `HookCall.success: true` since M1.2 (commit `c5716aa`). The wire payload
  (`ToolResultData.success` + `.error.message`, `HookEndData.success`) was
  fully present but never consumed, silently neutralizing three already-shipped
  UX features on real Copilot data:
  - **F1.13** RoiView Tool cell Red/Yellow failure-severity color
  - **F1.16** By Hook `OK%` + Hook cell color
  - **F2.3** `compose_tool_cell_style` failure-wins-over-pending precedence

  Fix: extends `Event` trait with 2 default-`None` methods
  (`payload_success`, `payload_error_message`); overrides them in
  `CopilotEvent` for `ToolExecComplete` + `HookEnd`; consumes them in
  `derive_episodes::on_tool_complete` + `on_hook_end`. `None` defaults to
  Success (forward-compat for older Copilot CLI 1.0.x / external adapters).
  `ToolCallStatus::Failure { message: Option<String> }` is now populated
  end-to-end — surfaced nowhere in UI yet but future-ready for RoiView
  detail / TurnDetail error display.

  Snapshot regen across 4 affected fixtures (`with-mcp-calls`, `with-aborts`,
  `with-hooks-heavy`, `multi-sess-c`). 16 other fixtures unchanged.

  Adds 9 new tests: 4 adapter-level unit (CopilotEvent overrides) + 5
  derive-level unit (stub Event with overridable success bit) + 4
  end-to-end fixture assertions (permanent regression guards).

  References: ADR-0013, spec `docs/superpowers/specs/2026-06-06-b1-failure-bit-design.md`,
  backlog `m1.6.2-followup-copilot-failure-bit`.
```

- [ ] **Step 6: Confirm no build/test regression from docs changes**

```bash
cargo test --workspace --all-features 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6} END {printf "%d passed, %d failed\n", p, f}'
```

Expected: all PASS, same count as Task 4 end.

- [ ] **Step 7: Commit**

```bash
git add docs/architecture.md \
        crates/agentprof-core/README.md \
        crates/agentprof-adapters/README.md \
        CHANGELOG.md
git commit -m "docs: B1 — sync L1/L2 + CHANGELOG for payload_success/payload_error_message

L1 docs/architecture.md: add the 2 new payload_* methods to the Event
trait surface listing.

L2 crates/agentprof-core/README.md: add the 2 methods to the adapter
trait surface table.

L2 crates/agentprof-adapters/README.md: note CopilotEvent overrides
for ToolExecComplete (success + error.message) + HookEnd (success only,
no wire error message field per ADR-0002 line 93).

CHANGELOG.md: add ### Fixed entry naming B1, the 3 affected shipped
features (F1.13/F1.16/F2.3), the 3 derive.rs change-sites, the
snapshot regen scope (4 fixtures), and the 9 new tests added.

Per project §4.2 (update-docs-on-code-change.instructions.md) + the
new-public-trait-API rule that mandates L1/L2 surface listing updates.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

## Task 6: Final verification gates

**No commits in this task** — pure verification. If any gate fails, return to the failing-test step and debug systematically (invoke `systematic-debugging` skill).

- [ ] **Step 1: Format check**

```bash
cargo fmt --all --check
```

Expected: clean (no output).

- [ ] **Step 2: Clippy with -D warnings**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
```

Expected: clean (`Finished` with no warnings).

- [ ] **Step 3: Full test suite**

```bash
cargo test --workspace --all-features 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6; i+=$8} END {printf "Tests: %d passed · %d failed · %d ignored\n", p, f, i}'
```

Expected: `Tests: ~795 passed · 0 failed · 1 ignored` (777 baseline + ~18 new across T1-T4).

- [ ] **Step 4: Doctest suite explicitly**

```bash
cargo test --doc --workspace --all-features 2>&1 | grep -E "^test result"
```

Expected: all PASS.

- [ ] **Step 5: Rustdoc -D warnings**

```bash
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace 2>&1 | tail -3
```

Expected: clean (`Finished` + `Generated ...`).

- [ ] **Step 6: Manual end-to-end smoke test** — confirm the user-visible payoff

```bash
cargo run --release -p agentprof-cli -- analyze \
    --agent copilot \
    --path crates/agentprof-adapters/tests/fixtures/copilot/with-mcp-calls \
    --export md 2>&1 | grep -iE "fail|error" | head -10
```

Expected: at least one line mentioning failure (failure_count column, OK% < 100, or the failing tool name colored / flagged in the markdown output).

- [ ] **Step 7: Confirm commit history matches the 5-commit plan**

```bash
git log --oneline -7
```

Expected last 5 entries (in order from HEAD):
```
<sha> docs: B1 — sync L1/L2 + CHANGELOG ...
<sha> test(adapters): B1 — end-to-end regression guards ...
<sha> fix(core): B1 — consume payload_success ... in derive_episodes
<sha> feat(adapters): CopilotEvent overrides payload_success ...
<sha> feat(core): add Event::payload_success + payload_error_message ...
```

Plus the earlier 2:
```
<sha> docs(adr): ADR-0013 — Event trait two-narrow-methods ...
<sha> docs(spec): B1 wire success bit design
```

- [ ] **Step 8: Optional — squash the 5 fix-wave commits into one `fix(core):` per spec §9**

If you prefer the spec's "single squashed commit" recommendation for the fix wave:

```bash
git rebase -i HEAD~5
# Mark the bottom commit as `pick`, the other 4 as `squash` (or `fixup` to drop their messages)
# Edit the final message to mention all 5 layers + tests
```

Otherwise leave the per-task commits — they're cleanly layered and easy to review.

- [ ] **Step 9: Update backlog**

```bash
# Mark the upstream bug as resolved in the session-store todo list.
# (Equivalent SQL run from the agent: UPDATE todos SET status='done' WHERE id='m1.6.2-followup-copilot-failure-bit')
echo "Mark m1.6.2-followup-copilot-failure-bit as done in todos table"
```

---

## Spec coverage self-check

| Spec section | Implemented by |
|---|---|
| §1.1 Three hardcoded sites | T3 Steps 3–5 (each site, with the `Abort` path NOT touched per §6.3) |
| §1.5 Existing fixture coverage | T4 (all 4 fixtures) + T3 (snapshot regen) |
| §4 Event trait extension | T1 |
| §5 CopilotEvent overrides | T2 |
| §6.1 on_tool_complete consumer | T3 Step 3 |
| §6.2 on_hook_end consumers (×2) | T3 Steps 4–5 |
| §6.3 Orphan path unchanged | T3 Step 5 explicit "Do NOT touch line ~607" warning |
| §7.1 Trait-level unit tests | T3 Step 1 (5 stub-Event tests) |
| §7.2 Adapter-level unit tests | T2 Step 4 (7 unit tests) |
| §7.3 End-to-end fixture assertions | T4 (4 assertions) |
| §7.4 Snapshot inspection protocol | T3 Step 8 (acceptance gate criteria) |
| §10 Documentation updates | T5 (all 4 files: architecture.md / 2× README / CHANGELOG) |
| §11 YAGNI explicit out-of-scope | Honored — no RoiView detail UX, no error sparklines, no MissingSuccessBit warning |
| §12 Success criteria | T6 (all 9 gate steps map to spec criteria) |

All spec sections covered. ADR-0013 D-1 through D-8 codified separately and referenced in T1/T2/T3 inline comments.
