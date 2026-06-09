//! [`SessionDataSource`] implementation backed by `SQLite` (M2.1 T2.6).
//!
//! Wraps a shared [`Db`] connection in `Arc<Mutex<…>>` so the data source
//! can be cloned and shared across threads while still serializing access
//! to the underlying `rusqlite::Connection` (which is `!Sync`).
//!
//! Errors from [`crate::query`] are translated:
//!
//! - `QueryReturnedNoRows` on `load_session` → [`DataSourceError::NotFound`]
//! - any other [`SqliteError`] → [`DataSourceError::Storage`] with
//!   `source = "sqlite"`.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::datasource::{DataSourceError, SessionDataSource, SessionRef};

use crate::query::{load_session, query_sessions_since};
use crate::{Db, SqliteError};

/// SQLite-backed [`SessionDataSource`].
///
/// Cloneable; all clones share one [`Db`] connection via
/// `Arc<Mutex<…>>` so concurrent `discover` / `load_session` calls are
/// serialized at the connection level.
///
/// # Examples
///
/// ```no_run
/// use std::sync::{Arc, Mutex};
/// use agentprof_storage::{Db, SqliteDataSource};
///
/// let db = Db::open_in_memory().unwrap();
/// let _src = SqliteDataSource::new(Arc::new(Mutex::new(db)));
/// ```
#[derive(Clone)]
pub struct SqliteDataSource {
    db: Arc<Mutex<Db>>,
    now_ms_fn: fn() -> i64,
}

impl SqliteDataSource {
    /// Construct a new data source over the given shared [`Db`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::{Arc, Mutex};
    /// use agentprof_storage::{Db, SqliteDataSource};
    ///
    /// let db = Db::open_in_memory().unwrap();
    /// let _src = SqliteDataSource::new(Arc::new(Mutex::new(db)));
    /// ```
    #[must_use]
    pub fn new(db: Arc<Mutex<Db>>) -> Self {
        Self {
            db,
            now_ms_fn: default_now_ms,
        }
    }

    /// Test-only clock override.
    ///
    /// Replaces the wall-clock used by [`SessionDataSource::discover`] to
    /// compute the `since` cutoff so tests can pin time.
    ///
    /// Hidden from public docs because it is not part of the supported
    /// surface area.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::{Arc, Mutex};
    /// use agentprof_storage::{Db, SqliteDataSource};
    ///
    /// let db = Db::open_in_memory().unwrap();
    /// let _src = SqliteDataSource::new(Arc::new(Mutex::new(db)))
    ///     .with_now_fn(|| 1_700_000_000_000);
    /// ```
    #[doc(hidden)]
    #[must_use]
    pub fn with_now_fn(mut self, f: fn() -> i64) -> Self {
        self.now_ms_fn = f;
        self
    }
}

fn default_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn map_storage(err: SqliteError) -> DataSourceError {
    DataSourceError::Storage {
        source: "sqlite",
        underlying: Box::new(err),
    }
}

impl SessionDataSource for SqliteDataSource {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn discover(&self, since: Duration) -> Result<Vec<SessionRef>, DataSourceError> {
        let now_ms = (self.now_ms_fn)();
        // M2.1 audit P1-5 / P2-1: a poisoned mutex means a *previous*
        // panicked holder left the protected `Db` in some state. The
        // SQLite connection itself is fine (the panic happened in
        // Rust-land, not in C-land), so recover via `into_inner`
        // rather than synthesising a misleading "config path" error.
        let guard = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        query_sessions_since(&guard, since, now_ms).map_err(map_storage)
    }

    fn load_session(&self, id: &str) -> Result<AnalysisReport, DataSourceError> {
        let guard = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        match load_session(&guard, id) {
            Ok(r) => Ok(r),
            Err(SqliteError::Rusqlite {
                source: rusqlite::Error::QueryReturnedNoRows,
                ..
            }) => Err(DataSourceError::NotFound { id: id.to_owned() }),
            Err(other) => Err(map_storage(other)),
        }
    }
}
