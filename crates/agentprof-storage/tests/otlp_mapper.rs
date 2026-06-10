//! Unit tests for [`agentprof_storage::otlp::mapper`] (M2.2 T5.2).
//!
//! Constructs OTLP request envelopes with the crate-private wire types
//! re-exported through `agentprof_storage::otlp::proto::*` (avoids
//! cross-crate type confusion with the dev-dep `opentelemetry-proto`).
//!
//! Coverage:
//!
//! - logs: session.start happy path, session.id fallback chain (resource
//!   `session.id` → `claude.session_id` → record-level), `user.prompt`
//!   variant, missing session.id → `MapperError::MissingResourceAttr`.
//! - metrics: token usage Sum gauge with explicit direction, multiple
//!   data points emit one `TypedEvent` each.
//! - traces: a `tool.execute` span yields a `ToolDecisionStart` +
//!   `ToolResult` pair from `start_time_unix_nano` / `end_time_unix_nano`.

#![cfg(feature = "otlp")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::result_large_err,
    clippy::missing_const_for_fn
)]

use agentprof_core::adapter::AgentKind;
use agentprof_core::episode::tool::ToolCallStatus;
use agentprof_storage::otlp::error::MapperError;
use agentprof_storage::otlp::mapper::{map_logs, map_metrics, map_traces};
use agentprof_storage::otlp::proto::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest;
use agentprof_storage::otlp::proto::opentelemetry::proto::collector::metrics::v1::ExportMetricsServiceRequest;
use agentprof_storage::otlp::proto::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
use agentprof_storage::otlp::proto::opentelemetry::proto::common::v1::{
    any_value, AnyValue, KeyValue,
};
use agentprof_storage::otlp::proto::opentelemetry::proto::logs::v1::{
    LogRecord, ResourceLogs, ScopeLogs,
};
use agentprof_storage::otlp::proto::opentelemetry::proto::metrics::v1::{
    metric, number_data_point, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum,
};
use agentprof_storage::otlp::proto::opentelemetry::proto::resource::v1::Resource;
use agentprof_storage::otlp::proto::opentelemetry::proto::trace::v1::{
    status::StatusCode, ResourceSpans, ScopeSpans, Span, Status,
};
use agentprof_storage::otlp::typed::{SignalKind, TokenDirection, TypedEvent};

// ---------------------------------------------------------------------------
// Tiny builders to keep test bodies readable.
// ---------------------------------------------------------------------------

fn kv_string(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_owned())),
        }),
    }
}

fn kv_int(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::IntValue(value)),
        }),
    }
}

fn kv_bool(key: &str, value: bool) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::BoolValue(value)),
        }),
    }
}

const fn ts_nanos() -> u64 {
    // 2026-06-10T12:00:00Z, well within i64 nanos range.
    1_780_488_000_000_000_000_u64
}

fn wrap_logs(resource_attrs: Vec<KeyValue>, records: Vec<LogRecord>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: None,
                log_records: records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn wrap_metrics(
    resource_attrs: Vec<KeyValue>,
    metrics: Vec<Metric>,
) -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn wrap_traces(resource_attrs: Vec<KeyValue>, spans: Vec<Span>) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn log_record(event_name: &str, attrs: Vec<KeyValue>) -> LogRecord {
    let mut all = attrs;
    if !event_name.is_empty() {
        all.push(kv_string("event.name", event_name));
    }
    LogRecord {
        time_unix_nano: ts_nanos(),
        observed_time_unix_nano: ts_nanos(),
        severity_number: 0,
        severity_text: String::new(),
        body: None,
        attributes: all,
        dropped_attributes_count: 0,
        flags: 0,
        trace_id: vec![],
        span_id: vec![],
    }
}

fn sum_metric_with_points(name: &str, points: Vec<NumberDataPoint>) -> Metric {
    Metric {
        name: name.to_owned(),
        description: String::new(),
        unit: String::new(),
        metadata: vec![],
        data: Some(metric::Data::Sum(Sum {
            data_points: points,
            aggregation_temporality: 2, // cumulative; mapper does not care
            is_monotonic: true,
        })),
    }
}

fn int_point(value: i64, attrs: Vec<KeyValue>) -> NumberDataPoint {
    NumberDataPoint {
        attributes: attrs,
        start_time_unix_nano: ts_nanos() - 1_000_000_000,
        time_unix_nano: ts_nanos(),
        exemplars: vec![],
        flags: 0,
        value: Some(number_data_point::Value::AsInt(value)),
    }
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

#[test]
fn logs_session_start_minimal() {
    let req = wrap_logs(
        vec![
            kv_string("session.id", "sess-foo"),
            kv_string("service.name", "claude-code"),
        ],
        vec![log_record(
            "session.start",
            vec![kv_string("model", "claude-sonnet-4.6")],
        )],
    );

    let out = map_logs(&req);
    assert_eq!(out.len(), 1);
    match out.into_iter().next().unwrap() {
        Ok(TypedEvent::SessionStart {
            session_id,
            agent,
            model,
            ..
        }) => {
            assert_eq!(session_id, "sess-foo");
            assert_eq!(agent, AgentKind::Claude);
            assert_eq!(model.as_deref(), Some("claude-sonnet-4.6"));
        }
        other => panic!("expected SessionStart, got {other:?}"),
    }
}

#[test]
fn logs_missing_session_id_returns_error() {
    let req = wrap_logs(vec![], vec![log_record("session.start", vec![])]);
    let out = map_logs(&req);
    assert_eq!(out.len(), 1);
    match out.into_iter().next().unwrap() {
        Err(MapperError::MissingResourceAttr { name }) => assert_eq!(name, "session.id"),
        other => panic!("expected MissingResourceAttr, got {other:?}"),
    }
}

#[test]
fn logs_fallback_to_claude_session_id() {
    let req = wrap_logs(
        vec![kv_string("claude.session_id", "sess-bar")],
        vec![log_record("session.start", vec![])],
    );
    let out = map_logs(&req);
    let ev = out.into_iter().next().unwrap().expect("event");
    assert_eq!(ev.session_id(), Some("sess-bar"));
}

#[test]
fn logs_record_level_session_id() {
    let req = wrap_logs(
        vec![],
        vec![log_record(
            "session.end",
            vec![kv_string("session.id", "sess-baz")],
        )],
    );
    let out = map_logs(&req);
    let ev = out.into_iter().next().unwrap().expect("event");
    assert!(matches!(ev, TypedEvent::SessionEnd { .. }));
    assert_eq!(ev.session_id(), Some("sess-baz"));
}

#[test]
fn logs_user_prompt() {
    let req = wrap_logs(
        vec![kv_string("session.id", "s1")],
        vec![log_record(
            "user.prompt",
            vec![
                kv_string("turn.id", "t-42"),
                kv_int("prompt.size_bytes", 128),
            ],
        )],
    );
    let out = map_logs(&req);
    let ev = out.into_iter().next().unwrap().expect("event");
    match ev {
        TypedEvent::UserPrompt {
            session_id,
            turn_id,
            prompt_size_bytes,
            ..
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(turn_id, "t-42");
            assert_eq!(prompt_size_bytes, Some(128));
        }
        other => panic!("expected UserPrompt, got {other:?}"),
    }
}

#[test]
fn logs_unrecognized_event_name() {
    let req = wrap_logs(
        vec![kv_string("session.id", "s1")],
        vec![log_record("future.unknown", vec![])],
    );
    let out = map_logs(&req);
    let ev = out.into_iter().next().unwrap().expect("event");
    match ev {
        TypedEvent::Unrecognized { signal, identity } => {
            assert_eq!(signal, SignalKind::Log);
            assert!(identity.contains("future.unknown"));
        }
        other => panic!("expected Unrecognized, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[test]
fn metrics_token_usage_input() {
    let req = wrap_metrics(
        vec![kv_string("session.id", "s1")],
        vec![sum_metric_with_points(
            "gen_ai.client.token.usage",
            vec![int_point(
                1234,
                vec![
                    kv_string("gen_ai.token.type", "input"),
                    kv_string("gen_ai.response.model", "claude-sonnet-4.6"),
                ],
            )],
        )],
    );

    let out = map_metrics(&req);
    assert_eq!(out.len(), 1);
    match out.into_iter().next().unwrap().expect("event") {
        TypedEvent::TokenUsage {
            session_id,
            model,
            direction,
            value,
            ..
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(model, "claude-sonnet-4.6");
            assert_eq!(direction, TokenDirection::Input);
            assert_eq!(value, 1234);
        }
        other => panic!("expected TokenUsage, got {other:?}"),
    }
}

#[test]
fn metrics_token_usage_output_and_cache() {
    let req = wrap_metrics(
        vec![
            kv_string("session.id", "s1"),
            kv_string("service.name", "claude-code"),
        ],
        vec![sum_metric_with_points(
            "agent.token.usage",
            vec![
                int_point(
                    100,
                    vec![
                        kv_string("direction", "output"),
                        kv_string("model", "claude-sonnet-4.6"),
                    ],
                ),
                int_point(
                    50,
                    vec![
                        kv_string("direction", "cache_read"),
                        kv_string("model", "claude-sonnet-4.6"),
                    ],
                ),
            ],
        )],
    );

    let out = map_metrics(&req);
    let events: Vec<TypedEvent> = out.into_iter().map(|r| r.expect("event")).collect();
    assert_eq!(events.len(), 2);
    let directions: Vec<TokenDirection> = events
        .iter()
        .map(|ev| match ev {
            TypedEvent::TokenUsage { direction, .. } => *direction,
            other => panic!("expected TokenUsage, got {other:?}"),
        })
        .collect();
    assert!(directions.contains(&TokenDirection::Output));
    assert!(directions.contains(&TokenDirection::CacheRead));
}

// ---------------------------------------------------------------------------
// Traces
// ---------------------------------------------------------------------------

#[test]
fn traces_tool_span_yields_start_and_end() {
    let span = Span {
        trace_id: vec![1u8; 16],
        span_id: vec![2u8; 8],
        trace_state: String::new(),
        parent_span_id: vec![],
        flags: 0,
        name: "tool.execute".into(),
        kind: 0,
        start_time_unix_nano: ts_nanos(),
        end_time_unix_nano: ts_nanos() + 500_000_000,
        attributes: vec![
            kv_string("gen_ai.operation.name", "tool.execute"),
            kv_string("tool.name", "bash"),
            kv_bool("user.approved", true),
        ],
        dropped_attributes_count: 0,
        events: vec![],
        dropped_events_count: 0,
        links: vec![],
        dropped_links_count: 0,
        status: Some(Status {
            message: String::new(),
            code: StatusCode::Ok as i32,
        }),
    };
    let req = wrap_traces(vec![kv_string("session.id", "s1")], vec![span]);

    let out = map_traces(&req);
    let events: Vec<TypedEvent> = out.into_iter().map(|r| r.expect("event")).collect();
    assert_eq!(events.len(), 2, "tool span must yield Start + Result");
    match &events[0] {
        TypedEvent::ToolDecisionStart {
            tool_name,
            user_approved,
            ..
        } => {
            assert_eq!(tool_name, "bash");
            assert!(*user_approved);
        }
        other => panic!("expected ToolDecisionStart first, got {other:?}"),
    }
    match &events[1] {
        TypedEvent::ToolResult {
            tool_name, status, ..
        } => {
            assert_eq!(tool_name, "bash");
            assert!(matches!(status, ToolCallStatus::Success));
        }
        other => panic!("expected ToolResult second, got {other:?}"),
    }
}

#[test]
fn traces_unknown_span_is_unrecognized() {
    let span = Span {
        trace_id: vec![1u8; 16],
        span_id: vec![2u8; 8],
        trace_state: String::new(),
        parent_span_id: vec![],
        flags: 0,
        name: "http.request".into(),
        kind: 0,
        start_time_unix_nano: ts_nanos(),
        end_time_unix_nano: ts_nanos(),
        attributes: vec![],
        dropped_attributes_count: 0,
        events: vec![],
        dropped_events_count: 0,
        links: vec![],
        dropped_links_count: 0,
        status: None,
    };
    let req = wrap_traces(vec![kv_string("session.id", "s1")], vec![span]);

    let out = map_traces(&req);
    assert_eq!(out.len(), 1);
    match out.into_iter().next().unwrap().expect("event") {
        TypedEvent::Unrecognized { signal, .. } => assert_eq!(signal, SignalKind::Trace),
        other => panic!("expected Unrecognized, got {other:?}"),
    }
}
