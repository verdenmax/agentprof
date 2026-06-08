---
title: "ADR-0016 — MCP tool token-cost view architecture"
status: "Accepted"
date: "2026-06-08"
authors: "@verdenmax (maintainer), AI assistant (Copilot CLI session 252068e5)"
supersedes: []
superseded_by: []
related:
  - "docs/superpowers/specs/2026-06-08-m1.6.6-token-cost-design.md"
  - "docs/internals/adr-0015-mcp-waste-architecture.md (extends D-5)"
---

# ADR-0016 — MCP tool token-cost view architecture

## Status
Accepted (2026-06-08).

## Context

M1.6.5 (ADR-0015) shipped per-tool-COUNT waste analysis but deferred token-cost (D-5)
to isolate `mcp.json` schema-variation risk. M1.6.6 lifts that constraint and lands
the "View C" from M1.6.5 brainstorming: "of my context budget, how many tokens are
wasted on tool descriptions that were never invoked?"

The MCP protocol stores tool descriptions in the server's `tools/list` JSON-RPC
response (not in `mcp.json`). agentprof is read-only and does NOT spawn / connect to
MCP servers. The maintainer's environment has no MCP servers installed, so this
feature is shipped for users who DO use MCP — heuristic default keeps it useful for
everyone.

## Decision summary

| # | Decision | Choice |
|---|---|---|
| D-1 | Tool-description data source | Heuristic constant + optional sidecar (fallback chain) |
| D-2 | Sidecar format | Auto-detect by path type: file → global JSON; dir → per-server `tools/list` responses |
| D-3 | Token scope per tool | Full tool entry serialized JSON (`name` + `description` + `inputSchema` + optional annotations) |
| D-4 | Tokenizer choice | Auto-infer from `session.meta.model` (`gpt-5*`/`gpt-4o*` → `o200k_base`; else `cl100k_base`) |
| D-5 | `compute_waste` signature | One BREAKING change to `(report, &WasteComputeContext)` builder-pattern struct; future additions non-breaking |
| D-6 | Drop M1.6.5-planned `WasteDataSource::TokenCostUnavailable` | Use new `TokenProvenance` enum instead — heuristic fallback always succeeds |

## D-1: Tool-description data source

Considered options:
- (a) Pure heuristic constant (zero user config, ships value today, lossy)
- (b) Sidecar required (most accurate, high friction)
- (c) Heuristic + optional sidecar (CHOSEN) — defaults to value-yielding heuristic; users with `tools/list` access get precision via `--tool-descriptions <PATH>`
- (d) Spawn MCP server + RPC (rejected: violates agentprof read-only invariant per ADR-0015 D-1)

Consequences: `Sidecar` is optional throughout the call chain; `compute_waste` accepts
`Option<&Sidecar>` and falls back to the heuristic per-tool. `TokenProvenance` enum
on each `WasteReport` records `Heuristic` / `SidecarExact` / `Mixed`.

## D-2: Sidecar format

Considered options:
- (a) Single global JSON file only (simplest for manual editing)
- (b) Per-server directory only (matches `curl <server>/tools/list > github.json` workflow)
- (c) Both, auto-detected by `path.is_file()` vs `path.is_dir()` (CHOSEN)

Consequences: One `--tool-descriptions <PATH>` flag covers both use cases. Implementation
needs `path.metadata()` + branch. Mixed JSON shapes within a dir (some `{"tools": [...]}`,
some bare `[...]`) tolerated; both shapes parse via `serde_json::Value` then normalize.

## D-3: Token scope per tool

Considered options:
- (a) Just `description` text (lossy — agent sees more than description)
- (b) `description` + `inputSchema` (better — biggest fields)
- (c) Full tool entry serialized JSON (CHOSEN — exactly what agent sees in system prompt's tool list)

Consequences: `compute_token_cost_for_tool` calls `serde_json::to_string(entry)` then
tokenizes the resulting JSON. Whitespace-significant — serde's default compact serializer
matches MCP's typical wire output closely enough.

## D-4: Tokenizer choice

Considered options:
- (a) Hardcode `cl100k_base` (GPT-4 baseline; broadest compat)
- (b) Hardcode `o200k_base` (GPT-4o/5 modern)
- (c) `--tokenizer cl100k|o200k` flag
- (d) Auto-infer from `session.meta.model` (CHOSEN) — `gpt-5*`/`gpt-4o*` → `o200k_base`; else `cl100k_base`

Consequences: For aggregate / mcp-waste across multi-model sessions, each session uses
its own tokenizer; aggregator sums verbatim. Renderer footer notes "tokenizers may vary
across sessions" when relevant. Unknown / missing model → `cl100k_base` (matches Claude
approximation too).

## D-5: `compute_waste` signature: builder-pattern context struct

The M1.6.5 signature was `(report, wire_loaded, config_loaded)`. M1.6.6 needs 3 more
inputs (sidecar, heuristic constant, tokenizer). Adding them as positional params would
break callers AGAIN. Builder pattern with `#[non_exhaustive] WasteComputeContext<'a>`
struct + `with_*` methods locks the signature shape — future additions append fields
to the struct without breaking callers.

Considered alternatives:
- Flat positional params (rejected: breaks every milestone)
- `#[derive(Default)] + #[non_exhaustive]` (rejected: non_exhaustive blocks struct-literal
  construction from outside the defining crate, even with Default — must use `new()` or
  builder)
- `Box<dyn ComputeWasteCtx>` trait object (rejected: overengineered)

Consequences: M1.6.6 is the ONLY remaining breaking change to `compute_waste`. Plan
T1.4 includes the migration of all callers (analyze, aggregate, mcp_waste, all tests).

## D-6: Drop `WasteDataSource::TokenCostUnavailable`

M1.6.5 spec §5.4 reserved a `WasteDataSource::TokenCostUnavailable` discriminant for
"no descriptions in mcp.json". That discriminant never fires under D-1's fallback chain
(heuristic always succeeds). Instead, `WasteReport` gains a `TokenProvenance` field
(`Heuristic` / `SidecarExact` / `Mixed`) that conveys the same information at the right
granularity (token-source quality, not data-source availability).

Consequences: `WasteDataSource` enum stays at its M1.6.5 size. JSON schema gains one new
field on `WasteReport` (`token_provenance`) but no enum-variant additions.

## Consequences

- core stays leaf; `Sidecar` lives in `agentprof-adapters::copilot::tool_sidecar`.
- `tiktoken-rs = "0.6"` (workspace dep, currently unused) becomes first activated.
- Future tokenizer additions (Anthropic, Gemini) add new `TokenizerKind` variants
  + extend `infer_tokenizer` match — non-breaking thanks to `#[non_exhaustive]`.
- M1.6.5 surface bodies (md/json/html/tui) stay the same; M1.6.6 adds columns/banner
  lines without restructuring layouts.
- Tests use the new `mcp-tool-sidecar/` fixture for sidecar-exact path coverage;
  existing `with-mcp-waste` fixture continues to exercise the heuristic-only path.

## References

- Spec: `docs/superpowers/specs/2026-06-08-m1.6.6-token-cost-design.md`
- Plan: `docs/superpowers/plans/2026-06-08-m1.6.6-token-cost.md`
- Predecessor: `docs/internals/adr-0015-mcp-waste-architecture.md` (D-5)
- MCP protocol: https://modelcontextprotocol.io/specification/server/tools
- tiktoken-rs: https://docs.rs/tiktoken-rs/0.6
