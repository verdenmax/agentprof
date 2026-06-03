# ADR-0011: Tool arguments plumbing and TurnDetailView state model

**Status**: Accepted
**Date**: 2026-06-03
**Milestone**: M1.6.4 follow-up wave (Phase 2 — UX feature with data layer)
**Spec**: [`docs/superpowers/specs/2026-06-03-turn-detail-view-design.md`](../superpowers/specs/2026-06-03-turn-detail-view-design.md)
**Plan**: [`docs/superpowers/plans/2026-06-03-f1-turn-detail-view.md`](../superpowers/plans/2026-06-03-f1-turn-detail-view.md) *(written next, after this ADR)*

## Context

The 2026-06-03 FlamegraphView UX wave (`b5c1429` → `13a4dbb`) shipped a
footer line showing the currently selected turn's first few tool calls:

```
T3 selected: bash(120ms) read_file(85ms) +K more
```

Real-session feedback against a 57-turn `~/.copilot/session-state/...`
profile exposed two adjacent gaps:

- **"+K more" hides the rest.** Any turn with >3 calls or long-named
  tools (`mcp:postgres::execute_query`) overflows and the user cannot
  see what those K calls are.
- **`bash(120ms)` answers when but not what.** The natural follow-up
  — "what command did bash run?" — is invisible.

The first gap is purely a UI affordance (a drill-down view). The second
gap is a **data plumbing** problem: `agentprof-core::episode::ToolCall`
currently stores only `{ span, status, turn_id, user_requested }`. The
adapter (`crates/agentprof-adapters/src/copilot/event.rs:364`) already
parses `ToolRequest.arguments: serde_json::Value`, but
`derive_episodes` discards the data when collapsing raw events into
`Episodes`.

Spec §3 prescribes fixing both in one PR. This ADR records the
architectural decisions that shape that fix — the trait extension
point, the field on `ToolCall`, the report-level data exposure, the
TUI state model, and the recursive "Enter = drill deeper" UX rule
that future detail-of-detail views will inherit.

## Decisions

(Each D-row maps to a spec section. Re-opening a decision requires
editing this ADR or recording a new ADR that explicitly supersedes
the affected D-row.)

- **D-1** Plumbing strategy: **add a method to the existing `Event`
  trait** rather than (a) adding a method to `Adapter`, (b) augmenting
  `RawSession`, (c) passing `RawSession` into the TUI, or (d) shipping
  the UI without data. Symmetrical with the four existing extension
  methods (`payload_name` / `payload_model` / `payload_output_tokens`
  / `payload_mode`); keeps `derive_episodes` operating on the trait
  contract only; keeps `agentprof-core` as the dependency graph leaf.
  Spec §3.2, §8.

- **D-2** Method signature: **`fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)>`**
  returning `(tool_call_id, arguments)` pairs, default empty `Vec`.
  Plural form because the Copilot `AssistantMessage` event carries
  multiple `tool_requests` in one payload; a singular
  `Option<(String, Value)>` would lose data. The `String` (owned)
  rather than `&str` keeps the trait dyn-friendly and avoids
  borrow-checker entanglement in the derive PASS 0 map collection.
  Spec §3.2.

- **D-3** Derive walk: **two passes** (collect args by call_id, then
  drive state machine), staying `O(N_events × max_requests_per_event)`
  total — same big-O class as before. PASS 0 is a tiny linear scan
  that materializes a `BTreeMap<String, Value>`; PASS 1 is the
  existing state-machine walk, modified to consult the map on
  `on_tool_complete` to stamp the new `ToolCall.arguments` field.
  Rejected the alternative of single-pass-with-lookahead because
  events for a given `tool_call_id` can appear in either order
  (`tool.user_requested` before `tool.execution_complete`,
  `assistant.message` before either) and a forward-buffer would
  complicate the state machine for no measurable runtime saving.
  Spec §3.2.

- **D-4** Conflict resolution: **`BTreeMap.entry().or_insert()`** — first
  occurrence wins on duplicate `tool_call_id`. Defends against
  degenerate replay / retry scenarios that the Copilot wire format
  technically allows. Logged at `tracing::debug!` level if the map
  insert is skipped (cheap detection path; not yet implemented in v1
  but planned in plan §). Spec §3.2.

- **D-5** New field type: **`ToolCall.arguments: Option<serde_json::Value>`**
  (rather than `Option<String>`, a typed enum per tool, or a separate
  side-table). `Value` preserves the JSON shape for both the TUI
  (pretty-print on expand) and JSON export (passthrough); `Option`
  conveys "not captured" (vs. "captured-as-empty") without a magic
  sentinel; orphan completes naturally land as `None`. The struct is
  already `#[non_exhaustive]`, so the field add is non-breaking. Spec
  §3.2, §6.

- **D-6** Episodes exposure in report: **add `pub episodes: Episodes`
  to `AnalysisReport`** by full clone. Rejected `Arc<Episodes>` for
  v1 (extra indirection; current sessions <50 KB; switch tracked as
  follow-up in spec §12 if a future M2 session hits MB scale).
  Rejected "thread `Episodes` as a separate `AppRunner::new(report,
  episodes)` parameter" because (a) it's the natural data dependency,
  (b) JSON / HTML exporters may want it too, (c) `AnalysisReport` is
  already `#[non_exhaustive]`. Spec §3.4.

- **D-7** TUI UI form: **full-screen `TurnDetailView`**, NOT a modal
  overlay and NOT a split panel. Modal overflows on 4+ tools / long
  args; split compresses both halves; full-screen reuses the `j/k`
  navigation muscle memory from FlamegraphView. Spec §2.1 / Q1.

- **D-8** Detail field set: **MVP + args preview** — tool name, duration,
  status, source color, `▶` selection marker, plus a single-line `args`
  preview truncated at 80 characters. Rejected "tool list only"
  (debug insight too shallow) and "tool list + args + result preview"
  (PII risk — result content can include user paths, secrets, file
  contents — requires a `--show-results` flag, scope creep). Spec
  §2.1 / Q2.

- **D-9** Detail-internal Enter semantics: **toggle expand/collapse of
  the selected tool_call's args full text** (word-wrapped, pretty-printed
  JSON). Rejected "no-op" (wastes the obvious follow-through key) and
  "expand args + result" (re-introduces PII). Establishes the recursive
  pattern *Enter = drill one level deeper* — FlamegraphView Enter goes
  to TurnDetailView; TurnDetailView Enter expands a call's args; a
  hypothetical future result viewer would extend the chain. Spec §2.1 /
  Q3.

- **D-10** Truncation rendering: **80 chars + `…`** for the
  single-line `args` preview; **`(not captured)` in dim gray** when
  `ToolCall.arguments == None` (orphan complete, adapter without
  `payload_tool_requests` impl, missing event). Communicates absence
  without alarming the user — distinguishes "tool was called but we
  don't have the args" from "args were empty". Spec §2.1.

- **D-11** Wide-char (CJK / emoji) handling: **`chars().take()` only,
  not `unicode-width`** for v1. Off by ±1 cell on wide CJK / emoji
  tool names or args; documented as known limitation in rustdoc.
  Pulling `unicode-width` for one cell-perfect edge case is poor
  ROI given the dominant English/ASCII tool ecosystem. Reassess if
  user reports surface. Spec §9.

- **D-12** Args in other exports: **JSON yes (passthrough field),
  HTML / Markdown / CSV / Speedscope no.** JSON consumers benefit
  from raw args data; tabular formats would either bloat rows
  unreadably or require their own truncation logic; Speedscope frame
  names already convey tool identity and args would multiply file
  size dramatically. Defer all four to follow-ups if user demand
  materializes. Spec §2.2, §7.

- **D-13** Privacy posture: **no redaction in v1** — args data is
  passed through as-is from the adapter. Matches the existing posture
  on tool names and turn contents. Documented in
  `docs/features/privacy.md` §8 (new) so users understand the
  trade-off before opting into args display. `AGENTPROF_LOG_FULL_PATHS`
  env var explicitly does NOT apply (it gates *logging fields*, not
  payload data). Separate args-redaction feature reserved for future
  privacy RFC. Spec §2.3, §7.

- **D-14** Reload safety: **on every `WatchRunner` reload, drop
  `detail_view` if its `turn_id` no longer exists** in the reloaded
  `Episodes.turns`, set a red footer banner explaining why. The
  `expanded_tools` HashSet is cleared by the same code path. Avoids
  silently rendering stale data; consistent with M1.6.3 watch's
  red-banner footer pattern from ADR-0009 D-13. Spec §3.4.

- **D-15** Cross-view key behavior: **`1` / `2` / `3` in detail view
  pop the detail then switch top-level views** (not "swallowed inside
  detail"). Lets the user pivot quickly without an extra Esc keystroke
  while preserving the global "1/2/3 ALWAYS switch view" rule
  established by `M1.5 audit #1` (CHANGELOG entry on commit `5c89...`).
  `q` keeps global quit semantics (no per-view rebinding). Spec §2.1.

## Consequences

### Positive

- **POS-001** Detail view fully closes the M1.5 / M1.6.x "I can see
  totals but not call-level context" gap that ROI table aggregations
  cannot answer.
- **POS-002** `Event::payload_tool_requests` provides a forward path
  for other adapters (Claude / Codex) to opt into rich detail-view UX
  by implementing the one method — symmetric with the four existing
  `payload_*` extension methods.
- **POS-003** `AnalysisReport.episodes` unlocks future exporters
  (JSON consumers, custom reporters) that want raw episode access
  without re-deriving from raw events.
- **POS-004** Recursive "Enter = drill deeper" UX rule establishes a
  consistent navigation model that future detail-of-detail views (result
  viewer, frame viewer) inherit cleanly.
- **POS-005** Non-breaking change at every public API boundary
  (trait default impl, `#[non_exhaustive]` struct fields, new view
  module).

### Negative

- **NEG-001** `AnalysisReport.episodes` carries a memory cost — for
  typical 57-turn sessions ~10–50 KB; for hypothetical 5000-call
  sessions with 10 KB args each, could reach ~50 MB. Bounded but
  real; follow-up tracked for `Arc<Episodes>` if M2 sessions surface
  this.
- **NEG-002** New trait method on `Event` is technically a
  source-compat-only addition with a default impl. Other adapter
  authors who *want* args-aware detail view UX must implement it —
  silent "no args shown" otherwise. Mitigated by documenting in
  `docs/adapters.md` as a recommended-but-optional method.
- **NEG-003** JSON export schema grows a new optional `arguments`
  field per tool call. Schema-strict consumers (none known to exist
  in v0.x) would need to handle the addition. Documented as
  non-breaking in CHANGELOG.
- **NEG-004** Args data carries PII risk (user paths, queries,
  variable contents). No redaction in v1; users opting into the TUI
  detail view see raw data. Documented in `docs/features/privacy.md`
  §8 (new).
- **NEG-005** Detail view adds ~280 LOC of new TUI surface to
  maintain (state machine, render code, vim key dispatch, snapshot
  tests). Justified by the user value but increases the TUI footprint
  by ~15 %.

## Alternatives Considered

### ALT-A: Pure UI fix without args data (Q4 option A)

- **ALT-A-1 Description**: Ship `TurnDetailView` rendering only the
  currently-available `ToolCall` fields (name / duration / status /
  source / `user_requested` marker). No data layer changes.
- **ALT-A-2 Rejection reason**: Solves only the "+K more truncation"
  half of the user-reported problem. The "what did `bash` actually
  run?" follow-up — the more interesting one — is still invisible.
  User explicitly selected Option B over this in Q4.

### ALT-B': `Adapter` trait method instead of `Event` trait method

- **ALT-B'-1 Description**: Add `fn tool_arguments(events: &[Event])
  -> BTreeMap<String, Value>` on the `Adapter` trait, called once by
  derive_episodes via dependency injection.
- **ALT-B'-2 Rejection reason**: Breaks the L1 "core operates on the
  `Event` trait contract only" rule. `derive_episodes` would suddenly
  need an `Adapter` reference. Awkward signature change. The
  per-event extension-method pattern is already established and works
  cleanly.

### ALT-B'': Add `RawSession.tool_args_map` side field

- **ALT-B''-1 Description**: Adapter pre-builds the args map and
  attaches it to `RawSession` alongside `events`. `derive_episodes`
  reads it directly.
- **ALT-B''-2 Rejection reason**: Pollutes `RawSession` with a
  derived-data field that's redundant with the events themselves. The
  events already carry the data; we just need a clean extraction
  surface, not duplication.

### ALT-B''': Pass `RawSession` directly to TUI

- **ALT-B'''-1 Description**: `AppRunner::new(report, &raw_session)`;
  TUI walks raw events to extract args on demand.
- **ALT-B'''-2 Rejection reason**: TUI becomes adapter-aware (which
  event variant carries which payload field). Violates the L1
  layering. Repeats work on every render.

### ALT-C: Two-phase release (data plumbing first, UI later)

- **ALT-C-1 Description**: Ship Phase 1 = `payload_tool_requests` +
  `ToolCall.arguments` + `AnalysisReport.episodes` alone, then a
  separate Phase 2 = `TurnDetailView`.
- **ALT-C-2 Rejection reason**: Doubles the spec/ADR/plan cycle for
  one cohesive user-facing feature. The data layer addition has no
  user value without the consuming view; landing both at once keeps
  CHANGELOG focused on user-visible deliverables.

### Detail view UI form alternatives (Q1 — see spec §7)

- Modal overlay rejected: args + 4+ tool list overflow the typical
  small modal area; UX inconsistent across terminal sizes.
- Split panel rejected: compresses both flamegraph and detail; no
  Enter "drill" gesture (always-on, can't focus).

### Detail field set alternatives (Q2 — see spec §7)

- Tool list only rejected: debug insight too shallow to justify the
  Enter gesture.
- Tool list + args + result rejected: result PII risk requires a
  `--show-results` flag (scope creep) and an in-view redaction
  toggle (more state).

### Detail-internal Enter alternatives (Q3 — see spec §7)

- No-op rejected: wastes the obvious follow-through key.
- Expand args + result rejected: PII re-enters.

## Implementation Notes

- **IMP-001** Implementation order (executed in this sequence by the
  Stage 3 plan):
  1. Core: add `Event::payload_tool_requests` (default empty).
  2. Adapter: implement on `CopilotEvent` for `AssistantMessage` and
     `ToolUserRequested`. Add unit tests.
  3. Core: add `ToolCall.arguments` field + adjust `ToolCall::new`
     to default `None`. Roundtrip serde test.
  4. Core: add `AnalysisReport.episodes`. Update `analyze()` to clone
     episodes into the report. Existing tests pass.
  5. Core: PASS 0 args-map collection in `derive_episodes`; stamp
     `ToolCall.arguments` on `on_tool_complete`. Test against
     fixture replay.
  6. TUI: new `views/turn_detail.rs` with helpers + render + state.
  7. TUI: `AppState.detail_view` field + key dispatch order.
  8. TUI: render fork in `AppRunner::draw`.
  9. TUI: WatchRunner reload safety.
  10. TUI: help overlay extension.
  11. Docs: rustdoc, CHANGELOG entries, `docs/adapters.md`
      addition, `docs/features/privacy.md` §8.

- **IMP-002** Per-step gates (verification): `cargo fmt --all
  --check` + `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` + `cargo test --workspace --all-features` +
  `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace`
  after each step's commit. Stop and fix before the next step.

- **IMP-003** Snapshot strategy: 5 new snapshots (4 in
  `views/turn_detail.rs` tests + 1 in WatchRunner reload integration).
  Snapshots use the color-blind `buffer_to_symbol_grid` helper —
  colors are not asserted via snapshot, they're asserted via
  `Style` span equality in dedicated tests. 0 existing snapshots are
  expected to change (new view does not interact with FlamegraphView,
  RoiView, or AggregateView rendering paths).

- **IMP-004** Anti-pattern guard: **do NOT** add `tool_call_id` to
  `ToolCall` "just in case TUI needs it". Args lookup happens once at
  derive time; the TUI works from the resolved `arguments`. Adding
  another back-pointer multiplies fixture maintenance and snapshot
  surface for no use case.

- **IMP-005** Branch model: direct-to-`main` per M1.6.4 follow-up
  wave convention. Each of the 11 IMP-001 steps lands as one commit
  with conventional-commit subject + Co-authored-by trailer. No
  long-lived feature branch — small enough scope, fast CI.

## References

- **REF-001** Spec: [`docs/superpowers/specs/2026-06-03-turn-detail-view-design.md`](../superpowers/specs/2026-06-03-turn-detail-view-design.md)
- **REF-002** ADR-0004 (episode derivation) — defines the
  `derive_episodes` pure-aggregator algorithm this ADR extends.
  [`docs/internals/adr-0004-episode-derivation.md`](./adr-0004-episode-derivation.md)
- **REF-003** ADR-0005 (analyzer + `payload_name`) — defines the
  `Event::payload_*` extension-method pattern this ADR follows.
  [`docs/internals/adr-0005-analyzer-and-payload-name.md`](./adr-0005-analyzer-and-payload-name.md)
- **REF-004** ADR-0009 (watch runner) D-13 — defines the
  red-banner-footer reload-error UX pattern this ADR's D-14 inherits.
  [`docs/internals/adr-0009-watch-runner-and-notify.md`](./adr-0009-watch-runner-and-notify.md)
- **REF-005** ADR-0010 (tracing infrastructure) — establishes the
  `tracing::debug!` logging pattern used in this ADR's D-4 conflict
  detection.
  [`docs/internals/adr-0010-tracing-infrastructure.md`](./adr-0010-tracing-infrastructure.md)
- **REF-006** L1 architecture rule: `agentprof-core` is the
  dependency graph leaf (cited in D-1, ALT-B').
  [`docs/architecture.md`](../architecture.md) §3 / §4
- **REF-007** Project ADR conventions (path / format / numbering)
  per `.github/copilot-instructions.md` §4.1 / §5.5.
