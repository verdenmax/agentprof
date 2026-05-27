---
title: "ADR-0004: Episode derivation — lenient single-pass algorithm with orphan synthesis and DeriveWarning model"
status: "Proposed"
date: "2026-05-27"
authors: "@verdenmax (project owner), AI assistant (Copilot CLI session 252068e5)"
tags: ["architecture", "decision", "data-model", "episode", "derive_episodes", "agentprof-core", "milestone-M1.3"]
supersedes: ""
superseded_by: ""
---

# ADR-0004: Episode derivation — lenient single-pass algorithm with orphan synthesis and DeriveWarning model

## Status

**Proposed**

## Context

After M1.2 shipped `CopilotAdapter` producing `RawSession<CopilotEvent>` — a flat stream of typed events — M1.3 must aggregate those events into higher-level structures the analyzer (M1.4), TUI (M1.5), and exporters (M1.4–M1.6) can consume agent-agnostically.

The M1.2 design spec (`docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md` §6.5) already defined five episode types — `Turn`, `ToolEpisode`, `HookEpisode`, `SkillEpisode`, `ModeSegment` — and an `Episodes` container. What it did NOT lock in was the **derivation algorithm**: how exactly do you go from `&[E: Event]` to `Episodes`, and how do you handle the dirty realities of real telemetry?

### Real-world dirt revealed by M1.2 demo

Running M1.2's adapter against the developer's actual `~/.copilot/session-state/` (187 sessions, current session has 1796 events) exposed three categories of "mess" we must design for:

1. **Imbalanced start/end pairs.** `HookStart=1` vs `HookEnd=210`; `ToolExecStart=755` vs `ToolExecComplete=209`. Most likely cause: M1.2 clean-room schema mis-named some variants — but even after Phase B fixes the naming, real edge cases will remain (Copilot CLI crashes mid-write, hook callbacks that never fire `end`, etc.).
2. **Async event timestamps.** Hook events and tool events come from concurrent threads inside Copilot; `events.jsonl` is appended in arrival order, not strict happens-before order. ~1325 `ParseWarning` were collected across the current 1796 events, many likely from non-monotonic timestamps.
3. **Live sessions.** `is_live = true` means the last events have not yet been written. Open Turns / ToolCalls / HookCalls at end-of-file are normal, not errors.

### Consumer expectations

- **CLI (M1.4)**: must never crash on real data. A `Result<Episodes, _>` that bubbles up to the user would mean "your session is corrupted, can't analyze" — but in practice the data is *messy*, not *unanalyzable*.
- **TUI (M1.5)**: needs to render a timeline. Missing events should be visually flagged (greyed / synthesized markers), not omitted.
- **Exporters (M1.4–M1.6)**: need a stable, deterministic data shape regardless of how broken the input is. Snapshot tests must reproduce identical output on identical input across CI runs and developer machines.

### Constraint: `agentprof-core` is the dependency-graph leaf

Per the workspace rules (`.github/copilot-instructions.md` §3), `agentprof-core` cannot depend on `agentprof-adapters`. Therefore the algorithm must be polymorphic over `E: Event` (where `Event` is the trait defined in M1.2) — it cannot pattern-match on `CopilotEvent::ToolExecStart` directly, only on `event.kind() == EventKind::ToolExecStart`.

### Constraint: pure, deterministic, single-pass

The whole pipeline (CLI → adapter → derive_episodes → analyzer → exporter) must be reproducible and snapshot-testable. `derive_episodes` therefore cannot consult the clock, cannot read environment variables, cannot do I/O. Given a fixed `&[E]` and `&SessionMeta`, it produces the same `Episodes` byte-for-byte.

## Decision

**`derive_episodes<E: Event>(events: &[E], meta: &SessionMeta) -> Episodes` is a pure function that always succeeds (no `Result`), performs one in-order pass over events using a small internal `DeriveState` machine, and reports all data-quality issues through `Episodes.warnings: Vec<DeriveWarning>` rather than aborting.**

Three sub-decisions follow:

### D-1: Lenient over strict (no `Result`)

`derive_episodes` returns `Episodes`, not `Result<Episodes, _>`. Any input — even completely empty event slice, or 100% orphan events — produces *some* `Episodes`. Quality issues collect into `Episodes.warnings`.

### D-2: Orphan-end synthesis (preserve event counts)

When the algorithm encounters an `End`-shaped event (`HookEnd`, `ToolExecComplete`, `TurnEnd`) without a matching open `Start`, it **synthesizes a zero-duration Start** at the same timestamp as the End. The resulting episode is included in `Episodes` with a special status (`ToolCallStatus::OrphanSynthesizedStart`, `HookCall { synthesized_start: true, .. }`). A `DeriveWarning::SynthesizedStart` is also pushed.

When the algorithm reaches end-of-events with an open `Start` (no `End`), it **closes the span at the last event's timestamp** with status `ToolCallStatus::OpenAtEndOfSession` / `TurnStatus::Open`, and pushes `DeriveWarning::OpenAtEndOfSession`.

### D-3: Single-pass O(N) with bounded-stack state machine

The algorithm makes one forward pass, maintaining:

- `open_turn_idx: Option<usize>` — at most one open Turn at any time (Copilot sessions are linearly turn-based).
- `open_tool_calls: Vec<(name, PartialToolCall)>` and `open_hook_calls: Vec<(name, PartialHookCall)>` — stack-like; matching uses last-opened-with-same-name semantics.
- `open_skills: Vec<(SkillInvocation, window_remaining)>` — countdown windows for `triggered_tools` tracking.
- `current_mode_segment` — at most one.

Concurrency depth is bounded in practice by Copilot's serial execution model; the stacks rarely exceed 5–10 entries. Total complexity: O(N_events × log K) time, O(N_episodes + N_warnings) space.

### `DeriveWarning` taxonomy (4 variants)

```rust
#[non_exhaustive]
pub enum DeriveWarning {
    SynthesizedStart { kind: EventKind, end_event_id: String },
    OpenAtEndOfSession { kind: EventKind, start_event_id: String },
    AbortWithoutOpenElement { reason: String, at: DateTime<Utc> },
    NonMonotonicTimestamp { event_id: String, prev_at: DateTime<Utc>, this_at: DateTime<Utc> },
}
```

These four cover every "data quality" issue without conflating "expected but unfortunate" (live session open Turn) with "bug worth investigating".

## Consequences

### Positive

- **POS-001**: **Robust against real-world dirty data.** `analyze` never fails because of telemetry gaps; users see warnings in the report, not an error exit code.
- **POS-002**: **UI-friendly.** The `OrphanSynthesizedStart` / `OpenAtEndOfSession` statuses let the TUI render greyed-out cells, hovers, or annotations — the *information* about the orphan-ness is preserved, not silently discarded.
- **POS-003**: **Snapshot-testable.** A pure function over `(events, meta)` means insta snapshots reproduce across CI / dev / OS / time zones; no `Utc::now()` or clock dependency anywhere.
- **POS-004**: **O(N) single-pass.** Even on the largest observed real session (~10k events), derive runs in <100ms. Streaming friendly if a future M-version needs incremental updates.
- **POS-005**: **Agent-agnostic.** Polymorphic over `E: Event`, so Claude / Codex adapters (Phase 2/3) get derive_episodes for free as long as their event enums implement the trait.
- **POS-006**: **Cheap to extend.** New `DeriveWarning` variants land via `#[non_exhaustive]` without breakage; new Episode types are additive in `Episodes` struct.

### Negative

- **NEG-001**: **Loss of "abort by source" attribution detail when no element is open.** Aborts that happen at "rest" (no open Turn / ToolCall / HookCall) go into `Episodes.aborts` and lose the contextual element they would have attached to. Mitigated by tagging timestamp + reason, but conceptually we lose the "who got aborted" answer.
- **NEG-002**: **Synthesized orphan Starts can mislead naïve consumers.** A consumer that just counts `ToolEpisode.calls.len()` without checking `status` will overstate actual tool invocations. Mitigation: rustdoc on `ToolCallStatus::OrphanSynthesizedStart` warns; analyzer (M1.4) rollups always filter or annotate.
- **NEG-003**: **Skill `triggered_tools` window is heuristic.** The K-event downstream window is not semantically precise; a tool invoked because the user said "do X" 60 events after a skill suggested "do X" could be falsely attributed. Default K=50 chosen empirically; configurable in future via `DeriveConfig`.
- **NEG-004**: **Single-pass loses "look-back correction" capability.** If a hook end arrives several events after its expected location (e.g., reordered timestamps), the algorithm cannot retroactively pair it with the still-open start; it will synthesize a new orphan instead. Mitigated by `DeriveWarning::NonMonotonicTimestamp` collecting these for inspection.
- **NEG-005**: **Lenient absorbs real bugs.** A genuine parser/adapter bug that produces 1000 orphan events will silently produce 1000 warnings, not a hard failure. Mitigated by the smoke test (`copilot_smoke.rs` from M1.2 asserts zero `Unknown`) and by analyzer (M1.4) reporting warning counts prominently.

## Alternatives Considered

### Alternative 1: Strict — `derive_episodes -> Result<Episodes, DeriveError>`

- **ALT-001**: **Description**: Return `Result`; any orphan event, any non-monotonic timestamp, any abort without open element produces an `Err`. CLI surfaces this as exit code 2 ("data error") and refuses to render.
- **ALT-002**: **Rejection Reason**: M1.2 demo proved real Copilot data has *hundreds* of these per session; `Result` would mean "every real session is uanalyzable", inverting the value proposition. Also fails POS-001 (robust) and POS-002 (UI-friendly).

### Alternative 2: Configurable — `derive_episodes(events, meta, mode: StrictOrLenient)`

- **ALT-003**: **Description**: Add a `DeriveMode` parameter. Lenient is default; Strict for schema-audit and CI gating. Two code paths in `DeriveState`.
- **ALT-004**: **Rejection Reason**: Doubles algorithm-level test surface; adds a public API knob that needs documentation and stability guarantees; only one consumer (potentially the schema-audit xtask) would use Strict, and that consumer can already get the same signal by counting `Episodes.warnings` from the lenient pass. **YAGNI**: keep one algorithm, defer until a real second consumer exists.

### Alternative 3: Drop orphans silently — no warnings, no synthesis

- **ALT-005**: **Description**: Drop `HookEnd` without `Start`; never synthesize; never warn. `Episodes` simply does not include orphan events.
- **ALT-006**: **Rejection Reason**: Loses information critical to debugging (POS-002): users / developers cannot tell whether their telemetry is broken because the silent drops are invisible. Also unstable under schema evolution — a renamed `hook.start` → `hook.begin` would silently zero out all hook visibility.

### Alternative 4: Half-synthesis — count but don't include

- **ALT-007**: **Description**: Increment per-name counters (e.g. `ToolEpisode.calls_count: u32` separate from `.calls: Vec<ToolCall>`) for orphans, but don't append a `ToolCall` to the list.
- **ALT-008**: **Rejection Reason**: Two parallel sources of truth (the counter and the vec length) is error-prone for analyzer rollups (M1.4). Full synthesis with a special `status` is conceptually simpler — `.calls.len()` is always the answer to "how many invocations were observed (real + synthesized)", and `.iter().filter(|c| c.status == OrphanSynthesizedStart).count()` answers the synthesized subset.

### Alternative 5: Streaming with lookahead window

- **ALT-009**: **Description**: Buffer K events in a sliding window before committing; allow late-arriving `Start` events to retroactively pair with already-seen `End` events.
- **ALT-010**: **Rejection Reason**: NEG-004 mitigation is not worth K-event memory + complexity overhead. M1.2 real-data demo showed `NonMonotonicTimestamp` is rare for *paired* events (hook start/end usually arrive in order); reordering typically affects sibling events (two hook starts). Defer until profiling demands it.

## Implementation Notes

- **IMP-001**: The `DeriveState` struct is **private to `agentprof-core::episode::derive`**. Only the free function `derive_episodes` is `pub`. State machine internals can change without breaking semver.
- **IMP-002**: All `Episodes.*` fields use `BTreeMap` (not `HashMap`) where order matters for snapshot tests. `BTreeMap<String, ToolEpisode>` ensures alphabetical tool ordering across runs.
- **IMP-003**: The `triggered_tools` window K is a const private to `derive.rs` for now (default 50). When M1.4 surfaces this in CLI, refactor into a `DeriveConfig { window_size: usize }` struct passed to a new `derive_episodes_with(events, meta, config)` helper; keep `derive_episodes(events, meta)` calling the helper with `DeriveConfig::default()`.
- **IMP-004**: `DeriveWarning::NonMonotonicTimestamp` records both `prev_at` and `this_at`. Even though the algorithm doesn't reorder, the warning lets downstream tools (analyzer / TUI) detect timestamp inversion patterns.
- **IMP-005**: All `Episode` payload types are `#[non_exhaustive]` + have `pub const fn new(...)` constructors taking only required fields (mirror M1.2's `SessionRef::new` / `SessionMeta::new` / `RawSession::new` pattern). Optional fields default to None and are set via direct field access from inside `agentprof-core` only.
- **IMP-006**: The integration-test fixture `crates/agentprof-adapters/tests/fixtures/copilot/orphan-events/` is mandatory. It must contain at minimum: one orphan `HookEnd`, one orphan `ToolExecComplete`, one `Abort` while all elements closed — to exercise all three synthesis paths in CI.
- **IMP-007**: Insta snapshots use the `serde_json::to_string_pretty` form of `Episodes`. `Span` and `Duration` are serialized in stable formats (ISO-8601 for timestamps, milliseconds-as-integer for durations) — never use `Debug` output, which is platform/version sensitive.
- **IMP-008**: When a `ParseWarning` was already collected during parsing (e.g. `Json` / `OutOfOrder`), `derive_episodes` does NOT duplicate it as a `DeriveWarning`. The two warning streams are *complementary*: `ParseWarning` for wire-format issues, `DeriveWarning` for semantic-pairing issues. Both end up in different fields (`RawSession.parse_warnings` and `Episodes.warnings`).

## References

- **REF-001**: ADR-0001 events-first pivot (`docs/internals/adr-0001-events-first-pivot.md`) — establishes events as the first-class data unit
- **REF-002**: ADR-0002 Copilot event schema (`docs/internals/adr-0002-copilot-event-schema.md`) — the per-agent event enum this algorithm consumes
- **REF-003**: ADR-0003 synthetic-fixture strategy (`docs/internals/adr-0003-synthetic-fixture-strategy.md`) — fixture rules for the `orphan-events/` fixture required by IMP-006
- **REF-004**: M1.2 design spec §6.5 (`docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md`) — Episode type definitions (this ADR locks the *algorithm*, that spec locked the *types*)
- **REF-005**: M1.3 design spec (`docs/superpowers/specs/2026-05-27-m1.3-episode-and-schema-fix-design.md`) — §6 data model, §7 algorithm pseudocode, §10 commit plan, §11 risks
- **REF-006**: Real-data finding from M1.2 finishing — `Unknown=69`, `ParseWarning=1325`, `HookStart=1 / HookEnd=210`, `ToolExecStart=755 / ToolExecComplete=209` over 1796 events in the developer's current session; the empirical evidence that motivates D-1 / D-2
- **REF-007**: `.github/copilot-instructions.md` §3 — workspace dependency-graph constraint (agentprof-core is leaf) that motivates the `E: Event` polymorphism
- **REF-008**: `agentprof_core::adapter::Event` trait (defined M1.2 / commit `1331bef`) — the trait the algorithm is polymorphic over
