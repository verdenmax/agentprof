//! Per-signal request-size caps (F2 / ADR-0022 D-2).
//!
//! For each of the 3 signals (logs / metrics / traces) and each of the
//! 2 transports (gRPC / HTTP), assert that a payload above the cap is
//! rejected by the transport layer BEFORE reaching `IngestPipeline`.
//!
//! Test pattern: bind an ephemeral port, configure a low cap (8 KiB),
//! send an oversize protobuf, assert the expected error code, assert
//! the pipeline counters are still zero.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::significant_drop_tightening
)]

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use agentprof_storage::otlp::config::{OtlpServerConfig, PartialOtlpServerConfig};
use agentprof_storage::otlp::pipeline::IngestPipeline;
use agentprof_storage::otlp::server_grpc::serve_grpc;
use agentprof_storage::otlp::server_http::serve_http;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{Metric, ResourceMetrics, ScopeMetrics};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;
use tonic::transport::Channel;

const SMALL_CAP_BYTES: usize = 8 * 1024;

fn ephemeral_addr() -> SocketAddr {
    let lis = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr = lis.local_addr().expect("local_addr");
    drop(lis);
    addr
}

fn tight_caps_grpc(addr: SocketAddr) -> OtlpServerConfig {
    let mut cfg = OtlpServerConfig::from_partial(PartialOtlpServerConfig::default())
        .expect("default partial");
    cfg.listen_grpc = Some(addr);
    cfg.listen_http = None;
    cfg.listen_token = None;
    cfg.max_logs_request_bytes = SMALL_CAP_BYTES;
    cfg.max_metrics_request_bytes = SMALL_CAP_BYTES;
    cfg.max_traces_request_bytes = SMALL_CAP_BYTES;
    cfg
}

fn tight_caps_http(addr: SocketAddr) -> OtlpServerConfig {
    let mut cfg = OtlpServerConfig::from_partial(PartialOtlpServerConfig::default())
        .expect("default partial");
    cfg.listen_grpc = None;
    cfg.listen_http = Some(addr);
    cfg.listen_token = None;
    cfg.max_logs_request_bytes = SMALL_CAP_BYTES;
    cfg.max_metrics_request_bytes = SMALL_CAP_BYTES;
    cfg.max_traces_request_bytes = SMALL_CAP_BYTES;
    cfg
}

fn fat_log_request() -> ExportLogsServiceRequest {
    let big = "x".repeat(16 * 1024);
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "session.id".into(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue("s1".into())),
                    }),
                }],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                log_records: vec![LogRecord {
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(big)),
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            schema_url: String::new(),
        }],
    }
}

fn fat_metric_request() -> ExportMetricsServiceRequest {
    let big = "y".repeat(16 * 1024);
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "session.id".into(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue("s2".into())),
                    }),
                }],
                ..Default::default()
            }),
            scope_metrics: vec![ScopeMetrics {
                metrics: vec![Metric {
                    name: big,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            schema_url: String::new(),
        }],
    }
}

fn fat_trace_request() -> ExportTraceServiceRequest {
    let big = "z".repeat(16 * 1024);
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "session.id".into(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue("s3".into())),
                    }),
                }],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: vec![Span {
                    name: big,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            schema_url: String::new(),
        }],
    }
}

// --- gRPC tests ------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_logs_resource_exhausted_over_cap() {
    let addr = ephemeral_addr();
    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (join, shutdown) = serve_grpc(tight_caps_grpc(addr), pipeline.clone())
        .await
        .expect("serve");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let chan = Channel::from_shared(format!("http://{addr}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut client = LogsServiceClient::new(chan);

    let err = client
        .export(fat_log_request())
        .await
        .expect_err("must reject");
    assert!(
        matches!(
            err.code(),
            tonic::Code::OutOfRange | tonic::Code::ResourceExhausted | tonic::Code::Internal
        ),
        "unexpected gRPC status: {:?}",
        err.code()
    );
    assert_eq!(pipeline.counts_for_test(), (0, 0, 0));

    let _ = shutdown.send(());
    let _ = join.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_metrics_resource_exhausted_over_cap() {
    let addr = ephemeral_addr();
    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (join, shutdown) = serve_grpc(tight_caps_grpc(addr), pipeline.clone())
        .await
        .expect("serve");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let chan = Channel::from_shared(format!("http://{addr}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut client = MetricsServiceClient::new(chan);

    let err = client
        .export(fat_metric_request())
        .await
        .expect_err("must reject");
    assert!(
        matches!(
            err.code(),
            tonic::Code::OutOfRange | tonic::Code::ResourceExhausted | tonic::Code::Internal
        ),
        "unexpected gRPC status: {:?}",
        err.code()
    );
    assert_eq!(pipeline.counts_for_test(), (0, 0, 0));

    let _ = shutdown.send(());
    let _ = join.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_traces_resource_exhausted_over_cap() {
    let addr = ephemeral_addr();
    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (join, shutdown) = serve_grpc(tight_caps_grpc(addr), pipeline.clone())
        .await
        .expect("serve");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let chan = Channel::from_shared(format!("http://{addr}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut client = TraceServiceClient::new(chan);

    let err = client
        .export(fat_trace_request())
        .await
        .expect_err("must reject");
    assert!(
        matches!(
            err.code(),
            tonic::Code::OutOfRange | tonic::Code::ResourceExhausted | tonic::Code::Internal
        ),
        "unexpected gRPC status: {:?}",
        err.code()
    );
    assert_eq!(pipeline.counts_for_test(), (0, 0, 0));

    let _ = shutdown.send(());
    let _ = join.await;
}

// --- HTTP tests ------------------------------------------------------------

async fn http_post(addr: SocketAddr, path: &str, body: Vec<u8>) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("http://{addr}{path}"))
        .header("content-type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("send")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_logs_413_when_body_over_cap() {
    let addr = ephemeral_addr();
    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (join, shutdown) = serve_http(tight_caps_http(addr), pipeline.clone())
        .await
        .expect("serve");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let body = fat_log_request().encode_to_vec();
    assert!(
        body.len() > SMALL_CAP_BYTES,
        "fixture not actually fat: {} bytes",
        body.len()
    );

    let resp = http_post(addr, "/v1/logs", body).await;
    assert_eq!(resp.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(pipeline.counts_for_test(), (0, 0, 0));

    let _ = shutdown.send(());
    let _ = join.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_metrics_413_when_body_over_cap() {
    let addr = ephemeral_addr();
    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (join, shutdown) = serve_http(tight_caps_http(addr), pipeline.clone())
        .await
        .expect("serve");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let body = fat_metric_request().encode_to_vec();
    assert!(body.len() > SMALL_CAP_BYTES);

    let resp = http_post(addr, "/v1/metrics", body).await;
    assert_eq!(resp.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(pipeline.counts_for_test(), (0, 0, 0));

    let _ = shutdown.send(());
    let _ = join.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_traces_413_when_body_over_cap() {
    let addr = ephemeral_addr();
    let pipeline = Arc::new(IngestPipeline::noop_for_test());
    let (join, shutdown) = serve_http(tight_caps_http(addr), pipeline.clone())
        .await
        .expect("serve");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let body = fat_trace_request().encode_to_vec();
    assert!(body.len() > SMALL_CAP_BYTES);

    let resp = http_post(addr, "/v1/traces", body).await;
    assert_eq!(resp.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(pipeline.counts_for_test(), (0, 0, 0));

    let _ = shutdown.send(());
    let _ = join.await;
}
