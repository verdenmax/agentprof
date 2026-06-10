//! # agentprof-storage
//!
//! `SQLite` persistence and (optional) OTLP telemetry receiver for **agentprof**.
//!
//! Depends only on [`agentprof-core`](../agentprof_core/index.html). The
//! `SQLite` schema is normative in
//! [`docs/architecture.md`](https://github.com/agentprof/agentprof/blob/main/docs/architecture.md#9-sqlite-schema)
//! §9; migrations under `src/sqlite/migrations/` must keep that schema in sync.
//!
//! ## Modules
//!
//! - [`config`] — [`StorageConfig`](config::StorageConfig) /
//!   [`StorageMode`](config::StorageMode) (M2.1 T2.2)
//! - [`error`]  — [`SqliteError`] for all fallible APIs in this crate
//! - [`db`]     — [`Db`] handle + embedded migrations (M2.1 T2.3)
//! - [`upsert`] — [`upsert_report`](upsert::upsert_report) atomic 3-table
//!   write for one session (M2.1 T2.4)
//! - [`query`]  — [`query_sessions_since`](query::query_sessions_since) /
//!   [`load_session`](query::load_session) read API (M2.1 T2.5)
//! - [`datasource`] — [`SqliteDataSource`] impl of
//!   [`agentprof_core::datasource::SessionDataSource`] (M2.1 T2.6)
//! - [`admin`] — [`stats`](admin::stats) /
//!   [`prune_before`](admin::prune_before) / [`vacuum`](admin::vacuum) /
//!   [`export_session_json`](admin::export_session_json) helpers for the
//!   `agentprof db` subcommand family (M2.1 T2.7)
//! - `otlp` (feature `otlp`) — Tonic-based OpenTelemetry receiver
//!   ([`otlp::config`] + [`otlp::error`] land in M2.2 T2.1; transport
//!   layers in subsequent tasks)
//!
//! ## Features
//!
//! - `otlp` (off by default) — enables the OTLP receiver used by
//!   `agentprof ingest-otlp`.
//! - `progress` (off by default) — enables the CLI ingest progress bar.
//!
//! ## Examples
//!
//! ```
//! use agentprof_storage::config::StorageConfig;
//! let cfg = StorageConfig::default();
//! println!("DB will live at {}", cfg.path.display());
//! ```

#![warn(missing_docs)]

pub mod admin;
pub mod config;
pub mod datasource;
pub mod db;
pub mod error;
#[cfg(feature = "otlp")]
pub mod otlp;
pub mod query;
pub mod upsert;

pub use datasource::SqliteDataSource;
pub use db::Db;
pub use error::SqliteError;
