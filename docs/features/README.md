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

## Planned files

- `otlp-receiver.md` — OpenTelemetry receiver spanning storage + cli
- `html-report.md` — askama template + d3.js bundling
- `tool-roi-matrix.md` — analyzer + tui + cli integration
