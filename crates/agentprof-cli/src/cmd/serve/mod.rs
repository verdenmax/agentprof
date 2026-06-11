//! `agentprof serve` — embedded HTTP dashboard (M2.3).
//!
//! Runs an HTTP server bound to `--bind` (default `127.0.0.1:4329`)
//! that renders the same data the existing CLI surfaces produce, with
//! browser-driven auto-refresh. Requires the `--storage-path` `SQLite`
//! store to be populated (run `agentprof db ingest` or
//! `agentprof ingest-otlp` first).
//!
//! See ADR-0024 for architecture decisions and
//! `docs/superpowers/specs/2026-06-11-m2.3-web-dashboard-design.md`
//! for the design spec.

pub mod state;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::Args;

/// CLI arguments for `agentprof serve`.
///
/// `interval_default` is intentionally `Option<u8>` (not `u8` with a
/// `default_value`) so the M2.3 T4 resolver can distinguish
/// "user explicitly set 5" from "clap defaulted" when merging with
/// config-file and env-var sources.
#[derive(Debug, Args)]
pub struct ServeCmd {
    /// Address to bind the HTTP listener on. Default `127.0.0.1:4329`.
    /// Non-loopback bind logs a warning recommending a reverse proxy.
    #[arg(long, value_name = "ADDR")]
    pub bind: Option<SocketAddr>,

    /// Path to the `SQLite` store. Overrides config-file `[storage] path`
    /// and the `AGENTPROF_STORAGE_PATH` env var.
    #[arg(long, value_name = "PATH", env = "AGENTPROF_STORAGE_PATH")]
    pub storage_path: Option<PathBuf>,

    /// Browser-side default poll interval in seconds. Range 1..=60.
    /// User can override per-tab via the toolbar (persisted in localStorage).
    #[arg(long, value_name = "S", value_parser = clap::value_parser!(u8).range(1..=60))]
    pub interval_default: Option<u8>,

    /// Skip the default "open browser on start" behavior.
    #[arg(long)]
    pub no_open: bool,

    /// Suppress per-request tracing output.
    #[arg(long)]
    pub quiet: bool,
}

/// Sync entry point: T5 will wrap this in a tokio runtime + the axum
/// listener. T3 wires the storage-path resolution + DB open so the
/// downstream handler tasks have an [`state::AppState`] to share.
///
/// # Errors
///
/// Returns `anyhow::Error` carrying an `ExitKind` per
/// `docs/architecture.md` §8.1:
///
/// - [`crate::cmd::exit::ExitKind::UserError`] when `--storage-path`
///   is missing or points to a non-existent file.
/// - [`crate::cmd::exit::ExitKind::DataError`] when the `SQLite` store
///   cannot be opened or migrated.
#[allow(
    clippy::needless_pass_by_value,
    reason = "T5 will move `cmd` into the axum runtime; consuming by value keeps the signature stable"
)]
pub fn run(cmd: ServeCmd) -> Result<()> {
    // Resolve storage path: CLI flag > env > config-file > default XDG.
    // (T4 adds config-file resolution; this commit handles CLI flag + env only —
    // the `AGENTPROF_STORAGE_PATH` env var is wired via clap `env = ...` on the field.)
    let storage_path = cmd.storage_path.clone().ok_or_else(|| {
        crate::cmd::exit::ExitKind::UserError.into_anyhow(
            "agentprof serve requires --storage-path (or AGENTPROF_STORAGE_PATH env / [storage] path config); \
             run `agentprof db init` then `agentprof db ingest` first".to_string(),
        )
    })?;
    if !storage_path.exists() {
        return Err(crate::cmd::exit::ExitKind::UserError.into_anyhow(format!(
            "storage path does not exist: {} — run `agentprof db init --storage-path <PATH>` first",
            storage_path.display(),
        )));
    }

    let db = agentprof_storage::Db::open_and_migrate(&storage_path).map_err(|e| {
        crate::cmd::exit::ExitKind::DataError.into_anyhow(format!(
            "open SQLite store at {}: {e}",
            storage_path.display()
        ))
    })?;

    let interval_default = cmd.interval_default.unwrap_or(5);
    let _state = state::AppState {
        db: Arc::new(Mutex::new(db)),
        interval_default,
    };

    // T5 wires the actual axum listener; for now, prove the DB opens cleanly.
    tracing::info!(
        path = %storage_path.display(),
        interval_default,
        "agentprof serve: store opened (T5 wires the listener)"
    );
    Ok(())
}

#[cfg(test)]
mod state_wire_tests {
    use super::{run, ServeCmd};
    use std::path::PathBuf;

    fn args(storage: Option<PathBuf>) -> ServeCmd {
        ServeCmd {
            bind: None,
            storage_path: storage,
            interval_default: None,
            no_open: true,
            quiet: true,
        }
    }

    #[test]
    fn run_without_storage_path_exits_user_error() {
        let res = run(args(None));
        assert!(res.is_err());
        let err = res.unwrap_err();
        let kind = err.downcast_ref::<crate::cmd::exit::ExitKind>().copied();
        assert!(matches!(kind, Some(crate::cmd::exit::ExitKind::UserError)));
    }

    #[test]
    fn run_with_missing_storage_file_exits_user_error() {
        let bogus = PathBuf::from("/nonexistent/path/agentprof.db");
        let res = run(args(Some(bogus)));
        assert!(res.is_err());
        let kind = res
            .unwrap_err()
            .downcast_ref::<crate::cmd::exit::ExitKind>()
            .copied();
        assert!(matches!(kind, Some(crate::cmd::exit::ExitKind::UserError)));
    }
}
