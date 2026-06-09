# ADR-0020: aggregate dual-path via separate Episodes storage

- **Status**: Accepted
- **Date**: 2026-06-09 (drafted alongside M2.1.1 implementation)
- **Deciders**: M2.1.1 milestone (aggregate cache coverage closure)
- **Related**: ADR-0018 (`SessionDataSource` trait), ADR-0019 (Hybrid storage), M2.1 deferred-scope note

## Context

M2.1 wired `list`, `mcp-waste`, and `analyze` through `DualPathDataSource`
to read from SQLite cache when available. `cmd::aggregate` was excluded:
all four `--by` arms (`tool` / `hook` / `day` / `model`) need raw per-call
durations from `Episodes` to recompute cross-session p50/p95 (averaging
per-session percentiles would be statistically wrong — see
`aggregate_by_tool::aggregate_by_tool` rustdoc).

`SessionDataSource::load_session() -> AnalysisReport` returns rollup data
(`tool_rank` totals, etc.) but not the per-call vec aggregate needs. So
aggregate stayed on the single-path adapter route and got no cache
acceleration.

## Decision

Add a new method `fn load_episodes(id: &str) -> Result<Episodes, _>` to
the `SessionDataSource` trait. Store `Episodes` in a **separate**
`episodes_json TEXT NOT NULL DEFAULT '{}'` column on the existing
`sessions` table (migration 002, additive `ALTER`). Aggregate calls both
`load_session` and `load_episodes` per session.

`AdapterDataSource::load_episodes` does NOT share work with
`load_session` — each is an independent discover → find → load → derive
pipeline. The 2× cost on the aggregate path is explicitly accepted (see
brainstorm transcript 2026-06-09 22:41–22:46; user picked option D1).

## Considered alternatives

### A. Hoist `Episodes` into `AnalysisReport` blob (rejected)

`Episodes` field on `AnalysisReport`, stored as part of the existing
`analysis_report_json` blob. Simplest caller change (aggregate just reads
`report.episodes`), but couples `Episodes` evolution to report blob format
and inflates the blob 5–20× even for callers that only want the report.
Aggregate-only readers would have to deserialize the full report just to
get `Episodes`.

### B. Aggregation-projection subset struct (rejected)

Define a slim `AggregationData` struct with just the fields aggregate
needs (per-call durations, per-turn timestamps), stored as a third blob.
Smaller blob (~1.5×) but introduces a permanent two-shape sync burden:
every change to `Episodes` that aggregate cares about must also update
`AggregationData`. YAGNI risk if aggregate's needs grow.

### C. Separate `episodes_json` column + trait method (chosen)

Explicit storage layout: aggregate-only readers skip the report blob
entirely; report-only readers skip the episodes blob entirely. Storage
layout is easy to reason about. Trait grows by one method, but no default
impl (all three internal impls must override).

## Consequences

### Positive

- Aggregate gets cache acceleration matching list / mcp-waste / analyze
- `Episodes` evolution stays internal to storage's blob format
- `AnalysisReport` blob stays compact (no `Episodes` payload bundled in)
- Trait shape mirrors existing pattern (`load_session` → `load_episodes`)

### Negative

- `AdapterDataSource` does 2× pipeline on aggregate path (per brainstorm
  decision D1). Caching is rejected as YAGNI; revisit when real users
  complain.
- Trait gains a method with no default impl. Breaks any external impl —
  but none exist in workspace, and the trait is `#[non_exhaustive]`
  signaling it's a moving target pre-v1.0.
- One more storage column to maintain (migration 002).

### Neutral

- Pre-M2.1.1 sessions load as `Episodes::default()` (empty). Aggregate
  gracefully skips empty `Episodes` in the percentile pool. Cache-mode
  users get full coverage on next ingest; store-mode users keep
  existing data and can choose when to backfill.
- Discovered during T2.5 RED: the migration default `'{}'` would otherwise
  fail to deserialize (Episodes had required fields without serde
  defaults). Fix in same wave: added `#[serde(default)]` to all `Episodes`
  required collection fields. Backward-compat with future schema bumps
  improves as a side effect.

## Implementation pointers

- Trait change: `crates/agentprof-core/src/datasource.rs`
- Migration: `crates/agentprof-storage/migrations/002_episodes_column.sql`
- Storage helpers: `upsert::upsert_episodes` + `query::load_episodes`
- Impl sites: `SqliteDataSource::load_episodes`,
  `AdapterDataSource::load_episodes` + `load_episodes_by_ref`,
  `DualPathDataSource::load_episodes`
- Caller refactor: `cmd::aggregate::run` (extracted `compute_phase2` +
  new `compute_aggregate_via_ds`), `cmd::analyze::run` (write-through
  extension), `cmd::db::ingest::run` (per-session loop extension)
- Tests: `crates/agentprof-storage/tests/episodes_smoke.rs` (storage
  round-trip), `crates/agentprof-cli/tests/cli_aggregate_dualpath.rs`
  (CLI dual-path)
