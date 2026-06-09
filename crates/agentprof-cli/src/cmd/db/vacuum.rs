//! `agentprof db vacuum` — reclaim free pages via `SQLite` `VACUUM`.
//!
//! Reports the before/after on-disk byte size so the user can see the
//! reclaim factor. For an in-memory DB both numbers are `0` (a quirk
//! of `SQLite`'s `page_count` for `:memory:` — see
//! [`agentprof_storage::admin::vacuum`]).

use std::path::PathBuf;

use anyhow::Result;

use agentprof_cli::config::resolve_storage_config;
use agentprof_storage::admin::vacuum;
use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;

/// Run `agentprof db vacuum`.
///
/// # Errors
///
/// - [`ExitKind::UserError`] for bad storage config.
/// - [`ExitKind::DataError`] if the DB can't be opened or `VACUUM`
///   fails.
pub fn run(storage_path: Option<PathBuf>) -> Result<()> {
    let cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let db = Db::open_and_migrate(&cfg.path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", cfg.path.display()))
    })?;
    let (before, after) =
        vacuum(&db).map_err(|e| ExitKind::DataError.into_anyhow(format!("vacuum: {e}")))?;
    println!("agentprof: vacuum complete; before={before} bytes after={after} bytes");
    Ok(())
}
