//! OTLP wire types → [`TypedEvent`] mapper (M2.2 T5.2).
//!
//! Three entry points — [`map_logs`], [`map_metrics`], [`map_traces`] —
//! walk an `Export*ServiceRequest` envelope and return one
//! `Result<TypedEvent, MapperError>` **per record**. Returning a `Vec<Result<...>>`
//! (rather than `Result<Vec<...>>`) is load-bearing: spec §5.2 and §5.5
//! require that a single malformed record never drop the whole batch.
//!
//! Session id is derived per spec §5.3 fallback chain:
//!
//! 1. `resource.attributes["session.id"]`
//! 2. `resource.attributes["claude.session_id"]`
//! 3. record/data-point-level `session.id`
//! 4. → [`MapperError::MissingResourceAttr`]
//!
//! Agent kind is derived from `service.name` (`"claude-code"` →
//! [`AgentKind::Claude`], `"codex-cli"` → [`AgentKind::Codex`], else
//! [`AgentKind::Copilot`] as the agentprof default).
//!
//! Event-name dispatch accepts both the bare suffix (`session.start`) and
//! Claude Code's `claude_code.session.start` form — we strip a leading
//! `claude_code.` prefix when present. Unrecognized names fall through to
//! [`TypedEvent::Unrecognized`] instead of an error so the router can log
//! once and drop without dropping the surrounding session (plan §T5.2
//! deviation noted in commit message).
//!
//! The module is pure-sync: no `.await`, no I/O, no allocation outside
//! the natural cost of cloning attribute strings.

use agentprof_core::adapter::AgentKind;
use agentprof_core::episode::tool::ToolCallStatus;
use agentprof_core::model::tool_source::ToolSource;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

use crate::otlp::error::MapperError;
use crate::otlp::proto::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest;
use crate::otlp::proto::opentelemetry::proto::collector::metrics::v1::ExportMetricsServiceRequest;
use crate::otlp::proto::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
use crate::otlp::proto::opentelemetry::proto::common::v1::{any_value, AnyValue, KeyValue};
use crate::otlp::proto::opentelemetry::proto::logs::v1::LogRecord;
use crate::otlp::proto::opentelemetry::proto::metrics::v1::{
    metric, number_data_point, Metric, NumberDataPoint,
};
use crate::otlp::proto::opentelemetry::proto::resource::v1::Resource;
use crate::otlp::proto::opentelemetry::proto::trace::v1::{status::StatusCode, Span};
use crate::otlp::typed::{SignalKind, TokenDirection, TypedEvent};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Map an OTLP Logs export request into a list of per-record [`TypedEvent`]
/// results.
///
/// A single bad record yields a single `Err` in the returned vector; the
/// surrounding records are still produced. Empty / missing resources are
/// silently skipped.
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::otlp::mapper::map_logs;
/// use agentprof_storage::otlp::proto::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest;
///
/// let req = ExportLogsServiceRequest { resource_logs: vec![] };
/// let events = map_logs(&req);
/// assert!(events.is_empty());
/// ```
#[must_use]
pub fn map_logs(req: &ExportLogsServiceRequest) -> Vec<Result<TypedEvent, MapperError>> {
    let mut out = Vec::new();
    for rl in &req.resource_logs {
        let resource_attrs = resource_attrs(rl.resource.as_ref());
        let agent = resolve_agent_kind(resource_attrs);
        for sl in &rl.scope_logs {
            for record in &sl.log_records {
                out.push(map_log_record(record, resource_attrs, agent));
            }
        }
    }
    out
}

/// Map an OTLP Metrics export request into a list of per-data-point
/// [`TypedEvent`] results.
///
/// Each `gen_ai.client.token.usage` / `agent.token.usage` data point
/// expands to one [`TypedEvent::TokenUsage`]. Unrecognized metric names
/// fall through to [`TypedEvent::Unrecognized`].
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::otlp::mapper::map_metrics;
/// use agentprof_storage::otlp::proto::opentelemetry::proto::collector::metrics::v1::ExportMetricsServiceRequest;
///
/// let req = ExportMetricsServiceRequest { resource_metrics: vec![] };
/// assert!(map_metrics(&req).is_empty());
/// ```
#[must_use]
pub fn map_metrics(req: &ExportMetricsServiceRequest) -> Vec<Result<TypedEvent, MapperError>> {
    let mut out = Vec::new();
    for rm in &req.resource_metrics {
        let resource_attrs = resource_attrs(rm.resource.as_ref());
        for sm in &rm.scope_metrics {
            for metric in &sm.metrics {
                map_metric(metric, resource_attrs, &mut out);
            }
        }
    }
    out
}

/// Map an OTLP Traces export request into a list of per-span [`TypedEvent`]
/// results.
///
/// A `gen_ai.operation.name = "tool.execute"` span produces TWO events
/// (a [`TypedEvent::ToolDecisionStart`] from `start_time_unix_nano` and a
/// [`TypedEvent::ToolResult`] from `end_time_unix_nano`). Spans carrying
/// `session.event = "session.start" | "session.end"` produce the
/// corresponding session-lifecycle event. Everything else falls through
/// to [`TypedEvent::Unrecognized`].
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::otlp::mapper::map_traces;
/// use agentprof_storage::otlp::proto::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
///
/// let req = ExportTraceServiceRequest { resource_spans: vec![] };
/// assert!(map_traces(&req).is_empty());
/// ```
#[must_use]
pub fn map_traces(req: &ExportTraceServiceRequest) -> Vec<Result<TypedEvent, MapperError>> {
    let mut out = Vec::new();
    for rs in &req.resource_spans {
        let resource_attrs = resource_attrs(rs.resource.as_ref());
        let agent = resolve_agent_kind(resource_attrs);
        for ss in &rs.scope_spans {
            for span in &ss.spans {
                map_span(span, resource_attrs, agent, &mut out);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

fn map_log_record(
    record: &LogRecord,
    resource_attrs: &[KeyValue],
    agent: AgentKind,
) -> Result<TypedEvent, MapperError> {
    let event_name = find_attr(&record.attributes, "event.name")
        .and_then(attr_as_str)
        .map_or("", strip_prefix);
    let timestamp = parse_unix_nano(record.time_unix_nano.max(record.observed_time_unix_nano))?;

    // Catch-all path: routing-only events with no session.id (e.g. unrecognized)
    // still need a session.id to be useful; we surface MissingResourceAttr only
    // for events we WOULD have mapped.
    match event_name {
        "session.start" => {
            let session_id =
                extract_session_id(SignalKind::Log, resource_attrs, &record.attributes)?;
            Ok(TypedEvent::SessionStart {
                session_id,
                agent,
                started_at: timestamp,
                model: find_attr(&record.attributes, "model")
                    .and_then(attr_as_str)
                    .map(str::to_owned),
                cwd: find_attr(&record.attributes, "cwd")
                    .and_then(attr_as_str)
                    .map(PathBuf::from),
            })
        }
        "session.end" => {
            let session_id =
                extract_session_id(SignalKind::Log, resource_attrs, &record.attributes)?;
            Ok(TypedEvent::SessionEnd {
                session_id,
                ended_at: timestamp,
            })
        }
        "user.prompt" | "user_prompt" => {
            let session_id =
                extract_session_id(SignalKind::Log, resource_attrs, &record.attributes)?;
            let turn_id = find_attr(&record.attributes, "turn.id")
                .or_else(|| find_attr(&record.attributes, "turn_id"))
                .and_then(attr_as_str)
                .unwrap_or_default()
                .to_owned();
            Ok(TypedEvent::UserPrompt {
                session_id,
                turn_id,
                timestamp,
                prompt_size_bytes: find_attr(&record.attributes, "prompt.size_bytes")
                    .or_else(|| find_attr(&record.attributes, "prompt_size_bytes"))
                    .and_then(attr_as_u64),
            })
        }
        "tool.decision_start" | "tool_decision" | "tool.decision" => {
            let session_id =
                extract_session_id(SignalKind::Log, resource_attrs, &record.attributes)?;
            Ok(TypedEvent::ToolDecisionStart {
                session_id,
                turn_id: find_attr(&record.attributes, "turn.id")
                    .and_then(attr_as_str)
                    .map(str::to_owned),
                tool_name: find_attr(&record.attributes, "tool.name")
                    .and_then(attr_as_str)
                    .unwrap_or("")
                    .to_owned(),
                source: ToolSource::Builtin,
                timestamp,
                user_approved: find_attr(&record.attributes, "user.approved")
                    .and_then(attr_as_bool)
                    .unwrap_or(false),
            })
        }
        "tool.result" | "tool_result" => {
            let session_id =
                extract_session_id(SignalKind::Log, resource_attrs, &record.attributes)?;
            let success = find_attr(&record.attributes, "success")
                .and_then(attr_as_bool)
                .unwrap_or(true);
            let status = if success {
                ToolCallStatus::Success
            } else {
                ToolCallStatus::Failure {
                    message: find_attr(&record.attributes, "error.message")
                        .and_then(attr_as_str)
                        .map(str::to_owned),
                }
            };
            Ok(TypedEvent::ToolResult {
                session_id,
                turn_id: find_attr(&record.attributes, "turn.id")
                    .and_then(attr_as_str)
                    .map(str::to_owned),
                tool_name: find_attr(&record.attributes, "tool.name")
                    .and_then(attr_as_str)
                    .unwrap_or("")
                    .to_owned(),
                timestamp,
                status,
            })
        }
        other => Ok(TypedEvent::Unrecognized {
            signal: SignalKind::Log,
            identity: format!("log.event_name={other}"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

fn map_metric(
    metric: &Metric,
    resource_attrs: &[KeyValue],
    out: &mut Vec<Result<TypedEvent, MapperError>>,
) {
    let is_token_usage = matches!(
        metric.name.as_str(),
        "gen_ai.client.token.usage" | "agent.token.usage" | "claude_code.token.usage"
    );
    if !is_token_usage {
        out.push(Ok(TypedEvent::Unrecognized {
            signal: SignalKind::Metric,
            identity: format!("metric.name={}", metric.name),
        }));
        return;
    }

    let points: &[NumberDataPoint] = match &metric.data {
        Some(metric::Data::Sum(s)) => &s.data_points,
        Some(metric::Data::Gauge(g)) => &g.data_points,
        _ => {
            out.push(Ok(TypedEvent::Unrecognized {
                signal: SignalKind::Metric,
                identity: format!("metric.name={};unsupported_data_kind", metric.name),
            }));
            return;
        }
    };

    for point in points {
        out.push(map_token_point(point, resource_attrs));
    }
}

fn map_token_point(
    point: &NumberDataPoint,
    resource_attrs: &[KeyValue],
) -> Result<TypedEvent, MapperError> {
    let session_id = extract_session_id(SignalKind::Metric, resource_attrs, &point.attributes)?;
    let timestamp = parse_unix_nano(point.time_unix_nano)?;
    let direction = find_attr(&point.attributes, "gen_ai.token.type")
        .or_else(|| find_attr(&point.attributes, "direction"))
        .and_then(attr_as_str)
        .map(parse_token_direction)
        .transpose()?
        .ok_or(MapperError::MissingResourceAttr {
            name: "gen_ai.token.type",
        })?;
    let model = find_attr(&point.attributes, "gen_ai.response.model")
        .or_else(|| find_attr(&point.attributes, "model"))
        .or_else(|| find_attr(resource_attrs, "gen_ai.response.model"))
        .and_then(attr_as_str)
        .unwrap_or("")
        .to_owned();
    let value = match &point.value {
        Some(number_data_point::Value::AsInt(v)) => {
            u64::try_from(*v).map_err(|_| MapperError::PayloadMismatch {
                event_name: "token.usage".into(),
                message: format!("negative token count: {v}"),
            })?
        }
        Some(number_data_point::Value::AsDouble(v)) => {
            if !v.is_finite() || *v < 0.0 {
                return Err(MapperError::PayloadMismatch {
                    event_name: "token.usage".into(),
                    message: format!("invalid token count: {v}"),
                });
            }
            // Round to nearest token; OTLP gauges can carry float deltas.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let rounded = v.round() as u64;
            rounded
        }
        None => {
            return Err(MapperError::PayloadMismatch {
                event_name: "token.usage".into(),
                message: "data point missing value".into(),
            });
        }
    };

    Ok(TypedEvent::TokenUsage {
        session_id,
        model,
        direction,
        value,
        timestamp,
    })
}

fn parse_token_direction(s: &str) -> Result<TokenDirection, MapperError> {
    match s {
        "input" | "prompt" => Ok(TokenDirection::Input),
        "output" | "completion" => Ok(TokenDirection::Output),
        "cache_read" | "cache.read" => Ok(TokenDirection::CacheRead),
        "cache_create" | "cache_creation" | "cache.create" => Ok(TokenDirection::CacheCreation),
        other => Err(MapperError::PayloadMismatch {
            event_name: "token.usage".into(),
            message: format!("unknown token direction: {other}"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Traces
// ---------------------------------------------------------------------------

fn map_span(
    span: &Span,
    resource_attrs: &[KeyValue],
    agent: AgentKind,
    out: &mut Vec<Result<TypedEvent, MapperError>>,
) {
    let op_name = find_attr(&span.attributes, "gen_ai.operation.name")
        .and_then(attr_as_str)
        .unwrap_or("");
    let session_event = find_attr(&span.attributes, "session.event")
        .and_then(attr_as_str)
        .unwrap_or("");

    // session.event in span attrs overrides everything else
    match session_event {
        "session.start" | "start" => {
            out.push(make_session_start_from_span(span, resource_attrs, agent));
            return;
        }
        "session.end" | "end" => {
            out.push(make_session_end_from_span(span, resource_attrs));
            return;
        }
        _ => {}
    }

    if op_name == "tool.execute" || span.name == "tool.execute" {
        out.extend(make_tool_pair_from_span(span, resource_attrs));
        return;
    }

    out.push(Ok(TypedEvent::Unrecognized {
        signal: SignalKind::Trace,
        identity: format!("span.name={}", span.name),
    }));
}

fn make_session_start_from_span(
    span: &Span,
    resource_attrs: &[KeyValue],
    agent: AgentKind,
) -> Result<TypedEvent, MapperError> {
    let session_id = extract_session_id(SignalKind::Trace, resource_attrs, &span.attributes)?;
    let started_at = parse_unix_nano(span.start_time_unix_nano)?;
    Ok(TypedEvent::SessionStart {
        session_id,
        agent,
        started_at,
        model: find_attr(&span.attributes, "model")
            .and_then(attr_as_str)
            .map(str::to_owned),
        cwd: find_attr(&span.attributes, "cwd")
            .and_then(attr_as_str)
            .map(PathBuf::from),
    })
}

fn make_session_end_from_span(
    span: &Span,
    resource_attrs: &[KeyValue],
) -> Result<TypedEvent, MapperError> {
    let session_id = extract_session_id(SignalKind::Trace, resource_attrs, &span.attributes)?;
    let ended_at = parse_unix_nano(span.end_time_unix_nano.max(span.start_time_unix_nano))?;
    Ok(TypedEvent::SessionEnd {
        session_id,
        ended_at,
    })
}

fn make_tool_pair_from_span(
    span: &Span,
    resource_attrs: &[KeyValue],
) -> [Result<TypedEvent, MapperError>; 2] {
    let session_id = match extract_session_id(SignalKind::Trace, resource_attrs, &span.attributes) {
        Ok(id) => id,
        Err(e) => return [Err(e.clone()), Err(e)],
    };
    let start_at = match parse_unix_nano(span.start_time_unix_nano) {
        Ok(t) => t,
        Err(e) => return [Err(e.clone()), Err(e)],
    };
    let end_at = match parse_unix_nano(span.end_time_unix_nano.max(span.start_time_unix_nano)) {
        Ok(t) => t,
        Err(e) => return [Err(e.clone()), Err(e)],
    };
    let tool_name = find_attr(&span.attributes, "tool.name")
        .or_else(|| find_attr(&span.attributes, "gen_ai.tool.name"))
        .and_then(attr_as_str)
        .unwrap_or("")
        .to_owned();
    let turn_id = find_attr(&span.attributes, "turn.id")
        .and_then(attr_as_str)
        .map(str::to_owned);
    let user_approved = find_attr(&span.attributes, "user.approved")
        .and_then(attr_as_bool)
        .unwrap_or(false);

    let status = match span.status.as_ref() {
        Some(s) if s.code == StatusCode::Error as i32 => ToolCallStatus::Failure {
            message: if s.message.is_empty() {
                None
            } else {
                Some(s.message.clone())
            },
        },
        _ => ToolCallStatus::Success,
    };

    [
        Ok(TypedEvent::ToolDecisionStart {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            tool_name: tool_name.clone(),
            source: ToolSource::Builtin,
            timestamp: start_at,
            user_approved,
        }),
        Ok(TypedEvent::ToolResult {
            session_id,
            turn_id,
            tool_name,
            timestamp: end_at,
            status,
        }),
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resource_attrs(resource: Option<&Resource>) -> &[KeyValue] {
    resource.map_or(&[][..], |r| r.attributes.as_slice())
}

fn extract_session_id(
    signal: SignalKind,
    resource_attrs: &[KeyValue],
    record_attrs: &[KeyValue],
) -> Result<String, MapperError> {
    for (scope, key) in [
        (resource_attrs, "session.id"),
        (resource_attrs, "claude.session_id"),
        (record_attrs, "session.id"),
    ] {
        if let Some(s) = find_attr(scope, key).and_then(attr_as_str) {
            if s.is_empty() {
                continue;
            }
            // ADR-0022 D-5: cap session_id at 256 bytes BEFORE allocating
            // any router buffer keyed on it. Pathologically long ids would
            // amplify F3 (unbounded-session) attacks.
            if s.len() > 256 {
                return Err(MapperError::SessionIdTooLong {
                    signal,
                    len: s.len(),
                });
            }
            return Ok(s.to_owned());
        }
    }
    Err(MapperError::MissingResourceAttr { name: "session.id" })
}

fn resolve_agent_kind(resource_attrs: &[KeyValue]) -> AgentKind {
    let svc = find_attr(resource_attrs, "service.name").and_then(attr_as_str);
    let kind = find_attr(resource_attrs, "agent.kind").and_then(attr_as_str);
    match (svc, kind) {
        (Some("claude-code"), _) | (_, Some("claude")) => AgentKind::Claude,
        (Some("codex-cli"), _) | (_, Some("codex")) => AgentKind::Codex,
        _ => AgentKind::Copilot,
    }
}

fn parse_unix_nano(ns: u64) -> Result<DateTime<Utc>, MapperError> {
    let signed = i64::try_from(ns)
        .map_err(|_| MapperError::BadTimestamp(format!("nanos overflow i64: {ns}")))?;
    Ok(DateTime::<Utc>::from_timestamp_nanos(signed))
}

fn find_attr<'a>(attrs: &'a [KeyValue], key: &str) -> Option<&'a AnyValue> {
    attrs
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| kv.value.as_ref())
}

fn attr_as_str(v: &AnyValue) -> Option<&str> {
    match v.value.as_ref()? {
        any_value::Value::StringValue(s) => Some(s.as_str()),
        _ => None,
    }
}

fn attr_as_u64(v: &AnyValue) -> Option<u64> {
    match v.value.as_ref()? {
        any_value::Value::IntValue(i) => u64::try_from(*i).ok(),
        any_value::Value::DoubleValue(d) if d.is_finite() && *d >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let rounded = d.round() as u64;
            Some(rounded)
        }
        _ => None,
    }
}

fn attr_as_bool(v: &AnyValue) -> Option<bool> {
    match v.value.as_ref()? {
        any_value::Value::BoolValue(b) => Some(*b),
        _ => None,
    }
}

fn strip_prefix(s: &str) -> &str {
    s.strip_prefix("claude_code.").unwrap_or(s)
}
