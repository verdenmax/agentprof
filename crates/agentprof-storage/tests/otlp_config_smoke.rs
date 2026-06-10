//! Smoke test: `OtlpServerConfig` default + `PartialOtlpServerConfig` resolve.

#![cfg(feature = "otlp")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentprof_storage::otlp::config::{OtlpServerConfig, PartialOtlpServerConfig};
use std::time::Duration;

#[test]
fn default_listens_on_loopback_both_ports() {
    let cfg = OtlpServerConfig::default();
    assert_eq!(cfg.listen_grpc.unwrap().to_string(), "127.0.0.1:4317");
    assert_eq!(cfg.listen_http.unwrap().to_string(), "127.0.0.1:4318");
    assert!(cfg.listen_token.is_none());
    assert!(cfg.tls_cert.is_none());
    assert_eq!(cfg.session_idle_timeout, Duration::from_secs(300));
    assert_eq!(cfg.shutdown_grace, Duration::from_secs(10));
}

#[test]
fn from_partial_resolves_with_defaults() {
    let partial = PartialOtlpServerConfig::default();
    let cfg = OtlpServerConfig::from_partial(partial).unwrap();
    assert_eq!(cfg.listen_grpc.unwrap().to_string(), "127.0.0.1:4317");
}

#[test]
fn from_partial_disable_grpc_via_empty_string() {
    let partial = PartialOtlpServerConfig {
        listen_grpc: Some(String::new()),
        ..Default::default()
    };
    let cfg = OtlpServerConfig::from_partial(partial).unwrap();
    assert!(
        cfg.listen_grpc.is_none(),
        "empty string should disable grpc"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn grpc_server_binds_and_shuts_down() {
    use agentprof_storage::otlp::pipeline::IngestPipeline;
    use agentprof_storage::otlp::server_grpc::serve_grpc;
    use std::sync::Arc;

    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = Some("127.0.0.1:0".parse().unwrap());
    cfg.listen_http = None;

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_grpc(cfg, pipeline).await.expect("bind");
    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[test]
fn from_partial_parses_humantime_durations() {
    let partial = PartialOtlpServerConfig {
        session_idle_timeout: Some("2m".to_owned()),
        shutdown_grace: Some("30s".to_owned()),
        ..Default::default()
    };
    let cfg = OtlpServerConfig::from_partial(partial).unwrap();
    assert_eq!(cfg.session_idle_timeout, Duration::from_secs(120));
    assert_eq!(cfg.shutdown_grace, Duration::from_secs(30));
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_round_trip_increments_logs_counter() {
    use agentprof_storage::otlp::pipeline::IngestPipeline;
    use agentprof_storage::otlp::server_grpc::serve_grpc;
    use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
    use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
    use std::sync::Arc;
    use tokio::net::TcpListener;

    // Pre-bind on port 0 to learn an OS-assigned port, then drop the listener
    // so `serve_grpc` can rebind on the same port. There is an inherent race
    // here (another process may grab the port), but it is acceptable for a
    // loopback smoke test.
    let probe = TcpListener::bind("127.0.0.1:0").await.expect("probe bind");
    let addr = probe.local_addr().expect("probe addr");
    drop(probe);

    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = Some(addr);
    cfg.listen_http = None;

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_grpc(cfg, pipeline.clone()).await.expect("serve_grpc");

    // Give the server task a moment to start serving on the rebinding port.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let endpoint = format!("http://{addr}");
    let resp = {
        let mut client = LogsServiceClient::connect(endpoint)
            .await
            .expect("connect logs client");
        client
            .export(ExportLogsServiceRequest {
                resource_logs: vec![],
            })
            .await
            .expect("export logs")
    };
    assert!(
        resp.into_inner().partial_success.is_none(),
        "expected no partial_success on empty export"
    );

    assert_eq!(
        pipeline.counts_for_test(),
        (1, 0, 0),
        "logs counter should have incremented exactly once"
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_server_binds_serves_logs_then_shuts_down() {
    use agentprof_storage::otlp::pipeline::IngestPipeline;
    use agentprof_storage::otlp::server_http::serve_http;
    use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
    use prost::Message;
    use std::sync::Arc;
    use tokio::net::TcpListener;

    // Pre-bind on port 0 to learn an OS-assigned port, then drop the listener
    // so `serve_http` can rebind on the same port. There is an inherent race
    // here (another process may grab the port), but it is acceptable for a
    // loopback smoke test.
    let probe = TcpListener::bind("127.0.0.1:0").await.expect("probe bind");
    let addr = probe.local_addr().expect("probe addr");
    drop(probe);

    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = None;
    cfg.listen_http = Some(addr);

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_http(cfg, pipeline.clone()).await.expect("serve_http");

    // Give the server task a moment to start accepting on the rebound port.
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
    assert!(
        resp.status().is_success(),
        "expected 2xx, got {}",
        resp.status()
    );

    assert_eq!(
        pipeline.counts_for_test(),
        (1, 0, 0),
        "logs counter should have incremented exactly once"
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}
