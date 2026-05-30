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
| `adapter` | `Adapter` trait, `Event` trait (7 methods: `kind` / `id` / `timestamp` / `parent_id` / `payload_name` / `payload_model` / `payload_output_tokens` / `payload_mode`), `EventKind` (29 variants), `AgentKind`, `SessionRef`, `AdapterError` |
| `model` | `RawSession<E>` (含 `parse_warnings: Vec<ParseWarning>`), `SessionMeta`, `ToolSource` + `ToolSource::infer` |
| `error` | `CoreError`, `ParseWarning` (7 variants: `Json` / `Io` / `OutOfOrder` / `UnclosedTurn` / `UnclosedToolCall` / `UnclosedHook` / `UnknownToolSourcePrefix`); derives `PartialEq + Eq` since M1.4 post-output-audit |
| `episode` | `derive_episodes<E>`, `Episodes`, `Turn`, `ToolEpisode`, `HookEpisode`, `SkillEpisode`, `ModeSegment`, `CallRef`, `Mode` (`Interactive` / `Plan` / `Autopilot` / `Unknown(String)`，对齐真实 Copilot wire), `TurnStatus`, `ToolCallStatus`, `DeriveWarning` (5 variants 含 `SynthesizedStart` / `OpenAtEndOfSession` / `AbortWithoutOpenElement` / `NonMonotonicTimestamp` / `PayloadNameMissing`), `pub const ORPHAN_TOOL_SENTINEL = "<orphan>"` |
| `analyzer` | **`analyze(&Episodes, &SessionMeta, &[ParseWarning]) → AnalysisReport`** (3-arg since M1.4 post-output-audit)；`AnalysisReport` 含 `parse_warnings: Vec<ParseWarning>` 字段；`turn_summary` / `tool_rank` / `hook_rank` rollups；`ToolRankRow.is_user_blocking: bool` + `pub const USER_BLOCKING_TOOLS: &[&str] = &["ask_user"]`；`percentile` helper；`duration_ms` / `duration_ms_opt` serde helpers |

## Quick start

```rust,ignore
use agentprof_core::adapter::Adapter;
use agentprof_core::analyzer::analyze;
use agentprof_core::episode::derive_episodes;

// Given an adapter implementation:
//   let adapter = SomeAdapter;
//   let session = adapter.load_session(&sref)?;
//   let episodes = derive_episodes(&session.events, &session.meta);
//   let report = analyze(&episodes, &session.meta, &session.parse_warnings);
//   for w in &report.parse_warnings { /* parser drops, e.g. schema mismatch */ }
//   for w in &report.warnings        { /* derive-time anomalies */ }
//   for row in &report.tool_rank {
//       if row.is_user_blocking { /* ask_user etc. — user think time, not work */ }
//   }
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
| 0005 | Analyzer foundations + payload-* trait extension + commit-call-turn-divergence fix; §6 (post-output-audit) documents the three Copilot CLI 1.0.x schema fixes + `parse_warnings` + `is_user_blocking` + `USER_BLOCKING_TOOLS` |

## Changelog

See the repo-root [`CHANGELOG.md`](../../CHANGELOG.md) — entries for this
crate are prefixed `core:`.
