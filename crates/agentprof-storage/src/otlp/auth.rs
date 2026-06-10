//! Bearer-token authentication for both OTLP transports (M2.2 T4.1).
//!
//! Provides two thin wrappers around a single shared secret token:
//!
//! - [`bearer_interceptor`] — a tonic [`Interceptor`]-compatible closure
//!   for the gRPC listener (`tonic::service::interceptor::InterceptedService`).
//! - [`bearer_middleware`] — an [`axum::middleware::from_fn`]-compatible
//!   async fn for the HTTP listener.
//!
//! Both helpers take the same `Option<Arc<String>>`:
//!
//! - `None` → passthrough (auth disabled), used when
//!   [`crate::otlp::config::OtlpServerConfig::listen_token`] is `None`.
//! - `Some(t)` → require `Authorization: Bearer <t>` (exact, ASCII case
//!   sensitive on the token; the `Bearer ` prefix itself is matched
//!   verbatim per RFC 6750 §2.1).
//!
//! On failure the gRPC variant returns [`tonic::Status::unauthenticated`]
//! and the HTTP variant returns [`StatusCode::UNAUTHORIZED`] (`401`). The
//! pipeline is never invoked on rejection — verified by
//! `tests/otlp_auth_smoke.rs` which asserts `counts_for_test() == (0, 0, 0)`
//! after a rejected request.
//!
//! TLS, mTLS and proxy-protocol land in later M2.2 tasks; bearer auth is
//! the only request-level check enforced here.
//!
//! [`Interceptor`]: tonic::service::Interceptor

use std::sync::Arc;

use axum::extract::Request as AxumRequest;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use tonic::{Request, Status};

/// RFC 6750 §2.1 scheme prefix for `Authorization` headers carrying a
/// bearer token. The trailing space is significant.
const BEARER_PREFIX: &str = "Bearer ";

/// Build a tonic interceptor that enforces `Authorization: Bearer <token>`.
///
/// When `expected_token` is `None` the returned closure is a passthrough,
/// preserving the existing behaviour of the gRPC listener for deployments
/// that have not configured a token.
///
/// When `expected_token` is `Some(t)`, the interceptor:
///
/// 1. Reads the `authorization` request metadata.
/// 2. Decodes it as ASCII (non-ASCII → `Unauthenticated`).
/// 3. Strips the `Bearer ` prefix (missing prefix → `Unauthenticated`).
/// 4. Compares the remainder against `*t` byte-for-byte
///    (mismatch → `Unauthenticated`).
///
/// Returned closure is `Clone + Send + Sync + 'static` so it can be moved
/// into one [`tonic::service::interceptor::InterceptedService`] per
/// registered service (logs, metrics, traces).
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::otlp::auth::bearer_interceptor;
/// use std::sync::Arc;
///
/// let interceptor =
///     bearer_interceptor(Some(Arc::new("secret-shared-token".to_owned())));
/// // `interceptor` now passes to e.g. `InterceptedService::new(svc, interceptor)`.
/// ```
pub fn bearer_interceptor(
    expected_token: Option<Arc<String>>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone + Send + Sync + 'static {
    // The `tonic::Status` error variant is large (≈176 bytes) but the
    // signature is fixed by `tonic::service::Interceptor`. Boxing would
    // diverge from upstream's trait contract.
    #[allow(clippy::result_large_err)]
    move |req: Request<()>| -> Result<Request<()>, Status> {
        let Some(expected) = expected_token.as_ref() else {
            return Ok(req);
        };
        let header = req
            .metadata()
            .get("authorization")
            .ok_or_else(|| Status::unauthenticated("missing authorization metadata"))?;
        let s = header
            .to_str()
            .map_err(|_| Status::unauthenticated("authorization metadata is not ASCII"))?;
        let token = s
            .strip_prefix(BEARER_PREFIX)
            .ok_or_else(|| Status::unauthenticated("authorization missing `Bearer ` prefix"))?;
        if token == expected.as_str() {
            Ok(req)
        } else {
            Err(Status::unauthenticated("bearer token mismatch"))
        }
    }
}

/// Axum tower middleware that enforces `Authorization: Bearer <token>`.
///
/// Wired into the router via
/// [`axum::middleware::from_fn`] in [`crate::otlp::server_http::serve_http`].
/// The semantics mirror [`bearer_interceptor`] exactly:
///
/// - `expected_token == None` → call `next.run(req).await` unchanged.
/// - `Some(t)` → require `Authorization: Bearer t` on the request, else
///   short-circuit with `401 Unauthorized`.
///
/// # Errors
///
/// Returns [`StatusCode::UNAUTHORIZED`] when:
///
/// - the `Authorization` header is missing,
/// - the value is not valid UTF-8 / ASCII,
/// - the value does not start with `Bearer `, or
/// - the token after the prefix does not equal `*t`.
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::otlp::auth::bearer_middleware;
/// use axum::{routing::get, Router};
/// use std::sync::Arc;
///
/// async fn ok() -> &'static str { "ok" }
///
/// let token = Some(Arc::new("secret-shared-token".to_owned()));
/// let app: Router = Router::new()
///     .route("/v1/logs", get(ok))
///     .layer(axum::middleware::from_fn(move |req, next| {
///         let t = token.clone();
///         async move { bearer_middleware(t, req, next).await }
///     }));
/// # let _ = app;
/// ```
pub async fn bearer_middleware(
    expected_token: Option<Arc<String>>,
    req: AxumRequest,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(expected) = expected_token.as_ref() else {
        return Ok(next.run(req).await);
    };
    let header = req
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let s = header.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
    let token = s
        .strip_prefix(BEARER_PREFIX)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if token == expected.as_str() {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
