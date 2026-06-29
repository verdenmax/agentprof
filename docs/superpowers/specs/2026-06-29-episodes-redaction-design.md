# Episodes Redaction + `list --privacy` (F-10)

| Field | Value |
|---|---|
| Date | 2026-06-29 |
| Status | Approved — entering writing-plans |
| Author | F-10 closure (L-1 follow-up) |
| Triggered by | F-10 (analyze html/speedscope flamegraph + `list` not yet redacted) |
| Builds on | L-1 (`docs/internals/adr-0026-report-redaction.md`) |
| Touches ADRs | candidate **ADR-0028** (RedactionContext refactor + Episodes redaction) |
| Target release | v0.4.0 (minor — opt-in, default `none` = zero behavior change) |

## 1. Problem statement

L-1 shipped `--privacy <none|redact|anonymize>` for `analyze` + `aggregate`,
but two surfaces were deliberately deferred (ADR-0026 Negative consequences):

1. **analyze html/speedscope flamegraph** frames are built from un-redacted
   `Episodes`, so they retain original **turn-ids** (html SVG `turn-{id}`,
   svg_flamegraph.rs:216) and **MCP server names** (`mcp:{server}::{tool}`).
   `analyze` warns and steers users to md/json for full redaction.
2. **`list`** has no `--privacy` flag — its per-session rows carry cwd /
   branch / model / session-id PII.

F-10 closes both: `Episodes::redact` makes html/speedscope fully redactable,
and `list --privacy` extends the flag to the last reporting surface. The crux
is that `analyze` renders the table from `AnalysisReport` and the flamegraph
from `Episodes`; the turn-id mapping must be **shared** so the two views
agree. That requires factoring L-1's redaction state out of
`AnalysisReport::redact` into a reusable context.

## 2. Scope

### In scope
- New `agentprof_core::analyzer::redact::RedactionContext { uuids, models,
  servers }` (the three accumulators L-1 builds internally), with
  `into_map() -> RedactionMap`.
- `AnalysisReport::redact_with(level, &mut ctx)` + `Episodes::redact_with(
  level, &mut ctx)`. Existing `redact(level) -> (Self, RedactionMap)` kept as
  a thin wrapper (build a one-shot ctx) — L-1 API + tests unchanged.
- `Episodes::redact_with` covers turns / tools / hooks / skills / aborts /
  warnings / model_metrics / loaded_mcp_tools, symmetric with L-1, keeping
  `Turn.{tool,hook,skill}_calls[].name` in sync with the rekeyed maps.
- `analyze` rewired: one ctx → redact report + episodes; html/speedscope
  render from the redacted episodes; drop `warn_unredacted_flamegraph`.
- `list --privacy <none|redact|anonymize>` redacting per-session rows.
- **ADR-0028** + L1/L2/L3 doc sync (privacy.md §4.3/§4.4 → fully redacted;
  ROADMAP F-10 → done; CHANGELOG; remove L-1 deferred wording).

### Out of scope
- Tool-argument scrubbing in `ToolCall.arguments` (privacy.md §8, separate RFC).
- TUI redaction (local-only).
- `aggregate` flamegraph — aggregate html is table-only, already fully redacted.

## 3. Design decisions

- **D-1 — shared `RedactionContext`.** Crux of F-10: table + flamegraph turn-id
  mappings must match. ctx is a mutable accumulator threaded through both
  reports; `redact(level)` stays as a one-shot wrapper. (Chosen over passing a
  bare `UuidRedactor`, or reconstructing maps from the inverse `RedactionMap`.)
- **D-2 — Episodes symmetric with AnalysisReport.** Same field policy: redact
  level handles ids/model-family/clear-warnings; anonymize adds timestamp-zero
  + MCP hashing. No new PII classes.
- **D-3 — CallRef ↔ map-key consistency.** `Episodes.tools`/`hooks`/`skills`
  are keyed by name; redacting a key (MCP hash) must rewrite every
  `Turn.*_calls[].name` referencing it, or the flamegraph cross-refs break.
- **D-4 — non-MCP names preserved.** Built-in tool/hook/skill names are not
  PII; only `mcp__server__tool` gets its server segment hashed (anonymize),
  mirroring L-1's `hash_mcp_tool_name`.
- **D-5 — `list` reuses the same flag + family helpers**, redacting session
  id/cwd/branch/model per row.

## 4. Episodes redaction surface

| Field | Redact | Anonymize |
|---|---|---|
| `turns[].id` | `<uuid-N>` (shared ctx) | same |
| `turns[].started_at/ended_at` | keep | UNIX_EPOCH |
| `turns[].model` | family | family |
| `turns[].status::Aborted.at` | keep | UNIX_EPOCH |
| `turns[].{tool,hook,skill}_calls[].name` | keep | MCP hash (sync w/ keys) |
| `tools/hooks/skills` keys | keep | MCP hash |
| `aborts[].at` | keep | UNIX_EPOCH |
| `warnings` | clear | clear |
| `model_metrics` | family-merge | family-merge |
| `loaded_mcp_tools` | keep | MCP hash |

## 5. Test plan
- core: `Episodes::redact_with` per-field; cross-site uuid stability
  (meta.id + report turns + episode turns share `<uuid-N>`); CallRef↔key sync;
  shared-ctx determinism. ~12 tests.
- cli: analyze html/speedscope show NO original turn-id / MCP name under
  anonymize (non-vacuous, real fixture); `list --privacy` e2e.
- ADR-0028; privacy.md coverage table → fully redacted.

## 6. Self-review
Placeholders: none. Consistency: D-1 wrapper keeps L-1 green; §4 matches
ADR-0026 anon rules. Scope: one plan (refactor + episodes + list + docs).
Ambiguity: CallRef sync pinned (D-3); non-MCP kept (D-4).
