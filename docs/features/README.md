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
| [`privacy.md`](./privacy.md) | PII tier table for `AnalysisReport` output fields + manual `sed`/`jq` redaction recipes + planned `--redact` / `--anonymize` CLI flags. Touches `agentprof-core::analyzer` (which fields exist) and `agentprof-cli::cmd::format` (which fields get rendered). |

## Planned files

- `otlp-receiver.md` — OpenTelemetry receiver spanning storage + cli (Phase 2;
  blocked on `agentprof-storage` leaving stub state)
- `tool-roi-matrix.md` — analyzer + tui + cli integration (post-MVP)

## Shipped without a dedicated L2 feature doc

These features touch multiple crates but were captured fully in a single ADR +
the affected crate READMEs, so no separate `docs/features/*.md` was needed.

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
