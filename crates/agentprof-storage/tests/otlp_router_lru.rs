//! Router LRU eviction under capacity pressure (F3b / ADR-0022 D-1).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agentprof_core::adapter::AgentKind;
use chrono::{DateTime, Utc};

use agentprof_storage::otlp::router::{
    CloseReason, FlushResult, FlushSink, PersistableSession, SessionBufferCaps, SessionId,
    SessionRouter,
};
use agentprof_storage::otlp::typed::TypedEvent;

#[derive(Default)]
struct Collector(Arc<Mutex<Vec<(SessionId, CloseReason)>>>);

impl FlushSink for Collector {
    fn flush(&self, sid: &SessionId, p: PersistableSession) -> FlushResult {
        self.0
            .lock()
            .expect("poisoned")
            .push((sid.clone(), p.close_reason));
        Ok(())
    }
}

fn iso(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("rfc3339")
        .with_timezone(&Utc)
}

fn ss(sid: &str, t: &str) -> TypedEvent {
    TypedEvent::SessionStart {
        session_id: sid.into(),
        agent: AgentKind::Claude,
        started_at: iso(t),
        model: None,
        cwd: Some(PathBuf::from("/")),
    }
}

#[test]
fn router_evicts_under_capacity_pressure() {
    let caps = SessionBufferCaps::default().with_max_open_sessions(4);
    let sink = Arc::new(Collector::default());
    let collector = Arc::clone(&sink.0);
    let router = SessionRouter::new(caps, sink);

    // Push 6 sessions; with cap=4, the first 2 (s1, s2) must be evicted
    // in admission order.
    for (i, sid) in ["s1", "s2", "s3", "s4", "s5", "s6"].iter().enumerate() {
        let t = format!("2026-06-10T10:00:{i:02}Z");
        router.ingest(vec![Ok(ss(sid, &t))]);
    }

    let flushed = collector.lock().expect("poisoned").clone();
    assert_eq!(flushed.len(), 2, "expected 2 evictions, got {flushed:?}");

    // Eviction order matches insertion order — s1 first, s2 second.
    assert_eq!(flushed[0].0, "s1");
    assert_eq!(flushed[0].1, CloseReason::CapacityEvict);
    assert_eq!(flushed[1].0, "s2");
    assert_eq!(flushed[1].1, CloseReason::CapacityEvict);

    // s3..s6 still resident.
    assert_eq!(router.open_buffers(), 4);
}
