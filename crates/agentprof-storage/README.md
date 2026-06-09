# agentprof-storage

> SQLite persistence and (optional) OTLP receiver for agentprof.
>
> **Status (2026-06-01): STUB CRATE — no implementation has shipped.** The
> 19-LOC `lib.rs` only contains `//!` module docs and the `[features]` /
> dependency wiring. Every API described below is *planned*, not present.
> Activated milestone: **Phase 2** (see [`docs/architecture.md`](../../docs/architecture.md)
> §15.5 and [`docs/plan.md`](../../docs/plan.md) "Phase 2 engineering"); not on
> the M1.7 v0.1.0 release path. The crate is kept in the workspace so the
> dependency wiring (`rusqlite` bundled + optional OTLP stack) compiles end-to-end
> and so the `storage:` CHANGELOG prefix is reserved.

## Status — M2.1 in progress (v0.2.0)

Phase 2 SQLite work has begun. As of M2.1 T2.2 the crate exposes its first
real public surface:

- [`config::StorageConfig`] / [`config::StorageMode`] — hybrid `cache` vs
  `store` configuration with XDG-aware path resolution.
- [`SqliteError`] — `thiserror`-based error type covering `rusqlite`,
  migrations, I/O, config-path and serde failures.
- [`Db`] (M2.1 T2.3) — opens a SQLite file (or in-memory db), applies
  standard pragmas (`journal_mode=WAL`, `synchronous=NORMAL`,
  `foreign_keys=ON`) and runs the embedded migrations in
  `migrations/001_initial.sql` (`sessions` / `tools_loaded` / `turn_buckets`
  + supporting indexes). Idempotent on re-open.
- [`upsert::upsert_report`] (M2.1 T2.4) — atomic 3-table write for a single
  session: `INSERT OR REPLACE` into `sessions` plus explicit
  `DELETE`+`INSERT` for the `tools_loaded` / `turn_buckets` children
  (the parent `OR REPLACE` does **not** cascade child deletes), all
  inside one transaction.
- [`query::query_sessions_since`] / [`query::load_session`] (M2.1 T2.5) —
  read API: enumerate `SessionRef`s within a time window (newest first),
  and hydrate a full `AnalysisReport` from `analysis_report_json` by id.

Subsequent T2.x tasks will land typed query modules on top of `Db`. A full README rewrite is scheduled for **T8.2** at the end of M2.1;
the "STUB CRATE" notice above will be removed then.

## Position in the agentprof architecture

Depends only on `agentprof-core`. *Will* provide durable storage for analysis
reports across sessions, plus an opt-in OpenTelemetry receiver for live
telemetry ingestion. See [`docs/architecture.md`](../../docs/architecture.md)
§9 (SQLite schema — normative) and §15.4 (feature flags). The MVP shipping
surface (M1.1 – M1.6.4) operates entirely on filesystem session logs and does
not touch this crate.

## Planned public interface

These items are **not yet exported**. Once Phase 2 work begins, this section
will become the L2 surface for the crate.

- `sqlite::Db` — bundled SQLite handle, migration-aware
- `sqlite::queries::*` — typed accessors for `sessions` / `tools_loaded` / `turn_buckets`
- `otlp::Receiver` (feature `otlp`) — Tonic-based OTLP gRPC server

```rust
// Planned shape (will become a real doctest once Phase 2 lands):
// let db = agentprof_storage::sqlite::Db::open_default()?;
// db.migrate()?;
```

## Planned modules

| Module | Purpose |
|---|---|
| `sqlite::schema` | DDL constants matching `docs/architecture.md` §9 |
| `sqlite::migrations` | Numbered migration files; runs idempotently on `open` |
| `sqlite::queries` | High-level typed accessors |
| `otlp` (feature `otlp`) | OpenTelemetry OTLP receiver |

## Features

| Feature | Default | Effect |
|---|---|---|
| `otlp` | off | Pulls in `opentelemetry`, `opentelemetry-otlp`, `opentelemetry_sdk`, `tonic`, `tokio`; enables the `Receiver` API used by `agentprof ingest-otlp`. |

## Dependencies

- Workspace internal: `agentprof-core`
- External: `serde`, `serde_json`, `thiserror`, `tracing`, `chrono`, `rusqlite` (bundled)
- Optional (feature `otlp`): `opentelemetry`, `opentelemetry-otlp`, `opentelemetry_sdk`, `tonic`, `tokio`

## Local commands

```sh
cargo test -p agentprof-storage --all-features
cargo doc  -p agentprof-storage --no-deps --open
```

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) — entries prefixed `storage:`.
