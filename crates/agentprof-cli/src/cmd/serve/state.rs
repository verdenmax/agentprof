//! Per-request shared state for the dashboard handlers (M2.3 T3).

use std::sync::{Arc, Mutex};

use agentprof_storage::Db;

/// Shared state injected into every dashboard route handler via
/// axum's `State` extractor.
///
/// `db` is wrapped in `Arc<Mutex<Db>>` mirroring M2.2's
/// `StorageFlushSink` pattern — rusqlite is sync; the mutex
/// serializes connection access across concurrent request handlers
/// without spawning a connection pool (a single dashboard user
/// generates trivially low load).
#[derive(Clone)]
#[allow(
    dead_code,
    reason = "T5 wires the axum handlers that read these fields"
)]
pub struct AppState {
    /// `SQLite` store handle.
    pub db: Arc<Mutex<Db>>,
    /// Browser-side default poll interval (seconds, 1..=60). Embedded
    /// into the HTML chrome so the JS poller picks it up before the
    /// user touches the toolbar.
    pub interval_default: u8,
}

impl AppState {
    /// Construct a new [`AppState`] from an already-opened `Db`
    /// handle and the browser poll interval default (seconds).
    ///
    /// The caller is responsible for opening (and migrating) `db`;
    /// this constructor does no further validation. Used by
    /// `cmd::serve::run_async` to assemble the live state and by
    /// integration tests in `tests/cli_serve_router_unit.rs` to
    /// build a state against a tempfile DB.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::sync::{Arc, Mutex};
    /// use agentprof_storage::Db;
    /// # let tmp = tempfile::NamedTempFile::new().unwrap();
    /// let db = Db::open_and_migrate(tmp.path()).unwrap();
    /// // `AppState` is crate-internal; this example mirrors how
    /// // `cmd::serve::run_async` assembles the state.
    /// let _state = AppState::new(Arc::new(Mutex::new(db)), 5);
    /// ```
    #[must_use]
    pub const fn new(db: Arc<Mutex<Db>>, interval_default: u8) -> Self {
        Self {
            db,
            interval_default,
        }
    }
}
