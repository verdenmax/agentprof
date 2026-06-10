//! HTTP/protobuf OTLP listener (M2.2 T3.1).
//!
//! Binds an [`axum`] router on [`crate::otlp::config::OtlpServerConfig::listen_http`]
//! and exposes the three standard OTLP/HTTP collector endpoints:
//!
//! | Method + path     | Request body                              | Response body                              |
//! |-------------------|-------------------------------------------|--------------------------------------------|
//! | `POST /v1/logs`   | [`proto::logs::ExportLogsServiceRequest`] | [`proto::logs::ExportLogsServiceResponse`] |
//! | `POST /v1/metrics`| [`proto::metrics::ExportMetricsServiceRequest`] | [`proto::metrics::ExportMetricsServiceResponse`] |
//! | `POST /v1/traces` | [`proto::trace::ExportTraceServiceRequest`] | [`proto::trace::ExportTraceServiceResponse`] |
//!
//! In M2.2 T3.1 the binary protobuf wire format is the **only** content
//! type accepted. JSON (`application/json`) lands in a later milestone if
//! we decide to support it; for now any non-`application/x-protobuf*`
//! `Content-Type` returns `415 Unsupported Media Type`. A protobuf decode
//! failure returns `400 Bad Request`. Everything else delegates to the
//! shared [`crate::otlp::pipeline::IngestPipeline`] — the same fan-in
//! point used by [`crate::otlp::server_grpc`].
//!
//! TLS lands in M2.2 T4.2 (see [`crate::otlp::tls`]); proxy-protocol lands
//! in later M2.2 tasks. When `cfg.tls_cert`/`tls_key` are set this module
//! transparently switches from plaintext `axum::serve` to
//! `axum_server::from_tcp_rustls`.
//!
//! [`proto::logs::ExportLogsServiceRequest`]: super::proto::logs::ExportLogsServiceRequest
//! [`proto::logs::ExportLogsServiceResponse`]: super::proto::logs::ExportLogsServiceResponse
//! [`proto::metrics::ExportMetricsServiceRequest`]: super::proto::metrics::ExportMetricsServiceRequest
//! [`proto::metrics::ExportMetricsServiceResponse`]: super::proto::metrics::ExportMetricsServiceResponse
//! [`proto::trace::ExportTraceServiceRequest`]: super::proto::trace::ExportTraceServiceRequest
//! [`proto::trace::ExportTraceServiceResponse`]: super::proto::trace::ExportTraceServiceResponse

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use prost::Message;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::otlp::auth::bearer_middleware;
use crate::otlp::config::OtlpServerConfig;
use crate::otlp::error::OtlpServerError;
use crate::otlp::pipeline::IngestPipeline;
use crate::otlp::proto::logs::ExportLogsServiceRequest;
use crate::otlp::proto::metrics::ExportMetricsServiceRequest;
use crate::otlp::proto::trace::ExportTraceServiceRequest;
use crate::otlp::tls::load_server_tls_config;

/// Handle pair returned by [`serve_http`]: the server's join handle and
/// a oneshot used to request graceful shutdown.
///
/// Sending `()` on the [`oneshot::Sender`] causes axum's
/// `with_graceful_shutdown` future to resolve, after which the join
/// handle resolves with the server's inner result.
pub type HttpServerHandle = (JoinHandle<Result<(), OtlpServerError>>, oneshot::Sender<()>);

/// Bind the OTLP HTTP listener, register the three collector routes, and
/// spawn the server task.
///
/// The bind step is awaited synchronously, so a port collision or other
/// `io::Error` surfaces immediately as [`OtlpServerError::Bind`]. After a
/// successful bind the task is spawned and the returned
/// [`oneshot::Sender`] can be used to request graceful shutdown at any
/// time.
///
/// Requires `cfg.listen_http` to be `Some(_)`; the caller is expected to
/// have already run [`OtlpServerConfig::validate`].
///
/// # Errors
///
/// - [`OtlpServerError::Config`] if `cfg.listen_http` is `None`.
/// - [`OtlpServerError::Bind`] if `TcpListener::bind` fails.
///
/// # Examples
///
/// ```no_run
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// use agentprof_storage::otlp::config::OtlpServerConfig;
/// use agentprof_storage::otlp::pipeline::IngestPipeline;
/// use agentprof_storage::otlp::server_http::serve_http;
/// use std::sync::Arc;
///
/// let cfg = OtlpServerConfig::default();
/// let pipeline = Arc::new(IngestPipeline::noop_for_test());
/// let (handle, shutdown) = serve_http(cfg, pipeline).await?;
/// // ... later:
/// let _ = shutdown.send(());
/// handle.await??;
/// # Ok(()) }
/// ```
pub async fn serve_http(
    cfg: OtlpServerConfig,
    pipeline: Arc<IngestPipeline>,
) -> Result<HttpServerHandle, OtlpServerError> {
    let addr = cfg.listen_http.ok_or_else(|| {
        OtlpServerError::Config("serve_http requires cfg.listen_http = Some(_)".into())
    })?;

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| OtlpServerError::Bind { addr, source })?;

    let token = cfg.listen_token.clone().map(Arc::new);
    let app: Router = Router::new()
        .route("/v1/logs", post(handle_logs))
        .route("/v1/metrics", post(handle_metrics))
        .route("/v1/traces", post(handle_traces))
        .layer(axum::middleware::from_fn(move |req, next| {
            let t = token.clone();
            async move { bearer_middleware(t, req, next).await }
        }))
        .with_state(pipeline);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let join = if let Some(cert_path) = cfg.tls_cert.as_deref() {
        let key_path = cfg.tls_key.as_deref().ok_or_else(|| {
            OtlpServerError::Config(
                "serve_http: tls_cert set but tls_key is None (validate() should have caught this)"
                    .into(),
            )
        })?;
        let tls_cfg = load_server_tls_config(cert_path, key_path, cfg.tls_client_ca.as_deref())?;
        let rustls_cfg =
            axum_server::tls_rustls::RustlsConfig::from_config(std::sync::Arc::new(tls_cfg));
        let std_listener = listener
            .into_std()
            .map_err(|e| OtlpServerError::Http(format!("convert tokio listener to std: {e}")))?;
        std_listener
            .set_nonblocking(true)
            .map_err(|e| OtlpServerError::Http(format!("set_nonblocking on std listener: {e}")))?;
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        tokio::spawn(async move {
            let _ = shutdown_rx.await;
            // Trigger graceful shutdown; `None` => no forced-deadline timeout.
            shutdown_handle.graceful_shutdown(None);
        });
        let server = axum_server::from_tcp_rustls(std_listener, rustls_cfg).handle(handle);
        tokio::spawn(async move {
            server
                .serve(app.into_make_service())
                .await
                .map_err(|e| OtlpServerError::Http(format!("axum_server::serve: {e}")))
        })
    } else {
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .map_err(|e| OtlpServerError::Http(format!("axum::serve: {e}")))
        })
    };

    Ok((join, shutdown_tx))
}

/// Reject any request whose `Content-Type` is not `application/x-protobuf`
/// (possibly with parameters such as `; charset=…`).
///
/// Returns [`StatusCode::UNSUPPORTED_MEDIA_TYPE`] for absent / mismatched
/// values. The check is case-insensitive on the type/subtype as required
/// by RFC 9110 §8.3.1.
fn require_protobuf_content_type(headers: &HeaderMap) -> Result<(), StatusCode> {
    let Some(ct) = headers.get(header::CONTENT_TYPE) else {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    };
    let ct = ct
        .to_str()
        .map_err(|_| StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    // Split off any `;` parameters and trim whitespace before comparing.
    let media_type = ct.split(';').next().unwrap_or("").trim();
    if media_type.eq_ignore_ascii_case("application/x-protobuf") {
        Ok(())
    } else {
        Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
    }
}

/// Encode an OTLP response message as `application/x-protobuf` with status
/// `200 OK`.
///
/// `prost::Message::encode` on a `Vec<u8>` (which grows on demand) is
/// effectively infallible, but we still bubble any encode error up as
/// `500 Internal Server Error` rather than `expect`-panicking, per the
/// workspace rule that lib code must not `unwrap`/`expect`.
fn encode_proto_response<M: Message>(resp: &M) -> Response {
    let mut buf = Vec::with_capacity(resp.encoded_len());
    if let Err(e) = resp.encode(&mut buf) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("encode OTLP response: {e}"),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/x-protobuf")],
        buf,
    )
        .into_response()
}

/// `POST /v1/logs` handler.
async fn handle_logs(
    State(pipeline): State<Arc<IngestPipeline>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(status) = require_protobuf_content_type(&headers) {
        return status.into_response();
    }
    let req = match ExportLogsServiceRequest::decode(body.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("protobuf decode: {e}")).into_response()
        }
    };
    match pipeline.clone().ingest_logs(req).await {
        Ok(resp) => encode_proto_response(&resp),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ingest_logs: {e}"),
        )
            .into_response(),
    }
}

/// `POST /v1/metrics` handler.
async fn handle_metrics(
    State(pipeline): State<Arc<IngestPipeline>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(status) = require_protobuf_content_type(&headers) {
        return status.into_response();
    }
    let req = match ExportMetricsServiceRequest::decode(body.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("protobuf decode: {e}")).into_response()
        }
    };
    match pipeline.clone().ingest_metrics(req).await {
        Ok(resp) => encode_proto_response(&resp),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ingest_metrics: {e}"),
        )
            .into_response(),
    }
}

/// `POST /v1/traces` handler.
async fn handle_traces(
    State(pipeline): State<Arc<IngestPipeline>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(status) = require_protobuf_content_type(&headers) {
        return status.into_response();
    }
    let req = match ExportTraceServiceRequest::decode(body.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("protobuf decode: {e}")).into_response()
        }
    };
    match pipeline.clone().ingest_traces(req).await {
        Ok(resp) => encode_proto_response(&resp),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ingest_traces: {e}"),
        )
            .into_response(),
    }
}
