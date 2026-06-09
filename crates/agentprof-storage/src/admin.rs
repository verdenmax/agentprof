//! DB administration helpers: stats / prune / vacuum / export.
//!
//! These four functions back the `agentprof db {stats|prune|vacuum|export}`
//! CLI subcommands (M2.1 T2.7). They all sit directly on a [`Db`] handle
//! and return [`SqliteError`] from this crate; the CLI layer adapts them
//! into `anyhow::Result<()>` at its boundary per the iron error-model
//! rule.
//!
//! # Cascading semantics
//!
//! [`prune_before`] deletes from `sessions` only; child rows in
//! `tools_loaded` / `turn_buckets` go away via the `ON DELETE CASCADE`
//! foreign keys declared in `migrations/001_initial.sql`. The `PRAGMA
//! foreign_keys=ON` set inside [`Db::open_and_migrate`] /
//! [`Db::open_in_memory`] is required for that cascade to fire.
//!
//! # Size accounting
//!
//! [`stats`] computes `size_bytes` as `page_count * page_size`. For an
//! in-memory database `page_count` is `0`, so `size_bytes` is `0`; this
//! is a documented quirk of `SQLite`, not a bug.

use std::time::Duration;

use rusqlite::params;

use crate::{error::SqliteError, Db};

/// Aggregate row counts and size for the `agentprof db stats` command.
///
/// `#[non_exhaustive]` so additional counters (e.g. `events_count` for
/// future telemetry tables) can be added without a breaking release.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DbStats {
    /// Number of rows in the `sessions` table.
    pub session_count: i64,
    /// Number of rows in the `tools_loaded` child table (summed across
    /// all sessions).
    pub tools_loaded_count: i64,
    /// Number of rows in the `turn_buckets` child table (summed across
    /// all sessions).
    pub turn_buckets_count: i64,
    /// On-disk size of the database file in bytes, computed as
    /// `page_count * page_size`. Reports `0` for in-memory databases.
    pub size_bytes: u64,
    /// Smallest `started_at` (unix epoch ms) across all sessions, or
    /// `None` if the table is empty / every row has `NULL`.
    pub oldest_started_ms: Option<i64>,
    /// Largest `started_at` (unix epoch ms) across all sessions, or
    /// `None` if the table is empty / every row has `NULL`.
    pub newest_started_ms: Option<i64>,
}

/// Compute current database statistics for `agentprof db stats`.
///
/// Runs four queries: three `COUNT(*)`s (one per table) plus a single
/// `MIN/MAX(started_at)` over `sessions`, and reads `page_count` /
/// `page_size` pragmas for the on-disk size.
///
/// # Errors
///
/// [`SqliteError::Rusqlite`] if any of the underlying queries fails.
///
/// # Examples
///
/// ```
/// use agentprof_storage::{admin::stats, Db};
/// let db = Db::open_in_memory().expect("open db");
/// let s = stats(&db).expect("stats");
/// assert_eq!(s.session_count, 0);
/// ```
pub fn stats(db: &Db) -> Result<DbStats, SqliteError> {
    let conn = db.conn();

    let session_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .map_err(|source| SqliteError::Rusqlite {
            context: "COUNT(*) sessions".to_owned(),
            source,
        })?;
    let tools_loaded_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tools_loaded", [], |r| r.get(0))
        .map_err(|source| SqliteError::Rusqlite {
            context: "COUNT(*) tools_loaded".to_owned(),
            source,
        })?;
    let turn_buckets_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM turn_buckets", [], |r| r.get(0))
        .map_err(|source| SqliteError::Rusqlite {
            context: "COUNT(*) turn_buckets".to_owned(),
            source,
        })?;

    let (oldest_started_ms, newest_started_ms): (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT MIN(started_at), MAX(started_at) FROM sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: "MIN/MAX(started_at) sessions".to_owned(),
            source,
        })?;

    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .map_err(|source| SqliteError::Rusqlite {
            context: "PRAGMA page_count".to_owned(),
            source,
        })?;
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .map_err(|source| SqliteError::Rusqlite {
            context: "PRAGMA page_size".to_owned(),
            source,
        })?;
    let size_bytes = u64::try_from(page_count.max(0))
        .unwrap_or(0)
        .saturating_mul(u64::try_from(page_size.max(0)).unwrap_or(0));

    Ok(DbStats {
        session_count,
        tools_loaded_count,
        turn_buckets_count,
        size_bytes,
        oldest_started_ms,
        newest_started_ms,
    })
}

/// Delete (or count, when `dry_run`) all `sessions` rows whose
/// `started_at` is older than `now_ms - retention`.
///
/// Child rows in `tools_loaded` / `turn_buckets` are removed via the
/// `ON DELETE CASCADE` foreign keys (relies on `PRAGMA foreign_keys=ON`,
/// set inside `Db::open_*`).
///
/// Returns the number of `sessions` rows matched (when `dry_run = true`)
/// or actually deleted (when `dry_run = false`).
///
/// # Errors
///
/// [`SqliteError::Rusqlite`] on any query / delete failure.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use agentprof_storage::{admin::prune_before, Db};
/// let mut db = Db::open_in_memory().expect("open db");
/// let n = prune_before(&mut db, Duration::from_secs(86_400), 0, true).expect("prune");
/// assert_eq!(n, 0);
/// ```
pub fn prune_before(
    db: &mut Db,
    retention: Duration,
    now_ms: i64,
    dry_run: bool,
) -> Result<i64, SqliteError> {
    let retention_ms =
        i64::try_from(retention.as_millis().min(i64::MAX as u128)).unwrap_or(i64::MAX);
    let cutoff_ms = now_ms.saturating_sub(retention_ms);

    let conn = db.conn_mut();

    let candidates: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE started_at IS NOT NULL AND started_at < ?1",
            params![cutoff_ms],
            |r| r.get(0),
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: "COUNT prune candidates".to_owned(),
            source,
        })?;

    if dry_run {
        return Ok(candidates);
    }

    let deleted = conn
        .execute(
            "DELETE FROM sessions WHERE started_at IS NOT NULL AND started_at < ?1",
            params![cutoff_ms],
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: "DELETE prune candidates".to_owned(),
            source,
        })?;
    Ok(i64::try_from(deleted).unwrap_or(i64::MAX))
}

/// Reclaim free pages with `VACUUM`. Returns `(size_before, size_after)`
/// in bytes (both computed via [`stats`]).
///
/// For in-memory databases both numbers are `0` (`SQLite`'s `page_count`
/// is `0` for `:memory:`), so this is mostly useful for file-backed
/// stores.
///
/// # Errors
///
/// [`SqliteError::Rusqlite`] if the `VACUUM` command fails, or if either
/// of the surrounding [`stats`] calls fails.
///
/// # Examples
///
/// ```
/// use agentprof_storage::{admin::vacuum, Db};
/// let db = Db::open_in_memory().expect("open db");
/// let (_before, _after) = vacuum(&db).expect("vacuum");
/// ```
pub fn vacuum(db: &Db) -> Result<(u64, u64), SqliteError> {
    let before = stats(db)?.size_bytes;
    db.conn()
        .execute("VACUUM", [])
        .map_err(|source| SqliteError::Rusqlite {
            context: "VACUUM".to_owned(),
            source,
        })?;
    let after = stats(db)?.size_bytes;
    Ok((before, after))
}

/// Read back the stored `analysis_report_json` blob for one session by
/// id, suitable for piping into `jq` or another `agentprof` instance.
///
/// # Errors
///
/// - [`SqliteError::Rusqlite`] (inner `rusqlite::Error::QueryReturnedNoRows`)
///   if no session with that id exists.
/// - [`SqliteError::Rusqlite`] on any other query failure.
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::{admin::export_session_json, Db};
/// let db = Db::open_in_memory().expect("open db");
/// // `"missing"` is absent — call returns `Err(Rusqlite{QueryReturnedNoRows})`.
/// let _ = export_session_json(&db, "missing");
/// ```
pub fn export_session_json(db: &Db, id: &str) -> Result<String, SqliteError> {
    db.conn()
        .query_row(
            "SELECT analysis_report_json FROM sessions WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: format!("SELECT analysis_report_json WHERE id = {id}"),
            source,
        })
}
