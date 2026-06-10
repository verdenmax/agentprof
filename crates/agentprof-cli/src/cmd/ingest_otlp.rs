//! `agentprof ingest-otlp` — run the embedded OTLP receiver and persist
//! incoming sessions to the local `SQLite` store (M2.2 T8.1).
//!
//! Wires up the M2.2 receiver subsystem end-to-end:
//!
//! ```text
//! clap args ─▶ OtlpServerConfig + SessionBufferCaps
//!                            │
//!                            ▼
//!     StorageFlushSink(Db) ─▶ SessionRouter ─▶ IngestPipeline
//!                            │                  │
//!                            ├─▶ serve_grpc ◀──┤
//!                            ├─▶ serve_http ◀──┤
//!                            └─▶ spawn_idle_sweeper
//! ```
//!
//! After binding the configured listeners the command awaits SIGINT
//! (`Ctrl-C`) or SIGTERM and then drains every open session buffer
//! through the storage sink before returning. The graceful shutdown
//! order — stop accepting → join servers → flush sweeper — guarantees
//! that no in-flight OTLP request races with the final
//! `flush_all(Shutdown)` sweep.
//!
//! Config-file (`[otlp]` block) integration is intentionally **out of
//! scope** for T8.1: only CLI args + the documented defaults are
//! honored here. T8.2 will fold a `PartialOtlpServerConfig` from disk
//! between defaults and CLI overrides.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Args;

use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::otlp::config::OtlpServerConfig;
use agentprof_storage::otlp::pipeline::IngestPipeline;
use agentprof_storage::otlp::router::{SessionBufferCaps, SessionRouter};
use agentprof_storage::otlp::server_grpc::serve_grpc;
use agentprof_storage::otlp::server_http::serve_http;
use agentprof_storage::otlp::sink_storage::StorageFlushSink;
use agentprof_storage::otlp::sweeper::spawn_idle_sweeper;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;
use agentprof_cli::config::resolve_storage_config;

const DEFAULT_MAX_SESSION_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_SESSION_EVENTS: usize = 100_000;
const DEFAULT_IDLE_SECONDS: u64 = 300;
const SWEEPER_INTERVAL_SECONDS: u64 = 30;

/// CLI arguments for `agentprof ingest-otlp`.
///
/// See module docs and `docs/architecture.md` §8 for the canonical
/// CLI specification.
#[derive(Debug, Args)]
pub struct IngestOtlpCmd {
    /// Address to bind the gRPC listener on (e.g., `127.0.0.1:4317`).
    /// Pass `--no-grpc` to disable.
    #[arg(long, value_name = "ADDR")]
    pub grpc: Option<SocketAddr>,

    /// Disable the gRPC listener (must explicitly enable HTTP via `--http`).
    #[arg(long, conflicts_with = "grpc")]
    pub no_grpc: bool,

    /// Address to bind the HTTP/protobuf listener on (e.g., `127.0.0.1:4318`).
    #[arg(long, value_name = "ADDR")]
    pub http: Option<SocketAddr>,

    /// Disable the HTTP listener.
    #[arg(long, conflicts_with = "http")]
    pub no_http: bool,

    /// Shared bearer token required on `Authorization: Bearer <token>`.
    /// If omitted, auth is disabled.
    #[arg(long, value_name = "TOKEN", env = "AGENTPROF_OTLP_TOKEN")]
    pub bearer_token: Option<String>,

    /// Path to server TLS certificate (PEM). Both `--tls-cert` and
    /// `--tls-key` must be set together to enable TLS.
    #[arg(long, value_name = "PATH", requires = "tls_key")]
    pub tls_cert: Option<PathBuf>,

    /// Path to server TLS private key (PEM).
    #[arg(long, value_name = "PATH", requires = "tls_cert")]
    pub tls_key: Option<PathBuf>,

    /// Path to client CA PEM bundle. Setting this enables mTLS.
    #[arg(long, value_name = "PATH", requires = "tls_cert")]
    pub client_ca: Option<PathBuf>,

    /// Maximum bytes per session buffer (default 16777216 = 16 MiB).
    #[arg(long, value_name = "BYTES")]
    pub max_session_bytes: Option<usize>,

    /// Maximum events per session buffer (default 100000).
    #[arg(long, value_name = "N")]
    pub max_session_events: Option<usize>,

    /// Idle timeout before flushing an inactive session (default 300).
    #[arg(long, value_name = "SECONDS")]
    pub idle_seconds: Option<u64>,

    /// Override the `SQLite` store path (default: resolved from config).
    #[arg(long, value_name = "PATH")]
    pub store: Option<PathBuf>,
}

/// Entry point for `agentprof ingest-otlp`.
///
/// Builds a multi-thread tokio runtime on demand so the rest of the
/// CLI stays sync-flavored. Blocks until shutdown completes.
///
/// # Errors
///
/// - [`ExitKind::UserError`] for invalid flag combinations
///   (e.g., both listeners disabled) or storage-config resolution failures.
/// - [`ExitKind::DataError`] if the `SQLite` store cannot be opened.
/// - [`ExitKind::OutputError`] if any listener fails to bind or a
///   server task panics during shutdown.
pub fn run(cmd: IngestOtlpCmd, storage_path: Option<PathBuf>) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime for ingest-otlp")?;
    runtime.block_on(run_async(cmd, storage_path))
}

async fn run_async(cmd: IngestOtlpCmd, storage_path: Option<PathBuf>) -> Result<()> {
    let cfg = build_otlp_server_config(&cmd)?;

    let store_path = if let Some(explicit) = cmd.store.clone() {
        explicit
    } else {
        let resolved = resolve_storage_config(PartialStorageConfig::default(), storage_path)
            .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
        resolved.path
    };

    let db = Db::open_and_migrate(&store_path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", store_path.display()))
    })?;
    let storage = Arc::new(Mutex::new(db));

    let sink = Arc::new(StorageFlushSink::new(storage));
    let caps = SessionBufferCaps::default()
        .with_max_bytes(cmd.max_session_bytes.unwrap_or(DEFAULT_MAX_SESSION_BYTES))
        .with_max_events(cmd.max_session_events.unwrap_or(DEFAULT_MAX_SESSION_EVENTS))
        .with_idle_timeout(Duration::from_secs(
            cmd.idle_seconds.unwrap_or(DEFAULT_IDLE_SECONDS),
        ));
    let router = Arc::new(SessionRouter::new(caps, sink));
    let pipeline = Arc::new(IngestPipeline::new(router.clone()));
    let sweeper = spawn_idle_sweeper(router, Duration::from_secs(SWEEPER_INTERVAL_SECONDS));

    let grpc_handle = if let Some(addr) = cfg.listen_grpc {
        let (join, shutdown_tx) = serve_grpc(cfg.clone(), pipeline.clone())
            .await
            .map_err(|e| {
                ExitKind::OutputError.into_anyhow(format!("bind OTLP gRPC on {addr}: {e}"))
            })?;
        tracing::info!(%addr, "OTLP gRPC listener bound");
        Some((join, shutdown_tx))
    } else {
        None
    };
    let http_handle = if let Some(addr) = cfg.listen_http {
        let (join, shutdown_tx) = serve_http(cfg.clone(), pipeline.clone())
            .await
            .map_err(|e| {
                ExitKind::OutputError.into_anyhow(format!("bind OTLP HTTP on {addr}: {e}"))
            })?;
        tracing::info!(%addr, "OTLP HTTP listener bound");
        Some((join, shutdown_tx))
    } else {
        None
    };

    wait_for_shutdown().await?;

    tracing::info!("shutdown signal received, draining buffers");

    if let Some((join, shutdown_tx)) = grpc_handle {
        let _ = shutdown_tx.send(());
        join.await
            .map_err(|e| ExitKind::OutputError.into_anyhow(format!("OTLP gRPC task join: {e}")))?
            .map_err(|e| {
                ExitKind::OutputError.into_anyhow(format!("OTLP gRPC server exit: {e}"))
            })?;
    }
    if let Some((join, shutdown_tx)) = http_handle {
        let _ = shutdown_tx.send(());
        join.await
            .map_err(|e| ExitKind::OutputError.into_anyhow(format!("OTLP HTTP task join: {e}")))?
            .map_err(|e| {
                ExitKind::OutputError.into_anyhow(format!("OTLP HTTP server exit: {e}"))
            })?;
    }

    sweeper
        .shutdown()
        .await
        .map_err(|e| ExitKind::OutputError.into_anyhow(format!("sweeper shutdown: {e}")))?;

    tracing::info!("OTLP receiver stopped cleanly");
    Ok(())
}

/// Build an [`OtlpServerConfig`] from CLI args + documented defaults.
///
/// Validation rules:
/// - At least one of gRPC / HTTP must be enabled.
/// - `--tls-cert` / `--tls-key` pairing and `--client-ca` requiring
///   `--tls-cert` are already enforced by clap (`requires = …`).
fn build_otlp_server_config(cmd: &IngestOtlpCmd) -> Result<OtlpServerConfig> {
    if cmd.no_grpc && cmd.no_http {
        return Err(ExitKind::UserError.into_anyhow(
            "at least one of --grpc / --http must be enabled (got both --no-grpc and --no-http)"
                .to_string(),
        ));
    }

    let mut cfg = OtlpServerConfig::default();

    cfg.listen_grpc = if cmd.no_grpc {
        None
    } else {
        Some(cmd.grpc.unwrap_or_else(default_grpc_addr))
    };
    cfg.listen_http = if cmd.no_http {
        None
    } else {
        Some(cmd.http.unwrap_or_else(default_http_addr))
    };
    cfg.listen_token.clone_from(&cmd.bearer_token);
    cfg.tls_cert.clone_from(&cmd.tls_cert);
    cfg.tls_key.clone_from(&cmd.tls_key);
    cfg.tls_client_ca.clone_from(&cmd.client_ca);
    cfg.session_idle_timeout =
        Duration::from_secs(cmd.idle_seconds.unwrap_or(DEFAULT_IDLE_SECONDS));

    cfg.validate().map_err(|e| {
        ExitKind::UserError.into_anyhow(format!("invalid OTLP receiver config: {e}"))
    })?;
    Ok(cfg)
}

const fn default_grpc_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4317)
}

const fn default_http_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4318)
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|e| ExitKind::OutputError.into_anyhow(format!("install SIGTERM handler: {e}")))?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        _ = sigterm.recv() => tracing::info!("SIGTERM received"),
    }
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<()> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| ExitKind::OutputError.into_anyhow(format!("install Ctrl-C handler: {e}")))?;
    tracing::info!("Ctrl-C received");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(no_grpc: bool, no_http: bool) -> IngestOtlpCmd {
        IngestOtlpCmd {
            grpc: None,
            no_grpc,
            http: None,
            no_http,
            bearer_token: None,
            tls_cert: None,
            tls_key: None,
            client_ca: None,
            max_session_bytes: None,
            max_session_events: None,
            idle_seconds: None,
            store: None,
        }
    }

    #[test]
    fn build_config_defaults_to_loopback_on_both_listeners() {
        let cfg = build_otlp_server_config(&args(false, false)).expect("defaults validate");
        assert_eq!(cfg.listen_grpc.unwrap().to_string(), "127.0.0.1:4317");
        assert_eq!(cfg.listen_http.unwrap().to_string(), "127.0.0.1:4318");
    }

    #[test]
    fn build_config_rejects_both_listeners_disabled() {
        let err = build_otlp_server_config(&args(true, true)).expect_err("must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("at least one of"), "got: {msg}");
    }

    #[test]
    fn build_config_no_grpc_keeps_http_only() {
        let cfg = build_otlp_server_config(&args(true, false)).expect("http-only validates");
        assert!(cfg.listen_grpc.is_none());
        assert!(cfg.listen_http.is_some());
    }
}
