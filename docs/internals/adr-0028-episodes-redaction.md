# ADR-0028: Episodes Redaction — shared `RedactionContext` + `list --privacy` (F-10)

**Status:** Accepted (2026-06-29)
**Context:** F-10 closure (`docs/superpowers/specs/2026-06-29-episodes-redaction-design.md`, `docs/superpowers/plans/2026-06-29-episodes-redaction.md`)
**Implements:** F-10 (analyze html/speedscope flamegraph + `list` not yet redacted) — `tasks/ROADMAP.md` §6.2
**Builds on:** ADR-0026 (`--privacy` level semantics + core redaction layer)
**Supersedes:** the "deferred Episodes::redact / `warn_unredacted_flamegraph`" residual-leak consequence of ADR-0026
**Superseded by:** None
**Related:** ADR-0007 (speedscope export — frame table reads `tool.source`), ADR-0004 (episode derivation — `Episodes` shape), ADR-0017 (unified session-id namespace — the UUIDs being mapped), ADR-0016 (MCP token cost — server names hashed)

## Context

ADR-0026 shipped `--privacy <none|redact|anonymize>` for `analyze` + `aggregate`
but documented two **deferred** surfaces:

1. **analyze html/speedscope flamegraph** is built from un-redacted `Episodes`,
   so its frames retained original turn-ids (html SVG `turn-{id}`) and raw MCP
   server names (`mcp:{server}::{tool}`). `analyze` fired
   `warn_unredacted_flamegraph` steering users to `md` / `json`.
2. **`list`** had no `--privacy` flag — per-session rows carry session-id +
   model PII.

The crux: `analyze` renders the table from `AnalysisReport` and the flamegraph
from `Episodes`. To make both fully redactable, the turn-id mapping must be
**shared** so table and flamegraph agree. That requires factoring L-1's
internal redaction state out of `AnalysisReport::redact` into a reusable
context. This ADR codifies the shipped F-10 design.

## Considered options

### How is turn-id state shared between report and episodes?

- **Shared `RedactionContext` accumulator** (chosen, D-1). One mutable ctx
  threaded through both `AnalysisReport::redact_with` and
  `Episodes::redact_with`; `redact()` becomes a one-shot wrapper. DRY, keeps
  L-1 API + tests green, guarantees identical `<uuid-N>` mapping.
- **Pass a bare `UuidRedactor`** (rejected). Models/servers maps would still
  diverge between the two passes; no single `into_map()`.
- **Reconstruct from the inverse `RedactionMap`** (rejected). Fragile, only
  exists at anonymize, invents a re-parse step.

## Decisions

### D-1: Shared `RedactionContext` + `redact()` wrapper

New `RedactionContext { uuids, models, servers }` is the mutable accumulator.
`AnalysisReport::redact_with(level, &mut ctx)` and `Episodes::redact_with(level,
&mut ctx)` take it; `ctx.into_map()` consumes it into the exported
`RedactionMap`. The L-1 `redact(level) -> (Self, RedactionMap)` is kept as a
thin wrapper (build one ctx, run, `into_map`) so the L-1 public API and tests
are unchanged.

### D-2: Episodes symmetric with AnalysisReport (L-1 field policy)

`Episodes::redact_with` mirrors L-1: `Redact` maps turn ids → `<uuid-N>`,
collapses models to family, clears `warnings`; `Anonymize` additionally zeroes
every wall-clock instant and hashes MCP names + emits the sidecar. No new PII
classes — turns / tools / hooks / skills / aborts / model_metrics /
loaded_mcp_tools are covered symmetrically.

### D-3: CallRef ↔ map-key + `source.server` + call `turn_id` sync

`Episodes.{tools,hooks,skills}` are keyed by name; redacting a key (MCP hash)
must rewrite every reference or the flamegraph cross-refs break. The **four**
`CallRef` reference sites kept in sync with the rekeyed maps are:

1. `Turn.tool_calls[].name`
2. `Turn.hook_calls[].name`
3. `Turn.skill_calls[].name`
4. `SkillInvocation.triggered_tools[].name`

all hashed via the **same** `hash_mcp_tool_name`. Plus `ToolEpisode.source`'s
`Mcp { server }` is hashed with the same `hash_short` already embedded in the
rekeyed name, because speedscope reads `tool.source` for its `mcp:{server}`
frame. Each call/invocation `turn_id` is remapped to the **same** `<uuid-N>` as
`turns[].id` at BOTH levels, so speedscope/html frames stay joinable with the
table.

### D-4: Non-MCP names preserved

Built-in tool/hook/skill names (`bash` / `view` / …) are not PII and pass
through unchanged; only `mcp__server__tool` has its server segment hashed
(anonymize), mirroring L-1's `hash_mcp_tool_name`.

### D-5: `list --privacy` reuses the same flag + helpers

`list --privacy <none|redact|anonymize>` threads one `RedactionContext` per
invocation so identical session ids collapse to the same `<uuid-N>` and models
family-ize. `cwd` / `branch` never reach the table, so `list` writes no sidecar
and `Redact` / `Anonymize` behave identically there.

## Caveats

- **span-zero @ anon**: anonymize zeroes every call's `span.{started_at,ended_at}`
  to UNIX_EPOCH (mirroring `turn.started_at`); keeping wall-clock here would
  leak working hours and break flamegraph zero offsets. Durations are kept
  (the ROI signal).
- **arguments deferred**: `ToolCall.arguments` are retained verbatim at every
  level — tool-arg scrubbing is a separate RFC (privacy.md §8), so JSON export
  of anonymized episodes may still carry path/secret PII in args.

## Consequences

**Positive:**

- Closes F-10: analyze `html` / `speedscope` are now fully redacted; the
  `warn_unredacted_flamegraph` warning is dropped.
- `list` joins the redacted surfaces; one ctx → stable per-session `<uuid-N>`.
- L-1 API + tests unchanged (`redact()` wrapper).

**Negative:**

- Tool-arg PII in episode JSON remains an open item (separate RFC).

**Neutral:**

- No SQLite migration, no new feature gate (reuses `clap-derive`).
- TUI is intentionally not redacted (local-only surface).

## References

- Spec: `docs/superpowers/specs/2026-06-29-episodes-redaction-design.md`
- Plan: `docs/superpowers/plans/2026-06-29-episodes-redaction.md`
- Core: `crates/agentprof-core/src/analyzer/redact.rs` (`RedactionContext`,
  `AnalysisReport::redact_with`, `Episodes::redact_with`)
- CLI: `crates/agentprof-cli/src/cmd/analyze.rs`, `crates/agentprof-cli/src/cmd/list.rs`
- ADR-0026 (deferred scope closed here)
- F-10 tracking: `tasks/ROADMAP.md` §6.2
