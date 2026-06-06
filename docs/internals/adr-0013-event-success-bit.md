# ADR-0013: Event trait `payload_success` + `payload_error_message` (two narrow methods)

**Status**: Accepted
**Date**: 2026-06-06
**Milestone**: M1.6.4 follow-up wave Phase 4 (B1 — wire success bit closes the
silent-correctness gap behind F1.13 / F1.16 / F2.3)
**Spec**: [`docs/superpowers/specs/2026-06-06-b1-failure-bit-design.md`](../superpowers/specs/2026-06-06-b1-failure-bit-design.md)
**Plan**: [`docs/superpowers/plans/2026-06-06-b1-failure-bit.md`](../superpowers/plans/2026-06-06-b1-failure-bit.md) *(written next, after this ADR)*

## Context

`agentprof-core::episode::derive_episodes` has been hardcoding
`ToolCallStatus::Success` (`derive.rs:383`) and `HookCall.success: true`
(`derive.rs:490` + `:504`) since M1.2 (commit `c5716aa`), with a TODO
comment ("Task 10b will read actual success bit") that was never closed.

The Copilot wire payload (per ADR-0002) carries the data:

| Wire event | Field path |
|---|---|
| `tool.execution_complete` | `ToolResultData.success: bool` |
| `tool.execution_complete` | `ToolResultData.error: Option<ToolError { message: String }>` |
| `hook.end` | `HookEndData.success: bool` |

The bug silently neutralizes three already-shipped UX features that depend
on `failure_count > 0`: F1.13 RoiView Red/Yellow Tool cell, F1.16 By Hook
OK% + color, and F2.3 `compose_tool_cell_style` failure-wins-over-pending
precedence. All three look fine in unit tests (which feed synthetic
`Failure` variants) but lie to users on real Copilot data.

This ADR records the architectural choice for **how** the new wire bits
get exposed across the adapter / core seam — i.e. the shape of the
`Event`-trait extension. The behavior of consumers (which call paths read
them, how `None` is defaulted) is captured in the spec; this ADR is
strictly about the trait-API shape because that's the new public API
surface that adapter authors will see.

## Decisions

(Each row maps 1:1 to a design choice in §4 of the spec. Re-opening a
decision requires editing this ADR or recording a new ADR that explicitly
supersedes the affected D-row.)

- **D-1** Trait extension over enum extension: **add 2 new methods to
  `pub trait Event`** (default-`None` impls), NOT add a new variant to
  `pub enum EventKind`. Rationale: `EventKind` is the coarse
  pattern-matching seam (`ToolExecComplete` already exists as a variant);
  the per-payload-field data lives behind `payload_*` accessors. The
  existing 5-method `payload_*` family (`payload_name`, `payload_model`,
  `payload_output_tokens`, `payload_mode`, `payload_tool_requests`) plus
  `tool_call_id` is the established pattern — this extension follows it
  rather than inventing a parallel structure. Spec §4.

- **D-2** Method shape: **two narrow methods —
  `payload_success() -> Option<bool>` + `payload_error_message() -> Option<&str>`**
  — rather than one bundled `payload_outcome() -> Option<EventOutcome>`
  where `EventOutcome { success: bool, error_message: Option<String> }`.
  Alternative considered:

  ```rust
  // REJECTED — bundled struct
  pub struct EventOutcome<'a> {
      pub success: bool,
      pub error_message: Option<&'a str>,
  }
  fn payload_outcome(&self) -> Option<EventOutcome<'_>> { None }
  ```

  Rejected because:
  (a) It introduces a new public type (`EventOutcome`) that exists solely
  to carry 2 fields — pure overhead vs naming each method;
  (b) Adapter authors implementing the trait would need to construct
  `EventOutcome` literals instead of returning bare `Option<bool>` /
  `Option<&str>` — more ceremony, less ergonomic;
  (c) Consumers in `derive_episodes` would need to destructure
  `EventOutcome` then match anyway — no savings vs calling 2 methods;
  (d) Future adapter additions to this signal family (e.g. hypothetical
  `payload_error_kind() -> Option<ErrorKind>`) extend cleanly as more
  narrow methods — the bundled struct would either need versioning
  (`EventOutcomeV2`) or `#[non_exhaustive]` shenanigans (which still
  break construction-by-literal for adapter authors);
  (e) Symmetry with the existing 5-method `payload_*` family — adding
  2 more siblings keeps the mental model uniform.

  The bundled-struct alternative was the only serious alternative
  considered; lumping by event semantics (e.g. `payload_tool_complete()
  -> Option<ToolCompleteInfo>`) is even worse — it ties the shape to
  one event variant instead of the abstract "this event has a
  pass/fail bit" concept (`HookEnd` also has the success bit but no
  error message). Spec §4.

- **D-3** Default `None` impls: **both methods default to `None`**, not
  to `Some(true)` or `Some("")`. Rationale: `None` is the universal
  "this event has no concept of success/error" signal. Three downstream
  semantics share the same default:
    1. Adapter author hasn't overridden yet (future adapter under
       development) → `None` → consumers fall back to existing behavior.
    2. Event variant doesn't carry the concept (e.g. `SessionStart`) →
       `None` → consumers correctly skip it.
    3. Wire payload is malformed or older Copilot CLI 1.0.x without
       the field → `None` → consumers fall back to existing behavior.

  Consumers (`derive.rs`) interpret `None` as Success (matches the
  pre-fix hardcoded behavior — see ADR Spec Q4 / §6). Spec §4.

- **D-4** Error message return type: **`&str` (borrowed)** not
  `String` (owned). Rationale: matches the existing `payload_name() ->
  Option<&str>` + `tool_call_id() -> Option<&str>` pattern. Consumers
  in `derive.rs` who need ownership call `.map(str::to_owned)`
  explicitly at the use site (same pattern as `payload_name` →
  `String` conversion in `resolve_payload_name`). Avoids
  unnecessary allocations for non-failure events. Spec §4.

- **D-5** No `#[must_use]` annotation on the methods: matches existing
  `payload_*` methods which return `Option<T>` and similarly lack
  `#[must_use]`. The `Option` return type itself signals "may be
  absent"; callers that ignore the return are obviously buggy and
  caught by `clippy::unused_results` rather than `must_use`. Spec §4.

- **D-6** Hook events get **`payload_success` only, NOT
  `payload_error_message`**: the Copilot `hook.end` wire payload
  (`HookEndData`, ADR-0002 line 93) carries `success: bool` but has no
  `error` field. `payload_error_message()` returns `None` for `HookEnd`
  (the default impl), not a `Some("")` placeholder. Future Copilot CLI
  versions that add a hook error field can extend the override without
  any API change. Spec §5.

- **D-7** Backward compatibility for adapter authors: **zero breakage**.
  Both methods have default `None` impls, so every existing `impl Event
  for ...` block compiles unchanged. Adapters that don't override get
  the existing always-Success behavior in `derive.rs` (via the `None`
  default-to-Success branch in `on_tool_complete` / `on_hook_end`).
  This matches the trait's growth pattern established by D-3 in
  ADR-0012 (`payload_model_metrics` was added the same way). Spec §4.

- **D-8** Backward compatibility for downstream consumers (analyzers
  / reports / TUI views): **the public output shape is unchanged**.
  `ToolEpisode.failure_count: u32` and `HookEpisode.failure_count: u32`
  already exist (they were just always 0); their integer values now
  reflect real wire data, which is the entire point of the fix. No
  consumer needs to know the change happened — F1.13's
  `failure_severity_color(call_count, failure_count)` etc. start firing
  Red/Yellow on real data automatically. Spec §1.3.

## Consequences

**Positive:**

- F1.13 / F1.16 / F2.3 finally work on real data (the bug-fix payoff).
- Adapter authors get a clean, symmetric extension point matching
  existing patterns; uniform mental model across the 7-method
  `payload_*` family.
- `ToolCallStatus::Failure { message }` is now populated end-to-end,
  unlocking future UX work (RoiView detail hover, TurnDetail error
  display) with no further parser-layer touches.

**Negative / risks:**

- **Snapshot regen blast radius**: 4 fixtures (`with-mcp-calls`,
  `with-aborts`, `with-hooks-heavy`, `multi-sess-c`) flip from
  `failure_count: 0` to `failure_count: N > 0` end-to-end. Snapshot
  diffs across `analyzer_on_fixtures`, `aggregate_on_fixtures`,
  `export_on_fixtures`, and TUI view snapshots will need inspection
  + acceptance. Mitigation: spec §7.4 codifies the inspection
  protocol (only-failure_count-and-derived-fields diffs accepted;
  any other diff halts).

- **Adapter parsing fidelity**: `payload_error_message` walks
  `ToolError.message` directly. If a future Copilot version changes
  the error schema, the override needs an update — but this is
  contained to one method body, not a sprawling change.

- **Forward-compat note for new adapters**: when adding a new
  `impl Event for FutureEvent`, authors should remember to override
  `payload_success` for any failure-bearing event variants, or the
  always-Success default leaks through silently. Caught by the
  per-adapter unit tests we mandate in the new-adapter recipe
  (`AGENTS.md` §9.1, item 4).

**Alternatives rejected:** see D-2 (bundled `EventOutcome` struct) +
implicit alternative in D-1 (new `EventKind` variants for
"success/failure" — incoherent with the existing kind/payload split).

## References

- ADR-0002 — Copilot event schema (wire fields)
- ADR-0004 — Episode derivation (failure_count field origin)
- ADR-0005 — Analyzer + `payload_name` (the seam this ADR extends)
- ADR-0012 — Session model metrics (most recent `payload_*` extension,
  templated this one)
- F1.13 — RoiView Tool cell failure-severity color
- F1.16 — By Hook OK% + color
- F2.3 — `compose_tool_cell_style` failure-wins-over-pending
- Spec — [`docs/superpowers/specs/2026-06-06-b1-failure-bit-design.md`](../superpowers/specs/2026-06-06-b1-failure-bit-design.md)
- Backlog: `m1.6.2-followup-copilot-failure-bit` (UPSTREAM tag)
