# ADR-0026: Report Redaction — `--privacy` Level Semantics + Layer

**Status:** Accepted (2026-06-28)
**Context:** L-1 closure (`docs/superpowers/specs/2026-06-28-privacy-redaction-design.md`, `docs/features/privacy.md` §2 PII tiers)
**Implements:** L-1 (🔴 HIGH PII exposed by default in `analyze` / `aggregate` reports) — `tasks/ROADMAP.md` §6.1
**Supersedes:** the `--redact` / `--anonymize` two-flag draft sketched in `docs/features/privacy.md` §4
**Superseded by:** None — the deferred-scope consequence (Episodes / flamegraph leak) is **CLOSED by F-10 / ADR-0028**
**Related:** ADR-0008 (aggregate report shape + buckets), ADR-0017 (unified session-id namespace — the UUIDs being redacted), ADR-0016 (MCP token cost — the MCP server names being hashed)

## Context

`agentprof analyze` (and `aggregate`) reports carry 🔴 HIGH tier PII by
default: `meta.cwd`, `meta.branch`, `meta.repository`, internal/preview
`model` names, the session UUID, and hundreds of per-turn UUIDs (full
inventory: `docs/features/privacy.md` §2). Sharing a report publicly
(issues, Discussions, blog posts) previously required a manual `sed` / `jq`
cheat sheet (privacy.md §3) — error-prone and easy to forget.

privacy.md §4 sketched `--redact` / `--anonymize` flags but they were never
implemented; this was tracked limitation **L-1 (HIGH severity)**.

This ADR codifies the opt-in report-redaction design now shipped: where the
transform lives, the level semantics, and the per-field rules. The surface
once deferred here (analyze flamegraph + `list`) is now closed by
F-10/ADR-0028. It does **not** touch the separate
*log-output* PII surface, which has had a default-on hashing model since
M1.6.4 (privacy.md §7).

## Considered options

### Where does redaction live?

- **Core report layer** (chosen, D-1). A single
  `agentprof_core::analyzer::redact` pass transforms `AnalysisReport` /
  `AggregateReport` before any render. DRY (every export format inherits
  one transform), type-safe, unit-testable on report structure, and the
  `RedactionMap` falls out of the same walk.
- **Format layer** (rejected). Redact in each of md / json / html / csv /
  speedscope renderers — 6+ duplicated sites that drift over time.
- **Post-serialization regex** (rejected). Fragile UUID / model matching on
  rendered text; no structured `RedactionMap`; silent misses.

## Decisions

### D-1: Redaction lives in the core report layer

New module `agentprof_core::analyzer::redact` owns the transform:
`PrivacyLevel`, `RedactionMap`, `UuidRedactor`, `model_family`,
`hash_mcp_tool_name`, `AnalysisReport::redact`, `AggregateReport::redact`
(via the `RedactBucket` trait per bucket type). Every render surface
serializes the already-redacted report — no surface re-implements the
transform. Rationale + rejected alternatives: see "Considered options".

### D-2: Single mutually-exclusive enum flag

`--privacy <none|redact|anonymize>` (a `clap::ValueEnum` behind the existing
`clap-derive` feature) rather than two booleans. The `redact ⊂ anonymize`
ordering is encoded in the type and cannot be given contradictorily. Default
`none` = `redact()` is never called → zero overhead, byte-identical output,
fully backward-compatible.

### D-3: Two levels

- `Redact` = strip 🔴 HIGH: `cwd` / `branch` / `repository` → `<redacted>`,
  all UUIDs → stable `<uuid-N>`, model → family.
- `Anonymize` = superset: also zeroes `meta.agent_version` / `meta.producer`
  and `meta.started_at` (+ each per-turn `started_at`), hashes MCP server
  names, and emits the reverse `RedactionMap` sidecar.

### D-4: `model → family` at BOTH levels

`model_family(m) = m.split('-').take(2).join('-')`
(`claude-opus-4.7-1m-internal` → `claude-opus`; `o1` → `o1`). Applied at
`redact` and `anonymize` because internal / preview model identifiers are
🔴 HIGH (privacy.md §2), not merely a release detail.

### D-5: UUIDs map to stable `<uuid-N>`

`UuidRedactor` assigns `<uuid-0>`, `<uuid-1>`, … in first-seen walk order
(`meta.id` first → `<uuid-0>`, then `turn_summary` in slice order). The same
original UUID always maps to the same replacement, so percentile rows /
turn cross-references stay internally consistent within a report.

### D-6: MCP hashing keeps the tool segment

`mcp__github__search_issues` → `mcp__<hash8>__search_issues`
(`hash_mcp_tool_name`, reusing `observability::pii::hash_short` = sha256[..8]).
The server is the identifying / private part; the tool verb is useful ROI
signal and non-PII, so it is preserved. Non-MCP builtin tool names
(`bash` / `view` / …) pass through unchanged.

### D-7: `aggregate --by day` bucket keys are NEVER redacted

The date is the aggregation dimension; redacting it makes the report
meaningless, and day-granularity is lower risk than a precise `started_at`
instant. `--by model` keys family-ize; `--by tool` / `--by mcp-server` keys
hash at `anonymize`; `--by day` keys are untouched at every level.

### D-8: `redact()` is a pure function

`redact(level) -> (Self, RedactionMap)` returns `Self` (not `Result`) and
never panics. It degrades safely on odd input (empty model kept, non-UUID
strings still mapped). Only the sidecar *file write* (CLI layer) can fail;
that failure is non-fatal (exit 3 + stderr warning *after* the report is
already emitted).

### D-9: Diagnostic warnings cleared under redaction

`warnings` / `parse_warnings` are emptied whenever a privacy level is active,
because free-form diagnostic strings may embed un-modeled paths / ids that
the structured walk does not reach.

## Deferred scope — CLOSED by F-10 / ADR-0028

> **Historical (M-original).** L-1 shipped with the redaction pass covering
> only the **report** (`meta`, summary tables, ranks, model metrics) — it did
> **not** cover `episodes`, from which the html SVG flamegraph and speedscope
> frames are built. F-10 ([ADR-0028](./adr-0028-episodes-redaction.md)) closed
> this gap: a shared `RedactionContext` now threads through both the report and
> its episodes, so analyze `html` / `speedscope` are fully redacted and `list`
> gained `--privacy`. `Episodes::redact` and the `warn_unredacted_flamegraph`
> warning described below no longer reflect shipped behavior.

The L-1 redaction pass originally left html/speedscope flamegraph frames as a
residual leak surface; under `--privacy redact|anonymize` they retained turn
UUIDs + raw MCP server names, and `analyze` fired a `tracing::warn!` steering
sharing to `md` / `json`. This is superseded by ADR-0028.

**Skill names** are deliberately preserved (🟢 LOW per privacy.md §2 — the
ROI signal is the point of redaction).

## Consequences

**Positive:**

- Closes L-1: opt-in redaction with `none` default = zero behavior change.
- All report-derived formats inherit one transform (md / json / csv fully
  redacted; html / speedscope meta + tables redacted).
- `RedactionMap` sidecar (`agentprof-redaction-map.json`, anonymize only)
  lets a holder un-redact without embedding originals inline.

**Negative:**

- ~~html / speedscope flamegraph frames remain a documented residual leak
  surface until `Episodes::redact` lands.~~ **CLOSED by F-10 / ADR-0028** —
  episodes now share the report's `RedactionContext`; all formats redacted.

**Neutral:**

- No SQLite migration, no new feature gate (reuses `clap-derive`).
- TUI is intentionally not redacted (local-only surface; `--privacy` +
  `--export tui` warns).

## References

- Spec: `docs/superpowers/specs/2026-06-28-privacy-redaction-design.md`
- Plan: `docs/superpowers/plans/2026-06-28-privacy-redaction.md`
- PII tiers + field inventory: `docs/features/privacy.md` §2
- Core module: `crates/agentprof-core/src/analyzer/redact.rs`
- CLI wiring: `crates/agentprof-cli/src/cmd/analyze.rs`,
  `crates/agentprof-cli/src/cmd/aggregate.rs`,
  `crates/agentprof-cli/src/cmd/privacy.rs`
- L-1 tracking: `tasks/ROADMAP.md` §6.1
