//! axum `Router` construction for the dashboard (M2.3).

use axum::response::Redirect;
use axum::routing::get;
use axum::Router;

use super::handlers;
use super::state::AppState;

/// Build the dashboard router. All routes share [`AppState`] via
/// axum's `State` extractor. T7 adds the `/sessions` view (plus a
/// `/` → `/sessions` redirect and the chunk endpoint that the JS
/// poller hits); T8+ adds the session-detail / aggregate / waste
/// views on top.
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
        .route("/", get(|| async { Redirect::to("/sessions") }))
        .route("/sessions", get(handlers::sessions_page))
        .route("/api/sessions.html", get(handlers::sessions_chunk))
        .route("/session/:id", get(handlers::session_page))
        .route("/api/session/:id.html", get(handlers::session_chunk))
        .route("/aggregate", get(handlers::aggregate_page))
        .route("/api/aggregate.html", get(handlers::aggregate_chunk))
        .route("/healthz", get(handlers::healthz))
        .route("/static/:name", get(handlers::static_asset))
        .with_state(state)
}
