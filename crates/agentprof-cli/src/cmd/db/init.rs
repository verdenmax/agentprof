//! `agentprof db init` — open the DB and run pending migrations.
//!
//! Idempotent: re-running against an already-migrated DB is a no-op
//! (the migration runner records its high-water mark in
//! `schema_migrations`). Parent directories are created on demand by
//! [`agentprof_storage::Db::open_and_migrate`].

use std::path::PathBuf;

use anyhow::Result;

use agentprof_cli::config::resolve_storage_config;
use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;

/// Run `agentprof db init`.
///
/// Resolves the storage config (default + optional CLI override),
/// opens the database file (creating it if absent), and runs all
/// pending migrations. Prints a single confirmation line to stdout.
///
/// # Errors
///
/// - [`ExitKind::UserError`] if the storage config is rejected.
/// - [`ExitKind::DataError`] if the DB file cannot be opened / migrated.
pub fn run(storage_path: Option<PathBuf>) -> Result<()> {
    let cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let _db = Db::open_and_migrate(&cfg.path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", cfg.path.display()))
    })?;
    println!("agentprof: db initialized at {}", cfg.path.display());
    Ok(())
}
