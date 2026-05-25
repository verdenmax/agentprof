# agentprof-storage

> SQLite persistence and (optional) OTLP receiver for agentprof.

## Position in the agentprof architecture

Depends only on `agentprof-core`. Provides durable storage for analysis reports across sessions, plus an opt-in OpenTelemetry receiver for live Claude Code telemetry ingestion. See [`docs/architecture.md`](../../docs/architecture.md) §9 (SQLite schema) and §15.4 (feature flags).

## Public interface

- `sqlite::Db` — bundled SQLite handle, migration-aware
- `sqlite::queries::*` — typed accessors for sessions / tools_loaded / turn_buckets
- `otlp::Receiver` (feature `otlp`) — Tonic-based OTLP gRPC server

```rust
// (will become a doctest once Phase 2 lands)
// let db = agentprof_storage::sqlite::Db::open_default()?;
// db.migrate()?;
```

## Modules (planned)

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
