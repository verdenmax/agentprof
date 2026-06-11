//! HTTP route handlers for the dashboard (M2.3).
//!
//! T5 ships only the liveness probe (`/healthz`); T6+ adds the
//! dynamic view handlers (overview / ROI / waste / aggregate).

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
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

/// `GET /static/:name` — serves bundled CSS / JS / favicon.
///
/// Assets are baked into the binary via `include_str!` / `include_bytes!`
/// (see [`super::static_assets`]). `Cache-Control: immutable` because the
/// assets only change when the agentprof binary itself changes (the
/// browser will only re-fetch on a server upgrade).
///
/// # Examples
///
/// ```text
/// $ curl -sI http://127.0.0.1:4329/static/dashboard.css | head -1
/// HTTP/1.1 200 OK
/// $ curl -sI http://127.0.0.1:4329/static/missing.png | head -1
/// HTTP/1.1 404 Not Found
/// ```
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn static_asset(Path(name): Path<String>) -> impl IntoResponse {
    if let Some((mime, body)) = super::static_assets::lookup(&name) {
        let headers = [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ];
        return (StatusCode::OK, headers, body).into_response();
    }
    (StatusCode::NOT_FOUND, "asset not found").into_response()
}
