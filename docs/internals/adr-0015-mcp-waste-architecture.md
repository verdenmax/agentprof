---
title: "ADR-0015 — MCP server waste analysis architecture"
status: "Accepted"
date: "2026-06-08"
authors: "@verdenmax (maintainer), AI assistant (Copilot CLI session 252068e5)"
supersedes: []
superseded_by: []
related: ["docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md"]
---

# ADR-0015 — MCP server waste analysis architecture

## Status
Accepted (2026-06-08).

## Context
agentprof's ROI view answers "of the tools you called, which returned value".
It does not answer "what tools did you pay for but never call?" — the
"waste" question. M1.6.5 fills that gap with a per-session `WasteReport`
+ cross-session aggregator + 4 surfaces.

## Decision summary

| # | Decision | Choice |
|---|---|---|
| D-1 | Data source for "loaded" set | Fallback chain: `mcp.json` if present → wire `<tools_changed_notice>` always (union semantics) |
| D-2 | "Loaded" semantics | Ever-loaded (Remove-notice does NOT decrement); precise loaded-window model deferred to M1.6.6+ |
| D-3 | Surface decomposition | Both extend existing commands (`analyze --section`, `aggregate` columns) AND add new `mcp-waste` subcommand |
| D-4 | TUI layout | Split-pane like Models view (server list left + tool detail right) |
| D-5 | Token-cost view deferral | Carved out as M1.6.6 (separate milestone) — isolates mcp.json schema-variation risk |
| D-6 | Defensive `LoadedSource::InferredFromCall` | When a tool was called but appeared in neither wire nor config, treat as loaded (no false "uncalled" reports) |

## D-1: Data source for "loaded" set

Considered options:
- (a) mcp.json only — plan.md's original assumption. Rejected: mcp.json absent on the maintainer's machine (2026-06-08 audit) and schema varies wildly (VSCode-flavored vs self-describing). Wire-only is more robust.
- (b) Wire `<tools_changed_notice>` only — robust but lossy: configured-but-never-enabled tools (e.g. mcp.json declares X but agent session never enabled it) are invisible.
- (c) Both, fallback chain (CHOSEN) — wire always parsed; mcp.json supplements when present. Captures both "configured but unused" (mcp.json + wire) and "agent enabled but never called" (wire alone).

Consequences: caller (cli) responsible for loading both sources and passing them to `compute_waste`. Core stays pure (no fs reads).

## D-2: Ever-loaded semantics

Considered options:
- (a) Currently-loaded (track add/remove deltas, maintain running set, compute waste at session end). Most precise but most complex — needs `LoadedWindow` model with start/end timestamps per tool.
- (b) Ever-loaded (any tool ever seen in `New tools available:` counts as loaded). CHOSEN: simple, captures "did agent have access at any point". Adequate for "should I remove this from mcp.json" answer.

Consequences: a tool removed via `Tools no longer available:` mid-session and never re-added still counts as "loaded" in the report. UI banner clarifies the semantics. Precise model deferred to M1.6.6+ if user demand surfaces.

## D-3: Surface decomposition

Considered options:
- (a) Extend existing commands only (`analyze --section`, `aggregate` columns). Lowest LOC but answers cross-session "which servers can I remove?" question awkwardly (have to aggregate over many sessions manually).
- (b) New `mcp-waste` subcommand only. Misses single-session use case (already running analyze, want section).
- (c) Both (CHOSEN). Extend existing commands for inline use AND add dedicated subcommand for the cross-session "MCP cleanup" report. Layered: same `compute_waste` core, different surface concerns.

Consequences: more surface area (4 entry points), but each is a thin renderer; algorithm shared. Test cost grows linearly with surface count.

## D-4: TUI layout

Considered options (mocked up in brainstorming visual companion):
- (a) Split-pane (server list left + tool detail right) — same pattern as F1.7 Models view. CHOSEN: reuses existing layout infrastructure, lower risk.
- (b) Single full-width table (all tools, server in column). Rejected: server-level summary buried in header line.
- (c) Stacked vertical (server table top, tool table bottom). Rejected: new layout pattern with no existing code to reuse.

Consequences: views::mcp_waste imports layout helpers from views::models (or via shared `app::layout::split_pane`).

## D-5: Defer token-cost view to M1.6.6

The brainstorming "View C" (token-cost waste via `tiktoken-rs` × actual tool descriptions) requires reading tool description text. Description text is NOT in events.jsonl. Sources:
- mcp.json self-describing schema may have it (rare)
- Copilot CLI does not log it (out of scope to query MCP server's tools/list RPC — agentprof is read-only)

mcp.json schema variation is the key risk. Phase 1 (counts-only) ships without this risk; Phase 2 (M1.6.6) layers token-cost on top using the same data model with additive `unused_tokens: u64` fields (non-breaking — all types are `#[non_exhaustive]`).

## D-6: Defensive InferredFromCall

If our wire parser misses a `<tools_changed_notice>` "New tools available" line (parser bug, novel content shape, etc.), the called tool would not appear in `wire_loaded`. Naive `waste = loaded - called` then computes negative waste for that tool, or worse, omits it from the report entirely.

Decision: when a called tool appears in neither wire nor config, synthesize it into `loaded` with provenance `LoadedSource::InferredFromCall`. UI may flag this provenance to hint at parser gaps. NO false "uncalled" reports.

## Consequences

- core stays leaf (no workspace deps), pure function `compute_waste`.
- Future adapters (Claude / Codex M3.x) provide their own `tools_changed` / `mcp_config` parsers — they implement the same `(wire_loaded, config_loaded)` contract.
- Cross-session subcommand reuses existing session-discovery and time-window logic from `aggregate`.
- 4 surfaces share one renderer-friendly data model (`WasteReport`).

## References

- Spec: `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md`
- Brainstorming session: 252068e5 (2026-06-08)
- Plan: `docs/superpowers/plans/2026-06-08-m1.6.5-mcp-waste.md`
