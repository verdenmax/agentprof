# agentprof-core

> Foundation library for agentprof. Defines the cross-agent abstractions
> (`Adapter`, `Event`, `EventKind`, `AgentKind`), the unified data model
> (`RawSession`, `SessionMeta`, `ToolSource`), the error taxonomy
> (`CoreError`, `AdapterError`, `ParseWarning`), and the episode aggregation
> layer (`derive_episodes` + `Episodes`).

## In agentprof's architecture

This is the **dependency-graph leaf**: no `agentprof_*` crate is in its
dependency tree. Adapter crates implement the `Adapter` trait defined here.
The CLI, TUI, and storage crates consume `RawSession` + `Episodes`.

See [`docs/architecture.md`](../../docs/architecture.md) §3 for the system
diagram and §5 / §5.1 / §6 for the data model and adapter contract.

## Public interface

| Module | Highlights |
|---|---|
| `adapter` | `Adapter` trait, `Event` trait (4 methods), `EventKind` (29 variants), `AgentKind`, `SessionRef`, `AdapterError` |
| `model` | `RawSession<E>`, `SessionMeta`, `ToolSource` + `ToolSource::infer` |
| `error` | `CoreError`, `ParseWarning` (7 variants: `Json`, `Io`, `OutOfOrder`, `UnclosedTurn`, `UnclosedToolCall`, `UnclosedHook`, `UnknownToolSourcePrefix`) |
| `episode` | `derive_episodes<E>`, `Episodes`, `Turn`, `ToolEpisode`, `HookEpisode`, `SkillEpisode`, `ModeSegment`, `CallRef`, `DeriveWarning` |

## Quick start

```rust,ignore
use agentprof_core::adapter::Adapter;
use agentprof_core::episode::derive_episodes;

// Given an adapter implementation:
//   let adapter = SomeAdapter;
//   let session = adapter.load_session(&sref)?;
//   let episodes = derive_episodes(&session.events, &session.meta);
//   for warning in &episodes.warnings { /* surface */ }
```

## Stability promises

All public extensibility points are `#[non_exhaustive]`:
`AgentKind`, `EventKind`, `AdapterError`, `SessionRef`, `SessionMeta`,
`RawSession`, every `Episode*` type, `Mode`, `TurnStatus`, `ToolCallStatus`,
`DeriveWarning`.

Construct cross-crate via `pub const fn new(...)` constructors. See each
type's rustdoc for the required-fields signature.

## Tests

```sh
cargo test  -p agentprof-core --all-features
cargo doc   -p agentprof-core --no-deps
cargo clippy -p agentprof-core --all-features -- -D warnings
```

## Reference ADRs

| ADR | Topic |
|---|---|
| 0001 | Events-first product pivot |
| 0002 | Copilot event schema (Updated 2026-05-27 for M1.3 Phase B) |
| 0004 | Episode derivation algorithm |

## Changelog

See the repo-root [`CHANGELOG.md`](../../CHANGELOG.md) — entries for this
crate are prefixed `core:`.
