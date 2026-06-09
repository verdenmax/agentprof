//! `agentprof db ingest` — batch-import sessions from an adapter.
//!
//! Selects sessions via mutually-exclusive `--since DUR` / `--all` /
//! `--session ID`, calls
//! [`SessionDataSource::load_session`] for each, and upserts the
//! resulting
//! [`AnalysisReport`](agentprof_core::analyzer::AnalysisReport) into
//! the `SQLite` store via
//! [`agentprof_storage::upsert::upsert_report`].
//!
//! Per-session failures are logged via `tracing` and counted; the
//! overall command always exits `0` unless argument resolution
//! itself fails (`ExitKind::UserError`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::{ArgGroup, Args};

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_adapters::AdapterDataSource;
use agentprof_cli::config::resolve_storage_config;
use agentprof_core::datasource::SessionDataSource;
use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::upsert::upsert_report;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;
use crate::cmd::since::parse_since;

/// Arguments for `agentprof db ingest`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
#[command(group(
    ArgGroup::new("ingest_scope")
        .args(["since", "all", "session"])
        .required(true),
))]
pub struct IngestArgs {
    /// Adapter / agent name. Today only `copilot` is wired.
    #[arg(long, default_value = "copilot")]
    pub agent: String,

    /// Override the agent's default session root directory.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Ingest sessions modified within this window
    /// (`<N>d`/`h`/`m`/`s` or `all`).
    #[arg(long, group = "ingest_scope")]
    pub since: Option<String>,

    /// Ingest every session the adapter can find.
    #[arg(long, group = "ingest_scope")]
    pub all: bool,

    /// Ingest exactly one session by id.
    #[arg(long, group = "ingest_scope")]
    pub session: Option<String>,
}

/// Run `agentprof db ingest`.
///
/// # Errors
///
/// - [`ExitKind::UserError`] for unknown agent, bad `--since`, missing
///   root, or none of `--since` / `--all` / `--session` supplied.
/// - [`ExitKind::DataError`] if the storage layer cannot be opened.
#[allow(clippy::needless_pass_by_value)]
pub fn run(args: IngestArgs, storage_path: Option<PathBuf>) -> Result<()> {
    if args.agent != "copilot" {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "unsupported --agent: {} (only `copilot` is wired today)",
            args.agent
        )));
    }

    let adapter = Arc::new(CopilotAdapter);
    let root = args.root.clone().or_else(|| {
        use agentprof_core::adapter::Adapter as _;
        CopilotAdapter.default_session_root()
    });
    let root = root.ok_or_else(|| {
        ExitKind::UserError
            .into_anyhow("could not determine session root (set --root or HOME)".to_owned())
    })?;
    if !root.is_dir() {
        return Err(
            ExitKind::UserError.into_anyhow(format!("session root not found: {}", root.display()))
        );
    }

    let ds = AdapterDataSource::new(adapter, root.clone());

    let cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let mut db = Db::open_and_migrate(&cfg.path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", cfg.path.display()))
    })?;

    let targets: Vec<(String, PathBuf)> = if let Some(id) = &args.session {
        let refs = ds.discover(Duration::MAX).map_err(|e| {
            ExitKind::DataError.into_anyhow(format!("discover {}: {e}", root.display()))
        })?;
        let hit = refs.into_iter().find(|r| r.id == *id).ok_or_else(|| {
            ExitKind::UserError.into_anyhow(format!("session id not found: {id}"))
        })?;
        vec![(hit.id, hit.raw_path.unwrap_or_default())]
    } else {
        let window = if args.all {
            Duration::MAX
        } else if let Some(s) = &args.since {
            parse_since(s)
                .map_err(|e| ExitKind::UserError.into_anyhow(format!("invalid --since: {e}")))?
        } else {
            return Err(
                ExitKind::UserError.into_anyhow("must specify --since, --all, or --session".into())
            );
        };
        ds.discover(window)
            .map_err(|e| {
                ExitKind::DataError.into_anyhow(format!("discover {}: {e}", root.display()))
            })?
            .into_iter()
            .map(|r| (r.id, r.raw_path.unwrap_or_default()))
            .collect()
    };

    let total = targets.len();
    eprintln!(
        "agentprof: ingesting {total} session(s) from agent={} root={}",
        args.agent,
        root.display()
    );
    let mut ok = 0usize;
    let mut fail = 0usize;
    for (idx, (id, raw_path)) in targets.iter().enumerate() {
        match ds.load_session(id) {
            Ok(report) => {
                let now_secs = chrono::Utc::now().timestamp();
                match upsert_report(&mut db, &report, raw_path, now_secs) {
                    Ok(_) => ok += 1,
                    Err(e) => {
                        tracing::error!(session = %id, error = %e, "upsert failed");
                        fail += 1;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(session = %id, error = %e, "load failed");
                fail += 1;
            }
        }
        if (idx + 1) % 10 == 0 || idx + 1 == total {
            eprintln!("  {} / {} done", idx + 1, total);
        }
    }
    eprintln!("agentprof: ingested {ok} ok, {fail} failed");
    Ok(())
}
