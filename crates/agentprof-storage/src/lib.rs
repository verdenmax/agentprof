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
//! - `sqlite::queries` (planned, M2.1 T2.5+) — typed accessors
//! - `otlp` (planned, feature `otlp`) — Tonic-based OpenTelemetry receiver
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

pub mod config;
pub mod db;
pub mod error;
pub mod upsert;

pub use db::Db;
pub use error::SqliteError;
