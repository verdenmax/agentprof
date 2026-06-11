//! HTTP route handlers for the dashboard (M2.3).
//!
//! T5 ships only the liveness probe (`/healthz`); T6+ adds the
//! dynamic view handlers (overview / ROI / waste / aggregate).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::state::AppState;

/// `GET /healthz` — always returns `200 OK` with body `"healthy"`.
///
/// Suitable for liveness probes; no tracing emitted to avoid log
/// spam under load-balancer health checks. Ignores the [`AppState`]
/// but still takes it via the extractor so the type-state matches
/// the rest of the router.
///
/// # Examples
///
/// ```text
/// $ curl -s http://127.0.0.1:4329/healthz
/// healthy
/// ```
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn healthz(State(_state): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, "healthy")
}
