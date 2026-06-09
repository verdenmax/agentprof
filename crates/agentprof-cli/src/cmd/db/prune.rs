//! `agentprof db prune` — delete sessions older than a retention window.
//!
//! Counts (or, without `--dry-run`, deletes) every `sessions` row whose
//! `started_at` is older than `now - --before`. Child rows in
//! `tools_loaded` / `turn_buckets` are removed via `ON DELETE CASCADE`
//! foreign keys (see
//! [`agentprof_storage::admin::prune_before`] for the cascading
//! semantics).

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use agentprof_cli::config::resolve_storage_config;
use agentprof_storage::admin::prune_before;
use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;
use crate::cmd::since::parse_since;

/// Arguments for `agentprof db prune`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct PruneArgs {
    /// Retention window — anything older than `now - <BEFORE>` is
    /// pruned. Same grammar as `--since`: `<N>d`/`h`/`m`/`s` / `all`.
    #[arg(long, default_value = "30d")]
    pub before: String,

    /// Report the count without actually deleting.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `agentprof db prune`.
///
/// # Errors
///
/// - [`ExitKind::UserError`] for bad `--before` or storage config.
/// - [`ExitKind::DataError`] if the DB can't be opened or the delete
///   query fails.
#[allow(clippy::needless_pass_by_value)]
pub fn run(args: PruneArgs, storage_path: Option<PathBuf>) -> Result<()> {
    let retention = parse_since(&args.before)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("invalid --before: {e}")))?;
    let cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let mut db = Db::open_and_migrate(&cfg.path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", cfg.path.display()))
    })?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let n = prune_before(&mut db, retention, now_ms, args.dry_run)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("prune: {e}")))?;
    if args.dry_run {
        println!(
            "agentprof: would prune {n} session(s) older than {}",
            args.before
        );
    } else {
        println!(
            "agentprof: pruned {n} session(s) older than {}",
            args.before
        );
    }
    Ok(())
}
