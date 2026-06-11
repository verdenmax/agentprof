//! gRPC OTLP listener (M2.2 T2.2).
//!
//! Binds tonic's `Server` to [`crate::otlp::config::OtlpServerConfig::listen_grpc`]
//! and registers the three OTLP collector services
//! (`LogsService` / `MetricsService` / `TraceService`). Each service impl
//! delegates to [`crate::otlp::pipeline::IngestPipeline`].
//!
//! The bind+shutdown lifecycle returned from [`serve_grpc`] is:
//!
//! 1. The function awaits `TcpListener::bind`, so a bind error surfaces
//!    synchronously as [`crate::otlp::error::OtlpServerError::Bind`].
//! 2. On success, the server task is spawned and a `(JoinHandle, oneshot::Sender<()>)`
//!    tuple is returned. Sending on the oneshot triggers graceful shutdown
//!    via tonic's `serve_with_shutdown`.
//!
//! TLS / mTLS land in M2.2 T4.2 (see [`crate::otlp::tls`]); proxy-protocol
//! and per-session buffering land in later M2.2 tasks. This module
//! activates TLS automatically when `cfg.tls_cert`/`tls_key` are set.

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::server::TcpIncoming;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};

use crate::otlp::auth::bearer_interceptor;
use crate::otlp::config::OtlpServerConfig;
use crate::otlp::error::OtlpServerError;
use crate::otlp::pipeline::IngestPipeline;
use crate::otlp::proto::logs::logs_service_server::{LogsService, LogsServiceServer};
use crate::otlp::proto::logs::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use crate::otlp::proto::metrics::metrics_service_server::{MetricsService, MetricsServiceServer};
use crate::otlp::proto::metrics::{ExportMetricsServiceRequest, ExportMetricsServiceResponse};
use crate::otlp::proto::trace::trace_service_server::{TraceService, TraceServiceServer};
use crate::otlp::proto::trace::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use crate::otlp::tls::read_pem_file;

/// Handle pair returned by [`serve_grpc`]: the server's join handle and
/// a oneshot used to request graceful shutdown.
///
/// Sending `()` on the [`oneshot::Sender`] causes tonic's
/// `serve_with_shutdown` future to resolve, after which the join handle
/// resolves with the server's inner result.
pub type GrpcServerHandle = (JoinHandle<Result<(), OtlpServerError>>, oneshot::Sender<()>);

/// Bind the OTLP gRPC listener, register the three collector services, and
/// spawn the server task.
///
/// The bind step is awaited synchronously, so a port collision or other
/// `io::Error` is surfaced immediately as
/// [`OtlpServerError::Bind`]. After a successful bind the task is spawned
/// and the returned [`oneshot::Sender`] can be used to request graceful
/// shutdown at any time.
///
/// Requires `cfg.listen_grpc` to be `Some(_)`; the caller is expected to
/// have already run [`OtlpServerConfig::validate`].
///
/// # Errors
///
/// - [`OtlpServerError::Config`] if `cfg.listen_grpc` is `None`.
/// - [`OtlpServerError::Bind`] if `TcpListener::bind` fails.
///
/// # Examples
///
/// ```no_run
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// use agentprof_storage::otlp::config::OtlpServerConfig;
/// use agentprof_storage::otlp::pipeline::IngestPipeline;
/// use agentprof_storage::otlp::server_grpc::serve_grpc;
/// use std::sync::Arc;
///
/// let cfg = OtlpServerConfig::default();
/// let pipeline = Arc::new(IngestPipeline::noop_for_test());
/// let (handle, shutdown) = serve_grpc(cfg, pipeline).await?;
/// // ... later:
/// let _ = shutdown.send(());
/// handle.await??;
/// # Ok(()) }
/// ```
pub async fn serve_grpc(
    cfg: OtlpServerConfig,
    pipeline: Arc<IngestPipeline>,
) -> Result<GrpcServerHandle, OtlpServerError> {
    crate::otlp::tls::ensure_crypto_provider_installed();

    let addr = cfg.listen_grpc.ok_or_else(|| {
        OtlpServerError::Config("serve_grpc requires cfg.listen_grpc = Some(_)".into())
    })?;

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| OtlpServerError::Bind { addr, source })?;
    let incoming = TcpIncoming::from_listener(listener, true, None)
        .map_err(|e| OtlpServerError::Config(format!("TcpIncoming::from_listener: {e}")))?;

    let token = cfg.listen_token.clone().map(Arc::new);
    let interceptor = bearer_interceptor(token);

    // Per-signal decode caps (ADR-0022 D-2): apply BEFORE wrapping in
    // InterceptedService so tonic enforces the cap during decode, not
    // post-auth.
    let logs = InterceptedService::new(
        LogsServiceServer::new(LogsImpl {
            pipeline: pipeline.clone(),
        })
        .max_decoding_message_size(cfg.max_logs_request_bytes),
        interceptor.clone(),
    );
    let metrics = InterceptedService::new(
        MetricsServiceServer::new(MetricsImpl {
            pipeline: pipeline.clone(),
        })
        .max_decoding_message_size(cfg.max_metrics_request_bytes),
        interceptor.clone(),
    );
    let traces = InterceptedService::new(
        TraceServiceServer::new(TracesImpl { pipeline })
            .max_decoding_message_size(cfg.max_traces_request_bytes),
        interceptor,
    );

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let mut builder = Server::builder();
    if let Some(cert_path) = cfg.tls_cert.as_deref() {
        let key_path = cfg.tls_key.as_deref().ok_or_else(|| {
            OtlpServerError::Config(
                "serve_grpc: tls_cert set but tls_key is None (validate() should have caught this)"
                    .into(),
            )
        })?;
        let cert_pem = read_pem_file(cert_path)?;
        let key_pem = read_pem_file(key_path)?;
        let mut tls = ServerTlsConfig::new().identity(Identity::from_pem(&cert_pem, &key_pem));
        if let Some(ca_path) = cfg.tls_client_ca.as_deref() {
            let ca_pem = read_pem_file(ca_path)?;
            tls = tls.client_ca_root(Certificate::from_pem(&ca_pem));
        }
        builder = builder.tls_config(tls)?;
    }
    let server = builder
        .add_service(logs)
        .add_service(metrics)
        .add_service(traces);

    let join = tokio::spawn(async move {
        server
            .serve_with_incoming_shutdown(incoming, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .map_err(OtlpServerError::from)
    });

    Ok((join, shutdown_tx))
}

/// `LogsService` impl that delegates every export to the shared pipeline.
struct LogsImpl {
    pipeline: Arc<IngestPipeline>,
}

#[tonic::async_trait]
impl LogsService for LogsImpl {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .pipeline
            .clone()
            .ingest_logs(req)
            .await
            .map_err(|e| Status::internal(format!("ingest_logs: {e}")))?;
        Ok(Response::new(resp))
    }
}

/// `MetricsService` impl mirroring [`LogsImpl`].
struct MetricsImpl {
    pipeline: Arc<IngestPipeline>,
}

#[tonic::async_trait]
impl MetricsService for MetricsImpl {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .pipeline
            .clone()
            .ingest_metrics(req)
            .await
            .map_err(|e| Status::internal(format!("ingest_metrics: {e}")))?;
        Ok(Response::new(resp))
    }
}

/// `TraceService` impl mirroring [`LogsImpl`].
struct TracesImpl {
    pipeline: Arc<IngestPipeline>,
}

#[tonic::async_trait]
impl TraceService for TracesImpl {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .pipeline
            .clone()
            .ingest_traces(req)
            .await
            .map_err(|e| Status::internal(format!("ingest_traces: {e}")))?;
        Ok(Response::new(resp))
    }
}
