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
| `adapter` | `Adapter` trait, `Event` trait (13 methods: 4 required — `kind` / `id` / `timestamp` / `parent_id` — plus 4 default-`None` payload-* overrides (`payload_name` / `payload_model` / `payload_output_tokens` / `payload_mode`), 1 default-empty-`Vec` override `payload_tool_requests` (adapter-supplied `(tool_call_id, arguments)` pairs), 1 default-`None` override `tool_call_id` (used by `derive_episodes` PASS 0 args-lookup), 1 default-`None` override `payload_model_metrics` (per-model `BTreeMap<String, ModelUsage>` rollup, F1.7), and 2 default-`None` overrides `payload_success` / `payload_error_message` (B1 — wire success bit + error message for `tool.execution_complete` + `hook.end`); see [ADR-0011](../../docs/internals/adr-0011-turn-detail-and-args-plumbing.md) + [ADR-0012](../../docs/internals/adr-0012-session-model-metrics-and-models-view.md) + [ADR-0013](../../docs/internals/adr-0013-event-success-bit.md)), `EventKind` (29 variants), `AgentKind`, `SessionRef`, `AdapterError` |
| `model` | `RawSession<E>` (含 `parse_warnings: Vec<ParseWarning>`), `SessionMeta`, `ToolSource` + `ToolSource::infer` |
| `model::waste` | M1.6.5 — `WasteReport`, `McpServerWaste`, `McpToolWaste`, `LoadedSource` (`Wire` / `Config` / `Both` / `InferredFromCall`), `WasteDataSource` (`Wire` / `Config` / `Both` / `None`), `AggregateWasteReport`, `McpServerCrossWaste`, `McpToolUsageAcrossSessions`; all `#[non_exhaustive]` to leave room for the M1.6.6 token-cost fields (see [ADR-0015](../../docs/internals/adr-0015-mcp-waste-architecture.md)). **M1.6.6 T1.2**: adds `TokenProvenance` (`Heuristic` / `SidecarExact` / `Mixed`), `TokenSource` (`Heuristic` / `SidecarExact`), `TokenizerKind` (`Cl100kBase` / `O200kBase`); extends `WasteReport` (`total_loaded_tokens`, `total_unused_tokens`, `token_provenance`, `tokenizer`), `McpServerWaste` (`unused_tokens`, `loaded_tokens`), `McpToolWaste` (`description_tokens`, `token_source`), `AggregateWasteReport` / `McpServerCrossWaste` (`total_unused_tokens`) — all `#[serde(default)]` for backward-compat with pre-M1.6.6 JSON (see [ADR-0016](../../docs/internals/adr-0016-mcp-token-cost-architecture.md)) |
| `error` | `CoreError`, `ParseWarning` (7 variants: `Json` / `Io` / `OutOfOrder` / `UnclosedTurn` / `UnclosedToolCall` / `UnclosedHook` / `UnknownToolSourcePrefix`); derives `PartialEq + Eq` since M1.4 post-output-audit |
| `episode` | `derive_episodes<E>`, `Episodes` (含 F1.7 新增的 `model_metrics: Option<BTreeMap<String, ModelUsage>>`，`#[serde(skip_serializing_if = "Option::is_none")]`；由 `derive_episodes` 在 `EventKind::Shutdown` 事件上从 `Event::payload_model_metrics()` 填充，last-wins per ADR-0012 D-6), `Turn`, `ToolEpisode`, `ToolCall` (含 F1 新增的 `arguments: Option<serde_json::Value>`，`#[serde(skip_serializing_if = "Option::is_none")]`；adapter-supplied、由 `derive_episodes` PASS 0 通过 `Event::payload_tool_requests` + `Event::tool_call_id` 配对回填), `HookEpisode`, `SkillEpisode`, `ModeSegment`, `CallRef`, `Mode` (`Interactive` / `Plan` / `Autopilot` / `Unknown(String)`，对齐真实 Copilot wire), `TurnStatus`, `ToolCallStatus`, `DeriveWarning` (5 variants 含 `SynthesizedStart` / `OpenAtEndOfSession` / `AbortWithoutOpenElement` / `NonMonotonicTimestamp` / `PayloadNameMissing`), `pub const ORPHAN_TOOL_SENTINEL = "<orphan>"` |
| `analyzer` | **`analyze(&Episodes, &SessionMeta, &[ParseWarning]) → AnalysisReport`** (3-arg since M1.4 post-output-audit)；`AnalysisReport` 含 `parse_warnings: Vec<ParseWarning>` 字段，以及 F1.7 新增的 `model_metrics: Option<BTreeMap<String, ModelUsage>>`（由 `analyze()` 从 `Episodes.model_metrics` clone，`#[serde(skip_serializing_if = "Option::is_none")]`，自动流向所有 exporter 与 TUI Models view）；`turn_summary` / `tool_rank` / `hook_rank` rollups；`ToolRankRow.is_user_blocking: bool` + `pub const USER_BLOCKING_TOOLS: &[&str] = &["ask_user"]`；`percentile` helper；`duration_ms` / `duration_ms_opt` serde helpers；`ModelUsage` (4 `u64` token fields + `new()` / `total()` saturating sum, `#[non_exhaustive]`, F1.7 foundation for per-model session totals) |
| `analyzer::waste` | M1.6.5 — `compute_waste(&AnalysisReport, &WasteComputeContext) → WasteReport` (per-session pure reducer, **signature switched from `(report, wire, config)` to `(report, ctx)` in M1.6.6 T1.4 — see [ADR-0016](../../docs/internals/adr-0016-mcp-token-cost-architecture.md) D-3**) + `aggregate_waste(&[WasteReport]) → AggregateWasteReport` (cross-session pure reducer). `compute_waste` merges wire-protocol `tools/list` + `mcp.json` baseline into a loaded superset, joins it against `report.tool_rank` to derive the called set, and tags provenance via `LoadedSource::{Wire,Config,Both,InferredFromCall}`; servers sorted by `unused_count desc` / name asc, tools within each server sorted alphabetically by `short_name`. Both functions carry `#[tracing::instrument(name = "analyzer.waste", ...)]` per ADR-0010. **M1.6.6** additions: `WasteComputeContext<'a>` builder (`new(&wire) → with_config(&cfg) → with_sidecar(impl SidecarLookup) → with_heuristic(u64) → with_tokenizer(TokenizerKind)`), `SidecarLookup` trait + `SidecarToolEntry` row, `infer_tokenizer(Option<&str>) → TokenizerKind` (`gpt-5*` / `o1*` / `o3*` → `O200kBase`; else `Cl100kBase`), `compute_token_cost_for_tool(name, sidecar, heuristic, tokenizer) → (u64, TokenSource)`, `pub const DEFAULT_HEURISTIC_TOKENS: u64 = 200`. See spec §6 + [ADR-0015](../../docs/internals/adr-0015-mcp-waste-architecture.md) + [ADR-0016](../../docs/internals/adr-0016-mcp-token-cost-architecture.md) |
| `datasource` | **M2.1 T1.1** — `SessionDataSource` trait (`name` / `discover(Duration) → Vec<SessionRef>` / `load_session(&str) → AnalysisReport`, `Send + Sync`); `SessionRef` lightweight summary (`id` / `agent: AgentKind` / `started_at_ms` / `raw_path` / `raw_mtime_ms` / `source` label, `#[non_exhaustive]`) distinct from `adapter::SessionRef` (the file-discovery row with mandatory `path` / `modified_at` / `size_bytes` / `is_live`); `DataSourceError` (`NotFound` / `Adapter` / `Storage`, `#[non_exhaustive]`, each non-`NotFound` variant carries `source: &'static str` + boxed `dyn Error + Send + Sync`). Symmetric to `adapter::Adapter`: future implementors are the file-adapter wrapper, the `SQLite` store, the OTLP receiver (M2.2), and the cli dual-path composer. ADR-0017 (drafted in M2.1 T8.1) records the decision. |

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

## `analyzer::aggregate` (M1.6.2)

Cross-session aggregation reports.

- [`analyzer::aggregate::AggregateReport<B>`](src/analyzer/aggregate/mod.rs) — generic per-bucket-type report
- [`analyzer::aggregate::AnyAggregateReport`](src/analyzer/aggregate/mod.rs) — serde-tagged outer enum (CLI/serde boundary)
- [`analyzer::aggregate::AggregateKey`](src/analyzer/aggregate/mod.rs) — Tool / McpServer / Day / Model
- 4 bucket types in [`analyzer::aggregate::bucket`](src/analyzer/aggregate/bucket.rs): `ToolBucket`, `McpServerBucket` (M1.6.5: also carries `unused_tool_count` + `fully_unused_session_count`), `DayBucket` (carries `utilization_pct` + `is_low_utilization`), `ModelBucket`
- 4 pure aggregator functions:
  - [`aggregate_by_tool`](src/analyzer/aggregate/group_by_tool.rs)
  - [`aggregate_by_mcp_server`](src/analyzer/aggregate/group_by_mcp.rs) — also takes `&[WasteReport]` (M1.6.5) so server buckets carry per-session waste counters
  - [`aggregate_by_day`](src/analyzer/aggregate/group_by_day.rs)
  - [`aggregate_by_model`](src/analyzer/aggregate/group_by_model.rs)

All aggregators take `&[AnalysisReport]` + `&[Episodes]` (the latter is needed
for per-call duration data used in **percentile recomputation** — averaging
per-session p50s would be statistically wrong).

See [ADR-0008](../../docs/internals/adr-0008-aggregate-report-and-utilization.md) for design decisions.

## `export` (M1.6.4)

Pure data transformations from `&Episodes` / `&SessionMeta` / `&AnalysisReport` into external formats.

- [`export::speedscope`](src/export/speedscope.rs) — emit a Speedscope evented JSON profile (`SpeedscopeProfile`, `to_speedscope()`)
- [`export::svg_flamegraph`](src/export/svg_flamegraph.rs) — render a responsive SVG flamegraph string (`SvgFlamegraph::from_episodes(...).into_svg_string()`)
- [`export::ExportWarning`](src/export/warning.rs) — non-fatal observations (e.g. `SpanAdjustedForSpeedscope`)

All pipelines are pure functions (no IO). The cli crate (`agentprof-cli::cmd::format::{speedscope, html}`) wraps them with serialization + writing + stderr warnings.

See [ADR-0007](../../docs/internals/adr-0007-speedscope-export.md) for design decisions.

## Observability (M1.6.4)

`agentprof_core::observability::pii::{hash_path, hash_short}` are the
canonical PII redaction helpers (sha256[..8] hex). Other workspace crates
emit session paths as `session = %hash_path(p)` in their tracing fields so
log consumers can correlate sessions across runs without seeing raw
filesystem paths.

- [`pii::hash_path`](src/observability/pii.rs) — hash a `&Path` via its
  lossy-UTF-8 string form. Returns an 8-char hex string.
- [`pii::hash_short`](src/observability/pii.rs) — hash an arbitrary `&str`
  to the same 8-char hex form (used for non-path identifiers).

8-char hex (32-bit) gives a small theoretical collision space but is the
right PII / readability trade-off for log-correlation use. See
[ADR-0010 D-5](../../docs/internals/adr-0010-tracing-infrastructure.md)
for the full discussion.

Direct dep: `sha2 = "0.10"` (added M1.6.4).

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
