# agentprof-storage

> SQLite persistence layer for **agentprof**, with a hybrid cache/store
> mode for the on-disk file and an optional OTLP receiver subsystem
> (feature `otlp`).
>
> **Status (M2.1, v0.2.0)**: shipped end-to-end — `Db` handle + embedded
> migrations, atomic `upsert_report`, typed read API, `SessionDataSource`
> impl for dual-path reads, and the `admin::*` helpers that back the
> `agentprof db` cli subcommand family.
>
> **Status (M2.2, v0.3.0, feature `otlp`)**: OTLP receiver subsystem
> shipped under `agentprof_storage::otlp` — gRPC + HTTP/protobuf
> listeners, per-`session.id` in-memory buffering with OOM caps, idle /
> size / shutdown flush triggers, and a `StorageFlushSink` that reuses
> the M2.1 `upsert_report` pipeline. Architecture: [ADR-0021](../../docs/internals/adr-0021-otlp-receiver-architecture.md).
>
> **M2.4 hardening** (v0.3.0): constant-time bearer compare via the
> `subtle` crate (auth module); per-signal request-size caps wired into
> both gRPC (`max_decoding_message_size`) and HTTP (`DefaultBodyLimit`)
> transports (`server_grpc`, `server_http`); LRU session eviction with
> `CloseReason::CapacityEvict` (router module); 256-byte `session.id`
> length cap in mapper. Closes audit findings F1/F2/F3. See
> [ADR-0022](../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md).

## Position in the agentprof architecture

Leaf-adjacent: depends **only** on
[`agentprof-core`](../agentprof-core/README.md). Provides:

- durable storage for `AnalysisReport`s across analyses (hybrid cache /
  store mode — see [ADR-0019](../../docs/internals/adr-0019-hybrid-storage-mode.md));
- a `SessionDataSource` implementation that the cli composes with the
  adapter to deliver dual-path reads — see
  [ADR-0018](../../docs/internals/adr-0018-session-datasource-trait.md);
- admin helpers (`stats`, `prune_before`, `vacuum`, `export_session_json`)
  that back `agentprof db {stats,prune,vacuum,export}`.

See `docs/architecture.md` §9 (SQLite schema — normative against this
crate's `migrations/001_initial.sql`), §10 (`[storage]` config block),
and §15.4 (feature flags).

## Public API

| Item | Purpose | Stability |
|---|---|---|
| [`config::StorageConfig`] | Resolved storage configuration (`mode` + `path` + `auto_prune_days`). Built by merging `PartialStorageConfig` (from TOML) with XDG-aware defaults. | `#[non_exhaustive]`, additive |
| [`config::StorageMode`] | `Cache` (default, `$XDG_CACHE_HOME`) vs `Store` (opt-in, `$XDG_DATA_HOME`). See [ADR-0019](../../docs/internals/adr-0019-hybrid-storage-mode.md). | `#[non_exhaustive]` |
| [`config::PartialStorageConfig`] | Lossless deserialisation target for the `[storage]` TOML block (all fields `Option`). cli merges this into `StorageConfig`. | `#[non_exhaustive]`, additive |
| [`Db`] | SQLite handle. `Db::open(path)` runs the embedded migrations (`migrations/001_initial.sql`) and applies pragmas `journal_mode=WAL` / `synchronous=NORMAL` / `foreign_keys=ON`. Idempotent on re-open. `Db::open_in_memory()` for tests. | Stable |
| [`upsert::upsert_report`] | `fn upsert_report(db, report, raw_path, ingested_at_secs)` — atomic write of one session's three rows (`sessions` + per-row `tools_loaded` + per-row `turn_buckets`) inside a single transaction. Explicit `DELETE` of child rows before `INSERT` to make re-upsert idempotent (parent `INSERT OR REPLACE` does **not** cascade in SQLite). | Stable |
| [`upsert::upsert_episodes`] | `fn upsert_episodes(db, id, &Episodes, _ingested_at_secs)` — UPDATE the `episodes_json` column. Pairs with `upsert_report` (which must run first to insert the session row); returns 0 if no row matched (caller MUST pair the two). M2.1.1. | Stable |
| [`query::query_sessions_since`] | Enumerate `agentprof_core::datasource::SessionRef`s with `started_at >= cutoff`, newest first. Backs `list --since`. | Stable |
| [`query::load_session`] | Hydrate a full `AnalysisReport` from `sessions.analysis_report_json` by id. `QueryReturnedNoRows` → `SqliteError::NotFound`. | Stable |
| [`query::load_episodes`] | SELECT the `episodes_json` column, deserialize to `Episodes`. Returns `Episodes::default()` for pre-M2.1.1 rows whose column holds the migration-default `'{}'` blob (backed by `#[serde(default)]` on `Episodes`' required fields). `QueryReturnedNoRows` for unknown id. M2.1.1. | Stable |
| [`SqliteDataSource`] | Implements [`agentprof_core::datasource::SessionDataSource`]. Wraps an `Arc<Mutex<Db>>`; maps `NotFound` → `DataSourceError::NotFound`, other `SqliteError`s → `DataSourceError::Storage { source: "sqlite", … }`. Consumed by the cli's `DualPathDataSource` composer per [ADR-0018](../../docs/internals/adr-0018-session-datasource-trait.md). | Stable |
| [`admin::stats`] | `DbStats { mode, path, file_size_bytes, sessions_count, tools_loaded_count, turn_buckets_count, oldest_started_at, newest_started_at }` for `agentprof db stats`. | Stable |
| [`admin::prune_before`] | Cascading delete by `started_at`. Cache mode honours `auto_prune_days`; store mode never auto-prunes. Backs `agentprof db prune`. | Stable |
| [`admin::vacuum`] | `VACUUM` SQLite; returns `(before_bytes, after_bytes)`. In-memory DBs report `(0, 0)` (SQLite quirk). | Stable |
| [`admin::export_session_json`] | Read the verbatim `analysis_report_json` for a session id. Backs `agentprof db export --format json`. | Stable |
| [`SqliteError`] | `thiserror` enum: `Open` / `Migrate` / `Io` / `Sqlite` / `Serde` / `ConfigPath` / `NotFound`. All variants carry user-actionable context (paths, ids). | `#[non_exhaustive]` |

### Typical usage

```rust,ignore
use std::sync::{Arc, Mutex};
use agentprof_storage::{config::StorageConfig, Db, SqliteDataSource, upsert};

let cfg = StorageConfig::default_cache();                          // ~/.cache/agentprof/cache.sqlite
let db  = Db::open(cfg.resolve_path()?)?;                          // applies pragmas + migrations
let shared = Arc::new(Mutex::new(db));

// Write-through caching on analyze:
let report = /* AnalysisReport */ unimplemented!();
let now_secs: i64 = 0; // chrono::Utc::now().timestamp()
upsert::upsert_report(&shared.lock().unwrap(), &report, std::path::Path::new("/path/to/events.jsonl"), now_secs)?;

// Wrap as a SessionDataSource for the cli's dual-path read:
let _ds = SqliteDataSource::new(Arc::clone(&shared));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Schema

Final DDL is in `migrations/001_initial.sql` and reproduced in
`docs/architecture.md` §9 (normative copy). Three tables:

- `sessions` (PK `id`) — agent / dominant_model / started_at /
  duration_ms / raw_path / raw_mtime / total_* / schema_version /
  ingested_at / **`analysis_report_json` (the full serialised
  `AnalysisReport`, including the M2.1 T5.2.5 hoisted
  `loaded_mcp_tools` field — read path hydrates from this column)**.
- `tools_loaded` (PK `(session_id, tool_name)`, FK CASCADE) — per-tool
  call counts + total duration + optional M1.6.6 token cost +
  `token_source`.
- `turn_buckets` (PK `(session_id, turn_index)`, FK CASCADE) — per-turn
  token totals + model.

Indexes: `idx_sessions_started`, `idx_sessions_agent_started`,
`idx_tools_call_count`.

**Migration 002 (M2.1.1)**: additive `ALTER TABLE sessions ADD COLUMN
episodes_json TEXT NOT NULL DEFAULT '{}'` for the per-call `Episodes`
blob that aggregate's percentile pool needs. Default `'{}'` keeps
pre-M2.1.1 rows queryable as `Episodes::default()`. See
[ADR-0020](../../docs/internals/adr-0020-aggregate-dualpath.md).

See `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`
§5 for the column-level design rationale and
[ADR-0019](../../docs/internals/adr-0019-hybrid-storage-mode.md) for the
cache-vs-store policy.

## Modules

| Module | Purpose |
|---|---|
| `config`     | `StorageConfig` / `StorageMode` / `PartialStorageConfig` + XDG path resolution |
| `db`         | `Db` handle + embedded migrations (`include_str!("../migrations/001_initial.sql")`) + pragmas |
| `upsert`     | `upsert_report(db, &AnalysisReport, &Path, secs)` atomic 3-table write; `upsert_episodes(db, id, &Episodes, secs)` UPDATE `episodes_json` (M2.1.1) |
| `query`      | `query_sessions_since(db, since) → Vec<SessionRef>` / `load_session(db, id) → AnalysisReport` / `load_episodes(db, id) → Episodes` (M2.1.1) |
| `datasource` | `SqliteDataSource` impl of `agentprof_core::datasource::SessionDataSource` (M2.1 T2.6) |
| `admin`      | `stats` / `prune_before` / `vacuum` / `export_session_json` (M2.1 T2.7) backing the `agentprof db` family |
| `error`      | `SqliteError` for every fallible API |
| `otlp` (feature `otlp`, M2.2 ✅, M2.4 hardened) | OTLP receiver subsystem. Submodules: `config` (`OtlpServerConfig` + `PartialOtlpServerConfig`; M2.4 T9 adds `max_{logs,metrics,traces}_request_bytes` + `max_open_sessions` fields), `error` (`OtlpServerError` / `MapperError` / `RouterError`; M2.4 T7 adds `MapperError::SessionIdTooLong`, T8 adds `CloseReason::CapacityEvict`), `pipeline` (`IngestPipeline` end-to-end fan-in: mapper → router → flush sink), `server_grpc` (tonic gRPC listener with 3 collector services; M2.4 T6 wires `max_decoding_message_size` per service), `server_http` (axum HTTP/protobuf listener with `/v1/{logs,metrics,traces}`; M2.4 T6 wires `DefaultBodyLimit` per route), `auth` (bearer-token tonic interceptor + axum middleware applied to both transports; **M2.4 T5: constant-time compare via `subtle::ConstantTimeEq`**), `tls` (rustls server config + optional mTLS), `typed` (`TypedEvent` IR + `SignalKind` + `TokenDirection`), `mapper` (OTLP wire types → `TypedEvent`; M2.4 T7 rejects `session.id` longer than 256 bytes), `router` (`SessionRouter` + `SessionBuffer` + OOM caps + `FlushSink` trait + tool-call pairing in `into_persistable`; **M2.4 T8: LRU eviction with `CloseReason::CapacityEvict` when `max_open_sessions` exceeded**), `sweeper` (`spawn_idle_sweeper` async wrapper that periodically calls `router.sweep_idle()` and drains via `flush_all(Shutdown)` on cancellation), `sink_storage` (`StorageFlushSink` that persists closed buffers via `upsert_report` with `raw_path = "otlp://<id>"`). Internal `proto` submodule holds tonic-generated server stubs (mirrors `opentelemetry::proto::*` layout). See [ADR-0021](../../docs/internals/adr-0021-otlp-receiver-architecture.md) + [ADR-0022](../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md). |

## Features

| Feature | Default | Effect |
|---|---|---|
| `otlp` | off | Pulls in `opentelemetry-proto`, `tonic`, `prost`, `tokio`, `axum`, `bytes`, `dashmap`, `rustls`, `rustls-pemfile`, `tower`, **`subtle`** (M2.4 T5: constant-time bearer compare); compiles the OTLP collector `.proto`s into server stubs at build time (build-dep on `tonic-build` + `prost-build`); enables the receiver subsystem under `agentprof_storage::otlp` (M2.2 ✅ — full gRPC + HTTP/protobuf transport, per-`session.id` buffering with OOM caps, idle/size/shutdown flush, bearer + TLS + mTLS auth, `StorageFlushSink` reusing `upsert_report`; **M2.4 ✅ hardened** — constant-time bearer, per-signal request size caps wired on both transports, LRU session eviction with `CloseReason::CapacityEvict`, 256-byte `session.id` cap in mapper). See [ADR-0021](../../docs/internals/adr-0021-otlp-receiver-architecture.md) + [ADR-0022](../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md). |

## Dependencies

- Workspace internal: `agentprof-core`
- External: `serde`, `serde_json`, `thiserror`, `tracing`, `chrono`,
  `directories`, `rusqlite` (bundled)
- Optional (feature `otlp`): `opentelemetry-proto`, `tonic`, `prost`,
  `tokio`, `axum`, `bytes`, `dashmap`, `rustls`, `rustls-pemfile`,
  `tower`, `subtle` (M2.4 T5 — constant-time bearer compare,
  [ADR-0022](../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md) D-4);
  build-dep `tonic-build` + `prost-build` for `.proto` codegen

## Local commands

```sh
cargo test  -p agentprof-storage --all-features
cargo clippy -p agentprof-storage --all-targets --all-features -- -D warnings
cargo doc   -p agentprof-storage --no-deps --open
```

Integration tests under `tests/` cover migration idempotency,
upsert+reload round-trips, cascade-delete invariants, and config
resolution for both modes.

## Reference ADRs

| ADR | Topic |
|---|---|
| [0017](../../docs/internals/adr-0017-unify-session-id-namespace.md) | Unify session id namespace (M2.1 hotfix — makes dual-path actually function) |
| [0018](../../docs/internals/adr-0018-session-datasource-trait.md) | `SessionDataSource` trait + dual-path semantics |
| [0019](../../docs/internals/adr-0019-hybrid-storage-mode.md) | Hybrid cache vs store mode |
| [0021](../../docs/internals/adr-0021-otlp-receiver-architecture.md) | OTLP receiver architecture (M2.2) — push path, per-session buffering, why it does **not** implement `Adapter` |
| [0022](../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md) | OTLP capacity caps + LRU eviction (M2.4) — constant-time auth, per-signal request size caps, `max_open_sessions` LRU evict, 256-byte `session.id` cap |

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) — entries prefixed `storage:`.
