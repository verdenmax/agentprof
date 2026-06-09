//! `agentprof db ingest` — batch-import sessions from an adapter.
//!
//! Selects sessions via mutually-exclusive `--since DUR` / `--all` /
//! `--session ID`, calls
//! [`AdapterDataSource::load_session_by_ref`] for each, and upserts
//! the resulting
//! [`AnalysisReport`](agentprof_core::analyzer::AnalysisReport) into
//! the `SQLite` store via
//! [`agentprof_storage::upsert::upsert_report`].
//!
//! Per-session failures are logged via `tracing` and counted; the
//! overall command exits `0` on partial success and **`2`
//! ([`ExitKind::DataError`])** when 100% of the discovered sessions
//! fail to ingest (M2.1 audit P1-4).
//!
//! ## Why `load_session_by_ref` and not the trait route
//!
//! `SessionDataSource::load_session(id)` re-runs
//! `Adapter::discover_sessions` and linearly searches for `id` —
//! O(N²) for an `ingest --all` over N sessions. The CLI already
//! holds the full [`AdapterRef`] list from its single up-front
//! `discover` call, so it calls
//! [`AdapterDataSource::load_session_by_ref`] to skip the rescan
//! (M2.1 audit P1-3).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::{ArgGroup, Args};

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_adapters::AdapterDataSource;
use agentprof_cli::config::resolve_storage_config;
use agentprof_core::adapter::{Adapter as _, SessionRef as AdapterRef};
use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::upsert::{upsert_episodes, upsert_report};
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
/// - [`ExitKind::DataError`] if the storage layer cannot be opened,
///   or if every single discovered session fails to ingest
///   (100 % failure rate; M2.1 audit P1-4). Partial failures still
///   exit `0` with the per-session counts logged to stderr.
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
pub fn run(args: IngestArgs, storage_path: Option<PathBuf>) -> Result<()> {
    if args.agent != "copilot" {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "unsupported --agent: {} (only `copilot` is wired today)",
            args.agent
        )));
    }

    let adapter = Arc::new(CopilotAdapter);
    let root = args
        .root
        .clone()
        .or_else(|| CopilotAdapter.default_session_root());
    let root = root.ok_or_else(|| {
        ExitKind::UserError
            .into_anyhow("could not determine session root (set --root or HOME)".to_owned())
    })?;
    if !root.is_dir() {
        return Err(
            ExitKind::UserError.into_anyhow(format!("session root not found: {}", root.display()))
        );
    }

    let ds = AdapterDataSource::new(Arc::clone(&adapter), root.clone());

    let cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let mut db = Db::open_and_migrate(&cfg.path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", cfg.path.display()))
    })?;

    // M2.1 audit P1-3: discover ONCE, hold the `AdapterRef`s, and
    // pass each by reference into `load_session_by_ref` to skip the
    // O(N²) rescan that the generic trait route would incur.
    let targets: Vec<AdapterRef> = if let Some(id) = &args.session {
        let refs = adapter.discover_sessions(root.as_path()).map_err(|e| {
            ExitKind::DataError.into_anyhow(format!("discover {}: {e}", root.display()))
        })?;
        let hit = refs.into_iter().find(|r| r.id == *id).ok_or_else(|| {
            ExitKind::UserError.into_anyhow(format!("session id not found: {id}"))
        })?;
        vec![hit]
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
        let cutoff = std::time::SystemTime::now().checked_sub(window);
        adapter
            .discover_sessions(root.as_path())
            .map_err(|e| {
                ExitKind::DataError.into_anyhow(format!("discover {}: {e}", root.display()))
            })?
            .into_iter()
            .filter(|sref| cutoff.map_or(true, |c| sref.modified_at >= c))
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
    for (idx, sref) in targets.iter().enumerate() {
        let id = &sref.id;
        let raw_path = &sref.path;
        match ds.load_session_by_ref(sref) {
            Ok(report) => {
                let now_secs = chrono::Utc::now().timestamp();
                // M2.1.1 T5.3: also load + upsert episodes per session.
                // load_episodes_by_ref keeps the loop O(N) (the T3.2
                // bypass mirror of load_session_by_ref); failure is
                // non-fatal — store empty Episodes so the row's
                // episodes_json holds the migration-default '{}' blob.
                let episodes = ds.load_episodes_by_ref(sref).unwrap_or_else(|e| {
                    tracing::warn!(
                        session = %id,
                        error = %e,
                        "load_episodes_by_ref failed; storing empty Episodes"
                    );
                    agentprof_core::episode::Episodes::default()
                });
                match upsert_report(&mut db, &report, raw_path, now_secs) {
                    Ok(_) => {
                        if let Err(e) =
                            upsert_episodes(&mut db, &report.meta.id, &episodes, now_secs)
                        {
                            tracing::warn!(
                                session = %report.meta.id,
                                error = %e,
                                "upsert_episodes failed; sessions row still written"
                            );
                        }
                        ok += 1;
                    }
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

    // M2.1 audit P1-4: a 100% failure rate is a data error, not a
    // success. Partial failures (some ok, some fail) keep exit 0 —
    // the user got *some* of what they asked for and per-session
    // errors are already on stderr.
    if total > 0 && ok == 0 {
        return Err(ExitKind::DataError.into_anyhow(format!(
            "all {total} session(s) failed to ingest; check stderr for per-session errors"
        )));
    }
    Ok(())
}
