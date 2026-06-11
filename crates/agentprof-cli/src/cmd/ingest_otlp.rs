//! `agentprof ingest-otlp` — run the embedded OTLP receiver and persist
//! incoming sessions to the local `SQLite` store (M2.2 T8.1 + T8.2).
//!
//! Wires up the M2.2 receiver subsystem end-to-end:
//!
//! ```text
//! [otlp] file partial ─┐
//!                      ▼
//! clap args ─▶ build_otlp_server_config ─▶ OtlpServerConfig
//!                            │            + SessionBufferCaps
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
//! ## Config priority (T8.2)
//!
//! For every field of [`OtlpServerConfig`], values are resolved in
//! this order (highest priority first):
//!
//! 1. CLI flags (`--grpc`, `--http`, `--bearer-token`, …).
//! 2. The `AGENTPROF_OTLP_TOKEN` environment variable — `clap`'s
//!    `env = …` attribute folds it into `--bearer-token`, so it
//!    shares priority 1 from the merge function's perspective.
//! 3. The `[otlp]` block from `$AGENTPROF_CONFIG` or
//!    `~/.config/agentprof/config.toml`.
//! 4. The built-in defaults documented on
//!    [`OtlpServerConfig::default`].

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Args;

use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::otlp::config::{OtlpServerConfig, PartialOtlpServerConfig};
use agentprof_storage::otlp::pipeline::IngestPipeline;
use agentprof_storage::otlp::router::{SessionBufferCaps, SessionRouter};
use agentprof_storage::otlp::server_grpc::serve_grpc;
use agentprof_storage::otlp::server_http::serve_http;
use agentprof_storage::otlp::sink_storage::StorageFlushSink;
use agentprof_storage::otlp::sweeper::spawn_idle_sweeper;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;
use agentprof_cli::config::{parse_toml, resolve_storage_config};

const DEFAULT_MAX_SESSION_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_SESSION_EVENTS: usize = 100_000;
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

    /// Maximum decoded protobuf bytes accepted on the Logs endpoint.
    /// Default 8388608 (8 MiB). See [ADR-0022] D-2.
    ///
    /// [ADR-0022]: ../../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md
    #[arg(long, value_name = "BYTES")]
    pub max_logs_request_bytes: Option<usize>,

    /// Maximum decoded protobuf bytes accepted on the Metrics endpoint.
    /// Default 2097152 (2 MiB).
    #[arg(long, value_name = "BYTES")]
    pub max_metrics_request_bytes: Option<usize>,

    /// Maximum decoded protobuf bytes accepted on the Traces endpoint.
    /// Default 8388608 (8 MiB).
    #[arg(long, value_name = "BYTES")]
    pub max_traces_request_bytes: Option<usize>,

    /// Maximum concurrent sessions tracked by the router. LRU eviction
    /// triggers when exceeded. Default 1024.
    #[arg(long, value_name = "N")]
    pub max_open_sessions: Option<usize>,

    /// Override the idle-sweeper tick interval, in seconds. Hidden
    /// from `--help`; intended for end-to-end tests that need the
    /// sweeper to fire well below the production default of
    /// [`SWEEPER_INTERVAL_SECONDS`]. Defaults to that constant.
    #[arg(long, value_name = "SECONDS", hide = true)]
    pub sweeper_interval_seconds: Option<u64>,
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
    let file_partial = load_partial_otlp_from_disk();
    let cfg = build_otlp_server_config(&cmd, file_partial)?;

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
        .with_idle_timeout(cfg.session_idle_timeout)
        .with_max_open_sessions(cfg.max_open_sessions);
    let router = Arc::new(SessionRouter::new(caps, sink));
    let pipeline = Arc::new(IngestPipeline::new(router.clone()));
    let sweeper_interval = Duration::from_secs(
        cmd.sweeper_interval_seconds
            .unwrap_or(SWEEPER_INTERVAL_SECONDS),
    );
    let sweeper = spawn_idle_sweeper(router, sweeper_interval);

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

/// Build an [`OtlpServerConfig`] from CLI args, an optional config-file
/// partial, and the documented defaults.
///
/// Merge priority (highest first):
///
/// 1. CLI flags (`cmd.grpc`, `cmd.http`, `cmd.bearer_token`, …) — and
///    the `AGENTPROF_OTLP_TOKEN` env var folded into `--bearer-token`
///    by `clap` via `env = …`.
/// 2. Fields present in `file_partial` (parsed from the `[otlp]` block
///    of the user's config file).
/// 3. Built-in defaults from [`OtlpServerConfig::default`] (loopback
///    listeners on the standard OTLP ports).
///
/// `cmd.no_grpc` / `cmd.no_http` are explicit "disable" toggles and
/// always override any positive value from file or defaults. clap
/// enforces `--tls-cert` ↔ `--tls-key` pairing and
/// `--client-ca` ⇒ `--tls-cert`, so this function trusts those
/// invariants for CLI-supplied paths.
///
/// # Errors
///
/// - [`ExitKind::UserError`] when both listeners are explicitly
///   disabled (`--no-grpc` + `--no-http`).
/// - [`ExitKind::UserError`] when the file partial fails to resolve
///   (malformed address / duration), or when the merged config fails
///   [`OtlpServerConfig::validate`] (e.g. mismatched TLS pair coming
///   from the file plus CLI).
fn build_otlp_server_config(
    cmd: &IngestOtlpCmd,
    file_partial: Option<PartialOtlpServerConfig>,
) -> Result<OtlpServerConfig> {
    if cmd.no_grpc && cmd.no_http {
        return Err(ExitKind::UserError.into_anyhow(
            "at least one of --grpc / --http must be enabled (got both --no-grpc and --no-http)"
                .to_string(),
        ));
    }

    let mut cfg =
        OtlpServerConfig::from_partial(file_partial.unwrap_or_default()).map_err(|e| {
            ExitKind::UserError.into_anyhow(format!("invalid [otlp] config-file block: {e}"))
        })?;

    if cmd.no_grpc {
        cfg.listen_grpc = None;
    } else if let Some(addr) = cmd.grpc {
        cfg.listen_grpc = Some(addr);
    }

    if cmd.no_http {
        cfg.listen_http = None;
    } else if let Some(addr) = cmd.http {
        cfg.listen_http = Some(addr);
    }

    if let Some(token) = &cmd.bearer_token {
        cfg.listen_token = Some(token.clone());
    }
    if let Some(p) = &cmd.tls_cert {
        cfg.tls_cert = Some(p.clone());
    }
    if let Some(p) = &cmd.tls_key {
        cfg.tls_key = Some(p.clone());
    }
    if let Some(p) = &cmd.client_ca {
        cfg.tls_client_ca = Some(p.clone());
    }
    if let Some(secs) = cmd.idle_seconds {
        cfg.session_idle_timeout = Duration::from_secs(secs);
    }
    if let Some(n) = cmd.max_logs_request_bytes {
        cfg.max_logs_request_bytes = n;
    }
    if let Some(n) = cmd.max_metrics_request_bytes {
        cfg.max_metrics_request_bytes = n;
    }
    if let Some(n) = cmd.max_traces_request_bytes {
        cfg.max_traces_request_bytes = n;
    }
    if let Some(n) = cmd.max_open_sessions {
        cfg.max_open_sessions = n;
    }

    cfg.validate().map_err(|e| {
        ExitKind::UserError.into_anyhow(format!("invalid OTLP receiver config: {e}"))
    })?;
    Ok(cfg)
}

/// Best-effort load of the `[otlp]` block from the user's config file.
///
/// Resolution order: `$AGENTPROF_CONFIG` (if set) → the platform
/// XDG config dir (`$XDG_CONFIG_HOME/agentprof/config.toml` on Linux,
/// equivalent on macOS / Windows via `directories`). A missing file is
/// silently treated as "no overrides"; a malformed file is logged at
/// `warn` level and likewise treated as "no overrides" so the receiver
/// can still start from CLI args + defaults.
fn load_partial_otlp_from_disk() -> Option<PartialOtlpServerConfig> {
    let path = resolve_config_file_path()?;
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e,
                "failed to read agentprof config file; ignoring [otlp] overrides");
            return None;
        }
    };
    match parse_toml(&src) {
        Ok(cfg) => cfg.otlp,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e,
                "failed to parse agentprof config file; ignoring [otlp] overrides");
            None
        }
    }
}

fn resolve_config_file_path() -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("AGENTPROF_CONFIG") {
        return Some(PathBuf::from(custom));
    }
    let dirs = directories::BaseDirs::new()?;
    Some(dirs.config_dir().join("agentprof").join("config.toml"))
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
            max_logs_request_bytes: None,
            max_metrics_request_bytes: None,
            max_traces_request_bytes: None,
            max_open_sessions: None,
            sweeper_interval_seconds: None,
        }
    }

    #[test]
    fn build_config_defaults_to_loopback_on_both_listeners() {
        let cfg = build_otlp_server_config(&args(false, false), None).expect("defaults validate");
        assert_eq!(cfg.listen_grpc.unwrap().to_string(), "127.0.0.1:4317");
        assert_eq!(cfg.listen_http.unwrap().to_string(), "127.0.0.1:4318");
    }

    #[test]
    fn build_config_rejects_both_listeners_disabled() {
        let err = build_otlp_server_config(&args(true, true), None).expect_err("must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("at least one of"), "got: {msg}");
    }

    #[test]
    fn build_config_no_grpc_keeps_http_only() {
        let cfg = build_otlp_server_config(&args(true, false), None).expect("http-only validates");
        assert!(cfg.listen_grpc.is_none());
        assert!(cfg.listen_http.is_some());
    }

    // ------------------------------------------------------------------
    // T8.2: [otlp] config-file block + CLI override merge
    // ------------------------------------------------------------------

    /// File partial provides defaults when CLI omits flags.
    #[test]
    fn t82_config_file_provides_defaults_when_cli_omits_flags() {
        let file = PartialOtlpServerConfig {
            listen_grpc: Some("127.0.0.1:9317".to_string()),
            listen_http: Some(String::new()), // disable HTTP from file
            listen_token: Some("from-file".to_string()),
            session_idle_timeout: Some("10m".to_string()),
            ..Default::default()
        };
        let cfg = build_otlp_server_config(&args(false, false), Some(file))
            .expect("file partial validates");
        assert_eq!(cfg.listen_grpc.unwrap().to_string(), "127.0.0.1:9317");
        assert!(
            cfg.listen_http.is_none(),
            "file-disabled HTTP must stay disabled when CLI doesn't set --http"
        );
        assert_eq!(cfg.listen_token.as_deref(), Some("from-file"));
        assert_eq!(cfg.session_idle_timeout, Duration::from_secs(600));
    }

    /// CLI flags override the config-file block on a per-field basis.
    #[test]
    fn t82_cli_args_override_config_file() {
        let file = PartialOtlpServerConfig {
            listen_grpc: Some("127.0.0.1:4317".to_string()),
            listen_token: Some("from-file".to_string()),
            session_idle_timeout: Some("10m".to_string()),
            ..Default::default()
        };
        let mut a = args(false, false);
        a.grpc = Some("0.0.0.0:9000".parse().unwrap());
        a.bearer_token = Some("from-cli".to_string());
        a.idle_seconds = Some(42);

        let cfg = build_otlp_server_config(&a, Some(file)).expect("validates");
        assert_eq!(cfg.listen_grpc.unwrap().to_string(), "0.0.0.0:9000");
        assert_eq!(cfg.listen_token.as_deref(), Some("from-cli"));
        assert_eq!(cfg.session_idle_timeout, Duration::from_secs(42));
    }

    /// `None` file partial (i.e., no `[otlp]` block in the TOML) falls
    /// back to the built-in defaults — same as the pre-T8.2 behavior.
    #[test]
    fn t82_missing_otlp_block_uses_defaults() {
        let cfg = build_otlp_server_config(&args(false, false), None).expect("validates");
        assert_eq!(cfg.listen_grpc.unwrap().to_string(), "127.0.0.1:4317");
        assert_eq!(cfg.listen_http.unwrap().to_string(), "127.0.0.1:4318");
        assert!(cfg.listen_token.is_none());
        assert_eq!(cfg.session_idle_timeout, Duration::from_secs(300));
    }

    /// `--no-grpc` always wins even if the file partial sets a value.
    #[test]
    fn t82_cli_disable_flag_overrides_file_listener() {
        let file = PartialOtlpServerConfig {
            listen_grpc: Some("127.0.0.1:9317".to_string()),
            ..Default::default()
        };
        let cfg = build_otlp_server_config(&args(true, false), Some(file)).expect("validates");
        assert!(cfg.listen_grpc.is_none());
        assert!(cfg.listen_http.is_some());
    }
}
