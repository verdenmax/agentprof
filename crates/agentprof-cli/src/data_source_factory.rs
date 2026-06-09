//! Build the right [`SessionDataSource`] for the current CLI invocation.
//!
//! `agentprof` exposes a single composition seam at the CLI layer: every
//! subcommand that reads sessions does so through a
//! [`SessionDataSource`] trait object. Which concrete object it gets
//! depends on the user's resolved [`StorageConfig`] and the global
//! `--no-cache` flag. Centralising that decision here keeps each
//! subcommand identical (no `if storage_enabled { … } else { … }`
//! ladders) and matches the [dual-path design] in the M2.1 spec.
//!
//! [dual-path design]: ../../../docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md
//!
//! Decision table:
//!
//! | `no_cache` | storage opens OK | returned source                       |
//! |------------|------------------|---------------------------------------|
//! | `true`     | (skipped)        | bare [`AdapterDataSource`]            |
//! | `false`    | ✅               | [`DualPathDataSource`] (adapter+SQLite) |
//! | `false`    | ❌               | bare [`AdapterDataSource`] + `tracing::warn!` |
//!
//! Storage-open failures **never** abort the command — the user's
//! request to see their sessions wins, the cache is just opportunistic.

use std::path::Path;
use std::sync::{Arc, Mutex};

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_adapters::AdapterDataSource;
use agentprof_core::datasource::SessionDataSource;
use agentprof_storage::config::StorageConfig;
use agentprof_storage::{Db, SqliteDataSource};

use crate::data_source::{DualPathDataSource, DualPathWarning};

/// Shared handle to dual-path divergence warnings collected during a
/// `discover`/`load_session` round.
///
/// Returned as the second element of [`build_data_source`]; callers drain
/// it and forward each entry to stderr (suppressed by `--quiet`).
pub type WarningsHandle = Arc<Mutex<Vec<DualPathWarning>>>;

/// Build the appropriate [`SessionDataSource`] given the resolved
/// agent name, log root, storage config, and `--no-cache` flag.
///
/// See the [module-level decision table](self) for the precise dispatch
/// rules. Notably:
///
/// - Storage-open failures are **logged via `tracing::warn!`** and the
///   factory transparently falls back to the bare adapter — graceful
///   degradation rather than a hard error.
/// - The returned trait object's [`SessionDataSource::name`] is one of
///   `"dual"` or `"adapter:<agent>"` and is intended for diagnostic
///   logging only.
/// - The second tuple element is an `Arc<Mutex<Vec<DualPathWarning>>>`
///   handle. For dual-path returns it is shared with the inner
///   [`DualPathDataSource`] and accumulates per-session divergences
///   that the caller should drain (and emit to stderr) after the
///   `discover` / `load_session` round. For adapter-only returns it is
///   an empty buffer that will stay empty; draining it is a no-op,
///   which lets callers use a single code path regardless of dispatch.
///
/// # Errors
///
/// Returns an error only when `agent` is not a recognised name. Today
/// only `"copilot"` is supported; `"claude"` and `"codex"` are reserved
/// for future milestones and currently rejected. Storage-open failures
/// do *not* surface here.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agentprof_cli::data_source_factory::build_data_source;
/// use agentprof_storage::config::StorageConfig;
///
/// let cfg = StorageConfig::default();
/// let (ds, warnings) = build_data_source("copilot", Path::new("/tmp/none"), &cfg, true)?;
/// # let _ = (ds, warnings);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn build_data_source(
    agent: &str,
    root: &Path,
    storage: &StorageConfig,
    no_cache: bool,
) -> anyhow::Result<(Box<dyn SessionDataSource>, WarningsHandle)> {
    let warnings: WarningsHandle = Arc::new(Mutex::new(Vec::new()));

    let adapter: Box<dyn SessionDataSource> = match agent {
        "copilot" => Box::new(AdapterDataSource::new(
            Arc::new(CopilotAdapter),
            root.to_path_buf(),
        )),
        other => anyhow::bail!(
            "unsupported agent: {other} (only `copilot` is wired today; \
             `claude` and `codex` are reserved for future milestones)"
        ),
    };

    if no_cache {
        return Ok((adapter, warnings));
    }

    let storage_box: Option<Box<dyn SessionDataSource>> = match Db::open_and_migrate(&storage.path)
    {
        Ok(db) => Some(Box::new(SqliteDataSource::new(Arc::new(Mutex::new(db))))),
        Err(e) => {
            tracing::warn!(
                path = %storage.path.display(),
                error = %e,
                "storage open failed; falling back to adapter-only data source"
            );
            None
        }
    };

    if storage_box.is_none() {
        return Ok((adapter, warnings));
    }

    let dual =
        DualPathDataSource::new_with_shared_warnings(adapter, storage_box, Arc::clone(&warnings));
    Ok((Box::new(dual), warnings))
}
