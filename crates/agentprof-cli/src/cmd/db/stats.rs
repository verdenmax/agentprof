//! `agentprof db stats` — print row counts, size, mode, path, and
//! oldest/newest session timestamps.
//!
//! Two output formats: a human-readable two-column `table` (default)
//! and a compact `json` blob suitable for piping into `jq`.

use std::path::PathBuf;

use anyhow::Result;
use chrono::{TimeZone as _, Utc};
use clap::Args;
use serde_json::json;

use agentprof_cli::config::resolve_storage_config;
use agentprof_storage::admin::stats;
use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;

/// Arguments for `agentprof db stats`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct StatsArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = StatsFormat::Table)]
    pub export: StatsFormat,
}

/// Output format for [`StatsArgs::export`].
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[non_exhaustive]
pub enum StatsFormat {
    /// Two-column human-readable table.
    Table,
    /// One-line JSON object.
    Json,
}

/// Run `agentprof db stats`.
///
/// # Errors
///
/// - [`ExitKind::UserError`] for bad storage config.
/// - [`ExitKind::DataError`] if the DB can't be opened or queried.
#[allow(clippy::needless_pass_by_value)]
pub fn run(args: StatsArgs, storage_path: Option<PathBuf>) -> Result<()> {
    let cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let db = Db::open_and_migrate(&cfg.path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", cfg.path.display()))
    })?;
    let s = stats(&db).map_err(|e| ExitKind::DataError.into_anyhow(format!("read stats: {e}")))?;

    let oldest = s.oldest_started_ms.and_then(fmt_ms);
    let newest = s.newest_started_ms.and_then(fmt_ms);
    let mode_str = format!("{:?}", cfg.mode).to_lowercase();

    match args.export {
        StatsFormat::Table => {
            println!("path:                {}", cfg.path.display());
            println!("mode:                {mode_str}");
            println!("size_bytes:          {}", s.size_bytes);
            println!("sessions:            {}", s.session_count);
            println!("tools_loaded:        {}", s.tools_loaded_count);
            println!("turn_buckets:        {}", s.turn_buckets_count);
            println!("oldest_started:      {}", oldest.as_deref().unwrap_or("-"));
            println!("newest_started:      {}", newest.as_deref().unwrap_or("-"));
        }
        StatsFormat::Json => {
            let v = json!({
                "path": cfg.path.display().to_string(),
                "mode": mode_str,
                "size_bytes": s.size_bytes,
                "sessions": s.session_count,
                "tools_loaded": s.tools_loaded_count,
                "turn_buckets": s.turn_buckets_count,
                "oldest_started": oldest,
                "newest_started": newest,
            });
            println!("{v}");
        }
    }
    Ok(())
}

fn fmt_ms(ms: i64) -> Option<String> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}
