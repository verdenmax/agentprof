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
