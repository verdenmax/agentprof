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
- [`copilot::CopilotEvent`] — 18-variant event enum (+ `Unknown` fallthrough)
- [`copilot::parser::parse_events_jsonl`] — JSONL → `RawSession<CopilotEvent>`
- [`copilot::paths::discover_sessions`] — walk session-state directory
- [`registry::adapter_for`] — `AgentKind → Option<Adapter>` resolver
- [`registry::supported_agents`] — static list of supported agents

## Modules

| Module | Responsibility |
|---|---|
| `copilot::event` | `CopilotEvent` enum + all payload structs + `impl Event` |
| `copilot::parser` | line-by-line JSONL parser + `MetaBuilder` |
| `copilot::paths` | filesystem discovery + `inuse.<pid>.lock` detection |
| `copilot::adapter` | `impl Adapter for CopilotAdapter` |
| `registry` | `AgentKind` → adapter resolver |

## Supported agents

| Agent | Module | Data source | Status |
|---|---|---|---|
| GitHub Copilot CLI | `copilot` | `~/.copilot/session-state/<uuid>/events.jsonl` | ✅ MVP |
| Anthropic Claude Code | (planned) | `~/.claude/projects/**/*.jsonl` | ⏳ Phase 2 |
| OpenAI Codex CLI | (planned) | (decided at Phase 3) | ⏳ Phase 3 |

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

See `docs/adapters.md` for the contribution guide.

## Variant table for `CopilotEvent`

Authoritative reference: `docs/internals/adr-0002-copilot-event-schema.md`.

## Changelog

See repo-root `CHANGELOG.md`.
