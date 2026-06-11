# docs/internals — L3 implementation notes and ADRs

This directory holds **L3 documentation** that does not belong in rustdoc:

- Algorithm explanations too long for a `///` block
- Architecture Decision Records (ADRs) — *why* something is the way it is, what
  was considered, what was rejected
- Cross-cutting technical investigations (file format reverse-engineering,
  performance characterizations)

Note: For function- and type-level documentation, **prefer rustdoc** (`///` +
`# Examples` + `# Errors` + `# Panics`). This directory is for material that
genuinely benefits from being separate from source.

## ADR template

Two header styles are both acceptable across `adr-000{1..N}.md`
(workspace review #4 — explicit non-prescription, after evaluating
the cost of mass-migration). Pick whichever fits the topic; the
content rules (Context / Considered options / Decision /
Consequences) are mandatory either way.

**Style A — YAML frontmatter** (used by `adr-0001..0005`):

```markdown
---
title: "ADR-NNNN: <Topic>"
status: "Accepted"
date: "YYYY-MM-DD"
authors: "<list>"
tags: ["..."]
supersedes: ""
superseded_by: ""
---

# ADR-NNNN: <Topic>

## Status
**Accepted**

## Context
What problem are we solving, why now?

## Considered options
1. Option A — pros / cons
2. Option B — pros / cons

## Decision
What was chosen, why.

## Consequences
Benefits, costs, follow-ups, escape hatches.
```

**Style B — bolded-line header** (used by `adr-0006/0008..0012`):

```markdown
# ADR-NNNN — <Topic>

**Status:** Accepted (YYYY-MM-DD, ships with <milestone>).
**Supersedes:** —
**Superseded by:** —
**Owner:** `<crate-name>` crate.

## Context
What problem are we solving, why now?

## Considered options
1. Option A — pros / cons
2. Option B — pros / cons

## Decision
What was chosen, why.

## Consequences
Benefits, costs, follow-ups, escape hatches.
```

Both styles are machine-parseable for table generation. `adr-0007`'s
blockquote variant is a historical artifact and should not be used
for new ADRs — convert to either Style A or B if you touch it for
unrelated reasons. (Not migrating it now for the same reason this
section accepts both styles: cost of churn vs cost of inconsistency.)

See [`docs/architecture.md`](../architecture.md) §14.4 for the full L3 spec.

## Accepted ADRs

ADRs are numbered monotonically (see `.github/copilot-instructions.md` §5.5 for
the gate that triggers a new ADR). Each entry below links to the file plus the
crate(s) it primarily binds.

| # | Topic | Status | Owner(s) |
|---|---|---|---|
| [0001](./adr-0001-events-first-pivot.md) | Events-first MVP pivot (perf flamegraph for AI agents, not a smarter ccusage) | Accepted | product / `core` |
| [0002](./adr-0002-copilot-event-schema.md) | `CopilotEvent` 28-variant clean-room schema from `events.jsonl` observation | Updated 2026-05-27 | `adapters::copilot` |
| [0003](./adr-0003-synthetic-fixture-strategy.md) | Synthetic-only fixture strategy — hand-crafted test data, no anonymizer tool | Accepted | `adapters` / fixtures |
| [0004](./adr-0004-episode-derivation.md) | Episode derivation — lenient single-pass algorithm + orphan synthesis + `DeriveWarning` | Accepted | `core::episode` |
| [0005](./adr-0005-analyzer-and-payload-name.md) | Analyzer foundations — `Event::payload_*()` trait extension + `AnalysisReport` in core | Accepted | `core::analyzer` |
| [0006](./adr-0006-panic-safe-tui.md) | Panic-safe TUI lifecycle (`install_panic_hook` + `enter` / `leave` ordering) | Accepted (M1.5) | `tui` + `cli::cmd::analyze` |
| [0007](./adr-0007-speedscope-export.md) | Speedscope evented format + frame naming + span-overlap adjustment | Accepted (M1.6.4) | `core::export` |
| [0008](./adr-0008-aggregate-report-and-utilization.md) | `AggregateReport<B>` + `utilization_pct` metric for day buckets | Accepted (M1.6.2) | `core::analyzer::aggregate` + `cli::cmd::aggregate` |
| [0009](./adr-0009-watch-runner-and-notify.md) | `WatchRunner` + `notify-debouncer-mini` file watcher | Accepted (M1.6.3) | `tui::watch` + `cli::cmd::watch` |
| [0010](./adr-0010-tracing-infrastructure.md) | Tracing infrastructure (4-layer span topology + reload-Layer TUI auto-redirect + `sha256[..8]` PII hash + global `--log-level` / `--log-file`) | Accepted (M1.6.4) | `core::observability::pii` + `cli::observability` + `adapters::copilot::{parser,paths}` |
| [0011](./adr-0011-turn-detail-and-args-plumbing.md) | Tool arguments plumbing (`Event::payload_tool_requests` + `ToolCall.arguments`) + TurnDetailView state model (full-screen, Enter = drill deeper, vim keys, reload-safe) | Accepted (M1.6.4 follow-up wave Phase 2) | `core::{adapter, episode::tool, episode::derive, analyzer}` + `adapters::copilot::event` + `tui::views::turn_detail` + `tui::app::state` |
| [0012](./adr-0012-session-model-metrics-and-models-view.md) | Session-level model metrics (`Event::payload_model_metrics` + `ModelUsage` struct + `AnalysisReport.model_metrics` + `Episodes.model_metrics`) + Models view (key `4`, sorted by input desc, centered empty-state) | Accepted (M1.6.4 follow-up wave Phase 3) | `core::{adapter, analyzer, episode::derive, episode::episodes}` + `adapters::copilot::event` + `tui::views::models` + `tui::app::state` |
| [0013](./adr-0013-event-success-bit.md) | Wire success bit + error message for `tool.execution_complete` + `hook.end` (`payload_success` / `payload_error_message`) | Accepted (M1.6.4 audit B1) | `core::adapter` + `adapters::copilot::event` + `core::analyzer` |
| [0014](./adr-0014-v0.1.0-release-strategy.md) | v0.1.0 release strategy — `cargo-dist` multi-platform, only `agentprof-cli` publishable, 4 internal libs `publish = false` | Accepted (M1.7) | repo-level (release pipeline) |
| [0015](./adr-0015-mcp-waste-architecture.md) | MCP waste analyzer — ever-loaded semantics + wire/config provenance + `WasteReport`/`AggregateWasteReport` shape | Accepted (M1.6.5) | `core::{model::waste, analyzer::waste}` + `adapters::copilot::{tools_changed, mcp_config}` + `cli::cmd::mcp_waste` |
| [0016](./adr-0016-mcp-token-cost-architecture.md) | MCP tool token-cost — `--tokens-per-tool` heuristic + `--tool-descriptions` sidecar + `WasteComputeContext` builder + `infer_tokenizer` (`cl100k_base` / `o200k_base`) | Accepted (M1.6.6) | `core::analyzer::waste` + `adapters::copilot::tool_sidecar` + `cli::cmd::{analyze, aggregate, mcp_waste, model_hint}` |
| [0017](./adr-0017-unify-session-id-namespace.md) | Unify session-id namespace — adapter `SessionRef.id` = canonical UUID (extracted from first event), not directory name, so dual-path can compare | Accepted (M2.1 hotfix) | `adapters::copilot::paths` + `storage` |
| [0018](./adr-0018-session-datasource-trait.md) | `SessionDataSource` trait + dual-path semantics (adapter-wins on drift, opportunistic re-upsert, `--quiet` suppresses divergence warnings). Footnote links to ADR-0021 for why OTLP is not an `Adapter` | Accepted (M2.1) | `core::datasource` + `adapters::datasource` + `storage::datasource` + `cli` |
| [0019](./adr-0019-hybrid-storage-mode.md) | Hybrid storage mode — default `cache` at `$XDG_CACHE_HOME` with auto-prune, opt-in `store` at `$XDG_DATA_HOME` without auto-prune | Accepted (M2.1) | `storage::config` + `cli::cmd::db` |
| [0020](./adr-0020-aggregate-dualpath.md) | Aggregate dual-path — additive `episodes_json` column (migration 002) + `SessionDataSource::load_episodes` so `aggregate` joins the dual-path read fleet | Accepted (M2.1.1) | `storage::{db,query,upsert}` + `core::datasource` + `cli::cmd::aggregate` |
| [0021](./adr-0021-otlp-receiver-architecture.md) | OTLP receiver architecture — push-mode, per-`session.id` buffering with OOM caps, idle/size/shutdown flush, `StorageFlushSink` reusing `upsert_report`; deliberately **not** an `Adapter` | Accepted (M2.2) | `storage::otlp::*` + `cli::cmd::ingest_otlp` |
| [0022](./adr-0022-otlp-capacity-caps-and-lru-eviction.md) | OTLP capacity caps + LRU eviction — constant-time bearer (`subtle`), per-signal request size caps (8/2/8 MiB), `max_open_sessions = 1024` LRU evict with `CloseReason::CapacityEvict`, 256-byte `session.id` cap in mapper | Accepted (M2.4) | `storage::otlp::{auth, config, server_grpc, server_http, router, mapper, error}` + `cli::cmd::ingest_otlp` |

No ADR is currently superseded. ADR-0011 extends ADR-0004 (the
`derive_episodes` algorithm gains a PASS 0 args-map collection) and
ADR-0005 (uses the `Event::payload_*` extension-method pattern for
the new `payload_tool_requests` method); inherits the red-banner-footer
reload-error UX from ADR-0009 D-13 and uses ADR-0010's
`tracing::debug!` for D-4 conflict detection.

ADR-0012 extends ADR-0004 (derive_episodes gains a `Episodes.model_metrics`
population arm on `EventKind::SessionShutdown`) and ADR-0005 (uses the
`payload_*` pattern for the new `payload_model_metrics` method); inherits
ADR-0011 D-7's centered-placeholder empty-state UX convention for the
"no shutdown event yet" Models view branch.

## Planned files

These are *not yet written*. Each requires its own ADR or algorithm note when
the underlying feature lands.

- `adr-NNNN-waste-formula.md` — derivation of `waste_estimate_usd`. **Note:**
  the MCP-waste-side derivation actually shipped under ADR-0015 (M1.6.5) +
  ADR-0016 (M1.6.6 token-cost); this slot now refers only to a hypothetical
  global `waste_estimate_usd` rollup beyond MCP scope (no concrete owner yet).
- `adr-NNNN-tokenizer-strategy.md` — `cl100k_base` approximation vs Anthropic
  API trade-offs (deferred to Phase 2 tokenizer milestone)
- `adr-NNNN-adapter-wire-format.md` — how each agent serializes tools into the
  prompt (deferred until a second adapter ships, i.e. Phase 3 M3.1)
