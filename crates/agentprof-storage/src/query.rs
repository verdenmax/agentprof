//! Read paths: enumerate session refs since a window, load a full report.
//!
//! The two functions in this module are the inverse of
//! [`crate::upsert::upsert_report`] (M2.1 T2.4):
//!
//! - [`query_sessions_since`] returns lightweight [`SessionRef`] summaries
//!   for `discover`-style listing UIs without touching the
//!   `analysis_report_json` blob.
//! - [`load_session`] hydrates a single full
//!   [`AnalysisReport`] from its stored JSON blob.
//!
//! Both functions are deliberately thin: they translate a SQL row into a
//! domain type and surface every failure as [`SqliteError`]. The
//! [`SessionDataSource`](agentprof_core::datasource::SessionDataSource)
//! adapter implemented in M2.1 T2.6 maps these errors to the typed
//! `DataSourceError` (notably converting
//! [`rusqlite::Error::QueryReturnedNoRows`] into `NotFound`).

use std::path::PathBuf;
use std::time::Duration;

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::datasource::SessionRef;

use crate::{error::SqliteError, Db};

/// Enumerate sessions started within the last `since` window, newest first.
///
/// `now_ms` is the reference "now" in unix milliseconds — injected so
/// callers can pin time in tests. Rows whose `started_at` column is `NULL`
/// are excluded.
///
/// The returned [`SessionRef`]s carry `source = "sqlite"` and always have
/// `raw_path` / `raw_mtime_ms` populated (both columns are `NOT NULL` in
/// the schema; see [`docs/architecture.md`] §9).
///
/// # Errors
///
/// [`SqliteError::Rusqlite`] on any prepare / bind / row decode failure.
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use agentprof_storage::{Db, query::query_sessions_since};
///
/// let db = Db::open_in_memory().unwrap();
/// let refs = query_sessions_since(
///     &db,
///     Duration::from_secs(7 * 86_400),
///     1_700_000_000_000,
/// ).unwrap();
/// assert!(refs.is_empty()); // fresh in-memory db
/// ```
///
/// [`docs/architecture.md`]: https://github.com/agentprof/agentprof/blob/main/docs/architecture.md#9-sqlite-schema
pub fn query_sessions_since(
    db: &Db,
    since: Duration,
    now_ms: i64,
) -> Result<Vec<SessionRef>, SqliteError> {
    let since_ms = i64::try_from(since.as_millis()).unwrap_or(i64::MAX);
    let cutoff_ms = now_ms.saturating_sub(since_ms);

    let mut stmt = db
        .conn_for_test()
        .prepare(
            "SELECT id, agent, started_at, raw_path, raw_mtime
             FROM sessions
             WHERE started_at IS NOT NULL AND started_at >= ?1
             ORDER BY started_at DESC",
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: "prepare query_sessions_since".to_owned(),
            source,
        })?;

    let rows = stmt
        .query_map([cutoff_ms], |row| {
            let id: String = row.get(0)?;
            let agent_str: String = row.get(1)?;
            let started_at_ms: i64 = row.get(2)?;
            let raw_path: String = row.get(3)?;
            let raw_mtime_ms: i64 = row.get(4)?;
            Ok(SessionRef::new(
                id,
                parse_agent(&agent_str),
                Some(started_at_ms),
                Some(PathBuf::from(raw_path)),
                Some(raw_mtime_ms),
                "sqlite",
            ))
        })
        .map_err(|source| SqliteError::Rusqlite {
            context: "query_map query_sessions_since".to_owned(),
            source,
        })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|source| SqliteError::Rusqlite {
            context: "collect query_sessions_since".to_owned(),
            source,
        })
}

/// Load a full [`AnalysisReport`] for the given session id.
///
/// Reads the `analysis_report_json` blob written by
/// [`crate::upsert::upsert_report`] and deserializes it.
///
/// Missing ids surface as [`SqliteError::Rusqlite`] wrapping
/// [`rusqlite::Error::QueryReturnedNoRows`]; the
/// [`SessionDataSource`](agentprof_core::datasource::SessionDataSource)
/// implementation (T2.6) maps that to `DataSourceError::NotFound`.
///
/// # Errors
///
/// - [`SqliteError::Rusqlite`] — including `QueryReturnedNoRows` for an
///   unknown id, or any underlying row-decode failure.
/// - [`SqliteError::Serde`] if the stored JSON cannot be parsed back into
///   [`AnalysisReport`] (schema drift between writer and reader).
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::{Db, query::load_session};
///
/// let db = Db::open_in_memory().unwrap();
/// let _maybe = load_session(&db, "some-id").ok();
/// ```
pub fn load_session(db: &Db, id: &str) -> Result<AnalysisReport, SqliteError> {
    let json: String = db
        .conn_for_test()
        .query_row(
            "SELECT analysis_report_json FROM sessions WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: "load_session SELECT".to_owned(),
            source,
        })?;
    serde_json::from_str(&json).map_err(|source| SqliteError::Serde {
        context: "deserialize analysis_report_json".to_owned(),
        source,
    })
}

/// Parse a stored agent string into [`AgentKind`].
///
/// The closed set mirrors [`AgentKind`]'s `FromStr` impl. Unknown strings
/// are tolerated (logged at `warn` and defaulted to
/// [`AgentKind::Copilot`]) so that a single corrupt row never crashes a
/// listing query — surfacing the broken row is the caller's job.
fn parse_agent(s: &str) -> AgentKind {
    match s {
        "copilot" => AgentKind::Copilot,
        "claude" => AgentKind::Claude,
        "codex" => AgentKind::Codex,
        other => {
            tracing::warn!(agent = %other, "unknown agent string in sessions row; defaulting to Copilot");
            AgentKind::Copilot
        }
    }
}
