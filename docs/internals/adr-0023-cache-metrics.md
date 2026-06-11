# ADR-0023: Cache Token Analytics — Formulas + Render Policy

**Status:** Accepted (2026-06-11)
**Context:** M2.5 observational cache analytics (`docs/superpowers/specs/2026-06-11-m2.5-cache-analytics-design.md`)
**Implements:** Q4a closure (`docs/architecture.md` §18) + audit finding F-NEW-2
**Supersedes:** None
**Superseded by:** None
**Related:** ADR-0008 (aggregate report shape), ADR-0019 (SQLite hybrid storage — where the cache token columns live)

## Context

Anthropic prompt caching emits per-request token counts that AgentProf
has captured end-to-end since M1.6.x (Copilot adapter
`cacheReadTokens`) and M2.2 (OTLP `gen_ai.token.type` data points).
The raw counts land in `agentprof_core::analyzer::ModelUsage` fields
(`cache_read_tokens`, `cache_write_tokens`) and SQLite columns
(`sessions.total_cache_read` / `total_cache_creation`,
`model_metrics.cache_read` / `cache_creation`).

The TUI Models view (M1.6.x) already displays raw `cache_read` per
model, but every other render surface (analyze md/html/json, list,
aggregate) silently drops the data. Audit finding F-NEW-2 flagged
these as "write-only columns".

This ADR codifies the formulas and the render policy used to surface
that data in M2.5.

## Decisions

### D-1: Centralized compute, distributed render

A single module `agentprof_core::analyzer::cache` owns the cache
math (struct `CacheMetrics` + 2 constants + builder). Every render
surface (md / html / json / list / aggregate{md,csv,html} / TUI
Models) reads the same `CacheMetrics` struct and formats. No surface
re-derives the formulas.

**Alternatives considered:**

- **Inline at each surface** — smaller initial diff but invites
  formula drift; in a year, three surfaces would have slightly
  different hit-rate definitions.
- **Macro-based render** — reduces format duplication but obscures
  per-surface tweaks (column widths, conditional sections).

### D-2: Dual formulas (naive + honest) for both hit-rate and saved-tokens

| Metric | Naive | Honest |
|---|---|---|
| Hit rate | `cache_read / (cache_read + input_tokens)` | `cache_read / (cache_read + cache_creation)` |
| Saved tokens | `cache_read × 0.9` | `cache_read × 0.9 − cache_creation × 0.25` |

- **Naive** = "of my prompt tokens, X% came from cache" — intuitive,
  doesn't penalize over-caching.
- **Honest** = "of my cache attempts, X% paid off" + accounts for
  write premium — exposes high-create-low-read mis-strategies; can
  be negative.

Render surfaces show **honest as primary**, **naive as supplement**.

Constants:

- `CACHE_READ_DISCOUNT = 0.9` (Anthropic cache read = 0.1× input)
- `CACHE_WRITE_PREMIUM = 0.25` (Anthropic cache write = 1.25× input)

These mirror Anthropic Claude Sonnet 4.x published rates (as of
2026-06). Changing them is a CHANGELOG `BREAKING:` event because
`saved_net` can flip sign.

**Alternatives considered:**

- **Naive only** — hides over-caching anti-patterns.
- **Honest only** — confuses users expecting the intuitive formula.
- **Single composite "effectiveness" metric** — opaque; users can't
  decompose the signal.

### D-3: No cache columns for `aggregate --by tool` / `--by mcp-server`

Cache tokens are accumulated per API call (per turn / per request),
not per tool invocation. A single API call's `cache_read = 8k` might
serve 3 tool calls within that turn; there is no defensible
attribution formula.

For `--by model` / `--by day`: bucket key is at least as coarse as a
session, so per-session cache totals fit; cache columns ON.

For `--by tool` / `--by mcp-server`: cache columns OFF. The render
layer skips them; `cache_metrics_per_bucket()` is not callable for
those bucket types (compile-time trait bound).

**Alternatives considered:**

- **`N/A` cells for symmetry** — every cell becomes noise; users
  start ignoring the column.
- **Naive proportional attribution** (`cache_read / tool_calls per
  turn`) — quantitatively misleading; users would make decisions on
  fictional data.

### D-4: `None` on zero activity

`CacheMetrics::from_raw` returns `None` when `creation == 0 && read
== 0`. The render layer uses `None` to skip the cache section /
column entirely rather than display a row of zeros.

For aggregate buckets with mixed cache / no-cache sessions, the
bucket emits `Some` if any session contributed. Per-session zeros
silently sum into the bucket totals.

**Alternatives considered:**

- **Always emit, render zeros** — clutters reports with meaningless
  rows.
- **`Some` with all-zero fields** — same problem one indirection later.

### D-5: Render rules per surface

| Surface | Renders cache? | Format |
|---|---|---|
| `analyze --export md` | Yes when `Some` | New `## Cache` section, markdown table |
| `analyze --export html` | Yes when `Some` | New `<section id="cache">` in `report.html` |
| `analyze --export json` | Always emit `cache_metrics` field | `null` when `None`, full struct when `Some` |
| `analyze --export speedscope` | No | Cache is observational, not flame-graph data |
| `analyze --export tui` | Existing TUI Models view + new NetSaved col | per D-1 |
| `list` | Yes when any session has cache | New `Cache%` column, empty cell when `None` |
| `aggregate --by model/day --export md` | Yes when `Some(map)` | 4 new cols: CacheCr / CacheRd / Hit% / NetSaved |
| `aggregate --by model/day --export csv` | Same | 4 new cols at end |
| `aggregate --by model/day --export html` | Same | conditional section in `aggregate.html` |
| `aggregate --by tool/mcp-server` (any export) | **No** (D-3) | spec footnote in README |
| `mcp-waste` | No | Cache attribution is per-prompt, not per-MCP-server |

### D-6: JSON field naming convention

`"cache_metrics"` (snake_case) on top-level `AnalysisReport` JSON.
Mirror naming for aggregate JSON: `"buckets": [{ ..., "cache_metrics":
{...} | null }, ...]`.

Consistent with existing snake_case JSON field convention
(`total_input_tokens`, `total_output_tokens`).

## Consequences

**Positive:**

- Closes audit finding F-NEW-2 (write-only schema columns become
  read + displayed).
- Users get actionable cache-utilization data across every relevant
  surface, not just the TUI Models view.
- "Honest" formula + negative `saved_net` exposes over-caching
  problems that the naive metric would hide.

**Negative:**

- 2 hard-coded pricing constants couple AgentProf to Anthropic
  pricing as of 2026-06. Constants are clearly labeled + documented;
  bumping requires CHANGELOG `BREAKING:`.
- `--by tool` / `--by mcp-server` reports gain a "why no cache
  column?" surface area question; README footnote + this ADR address
  it explicitly.

**Neutral:**

- No SQLite schema migration. All data already in M2.1+ columns.
- `CacheMetrics` is `#[non_exhaustive]` (project rule §7-5) — future
  fields (e.g. `cost_usd` for Q4b) can be added without major bump.

## Implementation notes

- `from_raw` uses `u64::saturating_add` for sums; f64 conversion is
  intentionally lossy (1-decimal-place render avoids float-display
  surprises).
- Zero-div guards: when either denominator is 0, the corresponding
  hit_rate field is `0.0`.
- `saved_net: i64` (not `u64`) — negative when creation dominates,
  intentional signal per D-2.

## References

- Spec: `docs/superpowers/specs/2026-06-11-m2.5-cache-analytics-design.md`
- Q4a closure: `docs/architecture.md` §18 (post-v0.3.1 update)
- Audit finding F-NEW-2: post-v0.3.0 comprehensive audit session
- Anthropic prompt caching pricing: https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching
- M2.1 schema (cache columns): `crates/agentprof-storage/migrations/001_initial.sql:20-21,46-47`
- Existing TUI integration: `crates/agentprof-tui/src/views/models.rs`
