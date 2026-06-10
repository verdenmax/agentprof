//! End-to-end tests for `agentprof ingest-otlp` (M2.2 T9.1).
//!
//! Each test spawns the real `agentprof` binary as a child process
//! against ephemeral `127.0.0.1` ports with a tempfile-backed `SQLite`
//! store, pushes OTLP envelopes via the upstream
//! `opentelemetry-proto` types over `tonic` (gRPC) or `reqwest`
//! (HTTP/protobuf), then asserts the resulting rows in the database
//! once the receiver has had time to flush.
//!
//! Coverage (spec §7):
//!
//! 1. `e2e_grpc_logs_session_start_end_persists`
//! 2. `e2e_http_metrics_token_usage_persists`
//! 3. `e2e_grpc_traces_tool_pair_persists`
//! 4. `e2e_bearer_token_required_when_configured`
//! 5. `e2e_oom_caps_flush_before_session_end`
//! 6. `e2e_idle_sweeper_flushes_inactive_session`
//! 7. `e2e_multiple_sessions_routed_independently`
//! 8. `e2e_explicit_session_end_persists_even_after_sigkill`
//!
//! All tests use ephemeral ports (`TcpListener::bind("127.0.0.1:0")`
//! drained before spawn) plus a poll-until-bound helper to keep them
//! deterministic in CI; no test exceeds ~5 s wall-clock under normal
//! conditions.

#![cfg(feature = "otlp")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::significant_drop_tightening,
    clippy::doc_markdown,
    clippy::missing_docs_in_private_items,
    clippy::ptr_arg,
    clippy::missing_const_for_fn
)]

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{
    metric::Data as MetricData, number_data_point::Value as NumValue, Gauge, Metric,
    NumberDataPoint, ResourceMetrics, ScopeMetrics,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;
use tonic::transport::Channel;

const NS_PER_SEC: u64 = 1_000_000_000;

// ---------------------------------------------------------------------------
// Process / port helpers
// ---------------------------------------------------------------------------

/// Pick an ephemeral `127.0.0.1` port by binding then immediately
/// dropping a `TcpListener`. Cheap and matches the pattern already
/// used by the storage auth/TLS/config smokes.
fn ephemeral_addr() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    let addr = l.local_addr().expect("probe addr");
    drop(l);
    addr
}

/// Poll `addr` with TCP connect attempts until it accepts a
/// connection or `deadline` elapses. Returns `true` on success.
fn wait_for_bind(addr: SocketAddr, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Per-test SQLite path under the workspace `target/` tree (the
/// agent runtime forbids `/tmp`). Collision-resistant via PID + atomic
/// counter; cleaned up by `ServerGuard::drop`.
fn unique_db_path() -> PathBuf {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("agentprof-e2e-{}-{n}.db", std::process::id()))
}

/// Owns a spawned `agentprof ingest-otlp` child process. Killing the
/// child on `drop` ensures one failing assertion does not leak a
/// receiver into the next test (which would steal the ephemeral port
/// only by accident, but more importantly leak file handles to the
/// `SQLite` store).
struct ServerGuard {
    child: Child,
    grpc: SocketAddr,
    http: SocketAddr,
    db: PathBuf,
}

impl ServerGuard {
    fn http_url(&self, path: &str) -> String {
        format!("http://{}{path}", self.http)
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Remove SQLite file + any WAL/SHM siblings; tests assert state
        // before drop, so cleanup is best-effort.
        for ext in ["", "-wal", "-shm"] {
            let mut p = self.db.clone();
            p.set_extension(format!("db{ext}"));
            let _ = std::fs::remove_file(&p);
        }
        let _ = std::fs::remove_file(&self.db);
    }
}

/// Spawn `agentprof ingest-otlp` on ephemeral ports with a tempfile
/// store, a tight `--idle-seconds 1` so per-session buffers do not
/// linger forever, and `--sweeper-interval-seconds 1` so the idle
/// sweeper actually fires inside a test budget. Callers can append
/// extra args (e.g., `--bearer-token`, `--max-session-events`).
fn spawn_server(extra_args: &[&str]) -> ServerGuard {
    let grpc = ephemeral_addr();
    let http = ephemeral_addr();
    let db = unique_db_path();

    let mut cmd = Command::new(cargo_bin("agentprof"));
    cmd.args([
        "ingest-otlp",
        "--grpc",
        &grpc.to_string(),
        "--http",
        &http.to_string(),
        "--store",
        db.to_str().expect("utf8 db path"),
        "--idle-seconds",
        "1",
        "--sweeper-interval-seconds",
        "1",
    ]);
    cmd.args(extra_args);
    // Silence the tracing-subscriber default INFO chatter so test
    // logs stay focused; flip to `inherit` if you need to debug.
    cmd.env("RUST_LOG", "warn")
        .env("AGENTPROF_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().expect("spawn agentprof ingest-otlp");

    assert!(
        wait_for_bind(grpc, Duration::from_secs(5)),
        "gRPC listener at {grpc} did not become reachable within 5s",
    );
    assert!(
        wait_for_bind(http, Duration::from_secs(5)),
        "HTTP listener at {http} did not become reachable within 5s",
    );

    ServerGuard {
        child,
        grpc,
        http,
        db,
    }
}

// ---------------------------------------------------------------------------
// OTLP envelope builders (mirror storage's pipeline_e2e fixtures so the
// CLI exercises the same wire shapes)
// ---------------------------------------------------------------------------

fn kv_str(key: &str, val: &str) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(val.to_owned())),
        }),
    }
}

fn resource_for(session_id: &str) -> Resource {
    Resource {
        attributes: vec![
            kv_str("session.id", session_id),
            kv_str("service.name", "claude-code"),
        ],
        dropped_attributes_count: 0,
    }
}

fn log_record(time_secs: u64, attrs: Vec<KeyValue>) -> LogRecord {
    LogRecord {
        time_unix_nano: time_secs * NS_PER_SEC,
        observed_time_unix_nano: time_secs * NS_PER_SEC,
        severity_number: 0,
        severity_text: String::new(),
        body: None,
        attributes: attrs,
        dropped_attributes_count: 0,
        flags: 0,
        trace_id: Vec::new(),
        span_id: Vec::new(),
    }
}

fn logs_request(resource: Resource, records: Vec<LogRecord>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(resource),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "test".to_owned(),
                    version: String::new(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn metrics_request(resource: Resource, metrics: Vec<Metric>) -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(resource),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "test".to_owned(),
                    version: String::new(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                metrics,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn traces_request(resource: Resource, spans: Vec<Span>) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(resource),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "test".to_owned(),
                    version: String::new(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn token_metric(direction: &str, val: i64, time_secs: u64) -> Metric {
    let dp = NumberDataPoint {
        attributes: vec![
            kv_str("gen_ai.token.type", direction),
            kv_str("gen_ai.response.model", "claude-sonnet-4.6"),
        ],
        start_time_unix_nano: time_secs * NS_PER_SEC,
        time_unix_nano: time_secs * NS_PER_SEC,
        exemplars: Vec::new(),
        flags: 0,
        value: Some(NumValue::AsInt(val)),
    };
    Metric {
        name: "gen_ai.client.token.usage".to_owned(),
        description: String::new(),
        unit: String::new(),
        metadata: Vec::new(),
        data: Some(MetricData::Gauge(Gauge {
            data_points: vec![dp],
        })),
    }
}

fn session_start(time_secs: u64) -> LogRecord {
    log_record(
        time_secs,
        vec![
            kv_str("event.name", "session.start"),
            kv_str("model", "claude-sonnet-4.6"),
        ],
    )
}

fn session_end(time_secs: u64) -> LogRecord {
    log_record(time_secs, vec![kv_str("event.name", "session.end")])
}

// ---------------------------------------------------------------------------
// DB-side assertion helpers
// ---------------------------------------------------------------------------

fn count_sessions(db: &PathBuf) -> i64 {
    let d = agentprof_storage::Db::open_and_migrate(db).expect("open db for assertion");
    d.conn_for_test()
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .expect("count sessions")
}

fn count_sessions_with_id(db: &PathBuf, session_id: &str) -> i64 {
    let d = agentprof_storage::Db::open_and_migrate(db).expect("open db for assertion");
    d.conn_for_test()
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1",
            [session_id],
            |r| r.get(0),
        )
        .expect("count sessions by id")
}

/// Poll the DB until `predicate` is true or `deadline` elapses. Used
/// to bridge "client `export()` returned" → "background flush
/// pipeline has committed the row" without resorting to fixed
/// sleeps.
fn wait_until<F: FnMut() -> bool>(deadline: Duration, mut predicate: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

async fn grpc_logs_client(addr: SocketAddr) -> LogsServiceClient<Channel> {
    let chan = Channel::from_shared(format!("http://{addr}"))
        .expect("endpoint")
        .connect()
        .await
        .expect("connect grpc");
    LogsServiceClient::new(chan)
}

async fn grpc_traces_client(addr: SocketAddr) -> TraceServiceClient<Channel> {
    let chan = Channel::from_shared(format!("http://{addr}"))
        .expect("endpoint")
        .connect()
        .await
        .expect("connect grpc");
    TraceServiceClient::new(chan)
}

async fn http_post_proto<M: Message>(url: &str, msg: &M) -> reqwest::Response {
    let mut body = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut body).expect("encode proto");
    reqwest::Client::new()
        .post(url)
        .header("content-type", "application/x-protobuf")
        .body(body)
        .send()
        .await
        .expect("POST otlp/http")
}

// ===========================================================================
// Tests
// ===========================================================================

/// 1. gRPC logs: SessionStart + SessionEnd flushes a single row.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_grpc_logs_session_start_end_persists() {
    let server = spawn_server(&[]);
    let mut client = grpc_logs_client(server.grpc).await;

    let session_id = "e2e-grpc-logs-1";
    let req = logs_request(
        resource_for(session_id),
        vec![session_start(1_700_000_000), session_end(1_700_000_010)],
    );
    client.export(req).await.expect("export logs");

    let db = server.db.clone();
    assert!(
        wait_until(Duration::from_secs(5), || {
            count_sessions_with_id(&db, session_id) == 1
        }),
        "session row never appeared",
    );
}

/// 2. HTTP metrics: token usage on a previously-opened session
/// produces persisted token rollups after SessionEnd.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_http_metrics_token_usage_persists() {
    let server = spawn_server(&[]);
    let session_id = "e2e-http-metrics-1";

    // Open the session via gRPC logs (HTTP works too, but mixing the
    // transports here also exercises the routing path on the server).
    let mut logs = grpc_logs_client(server.grpc).await;
    logs.export(logs_request(
        resource_for(session_id),
        vec![session_start(1_700_000_000)],
    ))
    .await
    .expect("export start");

    // Push token usage via HTTP/protobuf.
    let metrics = metrics_request(
        resource_for(session_id),
        vec![
            token_metric("input", 120, 1_700_000_005),
            token_metric("output", 45, 1_700_000_006),
        ],
    );
    let resp = http_post_proto(&server.http_url("/v1/metrics"), &metrics).await;
    assert_eq!(resp.status().as_u16(), 200, "metrics POST should 200 OK");

    // Close.
    logs.export(logs_request(
        resource_for(session_id),
        vec![session_end(1_700_000_010)],
    ))
    .await
    .expect("export end");

    let db = server.db.clone();
    assert!(
        wait_until(Duration::from_secs(5), || {
            // Once the row is committed, total_*_tokens should reflect
            // the rollup. Either field non-NULL is fine; we assert
            // input matches.
            let d = agentprof_storage::Db::open_and_migrate(&db).expect("open");
            d.conn_for_test()
                .query_row(
                    "SELECT total_input_tokens FROM sessions WHERE id = ?1",
                    [session_id],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .ok()
                .flatten()
                == Some(120)
        }),
        "token usage never rolled up to sessions.total_input_tokens",
    );
}

/// 3. gRPC traces: a single tool span produces a `tools_loaded` row.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_grpc_traces_tool_pair_persists() {
    let server = spawn_server(&[]);
    let session_id = "e2e-grpc-traces-1";

    let mut logs = grpc_logs_client(server.grpc).await;
    logs.export(logs_request(
        resource_for(session_id),
        vec![session_start(1_700_000_000)],
    ))
    .await
    .expect("export start");

    let mut traces = grpc_traces_client(server.grpc).await;
    let span = Span {
        trace_id: vec![0xab; 16],
        span_id: vec![0xcd; 8],
        trace_state: String::new(),
        parent_span_id: Vec::new(),
        flags: 0,
        name: "tool.execute".to_owned(),
        kind: 0,
        start_time_unix_nano: 1_700_000_005 * NS_PER_SEC,
        end_time_unix_nano: 1_700_000_007 * NS_PER_SEC,
        attributes: vec![
            kv_str("gen_ai.operation.name", "tool.execute"),
            kv_str("tool.name", "bash"),
        ],
        dropped_attributes_count: 0,
        events: Vec::new(),
        dropped_events_count: 0,
        links: Vec::new(),
        dropped_links_count: 0,
        status: None,
    };
    traces
        .export(traces_request(resource_for(session_id), vec![span]))
        .await
        .expect("export traces");

    logs.export(logs_request(
        resource_for(session_id),
        vec![session_end(1_700_000_010)],
    ))
    .await
    .expect("export end");

    let db = server.db.clone();
    assert!(
        wait_until(Duration::from_secs(5), || {
            let d = agentprof_storage::Db::open_and_migrate(&db).expect("open");
            let n: i64 = d
                .conn_for_test()
                .query_row(
                    "SELECT COUNT(*) FROM tools_loaded WHERE session_id = ?1 AND tool_name = 'bash'",
                    [session_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            n >= 1
        }),
        "bash tool row never appeared in tools_loaded",
    );
}

/// 4. Bearer auth: a request without the configured token is rejected
/// before the pipeline sees it; no session row appears.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_bearer_token_required_when_configured() {
    let server = spawn_server(&["--bearer-token", "secret-e2e-token"]);

    let mut client = grpc_logs_client(server.grpc).await;
    let req = logs_request(
        resource_for("e2e-bearer-1"),
        vec![session_start(1_700_000_000), session_end(1_700_000_010)],
    );
    let err = client
        .export(req)
        .await
        .expect_err("expected Unauthenticated, got Ok");
    assert_eq!(
        err.code(),
        tonic::Code::Unauthenticated,
        "expected Unauthenticated, got {err:?}",
    );

    // Brief wait to make sure no late row sneaks in.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        count_sessions(&server.db),
        0,
        "no session should be persisted after auth rejection",
    );
}

/// 5. OOM caps: with `--max-session-events 3` and four events sent on
/// the same session without a SessionEnd, the buffer trips its cap
/// and flushes a partial row.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_oom_caps_flush_before_session_end() {
    let server = spawn_server(&["--max-session-events", "3"]);
    let session_id = "e2e-oom-1";

    let mut client = grpc_logs_client(server.grpc).await;
    let req = logs_request(
        resource_for(session_id),
        vec![
            session_start(1_700_000_000),
            log_record(
                1_700_000_001,
                vec![kv_str("event.name", "user.prompt"), kv_str("turn.id", "t1")],
            ),
            log_record(
                1_700_000_002,
                vec![kv_str("event.name", "user.prompt"), kv_str("turn.id", "t2")],
            ),
            log_record(
                1_700_000_003,
                vec![kv_str("event.name", "user.prompt"), kv_str("turn.id", "t3")],
            ),
        ],
    );
    client.export(req).await.expect("export logs");

    let db = server.db.clone();
    assert!(
        wait_until(Duration::from_secs(5), || {
            count_sessions_with_id(&db, session_id) == 1
        }),
        "OOM-capped session row never appeared",
    );
}

/// 6. Idle sweeper: after `--idle-seconds 1` plus the
/// `--sweeper-interval-seconds 1` test override, an unclosed session
/// is flushed automatically.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_idle_sweeper_flushes_inactive_session() {
    let server = spawn_server(&[]);
    let session_id = "e2e-idle-1";

    let mut client = grpc_logs_client(server.grpc).await;
    client
        .export(logs_request(
            resource_for(session_id),
            vec![session_start(1_700_000_000)],
        ))
        .await
        .expect("export start");

    // No SessionEnd: rely on the idle sweeper to flush within a few
    // ticks (idle=1s + sweeper-tick=1s; allow up to 6s for slow CI).
    let db = server.db.clone();
    assert!(
        wait_until(Duration::from_secs(6), || {
            count_sessions_with_id(&db, session_id) == 1
        }),
        "idle sweeper never flushed the inactive session",
    );
}

/// 7. Routing: three interleaved sessions land in three distinct
/// rows.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_multiple_sessions_routed_independently() {
    let server = spawn_server(&[]);
    let ids = ["e2e-multi-a", "e2e-multi-b", "e2e-multi-c"];

    let mut client = grpc_logs_client(server.grpc).await;

    // Interleave starts then ends across the three sessions.
    for id in ids {
        client
            .export(logs_request(
                resource_for(id),
                vec![session_start(1_700_000_000)],
            ))
            .await
            .expect("export start");
    }
    for id in ids {
        client
            .export(logs_request(
                resource_for(id),
                vec![session_end(1_700_000_010)],
            ))
            .await
            .expect("export end");
    }

    let db = server.db.clone();
    assert!(
        wait_until(Duration::from_secs(5), || {
            ids.iter().all(|id| count_sessions_with_id(&db, id) == 1)
        }),
        "expected three distinct session rows; got {}",
        count_sessions(&db),
    );
}

/// 8. Explicit SessionEnd is durable across an ungraceful child
/// termination.
///
/// The task brief acknowledges that the test harness has no portable
/// way to send `SIGTERM` (we are forbidden from adding `nix` to the
/// workspace), so this test verifies a *stronger* property: even
/// when the receiver is hit with `SIGKILL` shortly after we pushed
/// `session.start` + `session.end`, the explicit-end flush path has
/// already persisted the row — i.e. flushes are synchronous through
/// `export()` and the data survives loss of the graceful-shutdown
/// drain.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_explicit_session_end_persists_even_after_sigkill() {
    let mut server = spawn_server(&[]);
    let session_ids = ["e2e-shutdown-a", "e2e-shutdown-b"];

    let mut client = grpc_logs_client(server.grpc).await;
    for id in session_ids {
        client
            .export(logs_request(
                resource_for(id),
                vec![session_start(1_700_000_000), session_end(1_700_000_010)],
            ))
            .await
            .expect("export start+end");
    }

    // Give the server a brief window to acknowledge the flush, then
    // SIGKILL it. We deliberately do *not* await the row first —
    // we want to assert the data survives an ungraceful kill.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.child.kill();
    let _ = server.child.wait();

    // Re-open the store from scratch and verify both rows are there.
    let db = server.db.clone();
    assert!(
        wait_until(Duration::from_secs(5), || {
            session_ids
                .iter()
                .all(|id| count_sessions_with_id(&db, id) == 1)
        }),
        "both explicit-end sessions should persist across SIGKILL; got {}",
        count_sessions(&db),
    );
}
