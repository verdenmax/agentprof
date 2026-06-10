//! End-to-end pipeline tests: OTLP wire → mapper → router → `SQLite`
//! [`StorageFlushSink`] (M2.2 T7.1).
//!
//! Builds OTLP `ExportLogsServiceRequest` / `ExportMetricsServiceRequest`
//! / `ExportTracesServiceRequest` envelopes by hand against the same
//! `proto` module the receiver consumes, fires them through
//! [`IngestPipeline`], and asserts the resulting rows appear in an
//! in-memory `SQLite` database.

#![cfg(feature = "otlp")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_wrap,
    clippy::significant_drop_tightening,
    clippy::missing_const_for_fn
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentprof_storage::otlp::pipeline::IngestPipeline;
use agentprof_storage::otlp::proto::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest;
use agentprof_storage::otlp::proto::opentelemetry::proto::collector::metrics::v1::ExportMetricsServiceRequest;
use agentprof_storage::otlp::proto::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
use agentprof_storage::otlp::proto::opentelemetry::proto::common::v1::{
    any_value, AnyValue, InstrumentationScope, KeyValue,
};
use agentprof_storage::otlp::proto::opentelemetry::proto::logs::v1::{
    LogRecord, ResourceLogs, ScopeLogs,
};
use agentprof_storage::otlp::proto::opentelemetry::proto::metrics::v1::{
    metric::Data as MetricData, number_data_point::Value as NumValue, Gauge, Metric,
    NumberDataPoint, ResourceMetrics, ScopeMetrics,
};
use agentprof_storage::otlp::proto::opentelemetry::proto::resource::v1::Resource;
use agentprof_storage::otlp::proto::opentelemetry::proto::trace::v1::{
    ResourceSpans, ScopeSpans, Span,
};
use agentprof_storage::otlp::router::{CloseReason, SessionBufferCaps, SessionRouter};
use agentprof_storage::otlp::sink_storage::StorageFlushSink;
use agentprof_storage::query::load_session;
use agentprof_storage::Db;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const NS_PER_SEC: u64 = 1_000_000_000;

fn kv_str(key: &str, val: &str) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(val.to_owned())),
        }),
    }
}

fn make_resource(session_id: &str, service_name: &str) -> Resource {
    Resource {
        attributes: vec![
            kv_str("session.id", session_id),
            kv_str("service.name", service_name),
        ],
        dropped_attributes_count: 0,
    }
}

fn make_log_record(time_secs: u64, attrs: Vec<KeyValue>) -> LogRecord {
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

fn wrap_logs(resource: Resource, records: Vec<LogRecord>) -> ExportLogsServiceRequest {
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

fn wrap_metrics(resource: Resource, metrics: Vec<Metric>) -> ExportMetricsServiceRequest {
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

fn wrap_traces(resource: Resource, spans: Vec<Span>) -> ExportTraceServiceRequest {
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

fn make_token_metric(name: &str, val: i64, time_secs: u64, attrs: Vec<KeyValue>) -> Metric {
    let dp = NumberDataPoint {
        attributes: attrs,
        start_time_unix_nano: time_secs * NS_PER_SEC,
        time_unix_nano: time_secs * NS_PER_SEC,
        exemplars: Vec::new(),
        flags: 0,
        value: Some(NumValue::AsInt(val)),
    };
    Metric {
        name: name.to_owned(),
        description: String::new(),
        unit: String::new(),
        metadata: Vec::new(),
        data: Some(MetricData::Gauge(Gauge {
            data_points: vec![dp],
        })),
    }
}

fn make_pipeline_with_caps(caps: SessionBufferCaps) -> (Arc<IngestPipeline>, Arc<Mutex<Db>>) {
    let db = Arc::new(Mutex::new(Db::open_in_memory().expect("memory db")));
    let sink = Arc::new(
        StorageFlushSink::new(Arc::clone(&db)).with_now_fn(Arc::new(|| 1_700_000_999_i64)),
    );
    let router = Arc::new(SessionRouter::new(caps, sink));
    let pipeline = Arc::new(IngestPipeline::new(router));
    (pipeline, db)
}

fn make_pipeline() -> (Arc<IngestPipeline>, Arc<Mutex<Db>>) {
    make_pipeline_with_caps(SessionBufferCaps::default())
}

fn session_row_count(db: &Arc<Mutex<Db>>) -> usize {
    let g = db.lock().expect("lock");
    agentprof_storage::query::query_sessions_since(&g, Duration::MAX, i64::MAX)
        .expect("query")
        .len()
}

fn raw_path_for(db: &Arc<Mutex<Db>>, session_id: &str) -> Option<String> {
    let g = db.lock().expect("lock");
    let refs = agentprof_storage::query::query_sessions_since(&g, Duration::MAX, i64::MAX).ok()?;
    refs.into_iter()
        .find(|r| r.id == session_id)
        .and_then(|r| r.raw_path.map(|p| p.to_string_lossy().into_owned()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_logs_to_storage_session_start_end() {
    let (pipeline, db) = make_pipeline();

    let session_id = "sess-logs-1";
    let resource = make_resource(session_id, "claude-code");
    let start = make_log_record(
        1_700_000_000,
        vec![
            kv_str("event.name", "session.start"),
            kv_str("model", "claude-sonnet-4.6"),
        ],
    );
    let end = make_log_record(1_700_000_300, vec![kv_str("event.name", "session.end")]);
    let req = wrap_logs(resource, vec![start, end]);

    pipeline.ingest_logs(req).await.expect("ingest_logs");

    assert_eq!(session_row_count(&db), 1, "exactly one session row");
    assert_eq!(
        raw_path_for(&db, session_id).as_deref(),
        Some(format!("otlp://{session_id}").as_str()),
    );

    let report = load_session(&db.lock().expect("lock"), session_id).expect("load");
    assert_eq!(report.meta.id, session_id);
    assert_eq!(
        report.meta.started_at.timestamp(),
        1_700_000_000,
        "started_at preserved",
    );
}

#[tokio::test]
async fn pipeline_metrics_token_usage_persisted() {
    let (pipeline, db) = make_pipeline();

    let session_id = "sess-metrics-1";
    let resource = make_resource(session_id, "claude-code");

    // Open the session with a logs SessionStart so the buffer has a clean
    // started_at and the agent kind is non-default.
    let start_req = wrap_logs(
        resource.clone(),
        vec![make_log_record(
            1_700_000_000,
            vec![
                kv_str("event.name", "session.start"),
                kv_str("model", "claude-sonnet-4.6"),
            ],
        )],
    );
    pipeline
        .clone()
        .ingest_logs(start_req)
        .await
        .expect("ingest_logs start");

    let m_input = make_token_metric(
        "gen_ai.client.token.usage",
        120,
        1_700_000_010,
        vec![
            kv_str("gen_ai.token.type", "input"),
            kv_str("gen_ai.response.model", "claude-sonnet-4.6"),
        ],
    );
    let m_output = make_token_metric(
        "gen_ai.client.token.usage",
        45,
        1_700_000_020,
        vec![
            kv_str("gen_ai.token.type", "output"),
            kv_str("gen_ai.response.model", "claude-sonnet-4.6"),
        ],
    );
    let metrics_req = wrap_metrics(resource.clone(), vec![m_input, m_output]);
    pipeline
        .clone()
        .ingest_metrics(metrics_req)
        .await
        .expect("ingest_metrics");

    // Close the session.
    let end_req = wrap_logs(
        resource,
        vec![make_log_record(
            1_700_000_030,
            vec![kv_str("event.name", "session.end")],
        )],
    );
    pipeline
        .ingest_logs(end_req)
        .await
        .expect("ingest_logs end");

    assert_eq!(session_row_count(&db), 1);
    let report = load_session(&db.lock().expect("lock"), session_id).expect("load");
    let mm = report.model_metrics.expect("model_metrics populated");
    let u = &mm["claude-sonnet-4.6"];
    assert_eq!(u.input_tokens, 120);
    assert_eq!(u.output_tokens, 45);
}

#[tokio::test]
async fn pipeline_traces_tool_pair_persisted() {
    let (pipeline, db) = make_pipeline();

    let session_id = "sess-traces-1";
    let resource = make_resource(session_id, "claude-code");

    // Start session via a logs SessionStart (traces tool span needs an
    // owning session buffer to land in).
    let start_req = wrap_logs(
        resource.clone(),
        vec![make_log_record(
            1_700_000_000,
            vec![kv_str("event.name", "session.start")],
        )],
    );
    pipeline
        .clone()
        .ingest_logs(start_req)
        .await
        .expect("start");

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
    let traces_req = wrap_traces(resource.clone(), vec![span]);
    pipeline
        .clone()
        .ingest_traces(traces_req)
        .await
        .expect("ingest_traces");

    let end_req = wrap_logs(
        resource,
        vec![make_log_record(
            1_700_000_010,
            vec![kv_str("event.name", "session.end")],
        )],
    );
    pipeline.ingest_logs(end_req).await.expect("end");

    assert_eq!(session_row_count(&db), 1);
    let report = load_session(&db.lock().expect("lock"), session_id).expect("load");
    let bash_row = report
        .tool_rank
        .iter()
        .find(|r| r.name == "bash")
        .expect("bash row");
    assert_eq!(bash_row.call_count, 1);
    assert_eq!(bash_row.success_count, 1);
}

#[tokio::test]
async fn pipeline_oom_flush_persists_partial_session() {
    let caps = SessionBufferCaps::default().with_max_events(3);
    let (pipeline, db) = make_pipeline_with_caps(caps);

    let session_id = "sess-oom-1";
    let resource = make_resource(session_id, "claude-code");
    let records = vec![
        make_log_record(
            1_700_000_000,
            vec![
                kv_str("event.name", "session.start"),
                kv_str("model", "claude-sonnet-4.6"),
            ],
        ),
        make_log_record(
            1_700_000_001,
            vec![kv_str("event.name", "user.prompt"), kv_str("turn.id", "t1")],
        ),
        make_log_record(
            1_700_000_002,
            vec![kv_str("event.name", "user.prompt"), kv_str("turn.id", "t2")],
        ),
        make_log_record(
            1_700_000_003,
            vec![kv_str("event.name", "user.prompt"), kv_str("turn.id", "t3")],
        ),
    ];
    let req = wrap_logs(resource, records);
    pipeline.ingest_logs(req).await.expect("ingest_logs");

    // OOM trip should have flushed the buffer despite no SessionEnd.
    assert_eq!(
        session_row_count(&db),
        1,
        "partial OOM-closed session must be persisted",
    );
    assert_eq!(
        raw_path_for(&db, session_id).as_deref(),
        Some(format!("otlp://{session_id}").as_str()),
    );
}

#[tokio::test]
async fn pipeline_shutdown_drains_all() {
    let (pipeline, db) = make_pipeline();

    for i in 0_u64..3 {
        let session_id = format!("sess-drain-{i}");
        let resource = make_resource(&session_id, "claude-code");
        let req = wrap_logs(
            resource,
            vec![make_log_record(
                1_700_000_000 + i,
                vec![
                    kv_str("event.name", "session.start"),
                    kv_str("model", "claude-sonnet-4.6"),
                ],
            )],
        );
        pipeline
            .clone()
            .ingest_logs(req)
            .await
            .expect("ingest_logs");
    }

    // No SessionEnd events => buffers are still open.
    assert_eq!(session_row_count(&db), 0, "no flush before shutdown");
    assert_eq!(pipeline.router_for_test().open_buffers(), 3);

    // Explicit shutdown drains all buffers through StorageFlushSink.
    let _results = pipeline.router_for_test().flush_all(CloseReason::Shutdown);
    assert_eq!(
        session_row_count(&db),
        3,
        "shutdown drain persisted all three sessions",
    );
}

#[tokio::test]
async fn pipeline_mapper_errors_dont_block_batch() {
    let (pipeline, db) = make_pipeline();

    let session_id = "sess-mixed-1";
    let resource = make_resource(session_id, "claude-code");

    // Good record (session.start) + bad record (invalid token direction
    // would only fail in metrics; here we fabricate a log without
    // session.id which should map to MissingResourceAttr through the
    // metrics path. For logs, the simpler bad-case is an `event.name=user.prompt`
    // log on a resource missing session.id — but our wrap_logs always
    // sets one. So mix in a metric with a malformed direction to trigger
    // a MapperError.
    let good = wrap_logs(
        resource.clone(),
        vec![
            make_log_record(
                1_700_000_000,
                vec![
                    kv_str("event.name", "session.start"),
                    kv_str("model", "claude-sonnet-4.6"),
                ],
            ),
            make_log_record(1_700_000_010, vec![kv_str("event.name", "session.end")]),
        ],
    );
    pipeline
        .clone()
        .ingest_logs(good)
        .await
        .expect("ingest_logs");

    // A metrics request with a malformed direction — surfaces as MapperError.
    let bad_metric = make_token_metric(
        "gen_ai.client.token.usage",
        99,
        1_700_000_011,
        vec![
            kv_str("gen_ai.token.type", "nonsense_direction"),
            kv_str("gen_ai.response.model", "claude-sonnet-4.6"),
        ],
    );
    let metrics_req = wrap_metrics(resource, vec![bad_metric]);
    pipeline
        .clone()
        .ingest_metrics(metrics_req)
        .await
        .expect("ingest_metrics still ok even with bad point");

    assert!(
        pipeline.error_count_for_test() >= 1,
        "mapper error counter incremented",
    );
    assert_eq!(session_row_count(&db), 1, "good batch still persisted");
}

#[tokio::test]
async fn pipeline_close_buffer_explicit_persists_session() {
    // Sanity coverage that `close_buffer(.., CloseReason::Idle)` —
    // the same code path the real idle sweeper exercises — also
    // routes through StorageFlushSink. We can't use std::time-based
    // `sweep_idle` deterministically here because `SessionBuffer.last_seen`
    // is an `Instant` (not tokio time), so we drive the idle flush
    // manually via close_buffer.
    let (pipeline, db) = make_pipeline();

    let session_id = "sess-idle-1".to_owned();
    let resource = make_resource(&session_id, "claude-code");
    let req = wrap_logs(
        resource,
        vec![make_log_record(
            1_700_000_000,
            vec![
                kv_str("event.name", "session.start"),
                kv_str("model", "claude-sonnet-4.6"),
            ],
        )],
    );
    pipeline.clone().ingest_logs(req).await.expect("ingest");

    assert_eq!(session_row_count(&db), 0, "no flush before close_buffer");

    pipeline
        .router_for_test()
        .close_buffer(&session_id, CloseReason::Idle)
        .expect("close_buffer through StorageFlushSink");
    assert_eq!(
        session_row_count(&db),
        1,
        "explicit close_buffer persisted through StorageFlushSink",
    );
}
