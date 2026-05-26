---
title: "ADR-0001: Events-First MVP Pivot — perf flamegraph for AI agents, not a smarter ccusage"
status: "Accepted"
date: "2026-05-26"
authors: "@verdenmax (project owner), AI assistant (Copilot CLI session 252068e5)"
tags: ["architecture", "decision", "product-positioning", "mvp", "phase-0", "phase-1"]
supersedes: ""
superseded_by: ""
---

# ADR-0001: Events-First MVP Pivot — perf flamegraph for AI agents, not a smarter ccusage

## Status

**Accepted**

## Context

The original `tasks/001-mvp-agent-token-profiler.md` (commit `ae2045a`) and `docs/plan.md` (commit `b47aeb5`) framed agentprof as a **token-cost ROI tool**:

- One-liner: *"花得值不值"* (is the spending worth it)
- Differentiator G2: `schema_utilization = Σ called_schema / Σ loaded_schema`
- Headline visualization: TokenBucket-per-turn (system / tools_schema / history / user / tool_result / output / cache_read / cache_creation)
- First-shipping adapter: `ClaudeAdapter` (reads `~/.claude/projects/**/*.jsonl`)

This framing positioned agentprof against an extremely crowded set of incumbents:

| Tool | Stars | Domain | Notes |
|---|---|---|---|
| `ccusage` (ryoppippi) | ~60k⭐ | Claude / Copilot / Gemini / etc. token cost | **Rust workspace, 15+ adapters, OTel-native for Copilot** |
| `tokscale` | 3.2k⭐ | Token cost | |
| `splitrail` | small | Multi-vendor token tracker | |
| `claude-usage` | small | Claude-specific | |
| `toktrack` | small | Same | |

During Stage 1 brainstorming (2026-05-26) the following facts surfaced that invalidate the original framing:

1. **The token-cost niche is saturated.** ccusage alone (with 60k stars, Rust workspace, native OTel integration, 15+ adapters) covers the "how much did I spend" question. Building a 16th token tracker is differentiation-negative.
2. **agentprof would have to fight on ccusage's turf with ccusage's tools.** ccusage already uses OpenTelemetry gen_ai semantic conventions (`gen_ai.usage.input_tokens` / `gen_ai.usage.cache_read.input_tokens` / `gen_ai.usage.reasoning.output_tokens` / `github.copilot.tool.call.count` etc.) — the industry standard for token telemetry.
3. **Copilot CLI session-state contains a much richer event stream** than the token-only OTel view: `hook.start` / `hook.end` (33+ pairs per session), `skill.invoked` (with `pluginName` / `pluginVersion` / `trigger`), `tool.execution_start/complete` (with `arguments` / `result.detailedContent` / `toolTelemetry`), `tool.user_requested` (manual vs autonomous), `session.mode_changed` (interactive↔plan↔autopilot), `session.model_change`, `session.plan_changed`, `abort` (with `reason`), `session.shutdown` (with `codeChanges{linesAdded,linesRemoved,filesModified}` + per-model `modelMetrics`).
4. **No tool today profiles "what the agent actually did"** at event granularity. Asking "which hook is noise? which skill drives tool spam? what tool was running when abort fired? how much time did Plan mode take vs Autopilot?" is unanswered by any incumbent.
5. **The user explicitly recalibrated priorities during brainstorming**: *"我听了你说的，OTel 并不完全满足（如 hook/skill/abort）、现阶段我反而更在意这些「事件」信息而不是 token 账单"* — confirming events are the higher-value signal than token billing for this MVP.
6. **`schema_utilization` (originally G2)** requires either (a) parsing system-block tool definitions from Claude jsonl, or (b) enabling Copilot OTel `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT=true`, neither of which is available on the user's current machine. Deferring it to Phase 2 avoids blocking MVP.
7. **session-state events.jsonl is default-enabled and currently has 100+ real sessions on the user's machine** (`~/.copilot/session-state/<uuid>/events.jsonl`), zero user setup needed. Claude data, by contrast, the user does not have on this machine yet.

Combined, these facts indicate the original token-first framing chose the wrong differentiation axis. A pivot is needed before any line of business code is written (M1.1 skeleton was finalized but M1.2–M1.7 had not started).

## Decision

**Reposition agentprof v0.1.0 MVP as "perf flamegraph for AI agents" — event-level observability is the primary signal; token billing is an optional sidebar.**

### Concrete decisions

1. **One-liner becomes**: *"agentprof = perf flamegraph for AI agents — visualize what your Claude/Copilot/Codex session actually **did** at event level (hooks, skills, tools, modes, aborts), not just what it **cost**."*

2. **First-shipping adapter** (M1.2) is `CopilotAdapter` reading `~/.copilot/session-state/<uuid>/events.jsonl`, **not** `ClaudeAdapter`.

3. **Primary signal is the event stream** (`CopilotEvent` enum, 17 variants — see ADR-0002) **plus derived `Episode` types** (`Turn`, `ToolEpisode`, `HookEpisode`, `SkillEpisode`, `ModeSegment`) in `agentprof-core::episode`.

4. **TokenBucket / schema_utilization / waste_estimate_usd** are **deferred to Phase 2** (when OTel content capture or `.mcp.json` reverse engineering lands and Claude data becomes available).

5. **TUI gets 5+1 event-oriented views** (default `flamegraph`, plus `tool_rank`, `hook_rank`, `turns`, `modes`, optional `summary`) instead of the original 3 token-oriented views (`flamegraph`, `roi`, `aggregate`).

6. **The `tokenizer` module in `agentprof-core` is removed from MVP scope** (was M1.3 P0); it returns in Phase 2.

7. **Differentiator becomes G2-NEW**: event-level visualization (hook noise rank / skill ROI / abort attribution / mode timing) — none of which any incumbent tool produces.

The full architectural elaboration lives in `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md` (commit `cf7e7ad`).

## Consequences

### Positive

- **POS-001**: **Unique market position.** Zero direct competitors in event-level agent profiling. Searched: ccusage / tokscale / splitrail / claude-usage / toktrack / langfuse / phoenix / helicone — none answer "what hooks fired in my session?" or "which tool was running at abort time?"
- **POS-002**: **MVP unblocked.** Data is available today on the user's machine (`~/.copilot/session-state/`, ~100+ sessions, no user setup). Claude data, originally needed for M1.2, isn't on this machine and would block development until provided.
- **POS-003**: **Cleaner abstraction layer.** Per-agent native `Event` enum + shared `Episode` types (ADR-0002) avoids the trap of single-normalized cross-agent enum (forces lowest-common-denominator, loses information).
- **POS-004**: **Architectural simplification.** Tokenizer dependency (`tiktoken-rs`) removed from MVP critical path. Episode-based analyzer is a stateless O(n) state machine — no caching, no network, no async runtime needed in MVP.
- **POS-005**: **Forward-compatible with token analytics.** Phase 2 can layer tokenizer + OTel content capture on top of the same Episode model without retroactive changes. Token billing becomes a derived view, not the core model.
- **POS-006**: **Speedscope export becomes natural.** Episode tree IS a flamegraph — direct mapping to Speedscope evented profile format. Users can drag `agentprof export --format speedscope > s.json` into https://speedscope.app and get a browser flamegraph for free.
- **POS-007**: **Aligns with the perf / dtrace / Jaeger mental model** that the user already understands, rather than re-inventing "spend dashboard" terminology.

### Negative

- **NEG-001**: **G2-OLD (`schema_utilization`) — the most-quoted differentiator in `docs/plan.md`** — is deferred. The original story "ccusage tells you you spent $5; agentprof tells you 70% of those tokens were wasted MCP schema" becomes "Phase 2".
- **NEG-002**: **Massive documentation churn**: `docs/plan.md` §1/§3/§5, `docs/architecture.md` §1/§3/§6/§7/§14, `tasks/001-mvp-agent-token-profiler.md` (rename + full §4/§5/§6/§10 rewrite), `tasks/ROADMAP.md` §2/§3, root `README.md` quickstart all need synchronized updates. Owner: M1.7 docs-sync milestone.
- **NEG-003**: **Phase 3 multi-agent assumption changes.** Original "Phase 3 adds Codex/Copilot/Gemini" reorders: Copilot is in v0.1.0 first; Claude moves to v0.1.x / v0.2. Tasks/002 and tasks/003 placeholders need scope review when written.
- **NEG-004**: **Marketing risk**: "yet another perf tool" lands less viscerally than "find your $$$ waste". Mitigation: lead screenshots with "look — that one hook fired 33 times and took 8 seconds" instead of total-dollar numbers.
- **NEG-005**: **Privacy surface broader**: events.jsonl contains user prompts, tool args (potentially code/secrets), tool results — broader than Claude jsonl which is mostly token tallies. Mitigated by synthetic-only fixtures (ADR-0003) and explicit "nothing leaves your machine" promise.
- **NEG-006**: **TUI scope grows**: 5+1 views instead of 3. M1.5 estimate revised upward (~5 days vs ~4).

## Alternatives Considered

### Keep token-first MVP, ship ClaudeAdapter first as planned

- **ALT-001**: **Description**: Follow original `tasks/001` verbatim. Reverse-engineer `~/.claude/projects/**/*.jsonl`. Build `tokenizer` + `schema_utilization` + `waste_estimate_usd`. Ship MVP as "smarter ccusage with ROI focus."
- **ALT-002**: **Rejection Reason**: (a) Claude data not available on this machine — would block until user provides; (b) competes head-on with ccusage's 60k⭐ in red-ocean turf; (c) primary differentiator `schema_utilization` is one metric vs ccusage's many — narrow value-prop; (d) user explicitly rebalanced priorities toward events during brainstorming.

### OTel-native architecture (read ~/.copilot/otel/*.jsonl using gen_ai semantic conventions)

- **ALT-003**: **Description**: Adopt ccusage's architecture for Copilot — read OTel file-exporter output (set via `COPILOT_OTEL_FILE_EXPORTER_PATH` env var). Use OTel gen_ai semantic conventions as the universal wire format. One adapter potentially covers multiple agents if they emit OTel.
- **ALT-004**: **Rejection Reason**: (a) OTel covers ~85% of MVP value but **misses** hook lifecycle / skill invocation / mode change / abort context — exactly the signals the user said they care most about; (b) requires user to set env vars before any data is collected — friction; (c) `~/.copilot/otel/` is empty on this machine currently; (d) becomes a Phase 2 enhancement to layer **on top of** the events-first MVP, not a replacement.

### Hybrid: events-primary + OTel-secondary in same MVP

- **ALT-005**: **Description**: Read both `~/.copilot/session-state/*/events.jsonl` (events) AND `~/.copilot/otel/*.jsonl` (token totals) in MVP. Join on session ID. Render unified view.
- **ALT-006**: **Rejection Reason**: (a) doubles parser complexity for marginal MVP value; (b) requires JOIN logic across two data sources with different identity schemes; (c) OTel is empty on user machine — JOIN partial; (d) phasing it as "events MVP → OTel Phase 2" gives same end-state with cleaner increments.

### Build on Copilot CLI SDK (`@github/copilot/copilot-sdk`)

- **ALT-007**: **Description**: Embed Node.js + use the official SDK at `/usr/lib/node_modules/@github/copilot/copilot-sdk/`. Translate its `.d.ts` / Zod schemas into Rust types. Get the "official" wire format definitions.
- **ALT-008**: **Rejection Reason**: (a) SDK is **JSON-RPC client for live session control**, not a parser of events.jsonl — wrong abstraction (no `parseEvents()` / `loadSessionFromDisk()` functions); (b) SDK's published event types cover only 8 of 17+ actually-observed event types (it's for SDK consumers, not internal telemetry); (c) embedding Node kills "single binary" architectural goal in `docs/architecture.md §2`; (d) **`LICENSE.md §3` of the SDK forbids "Modify, adapter, translate, or create derivative works"** — translating `.d.ts` to Rust would violate this; (e) clean-room observation of one's own session data (which is what we do) is fully legal.

### Reverse-engineer Anthropic SDK source / Claude Code internals to start with Claude

- **ALT-009**: **Description**: Even without Claude session data on this machine, use the `claude-code-source-code` directory (which the user has cloned) to reverse-engineer the JSONL writer code and produce a ClaudeAdapter speculatively.
- **ALT-010**: **Rejection Reason**: (a) speculative — no real data to validate against; (b) Claude Code may emit OTel in future, making the speculative work obsolete; (c) violates "build from observation, not from source code reading" clean-room principle; (d) doesn't help with the deeper "are we competing in red ocean?" concern.

## Implementation Notes

- **IMP-001**: **Docs sync owner is M1.7**, not M1.2/M1.3. The pivot reframes the product, but L1 doc rewrites only happen at MVP release time to amortize churn. Until then, the spec at `docs/superpowers/specs/2026-05-26-...` is the temporary source of truth.
- **IMP-002**: **Crate structure unchanged.** M1.1 skeleton (`agentprof-{core,adapters,storage,tui,cli}` + `xtask`) survives intact. No `cargo new` / `cargo workspace member` changes needed.
- **IMP-003**: **Phase 2 trigger conditions explicit**: token-cost analytics returns when **any** of (a) user provides ClaudeAdapter fixtures, (b) Copilot OTel content capture data appears, (c) `.mcp.json` reverse-engineering proven feasible. Phase 2 work tracked in `tasks/002-phase2-engineering.md` (not yet authored; referenced from `tasks/ROADMAP.md §3.2`).
- **IMP-004**: **Success criterion for the pivot is verified at M1.7 release time**: ≥1 user (outside @verdenmax) tries `agentprof analyze` on their own Copilot session and posts/shares a screenshot of their event flamegraph. If after 4 weeks no organic share materializes, revisit positioning.
- **IMP-005**: **`tasks/001` will be renamed** at M1.7 from `001-mvp-agent-token-profiler.md` → `001-mvp-agent-event-profiler.md` via `git mv` (preserving commit history).
- **IMP-006**: **`CHANGELOG.md` `[Unreleased]` MUST include** `BREAKING: refocus MVP from token-cost analysis to event-level agent profiling` to signal the pivot publicly at v0.1.0.

## References

- **REF-001**: `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md` (commit `cf7e7ad`) — full design specification this ADR captures
- **REF-002**: `tasks/001-mvp-agent-token-profiler.md` (commit `ae2045a`) — original task to be superseded
- **REF-003**: `docs/plan.md` §1/§3/§5 — to be updated at M1.7 sync
- **REF-004**: `docs/architecture.md` §1/§3/§6/§7/§14 — to be updated at M1.7 sync
- **REF-005**: `.github/copilot-instructions.md` §5.5 — trigger condition for this ADR (≥2 candidate alternatives + new public-API key design)
- **REF-006**: `ADR-0002` (this commit) — `CopilotEvent` enum wire-format reference, the concrete instantiation of this pivot
- **REF-007**: `ADR-0003` (this commit) — synthetic-only fixture strategy, downstream consequence of this pivot
- **REF-008**: `ryoppippi/ccusage` repo (`rust/crates/ccusage/src/adapter/`) — competitor architecture reviewed during brainstorming; structural template (per-agent adapter dirs) inspired our layout but not the data model
- **REF-009**: GitHub Copilot CLI `copilot help monitoring` — Copilot's official OTel exporter spec; deferred to Phase 2
- **REF-010**: `/usr/lib/node_modules/@github/copilot/copilot-sdk/LICENSE.md` §3 — clean-room boundary; we observe our own data, never translate SDK types
