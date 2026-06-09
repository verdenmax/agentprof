# agentprof-adapters

> Per-agent session log adapters for `agentprof`.

## In agentprof's architecture

This crate sits between `agentprof-cli` and `agentprof-core`. Each agent
has its own module here (`copilot`, future `claude`, future `codex`) that
parses agent-specific session logs into the unified
`agentprof_core::model::RawSession<E>` shape, where `E` is the adapter's
native event enum.

See `docs/architecture.md` §3 for the full system diagram and
`docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md`
for the M1.2 design.

## Public interface

- [`copilot::CopilotAdapter`] — GitHub Copilot CLI adapter
- [`copilot::CopilotEvent`] — 28-variant event enum (+ `Unknown` fallthrough)；`impl Event` 提供 10 个 payload-/lookup 方法 (`payload_name` / `payload_model` / `payload_output_tokens` / `payload_mode` / `payload_tool_requests` / `payload_model_metrics` / `tool_call_id` / `payload_success` (B1) / `payload_error_message` (B1) / `payload_loaded_mcp_tools` (M2.1 T5.2.5)) 让 `derive_episodes` / `analyze` 直接读 turn metadata + 失败信号 + ever-loaded MCP 工具集而无需 downcast。`payload_success` 覆盖 `ToolExecComplete` (读 `ToolResultData.success`) + `HookEnd` (读 `HookEndData.success`); `payload_error_message` 仅覆盖 `ToolExecComplete` (读 `ToolError.message`) — Copilot 的 `HookEnd` wire schema 无 error 字段 (ADR-0002 line 93 / ADR-0013 D-6); `payload_loaded_mcp_tools` 覆盖 `UserMessage` (调用 `tools_changed::extract_loaded_set_from_event` 解析 `<tools_changed_notice>` 块，已经 `mcp__*` 过滤) — ADR-0015 D-1/D-2 ever-loaded 语义
- [`copilot::parser::parse_events_jsonl`] — JSONL → `RawSession<CopilotEvent>`（含 `parse_warnings` 收集 + live-mode 末行截断容忍）
- [`copilot::paths::discover_sessions`] — walk session-state directory，按 mtime 倒序
- [`registry::adapter_for`] — `AgentKind → Option<Adapter>` resolver
- [`registry::supported_agents`] — static list of supported agents
- [`AdapterDataSource`] — wraps any [`copilot::CopilotAdapter`] (or future adapter) as an `agentprof_core::datasource::SessionDataSource`, running the `discover → load → derive_episodes → analyze` pipeline inline (M2.1 T3.1)
- [`copilot::tool_sidecar::load_sidecar`] / [`copilot::tool_sidecar::Sidecar`] — optional MCP tool-description sidecar for M1.6.6 token-cost; implements `agentprof_core::analyzer::waste::SidecarLookup` (ADR-0016 D-2)

## Modules

| Module | Responsibility |
|---|---|
| `copilot::event` | `CopilotEvent` enum + all payload structs + `impl Event` (含 5 个 payload-* override) |
| `copilot::parser` | line-by-line JSONL parser + `MetaBuilder` (`adapter.parse` span — M1.6.4) |
| `copilot::paths` | filesystem discovery + `inuse.<pid>.lock` detection (`adapter.discover` span — M1.6.4) |
| `copilot::adapter` | `impl Adapter for CopilotAdapter` |
| `copilot::tools_changed` | parse `<tools_changed_notice>` blocks in `user.message.transformedContent` → ever-loaded MCP tool set (M1.6.5 T2.1; ADR-0015 D-1/D-2) |
| `copilot::mcp_config` | best-effort `~/.copilot/mcp.json` loader → `ParsedMcpConfig` (recognizes VSCode `mcpServers` + self-describing `servers` schemas; degrades to empty on unknown; M1.6.5 T2.2; ADR-0015 spec §6.2) |
| `copilot::tool_sidecar` | optional MCP `tools/list` sidecar loader → `Sidecar` impl of `agentprof_core::analyzer::waste::SidecarLookup`; auto-detects file (global JSON) vs dir (per-server `*.json`, both `{"tools":[…]}` and bare-array shapes); per-file parse failures are skipped with `tracing::warn!` (M1.6.6 T2.1; ADR-0016 D-2) |
| `registry` | `AgentKind` → adapter resolver |
| `datasource` | `AdapterDataSource<A>` — bridge any `Adapter` impl into `agentprof_core::datasource::SessionDataSource`, runs full `discover → load → derive_episodes → analyze` pipeline inline (M2.1 T3.1) |

## Supported agents

| Agent | Module | Data source | Status |
|---|---|---|---|
| GitHub Copilot CLI | `copilot` | `~/.copilot/session-state/<uuid>/events.jsonl` | ✅ M1.2 + multiple iterations |
| Anthropic Claude Code | (planned) | `~/.claude/projects/**/*.jsonl` | ⏳ Phase 3 (M3.1) |
| OpenAI Codex CLI | (planned) | (decided at Phase 3) | ⏳ Phase 3 (M3.2) |

## Copilot CLI 1.0.x schema notes

Three payload fields are intentionally `Option<String>` (not `String`)
because real Copilot CLI 1.0.54 emits multiple wire shapes for the same
event type; making any of these required caused ~17 % of events to
silently drop with serde `"missing field X"` warnings. See
[ADR-0005 §6](../../docs/internals/adr-0005-analyzer-and-payload-name.md#update-6-post-output-audit-fixes-parse-warning-visibility-schema-mismatches-user-blocking-split):

- `HookInput.source: Option<String>` — `postToolUse` hooks have no `source`
  field (only `sessionStart` does).
- `UserMessageData.source: Option<String>` — many CLI-typed prompts omit it.
- `AssistantMessageData.turn_id: Option<String>` — subagent-spawned
  messages (via `subagent.started`) carry `parentToolCallId` instead of
  `turnId`. The new `AssistantMessageData.parent_tool_call_id:
  Option<String>` field captures the alternate shape for subagent
  visibility.

Real-session drop rate after the fix: ~17 % → 0 % (verified on an 11 806-line
live session). Locked in by fixture
`tests/fixtures/copilot/with-post-tool-use-hooks/`.

## Local commands

```bash
# Unit + integration tests
cargo test -p agentprof-adapters

# Lint
cargo clippy -p agentprof-adapters --all-targets --all-features -- -D warnings

# Docs
RUSTDOCFLAGS="-Dwarnings" cargo doc -p agentprof-adapters --no-deps
```

## Local smoke tests

To check the parser against your real local Copilot data without committing
anything (catches schema drift between Copilot CLI versions):

```bash
export AGENTPROF_LOCAL_FIXTURES_DIR=~/.copilot/session-state
cargo test -p agentprof-adapters --test copilot_smoke -- --include-ignored
```

These tests are `#[ignore]` by default; the line above opts in. Output is
informational and never committed.

## Adding a new adapter

See `docs/adapters.md` for the contribution guide. New adapters MUST
mirror the copilot adapter's tracing spans (`adapter.discover` on the
discovery path, `adapter.parse` on the per-session parse path) and
hash any session-state `path` field via
`agentprof_core::observability::pii::hash_path` before attaching it to a
span — see [ADR-0010](../../docs/internals/adr-0010-tracing-infrastructure.md)
(Layer 2 + D-3 / D-13) and
[`docs/features/privacy.md`](../../docs/features/privacy.md) §7.

## Variant table for `CopilotEvent`

Authoritative reference: `docs/internals/adr-0002-copilot-event-schema.md`.

## Changelog

See repo-root `CHANGELOG.md`.
