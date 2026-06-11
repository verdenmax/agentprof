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

use std::net::SocketAddr;
use std::path::PathBuf;

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

/// Sync entry point: T3 will wire in a tokio runtime + the actual handler.
/// For now, prove the subcommand dispatches correctly by echoing the
/// parsed arguments to stdout.
///
/// # Errors
///
/// Returns `anyhow::Error` carrying an `ExitKind` per
/// `docs/architecture.md` §8.1. The current stub never fails; the
/// `Result` return type is reserved for the T3 runtime + handler wiring.
#[allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    reason = "stub: T3 will consume `cmd` and propagate fallible runtime errors"
)]
pub fn run(cmd: ServeCmd) -> Result<()> {
    println!(
        "agentprof serve: bind={:?} storage_path={:?} interval_default={:?} no_open={} quiet={}",
        cmd.bind, cmd.storage_path, cmd.interval_default, cmd.no_open, cmd.quiet,
    );
    Ok(())
}
