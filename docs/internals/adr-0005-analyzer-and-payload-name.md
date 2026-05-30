---
title: "ADR-0005: Analyzer foundations — Event::payload_name() trait extension, start-time turn attribution, AnalysisReport in core"
status: "Accepted"
date: "2026-05-29"
authors: "@verdenmax (project owner), AI assistant (Copilot CLI session 252068e5)"
tags: ["architecture", "decision", "analyzer", "trait-extension", "agentprof-core", "agentprof-cli", "milestone-M1.4", "rollups"]
supersedes: ""
superseded_by: ""
---

# ADR-0005: Analyzer foundations — Event::payload_name() trait extension, start-time turn attribution, AnalysisReport in core

## Status

**Accepted**

## Context

M1.4 introduces the first user-facing CLI (`agentprof analyze`) on top of M1.3's `derive_episodes` algorithm. The CLI consumes 3 new analyzer rollups (`turn_summary` / `tool_rank` / `hook_rank`) and exports them as markdown or JSON.

Three architectural decisions interlock and need a single ADR because they all sit on the boundary between M1.3 (Episode layer) and M1.4 (analyzer + CLI):

### D-1 context: tool/hook/skill names are placeholder in M1.3

`Event` trait (defined ADR-0004 / M1.2) exposes only `id()`, `kind()`, `timestamp()`, `parent_id()`. It does NOT expose payload fields. As a result `derive_episodes` uses `event.id()` (the per-event UUID) as the key for `ToolEpisode` / `HookEpisode` / `SkillEpisode`. Every call gets a unique key. Every per-name episode contains exactly 1 call. Analyzer rollups would be meaningless — `tool_rank` would have N entries (N = total tool calls) each with `call_count=1`.

For analyzer to produce useful output, `derive_episodes` must use the **payload-defined name** (e.g., `tool.execution_start.data.toolName = "bash"`). This requires Event trait to expose name.

### D-2 context: commit-call-turn-divergence (M1.3 P0 follow-up)

The M1.3 final code review identified that `commit_tool_call` / `commit_hook_call` attribute the tool/hook back-reference into `self.turns[self.open_turn_idx]` — the Turn open **at commit (End) time**. But `ToolCall.turn_id` is set from `OpenToolCall.turn_id` — the Turn open **at Start time**.

When a tool spans a Turn boundary (Start in turn-A, Complete in turn-B), the two disagree:
- `ToolCall.turn_id == "turn-A"`
- `turns[turn-B].tool_calls` contains the back-reference

Two sources of truth, two answers to "which turn does this tool belong to". `turn_summary`'s per-turn tool counts go to the wrong turn. No M1.3 fixture exercised this scenario (because placeholder names made every per-name vec length 1), so the bug was hidden.

### D-3 context: AnalysisReport placement (core vs cli)

`AnalysisReport` is the export-ready container bundling `turn_summary` + `tool_rank` + `hook_rank` + `meta` + `warnings`. It can live in either:
- `agentprof-core::analyzer` — reusable by future TUI (M1.5), storage (Phase 2), and CLI alike
- `agentprof-cli::report` — closer to the export logic that consumes it

Choosing the wrong location forces either an awkward dependency or a refactor when TUI/storage lands.

### Constraints (carried from prior ADRs)

- ADR-0001 (events-first pivot): no token rollups, no `schema_utilization`. Analyzer is event-level only.
- ADR-0004 (lenient derive): `derive_episodes` cannot consult clock / I/O / mutate input. Pure function.
- Workspace dep rule (`.github/copilot-instructions.md` §3): `agentprof-core` cannot depend on `agentprof-adapters`. Analyzer functions operate on `&Episodes`, not on adapter-specific event enums.
- `#[non_exhaustive]` discipline on all extensibility points.
- Cross-version compatibility: M1.3 added 10 new variants for Copilot CLI 1.0.x; future versions will add more. Trait additions must be source-compatible via default-impl.

## Decision

**Three coordinated decisions:**

### D-1: Extend `Event` trait with `fn payload_name(&self) -> Option<&str> { None }`

A single new trait method with a `None` default implementation. CopilotEvent overrides for variants that have a meaningful payload name:

| Variant kind | `payload_name()` source |
|---|---|
| `ToolExecStart` / `ToolUserRequested` | `payload.tool_name` |
| `ToolExecComplete` | `None` — wire payload (`ToolResultData`) has no `tool_name` field; name is preserved by `derive_episodes` via the `OpenToolCall` stack pop |
| `HookStart` / `HookEnd` | `payload.hook_type` |
| `SkillInvoked` | `payload.name` |
| All other variants | `None` |

`derive_episodes` uses `event.payload_name().map(str::to_string).unwrap_or_else(|| event.id().to_string())` as the BTreeMap key. Variants returning `None` (e.g., `SessionStart`) don't reach the tool/hook/skill paths in the algorithm dispatch, so the fallback never triggers in practice — but the safety net is documented for future agent adapters.

> **Note**: Earlier drafts of this table listed `ToolExecComplete` alongside `ToolExecStart` with `payload.tool_name`. That was inaccurate — `ToolResultData` has no `tool_name` field (verify by inspecting `crates/agentprof-adapters/src/copilot/event.rs::ToolResultData`). The orphan branch of `on_tool_complete` (synthesized start for an orphan end) therefore cannot extract a name via `payload_name()`; see ADR-0005 Update §1 below for how the orphan branch handles this.

### D-2: Attribute back-references to start-time turn via `turn_id` lookup

Change `commit_tool_call` / `commit_hook_call` from `self.turns[self.open_turn_idx]` (end-time) to a lookup by name in the existing `turns: Vec<Turn>`:

```rust
if let Some(turn_id) = &call.turn_id {
    if let Some(turn_idx) = self.turns.iter().rposition(|t| &t.id == turn_id) {
        self.turns[turn_idx].tool_calls.push(CallRef::new(name.into(), new_idx));
    }
}
```

`rposition` (reverse linear search) is O(K) where K = number of turns seen so far. In real Copilot CLI sessions K is typically 100–300; the back-reference operations happen once per tool/hook commit. The amortized overhead is negligible (O(N×K) total worst case for K calls in a session of N turns, dwarfed by the JSON parse step).

The `turn_id` field on `ToolCall` / `HookCall` is now the canonical source of truth; `Turn.tool_calls` / `hook_calls` are just navigation indices that agree.

A new fixture `cross-turn-tool/` is added to lock this behavior in: tool start in turn-A, turn-A end, turn-B start, tool complete in turn-B. The snapshot verifies the back-reference lands in turn-A's `tool_calls`, not turn-B's.

`SkillInvocation` does NOT need this fix — skills are instant events (no span), they always commit at their event time inside whatever turn is currently open. `on_skill_invoked` continues to use `self.open_turn_idx`.

### D-3: `AnalysisReport` lives in `agentprof_core::analyzer`

The container ships in `agentprof-core` (the dependency-graph leaf), not in `agentprof-cli`. The TUI crate (M1.5) and a future storage crate (Phase 2) will consume `AnalysisReport` without taking a dependency on `agentprof-cli`.

Markdown and JSON renderers stay in `agentprof_cli::cmd::format`, because they encode CLI-specific concerns (writeable to stdout, exit-code mapping, terminal-friendly Duration formatting).

The split: **core defines data; cli (and future tui/storage) defines presentation.**

## Consequences

### Positive

- **POS-001**: **Analyzer produces meaningful output.** Tools/hooks/skills group by their real wire names ("bash", "view", "pre-tool"); `tool_rank` actually ranks N distinct tools instead of N distinct call-IDs.
- **POS-002**: **Cross-agent compatibility.** The default `payload_name() = None` lets any future adapter compile without overrides. Each adapter opts into accurate naming by overriding for its payload-bearing variants.
- **POS-003**: **`turn_id` and `Turn.tool_calls` agree.** Cross-turn tool spans (rare but real in async-write scenarios) attribute correctly. Per-turn tool counts in `turn_summary` are trustworthy.
- **POS-004**: **AnalysisReport reusable across crates.** TUI / storage / CLI all consume the same shape; no duplicated row types or refactors when TUI lands in M1.5.
- **POS-005**: **Additive change.** `payload_name()` has a default impl so external `Event` impls compile without modification. `commit_tool_call` fix is internal to `derive.rs` (no public API change). `AnalysisReport` is new (no migration).
- **POS-006**: **Snapshot-stable.** All analyzer rollups operate on already-sorted `BTreeMap` data; output ordering is deterministic. Existing M1.3 snapshots only need re-acceptance for real names.

### Negative

- **NEG-001**: **`payload_name()` requires per-variant boilerplate in CopilotEvent dispatch.** ~10 match arms (one per tool/hook/skill variant). Mechanical but adds lines.
- **NEG-002**: **`rposition` lookup is O(K) per commit.** Total worst-case O(N × K) where N = tool/hook commits and K = turn count. In observed real data K ≈ 200 and N ≈ 1000, so ~200K compares per session, microseconds at most. Acceptable but not zero.
- **NEG-003**: **M1.3 episode_derive snapshots must be re-accepted.** All 9 fixtures that previously showed `name = <event-uuid>` will now show `name = <tool-name>` (or hook-name/skill-name). Human review of each accepted snapshot required to catch genuine algorithm regressions.
- **NEG-004**: **The `_ms` Duration serialization in JSON loses precision below 1ms.** No real Copilot tool runs sub-millisecond, but the contract should document this.
- **NEG-005**: **`AnalysisReport.meta: SessionMeta` is cloned into the report.** SessionMeta is small (~200 bytes) so the clone is cheap, but it doubles the meta footprint when both `RawSession.meta` and `AnalysisReport.meta` are held in memory at once.
- **NEG-006**: **`AnalysisReport.warnings: Vec<DeriveWarning>` is also cloned.** For sessions with 1300+ warnings (observed in real data) this is a non-trivial allocation. Mitigated by `Episodes` typically dropping after `analyze()` returns; M1.5 TUI may need `Cow` to share.

## Alternatives Considered

### Alternative 1 (D-1): Dependency injection — pass `name_extractor: impl Fn(&E) -> Option<String>` to `derive_episodes`

- **ALT-001**: **Description**: Keep `Event` trait unchanged. Add a closure parameter to `derive_episodes` that extracts the name. Each adapter caller provides its own closure.
- **ALT-002**: **Rejection Reason**: Forces every consumer (CLI, future TUI, future storage) to know the adapter-specific closure for each agent kind. Couples `derive_episodes` callers to per-adapter knowledge that the trait was designed to abstract away. Violates the "agent-agnostic Episode layer" goal stated in ADR-0001 §"Adapter abstraction".

### Alternative 2 (D-1): Adapter-specific `derive_episodes_for_copilot` function

- **ALT-003**: **Description**: Skip trait extension entirely. `agentprof-adapters::copilot` exposes a free function `derive_episodes_for_copilot(&[CopilotEvent], &SessionMeta) -> Episodes` that pattern-matches CopilotEvent variants directly for name extraction.
- **ALT-004**: **Rejection Reason**: Duplicates the entire derive algorithm per agent. When Claude/Codex adapters land, they need their own copies of `derive_episodes_for_*`. Pure functions deduplicate via polymorphism; per-adapter functions don't.

### Alternative 3 (D-1): `Event::payload_data(&self) -> Option<&dyn Any>` for full payload access

- **ALT-005**: **Description**: Expose the entire payload (not just name) via `dyn Any`, letting `derive_episodes` downcast for any field it needs.
- **ALT-006**: **Rejection Reason**: `dyn Any` is the type-erasure tool of last resort. It defeats the type system, requires runtime downcast checks at every access, and only the algorithm's author knows which fields are safe to downcast. `payload_name()` exposes exactly what derive needs (and nothing more), preserving compile-time safety.

### Alternative 4 (D-2): Defer commit-call-turn-divergence to M1.5

- **ALT-007**: **Description**: Ship M1.4 with the M1.3 attribution behavior; M1.5 fixes it when TUI surfaces the discrepancy.
- **ALT-008**: **Rejection Reason**: M1.4's `turn_summary` analyzer reports per-turn tool/hook counts using `Turn.tool_calls.len()`. If those counts are wrong (per the divergence bug), the CLI's primary value proposition is undermined on day one. Fix is small (one function in derive.rs); deferring buys nothing.

### Alternative 5 (D-2): Two-source-of-truth — accept both `ToolCall.turn_id` and `Turn.tool_calls` may disagree

- **ALT-009**: **Description**: Document the discrepancy. Consumers that care can use either source. Pick one in analyzer (probably `turn_id`).
- **ALT-010**: **Rejection Reason**: "Two sources of truth that may disagree" is a famous antipattern. Even if the analyzer settles on `turn_id`, the JSON export will serialize both `ToolCall.turn_id` and `Turn.tool_calls`, exposing the inconsistency to downstream consumers (other tools, scripts, future TUI). Fix at the source.

### Alternative 6 (D-3): `AnalysisReport` in `agentprof-cli::report`

- **ALT-011**: **Description**: Keep the report type close to its primary consumer (CLI). Future TUI/storage either duplicates the shape or takes a dev-dep on cli.
- **ALT-012**: **Rejection Reason**: TUI/storage taking a dev-dep on `agentprof-cli` is wrong directionally — CLI is the orchestration layer that depends on others, not the other way around. Forces an unnecessary refactor at M1.5 when TUI lands.

### Alternative 7 (D-3): `AnalysisReport` as a separate `agentprof-analysis` crate

- **ALT-013**: **Description**: New crate dedicated to analyzer output, depended on by `agentprof-core` and consumed by cli/tui/storage.
- **ALT-014**: **Rejection Reason**: Splits the 5-crate workspace into 6 with no clear benefit. The analyzer + AnalysisReport are tightly coupled to Episodes (same crate). Splitting introduces a circular-feeling boundary without solving any concrete problem. YAGNI.

## Implementation Notes

- **IMP-001**: `Event::payload_name()` default impl returns `None`. Add the method to `agentprof-core::adapter::Event` trait. Make sure all existing `impl Event for ...` blocks (currently only `CopilotEvent`) compile without modification (the default impl ensures this). Then add the actual implementation to `CopilotEvent`'s impl block in `agentprof-adapters::copilot::event`.
- **IMP-002**: For variants whose payload struct has a `tool_name` / `hook_name` / `skill_name` field, return `Some(env.data.tool_name.as_str())`. For variants where the field name differs (verify per-variant against the M1.3 payload definitions), use the correct field. Document each match arm with a 1-line comment naming the source field.
- **IMP-003**: `derive_episodes`'s `on_tool_start` / `on_tool_complete` / `on_hook_start` / `on_hook_end` / `on_skill_invoked` change a single line: replace `let name = ev.id().to_string();` with `let name = ev.payload_name().map(str::to_string).unwrap_or_else(|| ev.id().to_string());`. The fallback should be vanishingly rare (only triggers if a future CopilotEvent variant forgot to override payload_name). Optionally log a warning when it triggers, but M1.4 keeps it silent.
- **IMP-004**: `commit_tool_call` and `commit_hook_call` change ~5 lines each:
  ```rust
  // before:
  if let Some(turn_idx) = self.open_turn_idx {
      self.turns[turn_idx].tool_calls.push(CallRef::new(name.to_string(), new_idx));
  }
  // after:
  if let Some(turn_id) = call.turn_id.as_ref() {
      if let Some(turn_idx) = self.turns.iter().rposition(|t| &t.id == turn_id) {
          self.turns[turn_idx].tool_calls.push(CallRef::new(name.to_string(), new_idx));
      }
  }
  ```
  Note: `call.turn_id` was set from `open.turn_id` (start-time turn) — this is the correct value to look up.
- **IMP-005**: New fixture `crates/agentprof-adapters/tests/fixtures/copilot/cross-turn-tool/`:
  - `events.jsonl` with 6 events: session.start, turn-A start, tool-X start (in turn-A), turn-A end, turn-B start, tool-X complete (matches turn-A start), turn-B end.
  - `README.md` per ADR-0003 conventions.
  - Snapshot verifies: `episodes.turns[0].tool_calls.len() == 1`, `episodes.turns[1].tool_calls.is_empty()`.
- **IMP-006**: `AnalysisReport` and `analyze()` go in `crates/agentprof-core/src/analyzer/mod.rs`. The 3 row types go in their respective `analyzer/{turn_summary,tool_rank,hook_rank}.rs` files. Re-export at module level.
- **IMP-007**: `analyze(&Episodes, &SessionMeta) -> AnalysisReport` is a pure function. `#[must_use]`. Test contract: identical input → byte-identical JSON serialization.
- **IMP-008**: `serde_json` Duration custom serializer: integer milliseconds (matches ADR-0004 IMP-007 for snapshot stability). Implement via `#[serde(with = "duration_ms")]` helper module in `agentprof-core::analyzer::mod` or a shared util.
- **IMP-009**: M1.3 `episode_derive` insta snapshots will all change (per IMP-002). After Phase A.4 runs `INSTA_UPDATE=always`, manually inspect every snapshot — especially `with-skill-invoked`, `with-hooks-heavy`, `with-mcp-calls` — and confirm the name change reflects real tool/hook names (not regression artifacts).
- **IMP-010**: A `cross-turn-tool` fixture should reveal whether the fix produces the expected snapshot. If it doesn't, Phase A.3's fix is incomplete and must be revisited before Phase B starts.

## References

- **REF-001**: ADR-0001 events-first pivot (`docs/internals/adr-0001-events-first-pivot.md`) — establishes events as first-class data unit and defers tokenization.
- **REF-002**: ADR-0002 Copilot event schema, Updated 2026-05-27 (`docs/internals/adr-0002-copilot-event-schema.md`) — the 28-variant CopilotEvent enum whose payloads carry tool/hook/skill name.
- **REF-003**: ADR-0004 episode derivation (`docs/internals/adr-0004-episode-derivation.md`) — defines `derive_episodes` algorithm + DeriveWarning + IMP-007 Duration serialization stability requirement.
- **REF-004**: M1.4 design spec (`docs/superpowers/specs/2026-05-29-m1.4-cli-and-analyzer-design.md`) — §6 data model, §7 algorithms, §10 phase plan.
- **REF-005**: M1.3 final code review findings (m14_followups SQL table) — `turn-back-references-need-name` and `triggered-tools-index-encoding` were fixed pre-merge via CallRef; `commit-call-turn-divergence` is the remaining P0 absorbed by this ADR.
- **REF-006**: `.github/copilot-instructions.md` §3 (dep graph: core is leaf) — motivates D-3 placement decision.
- **REF-007**: `.github/copilot-instructions.md` §7 (workspace coding rules) — `#[non_exhaustive]` discipline, `unwrap_used = deny` outside tests, rustdoc with `# Examples`.
- **REF-008**: `agentprof_core::adapter::Event` trait (M1.2 + extended here) — the seam at which name extraction becomes adapter-agnostic.

---

## Update 2026-05-29 (M1.4 audit followups)

A 4-part audit ran against the shipped M1.4 implementation (commits `ca69264` through `4e272fd`). Two additive refinements landed on `fix/m1.4-audit-followups`. Status stays **Accepted**; these are not reversals of D-1 / D-2 / D-3 but defensive elaborations.

### Update §1: Orphan `tool.execution_complete` aggregation via sentinel

**Context.** D-1 reads "use `payload_name()` as the `ToolEpisode` key with `event.id()` fallback". The audit (Part 2, M-2) found this fallback emits per-event UUIDs into `tool_rank` output for orphan `tool.execution_complete` events — because `ToolResultData` has no `tool_name` field, `payload_name()` returns `None`, and the fallback to `event.id()` produces UUIDs that masquerade as tool names.

**Decision.** `derive_episodes` aggregates ALL orphan completes under a single sentinel name, `ORPHAN_TOOL_SENTINEL = "<orphan>"`, exported from `agentprof_core::episode`. The accompanying `DeriveWarning::SynthesizedStart` already records the original event id, so per-call accountability is preserved without polluting the rollup.

**Implementation.** `on_tool_complete`'s orphan branch (`derive.rs:217-235`) replaces the fallback chain with a direct `ORPHAN_TOOL_SENTINEL.to_string()`. The angle-bracket form cannot collide with a real agent's tool name in any wire format. Future TUI / SQLite consumers can special-case this key for display, e.g. `(synthesized: 3 orphan completes)`.

**Tests.** `orphan_tool_complete_aggregates_under_sentinel_not_event_id` (3 orphan events → 1 ToolEpisode with 3 calls + 3 SynthesizedStart warnings carrying the original ids). Both `episode_derive__episode__orphan-events` and `analyzer_on_fixtures__analysis__orphan-events` snapshots re-accepted.

### Update §2: `DeriveWarning::PayloadNameMissing` for silent-failure defense

**Context.** D-1 establishes `Event::payload_name() -> Option<&str>` with a default `None` impl. This is great for forward compatibility (new variants are non-breaking), but the audit (Part 4, design D1) flagged it as a silent-failure footgun: a future Claude or Codex adapter author who forgets to override `payload_name()` for a name-bearing variant will see `derive_episodes` silently degrade into one episode per event with no warning.

**Decision.** A new `DeriveWarning::PayloadNameMissing { kind: EventKind, event_id: String }` variant is emitted whenever `payload_name()` returns `None` for an event whose dispatch kind indicates it SHOULD have a name (`ToolExecStart` / `HookStart` / `HookEnd` orphan branch / `SkillInvoked`). The graceful degradation behavior is preserved (output still produced via `event.id()` fallback), but downstream consumers can now flag the adapter as misconfigured.

**Implementation.** A new `DeriveState::resolve_payload_name(ev, expected_kind) -> String` helper consolidates the name-resolution + warning-emit pattern at all 4 callsites (`derive.rs:178-189`). The `expected_kind` parameter is the kind the caller dispatched on, reported as the warning's `kind` rather than `ev.kind()` so the signal stays stable even if a future adapter mislabels the event. The orphan `tool.execution_complete` branch does NOT use this helper because it has the `ORPHAN_TOOL_SENTINEL` aggregation instead (see Update §1) — this is documented in the `PayloadNameMissing` rustdoc.

**Markdown renderer.** `cmd::format::md::write_warnings` adds a `PayloadNameMissing: N` counter alongside the existing 4 categories.

**Tests.** `payload_name_missing_warning_fires_when_adapter_returns_none` feeds the stub `E` (no `payload_name` override) through `ToolExecStart` / `HookStart` / `SkillInvoked` dispatch and verifies 3 warnings emit with correct kind + event_id, AND that graceful degradation produces episodes keyed by `event.id()`. Existing fixtures and snapshots are unaffected because `CopilotEvent` correctly overrides `payload_name()` for all 5 name-bearing variants.

### Update §3: D-1 table corrections (documentation accuracy)

The original D-1 table had three documentation errors found by the audit (Part 1, observation 1):

- Listed `ToolExecComplete` alongside `ToolExecStart` as `payload.tool_name`. In reality `ToolResultData` has no `tool_name` field and `CopilotEvent::payload_name()` returns `None` for this variant.
- Listed `HookStart`/`HookEnd` source as `payload.hook_name`. Real field is `hook_type`.
- Listed `SkillInvoked` source as `payload.skill_name`. Real field is `name`.

The table has been corrected. No code changed — the implementation was always correct; only the documentation was stale.

### Update §4: `percentile` documentation precision

The audit (Part 2, M-1) found `ToolRankRow.p50_duration` / `HookRankRow.p50_duration` field docs said "Median per-call duration" but the implementation uses nearest-rank with `f64::round` (half-away-from-zero), which for even samples rounds UP to the upper midpoint (e.g. `[1s, 2s]` → `p50 = 2s`, not `1.5s`). The docs are now changed to "Approximate median (nearest-rank percentile, not averaged-when-even statistical median)". The algorithm is unchanged.

Rationale for keeping nearest-rank: every returned value is a real observation (no synthetic averages), at the cost of slight upward bias on even samples. Fine for the small per-tool / per-hook samples M1.4 emits. M1.5+ can add linear interpolation if strict statistical medians become a requirement; an opt-in `Percentile::Median` enum + separate function is the likely shape (would NOT change `p50_duration` field semantics).

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

**Implementation.** A new `DeriveState.current_mode: Option<Mode>` field threads through the algorithm. `on_mode_event` now reads the actual mode from `ev.payload_mode()` (replacing the M1.3 PLACEHOLDER `Mode::Unknown("changed")`). `on_turn_start` clones `current_mode` into the new turn via `clone_from` (clippy-preferred). A new `on_assistant_message` handler writes `Turn.model` + `Turn.output_tokens`. Dispatch table gains `EventKind::AssistantMessage => state.on_assistant_message(ev)`.

**No new `DeriveWarning` variants.** Unlike `PayloadNameMissing` (where missing → opaque-UUID `ToolEpisode` keys, a real degradation), missing `model` / `output_tokens` / `mode` only produces `None`-valued cells — a clear user signal on its own. Adding `PayloadXxxMissing` for every metadata field would inflate the warning vocabulary without proportional value.

**Tests.** 3 trait-default unit tests (`adapter.rs`), 5 CopilotEvent override tests (`event.rs`), 4 `derive.rs` integration tests covering sum / last-wins / mode-mid-turn / defensive-no-message scenarios, snapshot re-acceptance for 14 affected `.snap` files (7 episode + 7 analyzer layers), 1 CLI end-to-end regression test asserting `minimal` fixture's `turn_summary[0].output_tokens == 10`.

**Status.** Spec `docs/superpowers/specs/2026-05-29-turn-metadata-extraction-design.md` (Approved). This Update is the architectural record; the spec is the implementation contract.

### Update §6: Post-output audit fixes (parse-warning visibility, schema mismatches, user-blocking split)

**Context.** A 2026-05-29 audit of `agentprof analyze` against a real live Copilot CLI 1.0.54 session (11 806 lines) found three classes of issue that this Update closes:

1. **~17 % of events were silently dropping at the parser layer.** Real Copilot CLI 1.0.x emits multiple wire shapes for some event types:
   - `hook.start` for `postToolUse` hooks has no `source` field (only `sessionStart`-style hooks do); the `HookInput.source: String` required field caused 779 / 779 (100 %) of `postToolUse` hooks to drop, surfacing as `synthesized_start = true, total_duration = 0ms` for the entire hook category in the Tool Rank.
   - `user.message` emitted by some CLI-typed prompts has no `source` field (46 % drop rate).
   - `assistant.message` emitted by **subagents** (spawned via `subagent.started` events) carry `parentToolCallId` instead of `turnId`; the `AssistantMessageData.turn_id: String` required field caused 1 995 / 2 789 (71 %) of assistant messages to drop, with all subagent token usage invisible.

   The earliest scan ran while the session was still live and saw ~24 % drops; the final measurement at branch HEAD on a frozen copy was 17 %. The audit doc uses 17 % as the canonical number for this Update.

2. **`ParseWarning`s were collected by the loader but never surfaced in the report.** Users had no way to know their session had 17 % of events dropping — the report just showed missing hooks / under-counted tokens. This breaks the FR-1.8 contract (parse warnings must be user-visible).

3. **`ask_user` dominated the Tool Rank's top.** In a 56 h session, `ask_user` alone consumed ~90 % of total tool wall-clock (63 h) — but this is **user think time**, not agent / machine work. Mixing it into the headline Tool Rank made the chart unreadable for its primary "where is the agent burning time?" purpose.

**Decision.**

For (1): all three required `String` fields become `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. The new `AssistantMessageData.parent_tool_call_id: Option<String>` field is added for subagent visibility. A new fixture `with-post-tool-use-hooks/` locks the regression (10 events covering all three schema variants + an in-turn-message regression case). This is a **conservative widening** for events the parser already saw before, but it has one side effect worth calling out:

> **Side effect: subagent token attribution.** Before this fix, subagent
> `assistant.message` events were silently dropping at the parser layer,
> so their token counts never reached `derive_episodes::on_assistant_message`
> and never contributed to any `Turn.output_tokens`. After the fix they
> parse cleanly and *do* contribute. `on_assistant_message` writes to
> whichever turn is open (`open_turn_idx`) without checking the payload's
> `turn_id` / `parent_tool_call_id`, so subagent tokens are attributed to
> the parent turn that spawned them. This is **closer to reality** (the
> turn did cause those tokens to be produced), but users comparing pre-
> and post-fix reports will see per-turn `output_tokens` go up for any
> turn that contains a `task` tool call. The `with-post-tool-use-hooks`
> fixture locks `output_tokens == 19` (= main 7 + subagent 12) as the
> expected behavior.

A future enhancement (tracked for M1.5+) may split this into separate `Turn.output_tokens` (main) and `Turn.subagent_output_tokens` (subagent-spawned) fields so renderers can show the breakdown without losing the unified total.

For (2): `AnalysisReport` gains a `parse_warnings: Vec<ParseWarning>` field; `analyze()` signature becomes `analyze(&Episodes, &SessionMeta, &[ParseWarning]) -> AnalysisReport`. The markdown renderer's `Session` block gains a `- Parse warnings: N` line, and the `Warnings` section adds a parse-stage breakdown (Json / Io / OutOfOrder counts) *before* the existing derive-stage breakdown. JSON contract is additive: `#[serde(default, skip_serializing_if = "Vec::is_empty")]` keeps old empty reports byte-identical and old JSON deserializable. `ParseWarning` also gains `PartialEq + Eq` (needed for round-trip + test assertions, was previously omitted by oversight).

For (3): `ToolRankRow` gains `is_user_blocking: bool`. A new `pub const USER_BLOCKING_TOOLS: &[&str] = &["ask_user"]` in `agentprof_core::analyzer::tool_rank` is the single source of truth for the membership rule; `tool_rank()` populates the flag at analyzer time. The markdown renderer's `write_tool_rank` partitions rows: work tools render in `## Tool Rank` as before; user-blocking tools render in a separate `## User-blocking tools (wall-clock includes user think time)` section with an explanatory caption. JSON contract is additive: rendered flat as before, but each row now carries `is_user_blocking` (defaulting to `false` for older JSON via `#[serde(default)]`).

**Privacy.** The audit also surfaced that several fields in `meta` and `turn_summary` leak PII (Unix `cwd`, `branch`, model internal-name `claude-opus-4.7-1m-internal`, ~800 turn UUIDs per session). This Update **documents** the leak in the new `docs/internals/privacy-considerations.md` (PII tier table + manual `sed`/`jq` redaction cheat sheet + planned `--redact` / `--anonymize` flags). It does **not** implement redaction; that's deferred to M1.5+ where it lands as opt-in CLI flags alongside the `aggregate` subcommand.

**Tests.** New `with-post-tool-use-hooks/` fixture + episode + analyzer + raw-load registrations (3 places). New unit tests: `analyzer::tests::analyze_carries_parse_warnings_through`, `analyzer::tool_rank::tests::ask_user_is_flagged_as_user_blocking` + `user_blocking_constant_contains_ask_user`. Extended `analysis_report_json_round_trip_is_lossless` to exercise `parse_warnings`. Extended `failure_and_orphan_counts` to assert `bash !is_user_blocking`. Snapshot re-acceptance for 8 affected `.snap` files (7 analyzer_on_fixtures + 1 CLI integration).

**Status.** Spec `docs/superpowers/specs/2026-05-29-post-output-audit-design.md` (Approved). This Update is the architectural record; the spec + commits on `fix/post-output-audit` are the implementation contract.
