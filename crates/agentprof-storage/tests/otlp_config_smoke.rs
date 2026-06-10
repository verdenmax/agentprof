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
