# docs/features — L2 cross-crate feature docs

This directory holds **L2 documentation** for features that span multiple crates
(e.g. "OTLP receiver", "HTML report", "Tool ROI matrix").

Single-crate documentation lives in that crate's `README.md` instead
(e.g. `crates/agentprof-core/README.md`).

## When to create a file here

Create `docs/features/<feature>.md` when a feature touches **two or more** crates
and a single per-crate README would not capture the whole picture. Typical
contents:

- One-line definition
- Motivation (link back to `docs/plan.md` or relevant ADR in `docs/internals/`)
- Crates involved + their roles
- User-facing surface (CLI flags, config keys, environment variables)
- Data flow
- Failure modes
- Test plan
- Links to the rustdoc anchors (L3) that contain implementation details

See [`docs/architecture.md`](../architecture.md) §14 for the full L1/L2/L3
documentation system.

## Current files

| File | Purpose |
|---|---|
| [`privacy.md`](./privacy.md) | PII tier table for report/list/log output + shipped `--privacy <none\|redact\|anonymize>` behavior. Touches `agentprof-core::analyzer::redact` and CLI render/log surfaces. |
| [`web-dashboard.md`](./web-dashboard.md) | L2 cross-crate guide for `agentprof serve`, its store-backed dashboard routes, config, feature gate, and tests. |

## Planned files

- `tool-roi-matrix.md` — analyzer + tui + cli integration (post-MVP)

## Shipped without a dedicated L2 feature doc

These features touch multiple crates but were captured fully in a single ADR +
the affected crate READMEs, so no separate `docs/features/*.md` was needed.

- **OTLP receiver** (`agentprof ingest-otlp`, feature `otlp`, M2.2 ✅ /
  M2.4 ✅ hardened) — covered by
  [ADR-0021](../internals/adr-0021-otlp-receiver-architecture.md)
  (architecture — gRPC + HTTP transports, per-`session.id` buffering,
  flush triggers, `StorageFlushSink → upsert_report`, deliberately
  not an `Adapter`) + [ADR-0022](../internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md)
  (capacity hardening — constant-time bearer, per-signal request caps,
  LRU session eviction, 256-byte `session.id` cap) plus the L2
  surfaces in
  [`crates/agentprof-storage/README.md`](../../crates/agentprof-storage/README.md)
  (otlp module table row, `[otlp]` config block) and
  [`crates/agentprof-cli/README.md`](../../crates/agentprof-cli/README.md)
  (`ingest-otlp` flag table). L1: `docs/architecture.md` §7 (dataflow)
  + §8 (CLI) + §10 (config) + §15.4 (feature flags).
- **HTML report** (`agentprof analyze --export html`, M1.6.4) — covered by
  [ADR-0007](../internals/adr-0007-speedscope-export.md) (same export path) plus
  `crates/agentprof-cli/README.md`. No JS / no external assets / no d3 bundling
  (the earlier design sketch was simplified away).
- **Cross-session aggregate** (`agentprof aggregate`, M1.6.2 + M1.6.3 TUI) —
  covered by [ADR-0008](../internals/adr-0008-aggregate-report-and-utilization.md)
  plus `crates/agentprof-core/README.md` (`analyzer::aggregate`) and
  `crates/agentprof-cli/README.md`.
- **Live-refresh watch** (`agentprof watch`, M1.6.3) — covered by
  [ADR-0009](../internals/adr-0009-watch-runner-and-notify.md) plus
  `crates/agentprof-tui/README.md` (`watch` module) and
  `crates/agentprof-cli/README.md`.
- **Tracing infrastructure** (global `--log-level` / `--log-file`, TUI
  auto-redirect to `$XDG_STATE_HOME/agentprof/agentprof.log`, 4-layer
  span topology `cmd.* → adapter.* → analyzer.* / aggregator.* →
  events`, `sha256[..8]` PII path hash with `AGENTPROF_LOG_FULL_PATHS`
  opt-out, M1.6.4) — covered by
  [ADR-0010](../internals/adr-0010-tracing-infrastructure.md) plus the
  `## Observability` section of `crates/agentprof-core/README.md`, the
  `## Tracing & logging` section of `crates/agentprof-cli/README.md`,
  the log-output PII model in
  [`privacy.md` §7](./privacy.md#7-log-output-pii-model-m164), and
  [`docs/architecture.md`](../architecture.md) §15.5.
- **MCP server waste analysis** (`agentprof mcp-waste`, `analyze
  --section mcp-waste`, `aggregate --by mcp-server` waste columns,
  TUI `[5] McpWaste` split-pane view, M1.6.5) — covered by
  [ADR-0015](../internals/adr-0015-mcp-waste-architecture.md) plus
  `crates/agentprof-core/README.md` (`analyzer::waste` +
  `model::waste`), `crates/agentprof-adapters/README.md`
  (`copilot::tools_changed` + `copilot::mcp_config`),
  `crates/agentprof-tui/README.md` (`views::mcp_waste`), and
  `crates/agentprof-cli/README.md` (`cmd::mcp_waste`).
- **MCP tool token-cost view** (`--tokens-per-tool <N>` heuristic +
  `--tool-descriptions <path>` sidecar across the three MCP-waste
  surfaces; tokenizer auto-inference from session dominant model;
  M1.6.6) — covered by
  [ADR-0016](../internals/adr-0016-mcp-token-cost-architecture.md)
  plus `crates/agentprof-core/README.md` (`WasteComputeContext`
  builder + `SidecarLookup` + `infer_tokenizer` +
  `compute_token_cost_for_tool` + `TokenProvenance` /
  `TokenSource` / `TokenizerKind`),
  `crates/agentprof-adapters/README.md`
  (`copilot::tool_sidecar`), and `crates/agentprof-cli/README.md`
  (`cmd::model_hint::dominant_model` + the three subcommand
  flag tables), and [`docs/architecture.md`](../architecture.md)
  §11.1.
