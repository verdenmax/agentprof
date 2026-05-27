//! # agentprof-storage
//!
//! `SQLite` persistence and (optional) OTLP telemetry receiver for **agentprof**.
//!
//! Depends only on [`agentprof-core`](../agentprof_core/index.html). The `SQLite`
//! schema is normative in [`docs/architecture.md`](https://github.com/agentprof/agentprof/blob/main/docs/architecture.md#9-sqlite-schema)
//! (§9); migrations under `src/sqlite/migrations/` must keep that schema in sync.
//!
//! ## Modules (planned)
//!
//! - `sqlite::schema`     — DDL constants
//! - `sqlite::migrations` — idempotent migrations executed on `Db::open`
//! - `sqlite::queries`    — typed accessors
//! - `otlp` (feature `otlp`) — Tonic-based OpenTelemetry receiver
//!
//! ## Features
//!
//! - `otlp` (off by default) — enables the OTLP receiver used by
//!   `agentprof ingest-otlp`.
