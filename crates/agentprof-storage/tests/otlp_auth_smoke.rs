//! Smoke test: bearer-token auth on both OTLP transports (M2.2 T4.1).
//!
//! Covers three behaviours of [`agentprof_storage::otlp::auth`]:
//!
//! 1. gRPC unauthenticated when no token is supplied → `tonic::Code::Unauthenticated`.
//! 2. gRPC accepted when the correct `Authorization: Bearer <T>` is injected.
//! 3. HTTP unauthenticated → `401 UNAUTHORIZED`.
//!
//! The "no-token-configured passthrough" case is exercised by every
//! pre-existing test in `otlp_config_smoke.rs` (none of them set
//! `listen_token`), so we do not duplicate it here.

#![cfg(feature = "otlp")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::result_large_err,
    clippy::significant_drop_tightening
)]

use std::sync::Arc;
use std::time::Duration;

use agentprof_storage::otlp::config::OtlpServerConfig;
use agentprof_storage::otlp::pipeline::IngestPipeline;
use agentprof_storage::otlp::server_grpc::serve_grpc;
use agentprof_storage::otlp::server_http::serve_http;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tokio::net::TcpListener;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;

const SHARED_TOKEN: &str = "secret-shared-token";

async fn pick_port() -> std::net::SocketAddr {
    let probe = TcpListener::bind("127.0.0.1:0").await.expect("probe bind");
    let addr = probe.local_addr().expect("probe addr");
    drop(probe);
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_rejects_request_without_bearer_when_token_configured() {
    let addr = pick_port().await;

    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = Some(addr);
    cfg.listen_http = None;
    cfg.listen_token = Some(SHARED_TOKEN.to_owned());

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_grpc(cfg, pipeline.clone()).await.expect("serve_grpc");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let endpoint = format!("http://{addr}");
    let mut client = LogsServiceClient::connect(endpoint)
        .await
        .expect("connect logs client");
    let err = client
        .export(ExportLogsServiceRequest {
            resource_logs: vec![],
        })
        .await
        .expect_err("expected Unauthenticated, got Ok");
    assert_eq!(
        err.code(),
        tonic::Code::Unauthenticated,
        "expected Unauthenticated, got {err:?}",
    );
    assert_eq!(
        pipeline.counts_for_test(),
        (0, 0, 0),
        "pipeline must not have been invoked for unauthenticated request",
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_accepts_request_with_correct_bearer() {
    let addr = pick_port().await;

    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = Some(addr);
    cfg.listen_http = None;
    cfg.listen_token = Some(SHARED_TOKEN.to_owned());

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_grpc(cfg, pipeline.clone()).await.expect("serve_grpc");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let endpoint = format!("http://{addr}");
    let channel = Channel::from_shared(endpoint)
        .expect("endpoint")
        .connect()
        .await
        .expect("connect channel");

    let bearer: MetadataValue<_> = format!("Bearer {SHARED_TOKEN}")
        .parse()
        .expect("parse bearer");
    let mut client = LogsServiceClient::with_interceptor(channel, move |mut req: Request<()>| {
        req.metadata_mut().insert("authorization", bearer.clone());
        Ok(req)
    });

    let resp = client
        .export(ExportLogsServiceRequest {
            resource_logs: vec![],
        })
        .await
        .expect("export logs with bearer should succeed");
    assert!(resp.into_inner().partial_success.is_none());
    assert_eq!(
        pipeline.counts_for_test(),
        (1, 0, 0),
        "pipeline should have received the authenticated logs export",
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_rejects_request_without_bearer_when_token_configured() {
    let addr = pick_port().await;

    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = None;
    cfg.listen_http = Some(addr);
    cfg.listen_token = Some(SHARED_TOKEN.to_owned());

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_http(cfg, pipeline.clone()).await.expect("serve_http");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let req = ExportLogsServiceRequest {
        resource_logs: vec![],
    };
    let mut body = Vec::with_capacity(req.encoded_len());
    req.encode(&mut body).expect("encode request");

    let url = format!("http://{addr}/v1/logs");
    let client = reqwest::Client::builder().build().expect("build client");
    let resp = client
        .post(&url)
        .header("content-type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("send POST /v1/logs");
    assert_eq!(
        resp.status().as_u16(),
        401,
        "expected 401 UNAUTHORIZED, got {}",
        resp.status(),
    );
    assert_eq!(
        pipeline.counts_for_test(),
        (0, 0, 0),
        "pipeline must not have been invoked for unauthenticated request",
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_constant_time_bearer_compare_still_rejects_wrong_token() {
    // Setup mirrors grpc_rejects_request_without_bearer_when_token_configured
    // but supplies a wrong token of the *same length* as the expected one
    // to exercise the path where naive `==` would still short-circuit late
    // (so functional behavior is identical but the constant-time variant
    // is the one actually running).
    use agentprof_storage::otlp::config::{OtlpServerConfig, PartialOtlpServerConfig};
    use agentprof_storage::otlp::pipeline::IngestPipeline;
    use agentprof_storage::otlp::server_grpc::serve_grpc;
    use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
    use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Arc;
    use tonic::metadata::MetadataValue;
    use tonic::transport::Channel;
    use tonic::Request;

    let lis = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr: SocketAddr = lis.local_addr().expect("local_addr");
    drop(lis);

    let mut cfg = OtlpServerConfig::from_partial(PartialOtlpServerConfig::default())
        .expect("default partial");
    cfg.listen_grpc = Some(addr);
    cfg.listen_http = None;
    cfg.listen_token = Some("expected-secret-XYZ".to_owned());

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (join, shutdown) = serve_grpc(cfg, pipeline.clone()).await.expect("serve");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let channel = Channel::from_shared(format!("http://{addr}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut client = LogsServiceClient::with_interceptor(channel, |mut req: Request<()>| {
        // Same length as expected, last byte differs — naive `==` short-circuits at the last byte;
        // constant-time scans the whole slice. Behavior must be identical (Unauthenticated).
        let v: MetadataValue<_> = "Bearer expected-secret-XYY".parse().expect("metadata");
        req.metadata_mut().insert("authorization", v);
        Ok(req)
    });

    let err = client
        .export(ExportLogsServiceRequest {
            resource_logs: vec![],
        })
        .await
        .expect_err("must reject");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
    assert_eq!(
        pipeline.counts_for_test(),
        (0, 0, 0),
        "pipeline must not be invoked on rejected request"
    );

    let _ = shutdown.send(());
    let _ = join.await;
}
