//! `agentprof db` subcommand family — DB lifecycle and inspection.
//!
//! Six management subcommands for the `SQLite` cache introduced in M2.1:
//!
//! | Sub | Purpose |
//! |---|---|
//! | [`init`]   | Create the DB file and run migrations to the latest schema. |
//! | [`stats`]  | Print row counts, on-disk size, mode, path, and oldest/newest timestamps. |
//! | [`ingest`] | Batch-import sessions from an [`agentprof_core::adapter::Adapter`]. |
//! | [`prune`]  | Delete sessions older than a `--before` retention window (with `--dry-run`). |
//! | [`vacuum`] | Run `SQLite` `VACUUM` and report before/after byte sizes. |
//! | [`export`] | Dump one session's stored `AnalysisReport` as JSON or JSONL. |
//!
//! All actions honor the global `--storage-path` override so callers
//! (typically tests) can pin a per-invocation database file without
//! touching the user's real `~/.cache/agentprof/cache.sqlite`.
//!
//! See `docs/architecture.md` §8 (CLI protocol) and the M2.1 plan
//! `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-plan.md`
//! §T6.1–§T6.4 for the canonical surface.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand};

pub mod export;
pub mod ingest;
pub mod init;
pub mod prune;
pub mod stats;
pub mod vacuum;

/// Top-level arguments for `agentprof db <ACTION>`.
#[derive(Args, Debug)]
#[non_exhaustive]
pub struct DbArgs {
    /// Which DB action to perform.
    #[command(subcommand)]
    pub action: DbAction,
}

/// One of the six `db` actions — see the module-level docs.
#[derive(Subcommand, Debug)]
#[non_exhaustive]
pub enum DbAction {
    /// Create the DB file and run migrations to the latest schema.
    Init,
    /// Print row counts, size, mode, path, and oldest/newest session.
    Stats(stats::StatsArgs),
    /// Batch-import sessions from an adapter into the DB.
    Ingest(ingest::IngestArgs),
    /// Delete sessions older than `--before`.
    Prune(prune::PruneArgs),
    /// Run `SQLite` `VACUUM` and report before/after byte sizes.
    Vacuum,
    /// Dump a single session as JSON or JSONL.
    Export(export::ExportArgs),
}

/// Dispatch a parsed [`DbArgs`] to the appropriate action handler.
///
/// All actions take the global `--storage-path` override so tests
/// (and power users) can pin a per-invocation database file.
///
/// # Errors
///
/// Forwards any `anyhow::Error` returned by the underlying action.
/// Typical downcast targets are
/// [`crate::cmd::exit::ExitKind::UserError`] (bad args / bad path)
/// and [`crate::cmd::exit::ExitKind::DataError`] (corrupt DB / missing
/// session id).
pub fn run(args: DbArgs, storage_path: Option<PathBuf>) -> Result<()> {
    match args.action {
        DbAction::Init => init::run(storage_path),
        DbAction::Stats(a) => stats::run(a, storage_path),
        DbAction::Ingest(a) => ingest::run(a, storage_path),
        DbAction::Prune(a) => prune::run(a, storage_path),
        DbAction::Vacuum => vacuum::run(storage_path),
        DbAction::Export(a) => export::run(a, storage_path),
    }
}
