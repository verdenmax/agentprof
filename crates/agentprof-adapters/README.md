# agentprof-adapters

> Per-agent log adapters. Each agent (Claude Code, Codex CLI, Copilot CLI) gets a module that implements the `Adapter` trait defined in [`agentprof-core`](../agentprof-core).

## Position in the agentprof architecture

Depends only on `agentprof-core`. Adapters produce `RawSession` values that the rest of the pipeline consumes. See [`docs/architecture.md`](../../docs/architecture.md) §6 (Adapter trait) and [`docs/adapters.md`](../../docs/adapters.md) (how to add a new agent).

## Public interface

- `claude::ClaudeAdapter`
- `codex::CodexAdapter`
- `copilot::CopilotAdapter`
- `registry::register_default_adapters()`

```rust
// (will become a doctest once the first adapter ships)
// let adapter = agentprof_adapters::claude::ClaudeAdapter::default();
// let sessions = adapter.discover_sessions(&adapter.default_session_root())?;
```

## Modules (planned)

| Module | Purpose |
|---|---|
| `claude` | Parses `~/.claude/projects/**/*.jsonl` |
| `codex` | Parses `~/.codex/sessions/...` |
| `copilot` | Parses `~/.copilot/session-state/` |
| `registry` | Maps `AgentKind` → boxed `Adapter`; supports `--agent auto` |
| `discovery` | Shared helpers (XDG path resolution, glob walking) |

## Adding a new adapter

See [`docs/adapters.md`](../../docs/adapters.md). Required checklist:

1. New module `src/<name>.rs` with `//!` doc and an `impl Adapter`
2. Register in `src/registry.rs`
3. At least one anonymized fixture under `tests/fixtures/<name>/`
4. At least one `assert_cmd` integration test in `agentprof-cli/tests/cli.rs`
5. Documentation: update L1 `docs/architecture.md` §6 (default path) and L2 `docs/adapters.md` (detailed guide)
6. CHANGELOG entry

## Dependencies

- Workspace internal: `agentprof-core`
- External: `serde`, `serde_json`, `thiserror`, `chrono`, `tracing`, `walkdir`, `globset`

## Local commands

```sh
cargo test -p agentprof-adapters
cargo doc  -p agentprof-adapters --no-deps --open
```

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) — entries for this crate are prefixed `adapters:`.
