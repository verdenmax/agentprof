//! Integration tests for [`agentprof_storage::otlp::router`] (M2.2 T6.1).
//!
//! Covers spec §5.3 flush triggers (explicit end, OOM bytes, OOM events,
//! idle sweep, shutdown) and spec §5.4 tool-call pairing
//! (matched `ToolDecisionStart` + `ToolResult` survive; unpaired starts
//! get synthetic `OpenAtEndOfSession` results).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use agentprof_core::adapter::AgentKind;
use agentprof_core::episode::tool::ToolCallStatus;
use agentprof_core::model::tool_source::ToolSource;
use chrono::{TimeZone, Utc};

use agentprof_storage::otlp::error::MapperError;
use agentprof_storage::otlp::router::{
    CloseReason, FlushResult, FlushSink, PersistableSession, SessionBufferCaps, SessionId,
    SessionRouter,
};
use agentprof_storage::otlp::typed::{SignalKind, TokenDirection, TypedEvent};

// -- mock sink ---------------------------------------------------------------

#[derive(Default, Clone)]
struct Collector {
    inner: Arc<Mutex<Vec<PersistableSession>>>,
}

impl Collector {
    fn new() -> Self {
        Self::default()
    }
    fn take(&self) -> Vec<PersistableSession> {
        let mut g = self.inner.lock().expect("poisoned");
        std::mem::take(&mut *g)
    }
    fn len(&self) -> usize {
        self.inner.lock().expect("poisoned").len()
    }
}

impl FlushSink for Collector {
    fn flush(&self, _: &SessionId, p: PersistableSession) -> FlushResult {
        self.inner.lock().expect("poisoned").push(p);
        Ok(())
    }
}

// -- helpers -----------------------------------------------------------------

fn router(caps: SessionBufferCaps) -> (Arc<SessionRouter>, Collector) {
    let sink = Collector::new();
    let r = SessionRouter::new(caps, Arc::new(sink.clone()));
    (Arc::new(r), sink)
}

fn ts(sec: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + sec, 0)
        .single()
        .expect("valid ts")
}

fn start(session_id: &str, sec: i64) -> TypedEvent {
    TypedEvent::SessionStart {
        session_id: session_id.into(),
        agent: AgentKind::Claude,
        started_at: ts(sec),
        model: Some("claude-sonnet-4.6".into()),
        cwd: Some(PathBuf::from("/tmp/proj")),
    }
}

fn end(session_id: &str, sec: i64) -> TypedEvent {
    TypedEvent::SessionEnd {
        session_id: session_id.into(),
        ended_at: ts(sec),
    }
}

fn prompt(session_id: &str, sec: i64, turn: &str) -> TypedEvent {
    TypedEvent::UserPrompt {
        session_id: session_id.into(),
        turn_id: turn.into(),
        timestamp: ts(sec),
        prompt_size_bytes: Some(42),
    }
}

fn tool_start(session_id: &str, sec: i64, tool: &str, turn: Option<&str>) -> TypedEvent {
    TypedEvent::ToolDecisionStart {
        session_id: session_id.into(),
        turn_id: turn.map(str::to_owned),
        tool_name: tool.into(),
        source: ToolSource::Builtin,
        timestamp: ts(sec),
        user_approved: true,
    }
}

fn tool_result(session_id: &str, sec: i64, tool: &str, turn: Option<&str>) -> TypedEvent {
    TypedEvent::ToolResult {
        session_id: session_id.into(),
        turn_id: turn.map(str::to_owned),
        tool_name: tool.into(),
        timestamp: ts(sec),
        status: ToolCallStatus::Success,
    }
}

#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)]
fn ok(e: TypedEvent) -> Result<TypedEvent, MapperError> {
    Ok(e)
}

// -- tests -------------------------------------------------------------------

#[test]
fn router_ingest_groups_by_session_id() {
    let (r, sink) = router(SessionBufferCaps::default());
    r.ingest(vec![
        ok(start("a", 1)),
        ok(start("a", 2)),
        ok(start("b", 3)),
    ]);
    // Two distinct sessions → two buffers.
    assert_eq!(r.open_buffers(), 2);
    assert_eq!(sink.len(), 0, "no flush expected without close trigger");
}

#[test]
fn router_close_on_explicit_session_end() {
    let (r, sink) = router(SessionBufferCaps::default());
    r.ingest(vec![
        ok(start("s1", 1)),
        ok(prompt("s1", 2, "t1")),
        ok(end("s1", 3)),
    ]);
    assert_eq!(r.open_buffers(), 0, "buffer removed after explicit end");
    let flushed = sink.take();
    assert_eq!(flushed.len(), 1);
    let p = &flushed[0];
    assert_eq!(p.session_id, "s1");
    assert_eq!(p.close_reason, CloseReason::ExplicitEnd);
    assert_eq!(p.started_at, Some(ts(1)));
    assert_eq!(p.ended_at, Some(ts(3)));
    assert_eq!(p.events.len(), 3);
}

#[test]
fn router_oom_bytes_triggers_flush() {
    // Tiny cap so 2 events overflow.
    let caps = SessionBufferCaps::default().with_max_bytes(16);
    let (r, sink) = router(caps);
    let big_session = "x".repeat(200);
    r.ingest(vec![
        ok(start(&big_session, 1)),
        ok(prompt(&big_session, 2, "turn-with-a-big-payload-id")),
    ]);
    let flushed = sink.take();
    assert!(!flushed.is_empty(), "at least one OOM flush expected");
    assert!(flushed
        .iter()
        .all(|p| p.close_reason == CloseReason::OomBytes));
    assert_eq!(r.open_buffers(), 0);
}

#[test]
fn router_oom_events_triggers_flush() {
    let caps = SessionBufferCaps::default().with_max_events(2);
    let (r, sink) = router(caps);
    r.ingest(vec![
        ok(start("s", 1)),
        ok(prompt("s", 2, "t1")),
        ok(prompt("s", 3, "t2")),
    ]);
    let flushed = sink.take();
    assert_eq!(flushed.len(), 1);
    assert_eq!(flushed[0].close_reason, CloseReason::OomEvents);
    assert!(flushed[0].events.len() >= 3);
    assert_eq!(r.open_buffers(), 0);
}

#[test]
fn router_into_persistable_pairs_tool_calls() {
    let (r, sink) = router(SessionBufferCaps::default());
    r.ingest(vec![
        ok(start("s", 1)),
        ok(tool_start("s", 2, "bash", Some("t1"))),
        ok(tool_result("s", 3, "bash", Some("t1"))),
        ok(tool_start("s", 4, "edit", Some("t2"))), // never paired
        ok(end("s", 5)),
    ]);
    let mut flushed = sink.take();
    assert_eq!(flushed.len(), 1);
    let p = flushed.remove(0);

    let mut paired_results = 0;
    let mut orphan_results = 0;
    let mut starts = 0;
    for ev in &p.events {
        match ev {
            TypedEvent::ToolDecisionStart { .. } => starts += 1,
            TypedEvent::ToolResult {
                tool_name, status, ..
            } => {
                if matches!(status, ToolCallStatus::OpenAtEndOfSession) {
                    orphan_results += 1;
                    assert_eq!(tool_name, "edit");
                } else {
                    paired_results += 1;
                    assert_eq!(tool_name, "bash");
                }
            }
            _ => {}
        }
    }
    assert_eq!(starts, 2);
    assert_eq!(paired_results, 1, "matched ToolResult preserved");
    assert_eq!(orphan_results, 1, "unpaired ToolDecisionStart synthesized");

    // Events are sorted by timestamp.
    let timestamps: Vec<_> = p
        .events
        .iter()
        .filter_map(|e| match e {
            TypedEvent::SessionStart { started_at, .. } => Some(*started_at),
            TypedEvent::SessionEnd { ended_at, .. } => Some(*ended_at),
            TypedEvent::ToolDecisionStart { timestamp, .. }
            | TypedEvent::ToolResult { timestamp, .. } => Some(*timestamp),
            _ => None,
        })
        .collect();
    let mut sorted = timestamps.clone();
    sorted.sort();
    assert_eq!(timestamps, sorted);
}

#[test]
fn router_sweep_idle_closes_stale() {
    let caps = SessionBufferCaps::default().with_idle_timeout(Duration::from_millis(10));
    let (r, sink) = router(caps);
    r.ingest(vec![ok(start("idle-s", 1))]);
    assert_eq!(r.open_buffers(), 1);
    // Sweep before timeout → no closes.
    assert!(r.sweep_idle().is_empty());
    thread::sleep(Duration::from_millis(50));
    let closed = r.sweep_idle();
    assert_eq!(closed, vec!["idle-s".to_owned()]);
    assert_eq!(r.open_buffers(), 0);
    let flushed = sink.take();
    assert_eq!(flushed.len(), 1);
    assert_eq!(flushed[0].close_reason, CloseReason::Idle);
}

#[test]
fn router_flush_all_drains_buffers() {
    let (r, sink) = router(SessionBufferCaps::default());
    r.ingest(vec![
        ok(start("a", 1)),
        ok(start("b", 2)),
        ok(start("c", 3)),
        ok(TypedEvent::TokenUsage {
            session_id: "a".into(),
            model: "claude-sonnet-4.6".into(),
            direction: TokenDirection::Input,
            value: 100,
            timestamp: ts(4),
        }),
    ]);
    assert_eq!(r.open_buffers(), 3);
    let results = r.flush_all(CloseReason::Shutdown);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(r.open_buffers(), 0);
    let flushed = sink.take();
    assert_eq!(flushed.len(), 3);
    assert!(flushed
        .iter()
        .all(|p| p.close_reason == CloseReason::Shutdown));
}

#[test]
fn router_dropped_mapper_errors_are_logged_not_flushed() {
    let (r, sink) = router(SessionBufferCaps::default());
    r.ingest(vec![
        Err(MapperError::MissingResourceAttr { name: "session.id" }),
        Ok(TypedEvent::Unrecognized {
            signal: SignalKind::Log,
            identity: "claude_code.future".into(),
        }),
    ]);
    assert_eq!(
        r.open_buffers(),
        0,
        "mapper errors + unrecognized events open no buffers"
    );
    assert_eq!(sink.len(), 0);
}

#[test]
fn router_close_unknown_session_is_noop() {
    let (r, sink) = router(SessionBufferCaps::default());
    let res = r.close_buffer(&"never-existed".to_owned(), CloseReason::Shutdown);
    assert!(res.is_ok());
    assert_eq!(sink.len(), 0);
}
