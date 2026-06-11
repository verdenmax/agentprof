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

mod handlers;
mod router;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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

/// Entry point for `agentprof serve`.
///
/// Builds a multi-threaded tokio runtime and dispatches to
/// [`run_async`], which:
///
/// 1. resolves the merged `[serve]` config (CLI > file > defaults);
/// 2. opens (and migrates) the `SQLite` store at `--storage-path`;
/// 3. assembles an [`state::AppState`] and builds the axum router;
/// 4. binds a TCP listener and serves until SIGINT/SIGTERM.
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
/// - [`crate::cmd::exit::ExitKind::OutputError`] when the tokio
///   runtime cannot be built, the listener cannot bind, or
///   `axum::serve` returns an I/O error.
pub fn run(cmd: ServeCmd) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            crate::cmd::exit::ExitKind::OutputError.into_anyhow(format!("build tokio runtime: {e}"))
        })?;
    rt.block_on(run_async(cmd))
}

async fn run_async(cmd: ServeCmd) -> Result<()> {
    // T4: read the optional [serve] block from the user's config file
    // (best-effort: missing/malformed → ignored with a warn log) and
    // merge with CLI flags via the same priority chain documented on
    // [`resolve_serve_config`]: CLI > file > built-in default.
    let file_partial = load_partial_serve_from_disk();
    let resolved = resolve_serve_config(&cmd, file_partial.as_ref())?;

    // Resolve storage path: CLI flag > env (wired via clap `env = ...`).
    // T4-config-file storage-path resolution is out of scope for T5.
    let storage_path = cmd.storage_path.ok_or_else(|| {
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

    let state = state::AppState::new(Arc::new(Mutex::new(db)), resolved.interval_default);
    let app = router::build_router(state);

    if resolved.bind.ip().is_loopback() {
        tracing::info!(addr = %resolved.bind, "agentprof serve listening");
    } else {
        tracing::warn!(
            addr = %resolved.bind,
            "agentprof serve binding to non-loopback address — recommend reverse proxy for auth"
        );
    }

    let listener = tokio::net::TcpListener::bind(resolved.bind)
        .await
        .map_err(|e| {
            crate::cmd::exit::ExitKind::OutputError
                .into_anyhow(format!("bind {}: {e}", resolved.bind))
        })?;

    if resolved.auto_open {
        let url = format!("http://{}", resolved.bind);
        if let Err(e) = open::that(&url) {
            tracing::warn!(url = %url, error = %e, "failed to open browser (continuing)");
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(wait_for_shutdown())
        .await
        .map_err(|e| {
            crate::cmd::exit::ExitKind::OutputError.into_anyhow(format!("axum::serve: {e}"))
        })?;

    tracing::info!(
        path = %storage_path.display(),
        "agentprof serve stopped cleanly"
    );
    Ok(())
}

/// Await SIGINT (Ctrl-C) or SIGTERM on Unix; returns when either
/// arrives. If installing the SIGTERM handler fails (rare — would
/// require an exhausted FD table or seccomp), falls back to Ctrl-C
/// only after logging an error.
#[cfg(unix)]
async fn wait_for_shutdown() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(
                error = %e,
                "failed to install SIGTERM handler; falling back to Ctrl-C only"
            );
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        _ = sigterm.recv() => tracing::info!("SIGTERM received"),
    }
}

/// Non-Unix fallback: only Ctrl-C is wired (SIGTERM has no Windows
/// equivalent that tokio surfaces uniformly).
#[cfg(not(unix))]
async fn wait_for_shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Ctrl-C received");
}

/// Merged `serve` runtime config — CLI args overlaid on top of the
/// `[serve]` config-file block, with built-in defaults filling holes.
///
/// Produced by [`resolve_serve_config`]; consumed by T5+ handlers
/// (router builder, browser-open helper). Kept `pub(crate)` because
/// it is an internal contract between the resolver and the rest of
/// `cmd::serve`.
#[derive(Debug, Clone)]
struct ResolvedServeConfig {
    /// Resolved listener bind address. Built-in default
    /// `127.0.0.1:4329` per `docs/architecture.md` §8.x.
    pub bind: SocketAddr,
    /// Browser-side default poll interval in seconds (1..=60).
    pub interval_default: u8,
    /// Whether to open the user's browser on start. `--no-open` forces
    /// `false`; otherwise the file value (or `true` by default) wins.
    pub auto_open: bool,
}

/// Resolve a [`ResolvedServeConfig`] from CLI args + an optional
/// `[serve]` config-file partial.
///
/// Priority (highest first): CLI flag > `[serve]` file block > built-in
/// default (`bind = 127.0.0.1:4329`, `interval_default = 5`,
/// `auto_open = true`). The CLI's `--no-open` is a negative flag: when
/// set it forces `auto_open = false` regardless of the file value.
///
/// Mirrors the M2.2 T8.2 OTLP resolver pattern in
/// [`crate::cmd::ingest_otlp`] so users see uniform precedence
/// semantics across subcommands.
///
/// # Errors
///
/// Returns an [`anyhow::Error`] wrapping
/// [`crate::cmd::exit::ExitKind::UserError`] when:
///
/// - the config-file `bind` string is not a parseable
///   [`SocketAddr`] (e.g. `"not-an-address"`); or
/// - `interval_default` (from either source) is outside the allowed
///   range `1..=60` — clap already enforces this on the CLI flag, so
///   in practice this catches a bad config-file value.
fn resolve_serve_config(
    cmd: &ServeCmd,
    file_partial: Option<&agentprof_cli::config::PartialServeConfig>,
) -> Result<ResolvedServeConfig> {
    let default_bind: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 4329));

    let bind = if let Some(b) = cmd.bind {
        b
    } else if let Some(fb) = file_partial.and_then(|p| p.bind.as_deref()) {
        fb.parse().map_err(|e| {
            crate::cmd::exit::ExitKind::UserError.into_anyhow(format!(
                "invalid [serve] bind {fb:?} in agentprof config: {e}; \
                 expected an ADDR:PORT literal like \"127.0.0.1:4329\""
            ))
        })?
    } else {
        default_bind
    };

    let interval_default = cmd
        .interval_default
        .or_else(|| file_partial.and_then(|p| p.interval_default))
        .unwrap_or(5);
    if !(1..=60).contains(&interval_default) {
        return Err(crate::cmd::exit::ExitKind::UserError.into_anyhow(format!(
            "[serve] interval_default out of range: {interval_default} \
             (allowed 1..=60)"
        )));
    }

    let auto_open = if cmd.no_open {
        false
    } else {
        file_partial.and_then(|p| p.auto_open).unwrap_or(true)
    };

    Ok(ResolvedServeConfig {
        bind,
        interval_default,
        auto_open,
    })
}

/// Best-effort load of the `[serve]` block from the user's config file.
///
/// Resolution order mirrors `cmd::ingest_otlp::load_partial_otlp_from_disk`
/// (M2.2 T8.2): `$AGENTPROF_CONFIG` (if set) → the platform XDG config
/// dir. A missing file is silently treated as "no overrides"; a
/// malformed file is logged at `warn` level and likewise treated as
/// "no overrides" so the server can still start from CLI args +
/// defaults.
fn load_partial_serve_from_disk() -> Option<agentprof_cli::config::PartialServeConfig> {
    let path = resolve_config_file_path()?;
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e,
                "failed to read agentprof config file; ignoring [serve] overrides");
            return None;
        }
    };
    match agentprof_cli::config::parse_toml(&src) {
        Ok(cfg) => cfg.serve,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e,
                "failed to parse agentprof config file; ignoring [serve] overrides");
            None
        }
    }
}

fn resolve_config_file_path() -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("AGENTPROF_CONFIG") {
        return Some(PathBuf::from(custom));
    }
    let dirs = directories::BaseDirs::new()?;
    Some(
        Path::new(dirs.config_dir())
            .join("agentprof")
            .join("config.toml"),
    )
}

#[cfg(test)]
mod config_tests {
    use super::{resolve_serve_config, ServeCmd};
    use agentprof_cli::config::PartialServeConfig;
    use std::net::SocketAddr;

    fn cmd(bind: Option<SocketAddr>, interval: Option<u8>, no_open: bool) -> ServeCmd {
        ServeCmd {
            bind,
            storage_path: None,
            interval_default: interval,
            no_open,
            quiet: true,
        }
    }

    #[test]
    fn defaults_when_no_cli_no_file() {
        let r = resolve_serve_config(&cmd(None, None, false), None).expect("resolve");
        assert_eq!(r.bind.to_string(), "127.0.0.1:4329");
        assert_eq!(r.interval_default, 5);
        assert!(r.auto_open);
    }

    #[test]
    fn file_overrides_defaults() {
        let mut p = PartialServeConfig::default();
        p.bind = Some("0.0.0.0:9000".into());
        p.interval_default = Some(10);
        p.auto_open = Some(false);
        let r = resolve_serve_config(&cmd(None, None, false), Some(&p)).expect("resolve");
        assert_eq!(r.bind.to_string(), "0.0.0.0:9000");
        assert_eq!(r.interval_default, 10);
        assert!(!r.auto_open);
    }

    #[test]
    fn cli_overrides_file_overrides_defaults() {
        let mut p = PartialServeConfig::default();
        p.bind = Some("0.0.0.0:9000".into());
        p.interval_default = Some(10);
        p.auto_open = Some(false);
        let cli_bind: SocketAddr = "127.0.0.1:7777".parse().expect("valid literal");
        let r =
            resolve_serve_config(&cmd(Some(cli_bind), Some(2), true), Some(&p)).expect("resolve");
        assert_eq!(r.bind.to_string(), "127.0.0.1:7777");
        assert_eq!(r.interval_default, 2);
        assert!(!r.auto_open);
    }

    #[test]
    fn malformed_file_bind_errors_user() {
        let mut p = PartialServeConfig::default();
        p.bind = Some("not-an-address".into());
        let r = resolve_serve_config(&cmd(None, None, false), Some(&p));
        assert!(r.is_err());
        let err = r.expect_err("should error");
        assert!(matches!(
            err.downcast_ref::<crate::cmd::exit::ExitKind>(),
            Some(crate::cmd::exit::ExitKind::UserError)
        ));
    }
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

#[cfg(test)]
mod router_tests {
    //! Per-route unit tests via [`tower::ServiceExt::oneshot`] — no
    //! TCP listener, no signal handling. T5 covers `/healthz` plus a
    //! 404 sanity check; T6+ extends with view-handler tests.
    //!
    //! Lives inline (rather than in `tests/cli_serve_router_unit.rs`)
    //! because `cmd::serve::{build_router, AppState}` are only
    //! reachable from within the bin crate — the lib facade in
    //! `src/lib.rs` deliberately omits `mod cmd` to avoid colliding
    //! with the `agentprof_cli::config::...` self-references that
    //! sibling subcommand files (`db/*.rs`, `analyze.rs`, etc.)
    //! depend on being resolved as an extern crate.

    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::router::build_router;
    use super::state::AppState;

    fn empty_db_state() -> (tempfile::NamedTempFile, AppState) {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let db = agentprof_storage::Db::open_and_migrate(tmp.path()).expect("open");
        let state = AppState::new(Arc::new(Mutex::new(db)), 5);
        (tmp, state)
    }

    #[tokio::test]
    async fn healthz_returns_200_with_healthy_body() {
        let (_tmp, state) = empty_db_state();
        let app = build_router(state);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/healthz")
            .body(Body::empty())
            .expect("build req");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes();
        assert_eq!(&body[..], b"healthy");
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let (_tmp, state) = empty_db_state();
        let app = build_router(state);
        let req = Request::builder()
            .method(Method::GET)
            .uri("/nonexistent")
            .body(Body::empty())
            .expect("build req");
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
