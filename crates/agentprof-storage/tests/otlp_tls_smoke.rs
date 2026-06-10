//! Smoke test: rustls TLS + optional mTLS on both OTLP transports (M2.2 T4.2).
//!
//! Covers:
//!
//! 1. gRPC happy path — server with `tls_cert`/`tls_key` set, client
//!    trusts the self-signed CA, single `ExportLogsServiceRequest`
//!    succeeds end-to-end.
//! 2. gRPC mTLS rejection — server additionally sets `tls_client_ca`;
//!    client connects with TLS but no client identity → handshake /
//!    transport fails (we accept multiple error variants).
//! 3. HTTP happy path — `reqwest` client with `add_root_certificate`
//!    POSTs an OTLP logs export to `https://{addr}/v1/logs`, expects
//!    `200 OK`.
//! 4. Pure unit: `load_server_tls_config` surfaces a typed
//!    [`agentprof_storage::otlp::error::OtlpServerError::Io`] when the
//!    cert path does not exist (no network involved).
//!
//! Self-signed cert + key are generated in-memory with `rcgen` per-test
//! and written to a `tempfile::TempDir`; they never leave the process.

#![cfg(feature = "otlp")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::result_large_err,
    clippy::significant_drop_tightening
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use agentprof_storage::otlp::config::OtlpServerConfig;
use agentprof_storage::otlp::error::OtlpServerError;
use agentprof_storage::otlp::pipeline::IngestPipeline;
use agentprof_storage::otlp::server_grpc::serve_grpc;
use agentprof_storage::otlp::server_http::serve_http;
use agentprof_storage::otlp::tls::load_server_tls_config;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use rcgen::{generate_simple_self_signed, CertifiedKey};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tonic::transport::{Certificate as TonicCert, Channel, ClientTlsConfig};

/// Materialise a self-signed cert + key in `dir`, return their paths and
/// the cert PEM bytes (handy for client-side `add_root_certificate`).
struct SelfSigned {
    cert_path: PathBuf,
    key_path: PathBuf,
    cert_pem: String,
}

fn make_self_signed(dir: &Path, sans: &[&str]) -> SelfSigned {
    let sans_owned: Vec<String> = sans.iter().map(|s| (*s).to_string()).collect();
    let CertifiedKey { cert, key_pair } =
        generate_simple_self_signed(sans_owned).expect("rcgen self-signed");
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    std::fs::write(&cert_path, &cert_pem).expect("write cert");
    std::fs::write(&key_path, &key_pem).expect("write key");
    SelfSigned {
        cert_path,
        key_path,
        cert_pem,
    }
}

async fn pick_port() -> std::net::SocketAddr {
    let probe = TcpListener::bind("127.0.0.1:0").await.expect("probe bind");
    let addr = probe.local_addr().expect("probe addr");
    drop(probe);
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_serves_over_tls_with_trusted_client() {
    let dir = TempDir::new().unwrap();
    let server_cert = make_self_signed(dir.path(), &["localhost"]);

    let addr = pick_port().await;
    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = Some(addr);
    cfg.listen_http = None;
    cfg.tls_cert = Some(server_cert.cert_path.clone());
    cfg.tls_key = Some(server_cert.key_path.clone());

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_grpc(cfg, pipeline.clone())
        .await
        .expect("serve_grpc with TLS");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let endpoint = format!("https://localhost:{}", addr.port());
    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(TonicCert::from_pem(server_cert.cert_pem.as_bytes()));
    let channel = Channel::from_shared(endpoint)
        .expect("endpoint")
        .tls_config(tls)
        .expect("tls_config")
        .connect()
        .await
        .expect("tls channel connect");
    let mut client = LogsServiceClient::new(channel);

    let resp = client
        .export(ExportLogsServiceRequest {
            resource_logs: vec![],
        })
        .await
        .expect("TLS export should succeed");
    assert!(resp.into_inner().partial_success.is_none());
    assert_eq!(
        pipeline.counts_for_test(),
        (1, 0, 0),
        "pipeline should have received the TLS-protected logs export",
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_mtls_rejects_client_without_cert() {
    let dir = TempDir::new().unwrap();
    let server_cert = make_self_signed(dir.path(), &["localhost"]);
    // Re-use server self-signed cert as the client-CA bundle: any client
    // identity would have to chain back to it. The test client has no
    // identity at all, which is the failure mode we're asserting.
    let client_ca_path = server_cert.cert_path.clone();

    let addr = pick_port().await;
    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = Some(addr);
    cfg.listen_http = None;
    cfg.tls_cert = Some(server_cert.cert_path.clone());
    cfg.tls_key = Some(server_cert.key_path.clone());
    cfg.tls_client_ca = Some(client_ca_path);

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_grpc(cfg, pipeline.clone())
        .await
        .expect("serve_grpc with mTLS");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let endpoint = format!("https://localhost:{}", addr.port());
    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(TonicCert::from_pem(server_cert.cert_pem.as_bytes()));
    // Connect may succeed (handshake might be deferred) or fail outright
    // depending on tonic / rustls timing. The export attempt definitely
    // must fail because the server demands a client cert we don't have.
    let connect_res = Channel::from_shared(endpoint)
        .expect("endpoint")
        .tls_config(tls)
        .expect("tls_config")
        .connect()
        .await;

    let request_failed = match connect_res {
        Err(_) => true,
        Ok(channel) => {
            let mut client = LogsServiceClient::new(channel);
            client
                .export(ExportLogsServiceRequest {
                    resource_logs: vec![],
                })
                .await
                .is_err()
        }
    };
    assert!(
        request_failed,
        "mTLS server must reject a client that presents no certificate"
    );
    assert_eq!(
        pipeline.counts_for_test(),
        (0, 0, 0),
        "pipeline must not have observed the unauthenticated request",
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_serves_over_tls_with_trusted_client() {
    // Install the default rustls CryptoProvider once per process; reqwest
    // + rustls 0.23 both look this up on the first handshake.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let dir = TempDir::new().unwrap();
    let server_cert = make_self_signed(dir.path(), &["localhost"]);

    let addr = pick_port().await;
    let mut cfg = OtlpServerConfig::default();
    cfg.listen_grpc = None;
    cfg.listen_http = Some(addr);
    cfg.tls_cert = Some(server_cert.cert_path.clone());
    cfg.tls_key = Some(server_cert.key_path.clone());

    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (handle, shutdown) = serve_http(cfg, pipeline.clone())
        .await
        .expect("serve_http with TLS");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let root_cert =
        reqwest::Certificate::from_pem(server_cert.cert_pem.as_bytes()).expect("reqwest root cert");
    let client = reqwest::Client::builder()
        .add_root_certificate(root_cert)
        .build()
        .expect("reqwest client");

    let req = ExportLogsServiceRequest {
        resource_logs: vec![],
    };
    let mut body = Vec::with_capacity(req.encoded_len());
    req.encode(&mut body).expect("encode");

    let url = format!("https://localhost:{}/v1/logs", addr.port());
    let resp = client
        .post(&url)
        .header("content-type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("https POST");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert_eq!(
        pipeline.counts_for_test(),
        (1, 0, 0),
        "pipeline should have received the HTTPS logs export",
    );

    shutdown.send(()).expect("send shutdown");
    handle
        .await
        .expect("server task join")
        .expect("server inner");
}

#[test]
fn tls_config_load_error_on_missing_cert_path() {
    let dir = TempDir::new().unwrap();
    let bogus = dir.path().join("does-not-exist.pem");
    let err = load_server_tls_config(&bogus, &bogus, None)
        .expect_err("missing file must surface an Io error");
    match err {
        OtlpServerError::Io { path, .. } => assert_eq!(path, bogus),
        other => panic!("expected Io error, got {other:?}"),
    }
}
