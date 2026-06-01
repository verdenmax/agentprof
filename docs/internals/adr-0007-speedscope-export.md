# ADR-0007 — Speedscope evented format + frame naming + overlap adjustment

> **Status:** Accepted
> **Date:** 2026-05-31
> **Deciders:** agentprof team
> **Supersedes:** —
> **Superseded by:** —
> **Spec:** [`2026-05-31-m1.6.4-speedscope-and-html-export-design.md`](../superpowers/specs/2026-05-31-m1.6.4-speedscope-and-html-export-design.md)
> **Plan:** [`2026-05-31-m1.6.4-speedscope-and-html-export.md`](../superpowers/plans/2026-05-31-m1.6.4-speedscope-and-html-export.md)

## Context

M1.6.4 introduces a `--export speedscope` flag that emits a JSON file
consumable by the [speedscope.app](https://speedscope.app) interactive
flamegraph viewer. The Speedscope file-format spec at
<https://github.com/jlfwong/speedscope/blob/main/file-format.md> describes
three encodings (evented, sampled, speedscope-native) and requires that
events be **strictly nested** (every Open has a matching Close that pops
the same frame). agentprof's `Episodes` model does NOT enforce strict
nesting on tool/hook/skill calls — adapters may report overlap (e.g. clock
skew, sub-shells), and the derive layer accepts the data as reported.

This ADR documents the decisions that bridge those two worlds.

## Decision

### D-1 (M1.6.4): Use the "evented" format

`profiles[*].type = "evented"`. Reasons:
1. Maps 1-to-1 with agentprof's own open/close event model — no
   re-sampling or aggregation needed.
2. Preserves exact timing for non-overlapping spans.
3. Hierarchical (sampled format would lose the call-graph structure).

Sampled format is the natural future home for tokenizer-based weights
(Phase 2+).

### D-9: `unit = "milliseconds"`, integer `at`

Speedscope accepts any numeric unit but renders integers more cleanly. The
event JSONL stream has RFC3339 timestamps with millisecond granularity,
so finer precision would be false confidence.

### D-10: Timestamp anchor = session.started_at → at:0

All event `at` values are deltas from `session.started_at` rather than
absolute Unix timestamps. Reasons:
1. Snapshot tests are reproducible across runs (no wall-clock drift).
2. Speedscope's "left-heavy" view still works (it uses durations, not
   absolute times).
3. Files are smaller (4-5 digit integers vs 13-digit Unix ms).

### D-11: Frame naming with source prefix

| Source | Frame name |
|---|---|
| Builtin tool (bash, view, etc.) | `<name>` |
| MCP tool | `mcp:<server>::<leaf>` |
| Hook | `hook:<name>` |
| Skill invocation (instant) | `skill:<skill>` (single frame per skill — see implementation note) |
| Tool whose ToolSource is Skill | `skill:<skill>:<leaf>` (per-tool leaf, since each is a real Span with duration) |
| Synthetic | `session`, `turn-<N>`, `turn-<N> (open)`, `turn-orphan` |

Prefixes prevent collision (e.g. a builtin `read` vs an MCP `mcp:fs::read`)
and let the viewer's left-heavy / top-down views group by source.

**Implementation note for skill invocations**: The original spec D-11
phrasing was `skill:<skill>:<tool>`, based on an assumption that each
`SkillInvocation` carried a triggering tool name + span. The actual
`SkillInvocation` shape (in `crates/agentprof-core/src/episode/skill.rs`)
is `{ at: DateTime<Utc>, turn_id, triggered_tools: Vec<CallRef> }` — an
instant in time, with triggered tools recorded as separate top-level
tool frames. The correct dedup-respecting form is therefore `skill:<skill>`
(no per-invocation leaf); multiple invocations of the same skill collapse
to one frame, mirroring the `bash` / `view` dedup behavior for tools.

### D-12: Global frame dedup by name

`shared.frames` contains each unique name exactly once; events reference
frames by index. Required for speedscope's left-heavy view to merge calls
to the same tool across turns. Naming order: `session`, `turn-1..N`,
`turn-orphan` (if any), then BTreeMap-sorted leaf names.

### D-13: Open turn → `(open)` suffix + synthetic close at last event time

If `turn.ended_at == None`, the frame is named `turn-<N> (open)` and we
emit a synthetic `Close` at the timestamp of the last event in the
session. This keeps the profile strictly nested while giving users a
visible signal that the turn never completed.

> **Known limitation**: If an open turn precedes a closed turn in the
> same session (a data anomaly, not a healthy-session case), the
> synthetic close uses `session_last_event_time`, which can violate
> at-monotonicity against the subsequent closed-turn open. See follow-up
> tracker `m1.6.4-followup-i1` for the hardening fix (clamp synthetic
> close to `min(total_ms, turns[N+1].started_at)`).

### D-14: Orphan tool calls → synthetic `turn-orphan` frame

Tool calls with `turn_id == None` (orphans per ADR-0004) are grouped under
a synthetic `turn-orphan` frame appended after all real turns. The frame
spans from the first orphan's start to the last orphan's end.

> **Known limitation**: If an orphan's `started_at` precedes the last
> real turn's `ended_at`, the resulting event sequence has descending
> `at` values across the boundary. See follow-up `m1.6.4-followup-i2`.

### D-15: Span overlap → auto-adjust + ExportWarning

If two child spans within the same turn overlap (later.started_at <
earlier.ended_at), the later span's effective start is shifted to
`earlier.ended_at + 1ms` (and if its original_end <= adjusted_start, end
is shifted to `adjusted_start + 1ms`). An
`ExportWarning::SpanAdjustedForSpeedscope` is pushed for each adjustment;
the cli prints these to stderr.

Rejected alternatives:
- Fail-loud (refuse to export) — too disruptive for what's usually a clock
  skew artifact.
- Silently re-order — loses signal; users wouldn't know their timing was
  approximated.

### D-16: New `ExportWarning` type, NOT a `DeriveWarning` variant

`DeriveWarning` is the derive layer's observation of input data quality.
Span-overlap adjustment is a downstream side-effect of the export pipeline
(the derive layer accepted the overlap as valid). Adding a
`DeriveWarning` variant would have polluted derive's snapshot tests
without semantic justification.

`ExportWarning` lives in `agentprof-core::export` with its own module;
future formats (CSV, HTML-specific) get their own variants.

## Consequences

- Speedscope output is **strictly nested** within each turn — the viewer
  renders correctly for healthy sessions.
- Overlapping spans lose timing fidelity (≤ 1 ms shift), but the data is
  preserved and the user is notified.
- Frame naming convention is **load-bearing** across exporters: the same
  prefix scheme is used by `svg_flamegraph` for color coding (D-5 in the
  M1.6.4 spec — see "Locked decisions" section).
- Future tokenizer (Phase 2) will likely emit a second `Profile` with
  `unit = "tokens"`, demonstrating the multi-profile capability of the
  format.
- Edge-case robustness (mid-session open turns, orphan time collision,
  negative-duration data) is currently a known gap — tracked as
  follow-ups m1.6.4-followup-i1/i2/i3 for post-merge hardening.

## Implementation

- Module: `agentprof-core::export::speedscope` (lib, pure transformation,
  ~600 LOC across `to_speedscope` + 4 helper functions)
- Module: `agentprof-core::export::warning` (`ExportWarning` enum,
  `#[non_exhaustive]`)
- Module: `agentprof-core::export::svg_flamegraph` (responsive SVG
  flamegraph for HTML embedding; ToolSource-color-coded, ~450 LOC)
- CLI wrapper: `agentprof-cli::cmd::format::speedscope` (thin wrapper
  → JSON pretty-print + stderr warnings)
- CLI wrapper: `agentprof-cli::cmd::format::html` (askama 0.16 template
  + embedded CSS + SVG)
- Templates: `crates/agentprof-cli/templates/{report.html,styles.css}`
  (askama 0.16 default location)
- Tests: 8 in `crates/agentprof-adapters/tests/export_on_fixtures.rs`
  (6 speedscope-data + 2 SVG); 19 in `crates/agentprof-cli/tests/cli.rs`
  (including 2 new snapshots `cli__analyze_speedscope__with_skill_invoked.snap`
  and `cli__analyze_html__with_skill_invoked.snap` — both with version
  + generated_at normalized for stability)
- Smoke: upload generated JSON to https://speedscope.app
