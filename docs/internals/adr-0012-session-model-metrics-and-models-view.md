# ADR-0012: Session-level model metrics + Models view

**Status**: Accepted
**Date**: 2026-06-03
**Milestone**: M1.6.4 follow-up wave Phase 3 (F1.7 — token observability)
**Spec**: [`docs/superpowers/specs/2026-06-03-f1.7-models-view-design.md`](../superpowers/specs/2026-06-03-f1.7-models-view-design.md)
**Plan**: [`docs/superpowers/plans/2026-06-03-f1.7-models-view.md`](../superpowers/plans/2026-06-03-f1.7-models-view.md) *(written next, after this ADR)*

## Context

F1.6 surfaced per-turn `Turn.output_tokens` (sum of `assistant.message.outputTokens`
across each turn). User immediately asked: "input tokens and cache tokens?"

Empirical survey of real Copilot CLI session jsonl (session
`252068e5-…`, 2026-06-03):

| Token category | Wire source | Granularity |
|---|---|---|
| output | `assistant.message.outputTokens` (`u32`) | per-message (F1.6 sums per-turn) |
| input | `session.shutdown.modelMetrics[model].usage.inputTokens` (`u64`) | **per-model session total** |
| cache_read | `session.shutdown.modelMetrics[model].usage.cacheReadTokens` (`u64`) | **per-model session total** |
| cache_write | `session.shutdown.modelMetrics[model].usage.cacheWriteTokens` (`u64`) | **per-model session total** |
| input (compaction-only subset) | `session.compaction_complete.compactionTokensUsed.inputTokens` (`u64`) | per-compaction-event |
| conversation-token trajectory | `session.compaction_start.conversationTokens` (`u64`) | per-compaction-event |

**Key finding — Copilot CLI does NOT expose per-turn input/cache tokens.** They
only land in session-level rollups on `session.shutdown` and in
per-compaction-event totals (`session.compaction_complete`). Per-turn
estimation via `conversationTokens` diffing is theoretically possible
but lossy (only fires at compaction boundaries, typically every 700k
tokens — not per turn).

User feedback explicitly asked for "input tokens and cache tokens" with
no specific request for per-turn semantics, so the natural fit is to
expose what the wire actually carries: **session-level per-model
totals**.

This ADR records the architectural decisions that shape the
implementation: where the data lives in the core model, how adapters
expose it, how the TUI presents it, what the empty-state UX is, and
why an estimation-based per-turn approach was rejected.

## Decisions

(Each row maps 1:1 to a decision in the F1.7 spec §9 / §3. Re-opening
a decision requires editing this ADR or recording a new ADR that
explicitly supersedes the affected D-row.)

- **D-1** Wire-data fidelity: ship **session-level per-model totals**
  as the source of truth, NOT synthetic per-turn estimation. Rationale:
  (a) Copilot CLI's wire format is what it is — fabricating per-turn
  values via `conversationTokens` diffing introduces estimation error
  that users can't audit against any source; (b) session-level totals
  are already 100 % accurate; (c) Phase C (per-turn estimation) can
  always layer on later as an OPTIONAL surface clearly labeled
  "estimated" if user demand surfaces. Spec §1, §6.

- **D-2** Storage location: **`AnalysisReport.model_metrics:
  Option<BTreeMap<String, ModelUsage>>`** rather than
  `SessionMeta.model_metrics` or a new `SessionTotals` top-level
  struct. Rationale: parallels existing `tool_rank` / `hook_rank`
  rollups (also session-level data derived from events); all current
  exporters already serialize `AnalysisReport` so they pick up
  `model_metrics` automatically (free JSON/HTML/MD/CSV support); keeps
  `SessionMeta` focused on input metadata (session_id, agent_kind,
  cwd) and reserves `AnalysisReport` for derived data. Spec §3.1, §7 Q1.

- **D-3** Intermediate hop: **`Episodes.model_metrics:
  Option<BTreeMap<String, ModelUsage>>`** populated by `derive_episodes`
  and cloned into `AnalysisReport.model_metrics` by `analyze()`.
  Alternatives: (a) Add a new public `analyze_with_events(events,
  episodes, meta, parse_warnings)` fn so model_metrics is collected
  outside of `derive_episodes`. (b) Move the population up to the cli
  layer and stash on `SessionMeta`. Rejected both: (a) doubles the
  `analyze*` API surface for one feature; (b) breaks the L1 "derived
  data lives in `Episodes` / `AnalysisReport`" boundary established by
  ADR-0004. Decision: extend `Episodes` with the new field (matches
  F1 D-5 `ToolCall.arguments` precedent), populate during the existing
  `derive_episodes` walk on `EventKind::SessionShutdown` events,
  clone into `analyze()` output via the same `episodes.warnings.clone()`
  pattern. Spec §3.2.

- **D-4** Adapter exposure: **`Event::payload_model_metrics() ->
  Option<BTreeMap<String, ModelUsage>>`** with `None` default —
  symmetric with the existing 5 `payload_*` extension methods
  (`payload_name`, `payload_model`, `payload_output_tokens`,
  `payload_mode`, `payload_tool_requests`, `tool_call_id`). Alternative
  shape `Adapter::parse_model_metrics(&self, session: &RawSession) ->
  Map<String, ModelUsage>` rejected: would force `derive_episodes`
  (which currently operates on `&[Event]` via the `Event` trait) to
  acquire an `Adapter` reference, breaking the L1 "core consumes the
  Event trait, not the Adapter trait" rule established by ADR-0005.
  Spec §3.2, §7 Q2, ADR-0005 precedent.

- **D-5** Method return shape: **`Option<BTreeMap<String, ModelUsage>>`**
  (singular Option, not `Vec<(String, ModelUsage)>` like
  `payload_tool_requests`). Rationale: model metrics are a session-level
  rollup whose granularity is "everything one event knows about all
  models in this session"; multiple events emitting partial rollups
  would be confusing, so the contract is "an event either has the
  complete rollup or has nothing". `BTreeMap` (deterministic iteration
  order) instead of `HashMap` (per crate convention). Spec §3.2.

- **D-6** Conflict resolution on multiple emitting events: **last-wins**
  by event order (matches the existing `Turn::model` / `Turn.mode`
  semantics in `derive_episodes`). In practice Copilot CLI emits ≤1
  `session.shutdown` per session, so the conflict path is defensive only.
  No `tracing::debug!` logging on conflict (unlike F1 D-4 for
  `payload_tool_requests` duplicates) because the model_metrics
  payload is large (~hundreds of bytes serialized) and logging it
  would be noisy. Spec §3.2.

- **D-7** Adapter parsing fidelity: walk the existing
  `ShutdownData.model_metrics: BTreeMap<String, serde_json::Value>`
  free-form Value tree via `.get("usage")?.get("inputTokens").and_then(.as_u64).unwrap_or(0)`
  rather than introducing typed `ModelMetricsEntry` / `ModelUsageRaw`
  intermediate structs. Rationale: the Copilot wire format for
  `modelMetrics` has been observed in multiple shapes across CLI
  versions; free-form `Value` deserialization with field-wise unwrap
  is more robust against schema drift; absent fields produce `0`
  instead of failing the entire event. The `ShutdownData` struct
  retains its `serde_json::Value` field — only the new
  `payload_model_metrics()` method walks the tree. Spec §3.3.

- **D-8** New `ModelUsage` public struct in `agentprof-core::analyzer`:
  4 fields (`input_tokens`, `output_tokens`, `cache_read_tokens`,
  `cache_write_tokens`), all `u64`, `#[non_exhaustive]`, with a
  `pub const fn new()` zero-initializer + `pub const fn total()`
  convenience method. `u64` not `u32` because `cache_read_tokens` can
  exceed `u32::MAX` for long sessions (observed 654M / 4.29G upper
  bound). Spec §3.1.

- **D-9** TUI view form: **new dedicated `View::Models` accessible via
  key `4`**, NOT a banner injected into FlamegraphView header or a
  footer line under TurnDetailView. Rejected alternatives: (a) banner
  in Flamegraph (would compress the gantt area further after F1.6's
  prefix grew by 6 chars); (b) footer in TurnDetailView (session data
  shown in per-turn context creates confusion — "is this 781k input
  tokens for THIS turn or for the whole session?"); (c) both banner +
  footer (information duplication). The dedicated view is the cleanest
  separation: session-level data in a session-level view. Spec §2.1,
  user choice (A1) recorded 2026-06-03.

- **D-10** TUI key binding: **`4`** was previously unbound at the
  top-level number-key view-switch (`1`/`2`/`3`); `4` extends the
  range naturally. Help overlay updated to list `4` as a view-switch
  key. Spec §2.4.

- **D-11** Models view sort order: **input tokens descending by
  default, not interactively re-sortable in v1**. Models with highest
  input cost float to top — usually the model the user actually
  cares about. Interactive resort (cycle by `t`/`c`/`p` keys like
  RoiView) deferred until user demand surfaces (most sessions have 1-3
  models, so manual scan beats sort hotkeys). Spec §2.2.

- **D-12** Empty-state UX: **centered placeholder + multi-line
  explanation**, NOT an empty table with footer hint or "Esc back to
  prev view" silent fallback. Rationale: matches TurnDetailView's
  `(no tool calls)` / `(turn not found)` placeholder convention
  established in F1 D-7; explanation tells user *why* (no shutdown
  event yet) rather than letting them think the binary is broken.
  Spec §2.3, §7 Q3.

- **D-13** Watch mode integration: **same transient-AppState
  round-trip as F1 D-14** — `WatchViewState.models_selected: usize`
  field round-tripped across the render/dispatch transient `AppState`
  reconstruction; no reload-safety check needed (model_metrics is
  session-level rollup; reload simply re-derives it from fresh events,
  and the selected row stays valid since the model list rarely
  shrinks). Cross-session aggregate mode (`watch aggregate ...`) does
  NOT support Models view (session-level data; cross-session
  aggregation of token metrics is a separate design). Spec §2.6.

- **D-14** Empty-state at session-not-shutdown: `model_metrics ==
  None` is the trigger. In watch mode, this transitions to `Some(...)`
  after the agent exits and emits `session.shutdown` (next reload
  picks it up automatically). Spec §2.3, §2.6.

- **D-15** Default-zero field semantics: every `ModelUsage` field
  defaults to `0` (not `Option<u64>`) when the wire's `usage.*` field
  is absent or wrong-typed. Trade-off: distinguishing "we know it's 0"
  from "we don't know" is lost in v1. Acceptable because (a) Copilot
  CLI always reports these fields when `usage` is present; (b) the
  parser-vs-data ambiguity is rare; (c) `Option<u64>` cardinality
  would explode rendering / aggregation logic 16-fold. Spec §3.1, §8.

- **D-16** No new dependencies: implementation uses existing
  workspace `serde_json` + `chrono` + `ratatui` + `crossterm` + `insta`
  + `tracing`. No `Cargo.toml` `[dependencies]` additions. Spec §1.

## Consequences

### Positive

- **POS-001** Users get the wire's full token picture (input / output /
  cache_read / cache_write) per model, sourced from authoritative
  Copilot CLI accounting (`session.shutdown.modelMetrics`), not
  estimation. Highest-fidelity data.
- **POS-002** `Event::payload_model_metrics` provides a forward path
  for other adapters (Claude / Codex) to opt into rich token
  observability — symmetric with the 6 existing `payload_*` /
  `tool_call_id` extension methods.
- **POS-003** `AnalysisReport.model_metrics` flows automatically into
  every existing exporter (JSON / HTML / Markdown / CSV) via serde
  — no per-exporter work needed in F1.7.
- **POS-004** Dedicated `View::Models` separates concerns cleanly:
  per-turn flamegraph, per-tool RoiView, per-bucket Aggregate,
  per-model Models. Each view answers one question well.
- **POS-005** Non-breaking change at every public API boundary
  (trait default impl, `#[non_exhaustive]` struct fields, new view
  module, new key binding on previously-unbound `4`).
- **POS-006** Free-form `serde_json::Value` parsing in
  `payload_model_metrics` is robust against future Copilot wire
  schema changes — new fields under `usage` don't break parsing;
  field renames just produce `0` instead of failing the event.

### Negative

- **NEG-001** Session-level granularity is coarser than per-turn (which
  is what F1.6 set the expectation for). Some users may want per-turn
  input tokens and will be surprised this isn't available. Mitigation:
  spec §1 explicitly documents the wire constraint; empty-state UX
  text explains the source.
- **NEG-002** `ModelUsage.{input,output,cache_read,cache_write}_tokens`
  all default to `0` when absent (D-15) — loses the
  known-0-vs-not-reported distinction. Mitigation: in practice
  Copilot always reports all 4 fields when `usage` is present.
- **NEG-003** Models view adds ~250 LOC of new TUI surface to
  maintain (state, render, dispatch). Same magnitude as F1 D-5's
  TurnDetailView. Justified by user value (token observability is
  high-leverage for cost-conscious users).
- **NEG-004** `Episodes.model_metrics` intermediate field is a new
  derive_episodes output (alongside `turns`, `tools`, `hooks`,
  `skills`, `mode_segments`, `warnings`). Minor cognitive load
  increase for `Episodes` consumers.
- **NEG-005** Cross-session aggregate mode (`watch aggregate`) does
  NOT support Models view; users expecting "across all my sessions,
  what's my total token spend by model?" must wait for a follow-up.
  Documented in §2.6 Out of Scope.

## Alternatives Considered

### ALT-A: Synthetic per-turn input/cache estimation (Phase C)

- **ALT-A-1 Description**: Estimate per-turn input tokens by diffing
  `session.compaction_start.conversationTokens` across compaction
  boundaries, divide by number of turns since last compaction.
- **ALT-A-2 Rejection reason**: (a) `conversationTokens` only emits
  at compaction events (typically every ~700k tokens); for sessions
  without compaction (most short sessions), no diff is available;
  (b) the diff includes user input, system prompts, tool definitions,
  and cache effects — attributing it to "this turn's input" is
  uneditable estimation; (c) users can't audit estimated values
  against any source. Phase C deferred until user demand surfaces;
  if implemented, would land as a CLEARLY-LABELED "(est)" column,
  not as a peer to wire-sourced data.

### ALT-B': `Adapter::parse_model_metrics(&self, session)` trait method

- **ALT-B'-1 Description**: Add `fn parse_model_metrics(events:
  &[Event]) -> BTreeMap<String, ModelUsage>` on the `Adapter` trait,
  called once by `derive_episodes` via the adapter dependency.
- **ALT-B'-2 Rejection reason**: Breaks the L1 "core operates on the
  `Event` trait contract only" rule established by ADR-0005.
  `derive_episodes` would need an `Adapter` reference, requiring a
  signature change. Awkward indirection for what's naturally a
  per-event extension method.

### ALT-C: Banner injection into FlamegraphView header

- **ALT-C-1 Description**: Add a top banner line in FlamegraphView
  showing `Session: claude-opus-4.7-1m  in:98k  out:47k  cache:3.4M`
  when `model_metrics` is `Some`.
- **ALT-C-2 Rejection reason**: (a) FlamegraphView is already
  width-constrained (F1.6 shrunk gantt area by 6 chars to add tokens
  column); a top banner further reduces gantt vertical space; (b) the
  banner is sticky context that doesn't change with turn navigation,
  but FlamegraphView is "what's happening in this turn" — mixing
  session-level and per-turn data in one view confuses scope. Distinct
  view is the right separation.

### ALT-D: TurnDetailView footer with session-level totals

- **ALT-D-1 Description**: Add a footer row to TurnDetailView showing
  the session-level totals for the model that this turn used.
- **ALT-D-2 Rejection reason**: Confuses scope ("is this turn's
  contribution or the whole session?"); requires per-model lookup
  logic in the per-turn renderer; doesn't show ALL models if the
  session used multiple. Distinct view answers the user's question
  directly without context-switching.

### ALT-E: `SessionTotals` top-level struct distinct from AnalysisReport

- **ALT-E-1 Description**: New top-level type carried alongside
  `AnalysisReport`, e.g. `Cli::analyze() -> (AnalysisReport,
  SessionTotals)`.
- **ALT-E-2 Rejection reason**: Doubles the consumer surface for one
  feature; CLI / TUI / exporters all need to thread the new type;
  serialization story is harder (two top-level documents per session);
  no clean precedent in the codebase. `AnalysisReport.model_metrics`
  is the natural home.

### Q3 alternatives (empty-state UX — see spec §7 Q3)

- (a) Empty table + footer hint: footer hint easy to miss; user could
  reasonably believe the binary is broken or the view is unused.
- (b) Silent fallback to prev view: key feels broken — pressing 4 does
  nothing visible. Worst UX.
- **(c) Centered placeholder + explanation**: ✅ chosen — matches
  TurnDetailView convention, explains *why* (no shutdown event yet),
  shows users it's intentional empty-state not a bug.

## Implementation Notes

- **IMP-001** Implementation order (executed in this sequence by the
  Stage 3 plan):
  1. Core: define `ModelUsage` pub struct in `analyzer/mod.rs` (4
     fields + `new` + `total`).
  2. Core: add `Episodes.model_metrics` field.
  3. Core: add `Event::payload_model_metrics` trait method (default `None`).
  4. Adapter: implement `CopilotEvent::payload_model_metrics` for
     `Shutdown` variant — walk existing `serde_json::Value` tree.
  5. Core: extend `derive_episodes` to populate
     `Episodes.model_metrics` from `Event::payload_model_metrics`.
  6. Core: add `AnalysisReport.model_metrics` field; `analyze()`
     clones from `episodes.model_metrics`.
  7. TUI: new `views/models.rs` with state struct + render +
     two branches (with-data / empty-state).
  8. TUI: `View::Models` enum variant + `AppState.models_selected`
     field + key `4` dispatch + j/k/G/gg navigation.
  9. TUI: render fork in `AppRunner::draw_frame`.
  10. TUI: WatchRunner round-trip + transient AppState.
  11. TUI: help overlay legend updates.
  12. Docs: rustdoc, CHANGELOG entries, L2 READMEs,
      `docs/adapters.md` payload_model_metrics note.
  13. New fixture: `with-session-shutdown` carrying 2-model
      modelMetrics. Snapshot tests.

- **IMP-002** Per-step gates (verification): `cargo fmt --all
  --check` + `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` + `cargo test --workspace --all-features` +
  `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace`
  after each step's commit. Stop and fix before the next step.

- **IMP-003** Snapshot strategy: ≥2 new snapshots in
  `crates/agentprof-tui/tests/views.rs` (one for with-data, one for
  empty-state). The with-data snapshot needs a fixture that DOES
  emit `session.shutdown` (the new `with-session-shutdown` fixture);
  the empty-state snapshot can reuse any existing fixture (most
  current fixtures don't emit shutdown). Use the color-blind
  `buffer_to_symbol_grid` helper.

- **IMP-004** Anti-pattern guard: **do NOT** parse
  `session.compaction_*` events into Models view in F1.7 (those are
  Phase C territory and would prematurely commit to a per-compaction
  vs per-session schema).

- **IMP-005** Anti-pattern guard: **do NOT** add typed
  `ModelMetricsEntry` / `ModelUsageRaw` structs in
  `agentprof-adapters` (D-7) — keep parsing as free-form Value walk
  so wire format changes don't break.

- **IMP-006** Branch model: direct-to-`main` per M1.6.4 follow-up
  wave convention. Each of the 13 IMP-001 steps lands as one commit
  with conventional-commit subject + Co-authored-by trailer. No
  long-lived feature branch.

## References

- **REF-001** Spec: [`docs/superpowers/specs/2026-06-03-f1.7-models-view-design.md`](../superpowers/specs/2026-06-03-f1.7-models-view-design.md)
- **REF-002** ADR-0004 (episode derivation) — defines the
  `derive_episodes` pure-aggregator algorithm this ADR extends.
  [`docs/internals/adr-0004-episode-derivation.md`](./adr-0004-episode-derivation.md)
- **REF-003** ADR-0005 (analyzer + `payload_name`) — defines the
  `Event::payload_*` extension-method pattern this ADR follows.
  [`docs/internals/adr-0005-analyzer-and-payload-name.md`](./adr-0005-analyzer-and-payload-name.md)
- **REF-004** ADR-0011 (TurnDetailView + args plumbing) — its D-5
  (`ToolCall.arguments` Option field) is the precedent for D-3's
  `Episodes.model_metrics: Option<...>` pattern. Its D-7's
  empty-state placeholder convention is the precedent for D-12.
  [`docs/internals/adr-0011-turn-detail-and-args-plumbing.md`](./adr-0011-turn-detail-and-args-plumbing.md)
- **REF-005** L1 architecture rule: `agentprof-core` is the
  dependency graph leaf (cited in ALT-B').
  [`docs/architecture.md`](../architecture.md) §3
- **REF-006** Project ADR conventions (path / format / numbering)
  per `.github/copilot-instructions.md` §4.1 / §5.5.
- **REF-007** Empirical wire survey: 2026-06-03 inspection of
  `/home/verden/.copilot/session-state/252068e5-…/events.jsonl`
  recorded in spec §1 table. The
  `session.shutdown.modelMetrics[model].usage.{inputTokens,
  outputTokens, cacheReadTokens, cacheWriteTokens}` fields are
  confirmed present in Copilot CLI 1.0.x output.
