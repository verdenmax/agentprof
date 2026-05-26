# Design Spec — CopilotAdapter & Event-First MVP Pivot

> **Spec ID**: `2026-05-26-copilot-adapter-event-first-design`
> **Stage**: brainstorming → writing-plans
> **Status**: Draft awaiting user review
> **Author**: AI assistant (Copilot CLI session `252068e5-…`) collaborating with `@verdenmax`
> **Date**: 2026-05-26
> **Affects**: `tasks/001-mvp-agent-token-profiler.md`, `docs/plan.md`, `docs/architecture.md`, all 5 crates
> **Supersedes**: `tasks/001` original M1.2 Claude-first definition
> **Companion ADRs (to be written in Stage 2)**:
> - `docs/internals/adr-0001-events-first-pivot.md`
> - `docs/internals/adr-0002-copilot-event-schema.md`
> - `docs/internals/adr-0003-fixture-strategy.md`

---

## 0. TL;DR

This spec captures a **fundamental pivot** in agentprof's MVP direction, agreed during a brainstorming session on 2026-05-26:

1. **Product repositioning**: `agentprof` = **"perf flamegraph for AI agents"** (event-level observability), **not** "smarter ccusage" (token-cost optimizer).
2. **First-shipping adapter**: `CopilotAdapter` (reads `~/.copilot/session-state/<uuid>/events.jsonl`), not `ClaudeAdapter`.
3. **Data model**: flat event stream (`CopilotEvent` enum, 17 variants 1:1 to wire format) + shared `Episode` derived types (`Turn`, `ToolEpisode`, `HookEpisode`, `SkillEpisode`, `ModeSegment`).
4. **Differentiator**: nobody profiles **what an agent did** at event level (`hook.start` / `skill.invoked` / `tool.user_requested` / mode transitions / abort attribution); incumbents (ccusage / tokscale / splitrail) focus on token cost.
5. **Schema utilization (originally G2)** is **deferred to Phase 2** (when OTel content capture or `.mcp.json` reverse engineering lands).

This spec is the source of truth for `M1.2`–`M1.7` redefinitions.

---

## 1. Background & Why This Pivot

### 1.1 Problem with the original plan

The original `tasks/001-mvp-agent-token-profiler.md` framed agentprof as a token-cost ROI tool focused on `schema_utilization` and "MCP waste estimate in USD". During brainstorming we discovered:

- **Crowded space**: `ccusage` (~60k⭐), `tokscale` (3.2k⭐), `splitrail`, `claude-usage`, `toktrack` all do token cost. ccusage is already in Rust with multi-adapter architecture. We'd be competing in red ocean.
- **Event-level analysis is unique**: No tool today asks "**what did the agent actually do** during that hour?" Hook frequency, skill invocation patterns, tool failure spikes, abort attribution, mode-transition timing — these are blind spots in the market.
- **Copilot CLI's data shape favors events**: `~/.copilot/session-state/<uuid>/events.jsonl` is a rich event stream with `hook.start/end`, `skill.invoked`, `tool.execution_*`, `abort`, `session.mode_changed`, etc. — perfect raw material for event-first profiling. `outputTokens` is present but is one signal of many.

### 1.2 What changed in this brainstorming

| Decision | Original plan | New (this spec) |
|---|---|---|
| Product framing | "花得值不值" (cost ROI) | "agent 在做什么" (event flamegraph) |
| First adapter | Claude (`~/.claude/projects/`) | **Copilot** (`~/.copilot/session-state/`) |
| Primary signal | TokenBucket per turn | **Event stream + derived Episodes** |
| Differentiator G2 | `schema_utilization` | **Deferred to Phase 2** |
| Tokenizer in MVP | Core dependency | **Optional sidebar only** |
| Adapter abstraction | Per-agent `Adapter` trait → unified `RawSession` | **Same**, but `RawSession<E: Event>` with per-agent native Event enums + shared Episode types |

### 1.3 Key alternative considered and rejected

| Alternative | Why rejected |
|---|---|
| **OTel-native adapter (read `~/.copilot/otel/*.jsonl`)** | Copilot CLI does expose first-class OTel export. ccusage uses it. **But** OTel covers ~85% of features and misses Copilot-specific lifecycle events (hook/skill/mode/abort) that are core to event-first positioning. **Keep OTel as Phase 2 addition** for token-cost depth. |
| **Use Copilot SDK** (`@github/copilot-sdk`) | SDK is for **live JSON-RPC control**, not post-mortem JSONL analysis. Embedding Node kills single-binary goal; SDK LICENSE §3 forbids derivative works (can't translate `.d.ts` to Rust). SDK doesn't even fully describe `events.jsonl` — it only declares 8 of the 17+ event types. |
| **Single normalized `Event` enum across agents** | Copilot's `hook.start` doesn't exist in Claude; Claude's `thinking` doesn't map cleanly to Copilot's `reasoningOpaque`. Forcing a least-common-denominator loses information. **Per-agent native Events + shared Episodes** is the right abstraction. |
| **Reverse-engineer Anthropic SDK / Claude Code source** | Claude data not available on this machine yet; user will provide later. MVP doesn't block on it. |

---

## 2. Goals (revised G1–G5)

| # | Goal | Comment |
|---|---|---|
| **G1** | **End-to-end Copilot CLI session analysis**: `agentprof analyze` reads `~/.copilot/session-state/<uuid>/events.jsonl` and produces an event-level report (TUI / md / json / csv / speedscope / html) | Unchanged in shape; data source pivoted |
| **G2** | **Event-level visualization**: a **flamegraph** of `turn → tool → hook → skill` nesting over time, with abort attribution | **REPLACES** original G2 (schema utilization). New differentiator. |
| **G3** | **Cross-session aggregation**: `agentprof aggregate --by tool|hook|skill|mode|day|model` finds patterns (hook noise, tool failure trends, MCP server frequency) | Same intent, data source pivoted to events |
| **G4** | **Offline-first**: nothing leaves the user's machine; no network calls in MVP | Unchanged |
| **G5** | **Multi-format export**: TUI + MD + JSON + CSV + Speedscope + HTML, parity on core fields | Unchanged |
| ~~G2-old~~ | ~~`schema_utilization`~~ | **Deferred to Phase 2** (OTel content capture or `.mcp.json` reverse) |

---

## 3. User Stories (revised)

### US-1: Quick "what did my last session do?"

> As a Copilot CLI heavy user, I want one command that shows me what my latest session **actually did** (which tools, which hooks, where time went), so I can see whether I'm productive or fighting overhead.

**AC**:
- `agentprof analyze` defaults to the latest session in `~/.copilot/session-state/`
- TUI opens to flamegraph view by default
- Status bar shows: N turns · M tool calls · K hooks · A aborts · duration
- First-time invocation < 5s on a 1k-event session

### US-2: Hook noise hunt

> As a user with many hooks/plugins enabled, I want to see which hooks fired the most and took the longest, so I can disable noisy ones.

**AC**:
- `agentprof analyze` → press `k` (hook rank) → sorted table by total duration descending
- Columns: hook name, fires, total ms, p95 ms, fail%

### US-3: Tool failure forensics

> As a user, when my session aborts I want to know which tool was running when abort happened.

**AC**:
- `agentprof analyze --session <id>` shows abort events in red on flamegraph
- Each abort marker has hover/Enter → details panel showing: turn id, tool name, reason, surrounding events

### US-4: Cross-session trends

> As a user, I want to see "across the last 30 days, which tools failed most" so I can spot regressions.

**AC**:
- `agentprof aggregate --by tool --since 30d` outputs table sorted by failure count
- `--export md --out trends.md` writes Markdown to file

### US-5: Live session monitoring

> As a user, I want to watch my **currently running** Copilot session in a TUI that auto-refreshes, like `top` for AI.

**AC**:
- `agentprof watch` detects `inuse.<pid>.lock` and binds to that session
- TUI shows 🟢 LIVE badge; updates every 500ms or on file change
- No false "session ended" rendering until lock file disappears

### US-6: Sharable HTML report

> As a tech-lead, I want a single-file HTML I can paste in Slack/email showing my team's agent usage patterns.

**AC**:
- `agentprof export <session> --format html --out report.html`
- Single file < 500 KB; embedded d3 timeline + tables; opens in any browser

### US-7: First-run config

> As a first-time user, I want a guided setup.

**AC**:
- `agentprof init` detects `~/.copilot/session-state/`; writes `~/.config/agentprof/config.toml` with sensible defaults; prints next steps

### Removed user stories (postponed to Phase 2)

- ~~US: schema utilization explorer~~ (needs tool definitions, deferred)
- ~~US: USD waste estimate~~ (needs tokenizer + pricing table)

---

## 4. Functional Requirements (revised, replaces task 001 §4)

### FR-1: Copilot Adapter

| ID | Requirement | Priority |
|---|---|---|
| FR-1.1 | Implement `Adapter` trait with associated `type Event` | P0 |
| FR-1.2 | `CopilotAdapter::default_session_root()` → `~/.copilot/session-state` (XDG fallback, env override) | P0 |
| FR-1.3 | `discover_sessions(root)` → walkdir max_depth=1, filter to subdirs containing `events.jsonl`, sort by mtime desc | P0 |
| FR-1.4 | `SessionRef.is_live` set via existence of `inuse.<pid>.lock` glob | P0 |
| FR-1.5 | `load_session(sref)` → `RawSession<CopilotEvent>` via line-by-line `serde_json` parse | P0 |
| FR-1.6 | `CopilotEvent` enum: 17 variants tagged `type: <kind>` with `#[serde(other)] Unknown` fallback | P0 |
| FR-1.7 | Each `*Data` payload uses `Option<T>` for fields whose presence is uncertain | P0 |
| FR-1.8 | Parse failures emit `ParseWarning` accumulated in `RawSession.parse_warnings`; **do not abort whole file** | P0 |
| FR-1.9 | Live-session last incomplete line: silent skip if `is_live && looks_like_incomplete_json(line)` | P0 |
| FR-1.10 | Single file parse failure must not abort batch operations (e.g., `list`, `aggregate`) | P1 |

### FR-2: Event → Episode Derivation (replaces old FR-2 Tokenizer)

| ID | Requirement | Priority |
|---|---|---|
| FR-2.1 | `derive_episodes(events, meta) -> Episodes` pure function | P0 |
| FR-2.2 | Turn boundary detection from `assistant.turn_start` / `turn_end` event pairing | P0 |
| FR-2.3 | Tool call boundary detection from `tool.execution_start` + `tool.execution_complete` pairing by `toolCallId` | P0 |
| FR-2.4 | Tool source inference: `mcp__<server>__*` → `Mcp{server}`; `skill__<name>__*` → `Skill{name}`; else `Builtin` | P0 |
| FR-2.5 | Hook boundary detection (`hook.start` + `hook.end`); hooks may nest, use stack semantics | P0 |
| FR-2.6 | Skill → subsequent tool call attribution within same turn / N seconds window | P0 |
| FR-2.7 | Mode segment construction from `session.mode_changed` events | P0 |
| FR-2.8 | Abort attribution: link `abort` event to most recent open Turn/Tool/Hook | P0 |
| FR-2.9 | Shutdown summary extraction from `session.shutdown` (totalApiDurationMs, codeChanges, modelMetrics) | P0 |
| FR-2.10 | Unclosed `<X>` at end of session → `ParseWarning::Unclosed<X>` (except live sessions, which suppress these) | P1 |
| FR-2.11 | Pure determinism: same input → same output, no clock, no random | P1 |

### FR-3: Analyzer Rollups (replaces old FR-3 Analyzer)

| ID | Requirement | Priority |
|---|---|---|
| FR-3.1 | `tool_rank(episodes) -> Vec<ToolRankRow>` (per-tool count / success rate / duration percentiles / user-req ratio / source) | P0 |
| FR-3.2 | `hook_rank(episodes) -> Vec<HookRankRow>` (per-hook count / total duration / p95 / fail rate) | P0 |
| FR-3.3 | `skill_rank(episodes) -> Vec<SkillRankRow>` (per-skill invocations / subsequent tool count) | P0 |
| FR-3.4 | `turn_summary(episodes) -> Vec<TurnSummaryRow>` (per turn: duration / model / output_tokens / tool count / hook count / status) | P0 |
| FR-3.5 | `aggregate(sessions, by: AggregateKey) -> AggregateReport` for cross-session views | P0 |
| FR-3.6 | All algorithms pure; no clock/random | P1 |

### FR-4: TUI views (revised; 5 + 1 view types, replaces old FR-4)

| ID | Requirement | Priority |
|---|---|---|
| FR-4.1 | `AppRunner`: event loop + view switching + **panic-safe** terminal lifecycle (`set_panic_hook` that restores raw mode) | P0 |
| FR-4.2 | `views::flamegraph`: time-axis × nesting-depth canvas with color per event type | P0 |
| FR-4.3 | `views::tool_rank`: sortable / filterable tool ROI table | P0 |
| FR-4.4 | `views::hook_rank`: hook noise table | P0 |
| FR-4.5 | `views::turns`: per-turn breakdown table | P0 |
| FR-4.6 | `views::modes`: timeline of mode segments | P0 |
| FR-4.7 | `views::summary`: shutdown KPIs panel | P1 |
| FR-4.8 | Key bindings: `f/t/k/u/m/s/?/q/r/Enter/Esc/Tab/Shift-Tab/arrows/page` per Section C.1.7 | P0 |
| FR-4.9 | snapshot tests via `ratatui::backend::TestBackend` + `insta` | P1 |
| FR-4.10 | Live indicator: 🟢 LIVE badge if `meta.is_live` | P1 |

### FR-5: CLI (revised; 6+1 subcommands)

| ID | Requirement | Priority |
|---|---|---|
| FR-5.1 | `agentprof analyze [--session <id>\|--latest\|--path <jsonl>] [--agent copilot] [--export <fmt>] [--out <path>]` | P0 |
| FR-5.2 | `agentprof list [--agent copilot] [--since 7d] [--limit 50] [--live-only]` | P0 |
| FR-5.3 | `agentprof aggregate --by <key> [--since 30d] [--export <fmt>] [--out <path>]` | P0 |
| FR-5.4 | `agentprof export <session> --format <fmt> [--out <path>]` | P0 |
| FR-5.5 | `agentprof config [show\|edit\|path\|init]` | P0 |
| FR-5.6 | `agentprof watch [<session-id>]` (tail-f live session) | P0 |
| FR-5.7 | `agentprof init` (first-run setup) | P0 |
| FR-5.8 | `clap` derive + env defaults (`AGENTPROF_*`) | P0 |
| FR-5.9 | Exit codes per `architecture.md §8.1` (0/1/2/3/130) | P0 |
| FR-5.10 | `tracing_subscriber::EnvFilter` with `RUST_LOG=agentprof=info` default; TUI mode redirects logs to `~/.cache/agentprof/log.txt` | P1 |

### FR-6: Exports (revised; 5 formats, replaces old FR-6)

| ID | Requirement | Priority |
|---|---|---|
| FR-6.1 | **Markdown**: GFM tables for toolrank + hookrank + skillrank + turn summary + shutdown KPIs | P0 |
| FR-6.2 | **CSV**: per-`ToolCall` / per-`HookCall` flat dumps with `turn_id` foreign keys | P0 |
| FR-6.3 | **JSON**: `RawSession + Episodes` raw serialization | P0 |
| FR-6.4 | **Speedscope evented profile**: each Episode → Frame; loadable at https://speedscope.app | P0 |
| FR-6.5 | **HTML**: askama compile-time template + embedded d3.js timeline + tables; single-file output | P0 |
| FR-6.6 | Key fields must match across 5 formats (snapshot-tested) | P1 |

### FR-7: Config & Storage (revised; storage stays Phase 2 stub)

| ID | Requirement | Priority |
|---|---|---|
| FR-7.1 | `~/.config/agentprof/config.toml` via `directories` crate (XDG-aware) | P0 |
| FR-7.2 | Config sections: `[paths]`, `[display]`, `[analysis]` (no `[pricing]`/`[tokenizer]` in MVP) | P0 |
| FR-7.3 | Resolution order: CLI flag > env (`AGENTPROF_*`) > config file > built-in default | P0 |
| FR-7.4 | SQLite schema in `agentprof-storage` stays Phase 2 (only stub crate exists) | P2 |
| FR-7.5 | OTLP receiver stays Phase 2 (`otlp` feature flag exists, not wired) | P2 |

---

## 5. Non-Goals (MVP excludes)

| # | Excluded | Future Phase |
|---|---|---|
| NG-1 | **`schema_utilization` differentiator** | Phase 2 (OTel content capture / `.mcp.json` reverse) |
| NG-2 | `agentprof ingest-otlp` (OTLP receiver) | Phase 2 |
| NG-3 | `ClaudeAdapter` (`~/.claude/projects/`) | Phase 2 (when user provides Claude session data) or later |
| NG-4 | `CodexAdapter`, `GeminiAdapter`, others | Phase 3 |
| NG-5 | Web dashboard / persistent service / multi-user | Not planned |
| NG-6 | Token cost USD calculation | Phase 2 (when tokenizer + pricing land) |
| NG-7 | Tool schema utilization analytics | Phase 2 |
| NG-8 | Modify session files (write back / cleanup) | Never |
| NG-9 | Real-time hook / API interception | Never (LiteLLM/Helicone's territory) |
| NG-10 | Automatic `.mcp.json` editing suggestions | Phase 3+ |
| NG-11 | Authenticated content capture / telemetry uploads | Never (privacy-first) |
| NG-12 | Pricing table sync | Phase 3 |

---

## 6. Design Considerations

### 6.1 Architecture & dependency graph (unchanged from M1.1)

```
agentprof-cli  ──▶  agentprof-tui
       │                │
       ├──────────────▶ agentprof-adapters ──▶ agentprof-core
       │                                          ▲
       └──▶ agentprof-storage ───────────────────┘
```

`agentprof-core` remains the leaf (no workspace deps). Per `docs/architecture.md §3.1`.

### 6.2 `agentprof-core` revised module layout

```
agentprof-core/src/
├── lib.rs
├── error.rs                  CoreError (thiserror)
├── adapter.rs                trait Adapter, trait Event, AgentKind enum, SessionRef
├── model/
│   ├── session.rs            RawSession<E: Event>, ParseWarning
│   ├── meta.rs               SessionMeta
│   └── tool_source.rs        ToolSource enum (Builtin | Mcp{server} | Skill{name})
├── episode/                  shared derived types (multi-agent reusable)
│   ├── mod.rs
│   ├── span.rs               Span { start, end }
│   ├── turn.rs               Turn, AbortInfo
│   ├── tool.rs               ToolEpisode, ToolCall
│   ├── hook.rs               HookEpisode, HookCall
│   ├── skill.rs              SkillEpisode, SkillInvocation
│   ├── mode_segment.rs       ModeSegment, Mode enum
│   ├── episodes.rs           Episodes rollup container
│   └── derive.rs             derive_episodes<E: Event>(events, meta) -> Episodes
├── analyzer/                 rollup algorithms (consume Episodes, agent-agnostic)
│   ├── tool_rank.rs
│   ├── hook_rank.rs
│   ├── skill_rank.rs
│   ├── turn_summary.rs
│   ├── mode_summary.rs
│   └── aggregate.rs          cross-session
└── export/                   format-specific serializers
    ├── markdown.rs
    ├── csv.rs
    ├── json.rs
    ├── speedscope.rs
    └── html.rs               (askama compile-time templates)
```

### 6.3 `agentprof-adapters` revised module layout

```
agentprof-adapters/src/
├── lib.rs
├── error.rs                  AdapterError (thiserror)
├── registry.rs               register_default_adapters() -> HashMap<AgentKind, Box<dyn AnyAdapter>>
└── copilot/
    ├── mod.rs
    ├── event.rs              CopilotEvent enum (17 variants) + payload structs
    ├── parser.rs             parse_events_jsonl, MetaBuilder
    ├── paths.rs              discovery, XDG resolution, inuse.lock detection
    └── adapter.rs            impl Adapter for CopilotAdapter
```

Future: `claude/`, `codex/` sibling directories with the same shape.

### 6.4 `CopilotEvent` enum — full wire-format coverage

Variants observed from real session data (mix of older `2fcbfbca-…` and current-session `252068e5-…`):

| Variant | `serde(rename)` | Wire-format payload fields (observed in real events.jsonl) |
|---|---|---|
| `SessionStart` | `session.start` | `sessionId, version, producer, copilotVersion, startTime, context{cwd, gitRoot, branch, headCommit, repository, hostType}, alreadyInUse` |
| `SessionInfo` | `session.info` | `infoType, message` |
| `ModeChanged` | `session.mode_changed` | `previousMode, newMode` |
| `ModelChange` | `session.model_change` | `newModel` (note: no `previousModel`; derive previous from prior `ModelChange`/`AssistantMessage.model`) |
| `PlanChanged` | `session.plan_changed` | `operation` |
| `Shutdown` | `session.shutdown` | `shutdownType, totalPremiumRequests, totalApiDurationMs, sessionStartTime, codeChanges{linesAdded, linesRemoved, filesModified[]}, modelMetrics{<model>: {requests, usage}}, currentModel` |
| `UserMessage` | `user.message` | `content, transformedContent, source, attachments[], interactionId` |
| `TurnStart` | `assistant.turn_start` | `turnId, interactionId` |
| `AssistantMessage` | `assistant.message` | `messageId, model, content, toolRequests[]{toolCallId, name, arguments, type, intentionSummary?}, interactionId, turnId, reasoningOpaque?, reasoningText?, encryptedContent?, outputTokens, requestId?, serviceRequestId?` |
| `TurnEnd` | `assistant.turn_end` | `turnId` |
| `ToolExecStart` | `tool.execution_start` | `toolCallId, toolName, arguments{...tool-specific}` |
| `ToolExecComplete` | `tool.execution_complete` | `toolCallId, model, interactionId, turnId?, success, result{content, detailedContent}, toolTelemetry{properties{command, options?, inputs?, fileExtension?, ...}, metrics{resultLength?, resultForLlmLength?, responseTokenLimit?}, restrictedProperties}, error?` |
| `ToolUserRequested` | `tool.user_requested` | `toolCallId, toolName, arguments{command, description}` |
| `HookStart` | `hook.start` | `hookInvocationId, hookType, input{sessionId, timestamp /* unix ms */, cwd, source, initialPrompt?, ...hook-kind-specific}` |
| `HookEnd` | `hook.end` | `hookInvocationId, hookType, output?{additionalContext?, ...}, success` |
| `SkillInvoked` | `skill.invoked` | `name, path, content, source ("plugin"\|"project"\|"builtin"), pluginName?, pluginVersion?, description, trigger ("agent-invoked"\|"user-invoked"\|...)` |
| `SystemMessage` | `system.message` | `role ("system"), content` |
| `Abort` | `abort` | `reason` |
| `Unknown` | (`#[serde(other)]`) | — (forward compat for future Copilot event types) |

Each variant wrapped in:
```rust
pub struct WithEnvelope<D> {
    pub id: String,                               // event UUID
    pub timestamp: chrono::DateTime<Utc>,         // outer ISO-8601 timestamp (NOT to be confused with hook.start.input.timestamp which is unix ms)
    pub parent_id: Option<String>,                // chains events into a parent-child tree
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,                  // some events (e.g. logs) marked transient
    pub data: D,                                  // variant-specific payload
}
```

**Notes on observed wire format**:

- All payload string fields use `Option<T>` in our enum when uncertain; missing in real data → `None`.
- `hook.start.input.initialPrompt` (when present) contains the **user prompt** — privacy-relevant for fixtures (synthetic-only mitigates this).
- `hook.start.input.timestamp` is Unix epoch ms (not ISO-8601); only the outer `timestamp` is ISO.
- `skill.invoked.data.content` is the **full skill text** (~10KB); for fixtures we will use short synthetic skill text.
- `assistant.message.data.model` and `.turnId` exist alongside `interactionId`; helpful for cross-referencing.
- `tool.execution_complete.data.toolTelemetry.metrics` provides token-ish metrics (`resultLength`, `resultForLlmLength`, `responseTokenLimit`); these are byte/char counts not LLM tokens — we record them as `result_size` in `ToolCall` for now.
- The `parentId` chain forms a DAG that mirrors the trace tree; useful as an alternative to timestamp-pairing for matching `*_start` ↔ `*_end`.

### 6.5 `Episodes` derived types — shared across agents

```rust
pub struct Span { pub start: DateTime<Utc>, pub end: DateTime<Utc> }

pub struct Turn {
    pub turn_id: String,
    pub span: Span,
    pub model: Option<String>,
    pub mode_at_start: Mode,
    pub output_tokens: Option<u32>,
    pub tool_call_indices: Vec<usize>,
    pub hook_call_indices: Vec<usize>,
    pub aborted: Option<AbortInfo>,
}

pub struct ToolEpisode {
    pub name: String,
    pub source: ToolSource,
    pub calls: Vec<ToolCall>,
    pub total_duration: Duration,
    pub success_count: u32,
    pub failure_count: u32,
}

pub struct ToolCall {
    pub call_id: String,
    pub span: Span,
    pub turn_id: Option<String>,
    pub user_requested: bool,
    pub success: Option<bool>,
    pub result_size: Option<usize>,
}

pub struct HookEpisode { pub name: String, pub calls: Vec<HookCall>, pub total_duration: Duration, pub failure_count: u32 }
pub struct HookCall { pub span: Span, pub turn_id: Option<String>, pub success: bool }

pub struct SkillEpisode { pub name: String, pub invocations: Vec<SkillInvocation>, pub subsequent_tool_calls: u32 }
pub struct SkillInvocation { pub timestamp: DateTime<Utc>, pub turn_id: Option<String>, pub triggered_tools: Vec<usize> }

pub struct ModeSegment { pub mode: Mode, pub span: Span, pub turns_in_segment: u32 }
pub enum Mode { Interactive, Plan, Autopilot, #[non_exhaustive] Other(String) }

pub struct Episodes {
    pub turns: Vec<Turn>,
    pub tools: BTreeMap<String, ToolEpisode>,
    pub hooks: BTreeMap<String, HookEpisode>,
    pub skills: BTreeMap<String, SkillEpisode>,
    pub mode_segments: Vec<ModeSegment>,
    pub aborts: Vec<AbortInfo>,
    pub shutdown_summary: Option<ShutdownSummary>,
}
```

All types `#[non_exhaustive]` to allow non-breaking field additions.

---

## 7. Algorithms (detail of derive_episodes)

### 7.1 State machine

```rust
struct DeriveState {
    open_turns: HashMap<String /* turn_id */, OpenTurn>,
    open_tool_calls: HashMap<String /* tool_call_id */, OpenToolCall>,
    open_hook_stack: Vec<OpenHook>,           // hooks may nest
    current_mode: Mode,
    current_model: Option<String>,
    pending_abort: Option<AbortInfo>,
    tools: BTreeMap<String, ToolEpisode>,
    hooks: BTreeMap<String, HookEpisode>,
    skills: BTreeMap<String, SkillEpisode>,
    mode_segments: Vec<ModeSegment>,
    aborts: Vec<AbortInfo>,
    shutdown_summary: Option<ShutdownSummary>,
}

fn derive_episodes<E: Event>(events: &[E], meta: &SessionMeta) -> Episodes {
    let mut s = DeriveState::default();
    for (idx, event) in events.iter().enumerate() {
        match event.kind() {
            EventKind::TurnStart       => s.open_turn(idx, event),
            EventKind::AssistantMessage => s.record_assistant_message(idx, event),
            EventKind::TurnEnd         => s.close_turn(idx, event),
            EventKind::ToolExecStart   => s.open_tool_call(idx, event, /* user_requested */ false),
            EventKind::ToolUserRequested => s.open_tool_call(idx, event, true),
            EventKind::ToolExecComplete => s.close_tool_call(idx, event),
            EventKind::HookStart       => s.push_hook(idx, event),
            EventKind::HookEnd         => s.pop_hook(idx, event),
            EventKind::SkillInvoked    => s.record_skill(idx, event),
            EventKind::ModeChanged     => s.transition_mode(idx, event),
            EventKind::ModelChange     => s.switch_model(idx, event),
            EventKind::Abort           => s.attribute_abort(idx, event),
            EventKind::Shutdown        => s.finalize(idx, event),
            _ => {}                   // SessionStart/Info/UserMessage/SystemMessage/PlanChanged/Unknown: no-op
        }
    }
    s.into_episodes()
}
```

### 7.2 Abort attribution

When `Abort` event fires:
1. Look at all currently open elements (`open_turns`, `open_tool_calls`, `open_hook_stack`).
2. Pick the most-recently-opened element (max by `started_at` timestamp).
3. Mark that element's resulting Episode with `aborted = Some(AbortInfo{reason, at})`.
4. If nothing is open, attach AbortInfo to `Episodes.aborts` at session level.

### 7.3 Tool source inference

```rust
fn infer_tool_source(name: &str) -> ToolSource {
    if let Some(rest) = name.strip_prefix("mcp__") {
        if let Some((server, _tool)) = rest.split_once("__") {
            return ToolSource::Mcp { server: server.to_string() };
        }
    }
    if let Some(rest) = name.strip_prefix("skill__") {
        if let Some((skill, _)) = rest.split_once("__") {
            return ToolSource::Skill { name: skill.to_string() };
        }
    }
    ToolSource::Builtin
}
```

If neither prefix matches but the name looks suspicious (contains `__`), emit `ParseWarning::UnknownToolSourcePrefix`.

### 7.4 Skill → tool attribution

For each `SkillInvocation`:
1. Take the timestamp `t`.
2. Walk events with `timestamp > t` until next `SkillInvocation` (or `t + 60s`, whichever comes first).
3. Collect indices of `ToolCall` opened in that window into `triggered_tools`.
4. Add to `SkillEpisode.subsequent_tool_calls`.

### 7.5 Live-session handling

If `meta.is_live == true`:
- Skip emitting `Unclosed*` warnings (file still being written).
- `shutdown_summary = None` regardless of whether a Shutdown event appears.
- Last events.jsonl line that fails to parse with `looks_like_incomplete_json` → silent skip, not a warning.
- `looks_like_incomplete_json(line: &str) -> bool` heuristic: `serde_json` parse error AND (line doesn't end with `}` OR brace count imbalanced).

### 7.6 Complexity

| Operation | Time | Space |
|---|---|---|
| `discover_sessions(root)` | O(N_sessions) | O(N_sessions) |
| `load_session(path)` | O(N_lines × avg_line_len), streaming | O(N_events × ~200B) |
| `derive_episodes(events, meta)` | O(N_events × log N_episodes) | O(N_episodes) |
| Full `analyze` (1 session, 1k events) | < 50ms | < 5MB |
| Full `aggregate` (30 sessions × 1k events) | < 1s | < 50MB |

---

## 8. User Interface (TUI + CLI)

### 8.1 TUI views and key bindings

(See Section C of brainstorming summary; reproduced here for spec completeness.)

**Layout**:
```
┌─ agentprof analyze <session-id>  ─────────────── 🟢 LIVE (if applicable) ──┐
│ [F]lamegraph  [T]ools  [H]ooks  [U]turns  [M]odes  [S]ummary  [?]help  [Q]uit│
├─────────────────────────────────────────────────────────────────────────────┤
│                          <current view content>                              │
├─────────────────────────────────────────────────────────────────────────────┤
│ status bar: 27 turns · 35 tool calls · 33 hooks · 4 aborts · 14.2 min        │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Views**:
- `flamegraph` (default): x=time, y=nesting depth, colored by event type
- `tool_rank`: tool ROI table
- `hook_rank`: hook noise table
- `turns`: per-turn breakdown
- `modes`: mode segments timeline
- `summary`: shutdown KPI panel

**Key bindings**: see brainstorming Section C.1.7. `q` quit, `?` help, `f/t/k/u/m/s` switch view, `Enter` expand, `Esc` close, `/` filter, arrows navigate.

**Panic safety**: `AppRunner::install_panic_hook()` restores terminal raw mode before re-emitting any panic. Required by `architecture.md §16` rule 11.

### 8.2 CLI

| Command | Purpose | Key flags |
|---|---|---|
| `agentprof analyze` | Analyze one session, show report | `--session <id>`, `--latest`, `--path <jsonl>`, `--agent copilot`, `--export <fmt>`, `--out <path>` |
| `agentprof list` | List sessions | `--since 7d`, `--limit 50`, `--live-only` |
| `agentprof aggregate` | Cross-session aggregate | `--by <key>`, `--since 30d`, `--export <fmt>`, `--out <path>` |
| `agentprof export` | Export pre-analyzed report | `<session>`, `--format <fmt>`, `--out <path>` |
| `agentprof config` | Manage config | `show|edit|path|init` |
| `agentprof watch` | Tail-f live session | `<session-id>` (default: latest live) |
| `agentprof init` | First-run setup | (none) |

Exit codes per `architecture.md §8.1`: 0 success / 1 user err / 2 data err / 3 external err / 130 SIGINT.

### 8.3 Config file (`~/.config/agentprof/config.toml`)

```toml
[paths]
copilot_root = "~/.copilot/session-state"

[display]
default_view = "flamegraph"
theme = "dark"
mouse = true

[analysis]
include_unknown_events = false
warning_threshold = 5
```

Resolution: CLI flag > env (`AGENTPROF_*`) > config file > built-in default.

---

## 9. Testing Strategy

### 9.1 Fixture catalog (synthetic-only, all committed)

`crates/agentprof-adapters/tests/fixtures/copilot/`:

| Fixture | Scenario | Approx size |
|---|---|---|
| `minimal/` | Smallest valid: session.start → user.message → turn_start/end → shutdown | 5 events |
| `builtin-tools-only/` | Only bash + str_replace_editor; no MCP / skill / hook | 30 events |
| `with-mcp-calls/` | Several `mcp__github__*`, `mcp__filesystem__*` calls | 40 events |
| `with-skill-invoked/` | `skill.invoked` event with subsequent tool calls | 30 events |
| `with-hooks-heavy/` | 30+ `hook.start`/`hook.end` pairs | 80 events |
| `with-aborts/` | 3 abort events at different points (during tool, hook, turn) | 50 events |
| `with-mode-transitions/` | Interactive → Plan → Autopilot transitions | 40 events |
| `live-truncated/` | No shutdown, has `inuse.lock`, last line incomplete | 25 events |
| `corrupt/` | 1 broken JSON line, rest valid | 20 events |

Each fixture includes:
- `events.jsonl` (hand-crafted JSONL, each line independently parseable except where intentionally broken)
- `expected.json` (Episodes serialization for snapshot test)
- `README.md` notes inside the fixture dir (purpose / what it asserts)

### 9.2 Test layers

| Layer | Tool | What it covers |
|---|---|---|
| Unit (`#[cfg(test)] mod tests`) | `assert_eq!` + property tests where useful | Pure functions: `infer_tool_source`, `looks_like_incomplete_json`, individual `DeriveState` methods |
| Adapter integration (`crates/agentprof-adapters/tests/copilot_*.rs`) | `assert_cmd`-free direct calls | Load each fixture → assert event counts / `parse_warnings` |
| Episode derivation snapshot (`crates/agentprof-core/tests/episode_*.rs`) | `insta` | For each fixture: `derive_episodes` → serialize → match `expected.json` |
| CLI integration (`crates/agentprof-cli/tests/cli.rs`) | `assert_cmd` + `predicates` | `agentprof analyze --path fixtures/<f>/events.jsonl --export md` → exit 0 + stdout contains expected markdown |
| TUI snapshot (`crates/agentprof-tui/tests/views.rs`) | `ratatui::backend::TestBackend` + `insta` | Render each view → lock ASCII layout |
| Exports (`crates/agentprof-core/tests/export_*.rs`) | `insta` | Each fixture × each format → snapshot |
| Benchmark (`benches/`) | `criterion` | Performance regressions (parse, derive, render) |

### 9.3 Local-data smoke tests (optional, `#[ignore]`)

Developers may set `AGENTPROF_LOCAL_FIXTURES_DIR=~/.copilot/session-state/` and run:
```bash
cargo test -p agentprof-adapters --test smoke -- --include-ignored
```

These tests load **real local sessions** to catch schema drift but never commit results. The directory itself is in `.gitignore`.

### 9.4 CI gates (all required)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace
cargo deny check
```

All listed in `architecture.md §8`. No changes to this list from the pivot.

---

## 10. Documentation Sync Impact

### 10.1 L1 documents — must update

| File | Changes |
|---|---|
| `docs/plan.md` | §1 one-liner; §3 market comparison (ccusage moves from "compete" to "complement"); §5 roadmap (Phase 0/1 redefined); G2 `schema_utilization` deferred Phase 2 |
| `docs/architecture.md` | §1 one-liner; §3 data flow (events first-class); §6 data model (`Turn`/`TokenBucket` → `Event`/`Episode`); §7 dataflow constraints; §14 add ADR-NNNN-events-first reference |
| `tasks/001-mvp-agent-token-profiler.md` | **Rename** → `tasks/001-mvp-agent-event-profiler.md` (preserving commit history via `git mv`); rewrite M1.2–M1.7 to event-first; rewrite FR-1..7 to match this spec §4; rewrite G1–G5 to match §2 |
| `tasks/ROADMAP.md` | §2.2 current position; §3 task index status; §7 Phase milestones |
| `README.md` | Quickstart switched to Copilot CLI flow; add `agentprof init` step |
| `CHANGELOG.md` | `[Unreleased]` entry: `BREAKING: refocus MVP from token-cost analysis to event-level agent profiling` |

### 10.2 L2 documents

| File | Changes |
|---|---|
| `crates/agentprof-core/README.md` | Module table replaced (add `episode/`, revise `analyzer/`, drop `tokenizer/` from MVP scope) |
| `crates/agentprof-adapters/README.md` | "Supported agents" updated to Copilot as first, Claude marked future |
| `crates/agentprof-tui/README.md` | 5+1 views replacing original 3 |
| `crates/agentprof-cli/README.md` | 6+1 subcommands (`init` is new) |
| `crates/agentprof-storage/README.md` | No change (storage stays Phase 2 stub) |
| `docs/adapters.md` | Becomes "writing a Copilot adapter" guide; Claude marked Phase 2 |
| `xtask/README.md` | No `anonymize` command (synthetic-only); xtask scope shrinks for MVP |

### 10.3 L3 documents to produce

| File | Purpose |
|---|---|
| `docs/internals/adr-0001-events-first-pivot.md` | Captures **this brainstorming**: positioning change, why event-first beats token-first, OTel/SDK/ccusage research, rejected alternatives |
| `docs/internals/adr-0002-copilot-event-schema.md` | Wire-format observation notes; 17 event types; field-by-field reference; live vs closed differences |
| `docs/internals/adr-0003-fixture-strategy.md` | Why synthetic-only; xtask anonymize not built; tradeoffs |
| `docs/internals/copilot-events-jsonl-schema.md` | Per-event-type field inventory (the "L3 reverse-engineering doc") |

### 10.4 What does NOT change

- `.github/copilot-instructions.md` (9-stage pipeline unchanged)
- `.github/instructions/*.instructions.md` (Stage 0 always-on rules unchanged)
- `.github/skills/*` (5 vendored skills unchanged)
- `.github/workflows/*` (CI workflows unchanged)
- `Cargo.toml` workspace structure (crates unchanged)
- License (`MIT OR Apache-2.0`)
- MSRV (`1.78`)
- CI gates (`cargo fmt/clippy/test/doc/deny`)

### 10.5 Doc sync schedule (within milestones)

| Milestone | Docs updated |
|---|---|
| M1.2 (CopilotAdapter) | `docs/adapters.md` + `adr-0002` + `crates/agentprof-adapters/README.md` + `crates/agentprof-adapters/src/copilot/mod.rs` `//!` |
| M1.3 (Episode derivation) | `adr-0001` + `adr-0003` + `crates/agentprof-core/README.md` (module table) + Episode type rustdoc with `# Examples` |
| M1.4 (`analyze` + md export) | Root `README.md` quickstart + `crates/agentprof-cli/README.md` + `tasks/001` status update for M1.4 |
| M1.5 (TUI) | `crates/agentprof-tui/README.md` |
| M1.6 (list/aggregate/export) | `crates/agentprof-cli/README.md` (more commands) + `docs/features/html-report.md` |
| M1.7 (E2E + release) | **Full L1 sync**: `plan.md` / `architecture.md` / `ROADMAP.md` / rename `tasks/001` / `CHANGELOG.md` `[Unreleased]` → `[v0.1.0]` |

---

## 11. Open Questions (to resolve before/during writing-plans)

| # | Question | Suggested resolution path |
|---|---|---|
| OQ-1 | Tool-call ↔ Turn linkage strategy: use `interactionId` / `turnId` (in event payload) or rebuild from open/close pairing? | Both — payload IDs as primary, timestamp-pairing as fallback for legacy fixtures lacking IDs |
| OQ-2 | Does Copilot ever emit events out-of-order vs file order? | Add assertion in parser; if violations seen in smoke tests, change `is_monotonic` warning to "informational" only |
| OQ-3 | How to color-code 17 event types in flamegraph (limited terminal palette) | Group: turn/assistant.message=blue family; tool=green family; hook=yellow family; skill=purple; abort=red; mode background tint |
| OQ-4 | Should `agentprof watch` block forever or auto-exit on inuse.lock removal | Default: auto-exit on lock removal; `--keep-open` to stay |
| OQ-5 | Should `aggregate --by model` use raw model names or canonicalize (`claude-sonnet-4-5` ↔ `claude-sonnet-4.5`)? | Canonicalize via simple lowercase/dot-replace; document in rustdoc |
| OQ-6 | Live session snapshot — refresh interval | 500ms default; `--refresh-ms <N>` flag |
| OQ-7 | HTML export d3 version + bundling strategy | Embed as base64 at compile time; cap at d3 v7 LTS to limit size |
| OQ-8 | When Claude data arrives, do we extend `tasks/001` MVP scope or start `tasks/002`? | **Decide when Claude data arrives**, depending on how much M1.x is already done |
| OQ-9 | `tool.execution_complete.toolTelemetry.metrics.resultLength` — is this bytes or chars or tokens? Observed value matches `result.content.len()` (chars). | Document as `result_size: usize` (char count) in `ToolCall`; do **not** rename to `tokens` to avoid confusion |
| OQ-10 | `hook.start.input.initialPrompt` privacy — synthetic fixtures should NOT include real prompts | Use synthetic prompt placeholder in fixture (e.g., `"[fixture-prompt-1]"`); document in fixture README |

---

## 12. Implementation Path (preview; full plan in writing-plans)

```
M1.2  CopilotAdapter      (FR-1, FR-2 partial)
       ↓
M1.3  Episode derive       (FR-2 finish, FR-3 begin)
       ↓
M1.4  analyze CLI + md     (FR-3, FR-5.1, FR-6.1, FR-6.3) [Phase 0 exit]
       ↓
       ┌────┬────┐
       ↓    ↓    ↓
M1.5 TUI  M1.6 list/agg/export/watch  M1.6 speedscope+html
       ↓ ↙
M1.7  E2E + L1 docs sync + v0.1.0 release
```

Total estimate: 21 working days (~3 weeks elapsed at 7 days/week pace).

---

## 13. Change Log (this spec)

| Date | Change | Author |
|---|---|---|
| 2026-05-26 | Initial draft after brainstorming Stage 1 | AI assistant + @verdenmax |
