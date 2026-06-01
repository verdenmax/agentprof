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

```markdown
# <Topic>

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

No ADR is currently superseded; ADR-0009 builds on ADR-0006 (panic-safe lifecycle)
and ADR-0008 (aggregate compute path) without invalidating them — they are
complementary, not replaced.

## Planned files

These are *not yet written*. Each requires its own ADR or algorithm note when
the underlying feature lands.

- `adr-NNNN-waste-formula.md` — derivation of `waste_estimate_usd` (deferred to
  M1.6.5 MCP waste analyzer)
- `adr-NNNN-tokenizer-strategy.md` — `cl100k_base` approximation vs Anthropic
  API trade-offs (deferred to Phase 2 tokenizer milestone)
- `adr-NNNN-adapter-wire-format.md` — how each agent serializes tools into the
  prompt (deferred until a second adapter ships, i.e. Phase 3 M3.1)
