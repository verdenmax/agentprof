//! axum `Router` construction for the dashboard (M2.3).

use axum::routing::get;
use axum::Router;

use super::handlers;
use super::state::AppState;

/// Build the dashboard router. All routes share [`AppState`] via
/// axum's `State` extractor. T6+ adds the dynamic views; this
/// skeleton wires `/healthz` only.
///
/// # Examples
///
/// ```ignore
/// use std::sync::{Arc, Mutex};
/// use agentprof_storage::Db;
/// # let tmp = tempfile::NamedTempFile::new().unwrap();
/// let db = Db::open_and_migrate(tmp.path()).unwrap();
/// // `AppState` + `build_router` are crate-internal; this example
/// // mirrors how `cmd::serve::run_async` assembles the router.
/// let state = AppState::new(Arc::new(Mutex::new(db)), 5);
/// let _app = build_router(state);
/// ```
#[must_use = "the constructed Router must be passed to axum::serve or it does nothing"]
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(handlers::healthz))
        .route("/static/:name", get(handlers::static_asset))
        .with_state(state)
}
