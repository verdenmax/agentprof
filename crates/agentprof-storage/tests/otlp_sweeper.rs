//! M2.2 T6.2 — async idle sweeper lifecycle.
//!
//! Drives [`agentprof_storage::otlp::sweeper::spawn_idle_sweeper`] under
//! the four lifecycle conditions called out in the plan:
//!
//! 1. periodic tick → [`SessionRouter::sweep_idle`] flushes stale buffers
//! 2. explicit `SweeperHandle::shutdown` → drains remaining buffers with
//!    `CloseReason::Shutdown`
//! 3. handle dropped without await → background task still drains via the
//!    oneshot sender-drop signal
//! 4. cancel signal interrupts a long sleep (`start_paused = true`, no
//!    virtual time advanced) → shutdown completes near-instantly
//!
//! Sink and event helpers are kept tiny on purpose; the surface under test
//! is sweeper lifecycle, not router/mapper semantics (covered by
//! `otlp_router.rs`).

#![cfg(feature = "otlp")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentprof_core::adapter::AgentKind;
use agentprof_storage::otlp::router::{
    CloseReason, FlushResult, FlushSink, SessionBufferCaps, SessionId, SessionRouter,
};
use agentprof_storage::otlp::sweeper::spawn_idle_sweeper;
use agentprof_storage::otlp::typed::TypedEvent;
use chrono::{TimeZone, Utc};

/// Minimal sink that records `(session_id, close_reason)` for every flush.
/// Avoids cloning `PersistableSession` (which is `#[non_exhaustive]` and
/// intentionally not `Clone`).
#[derive(Default, Clone)]
struct CollectSink {
    inner: Arc<Mutex<Vec<(SessionId, CloseReason)>>>,
}

impl FlushSink for CollectSink {
    fn flush(
        &self,
        _session_id: &SessionId,
        p: agentprof_storage::otlp::router::PersistableSession,
    ) -> FlushResult {
        self.inner
            .lock()
            .expect("collect-sink mutex poisoned")
            .push((p.session_id.clone(), p.close_reason));
        Ok(())
    }
}

impl CollectSink {
    fn snapshot(&self) -> Vec<(SessionId, CloseReason)> {
        self.inner
            .lock()
            .expect("collect-sink mutex poisoned")
            .clone()
    }
    fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("collect-sink mutex poisoned")
            .len()
    }
}

fn session_start(id: &str) -> TypedEvent {
    TypedEvent::SessionStart {
        session_id: id.to_string(),
        agent: AgentKind::Claude,
        started_at: Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap(),
        model: Some("claude-sonnet-4.6".into()),
        cwd: Some(PathBuf::from("/tmp/proj")),
    }
}

fn router_with(caps: SessionBufferCaps, sink: CollectSink) -> Arc<SessionRouter> {
    Arc::new(SessionRouter::new(caps, Arc::new(sink)))
}

/// Periodic ticks must close any buffer whose `last_seen` exceeds the
/// configured `idle_timeout`.
///
/// Note: [`SessionRouter`] uses [`std::time::Instant`] (wallclock) for
/// `last_seen` accounting, not tokio's virtual clock, so this test
/// runs against the real runtime with small but realistic durations
/// rather than the paused-time trick used in
/// [`sweeper_can_be_cancelled_mid_sleep`].
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sweeper_runs_periodic_sweep() {
    let sink = CollectSink::default();
    let caps = SessionBufferCaps::default().with_idle_timeout(Duration::from_millis(20));
    let router = router_with(caps, sink.clone());

    router.ingest(vec![Ok(session_start("sess-idle-1"))]);
    assert_eq!(router.open_buffers(), 1);

    let handle = spawn_idle_sweeper(Arc::clone(&router), Duration::from_millis(10));

    // Wait for at least one tick to fire after `last_seen + idle_timeout`.
    // Poll the sink with a tight loop bounded by an overall timeout so a
    // CI box stall doesn't flake — we exit as soon as the flush is seen.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while sink.len() == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let flushed = sink.snapshot();
    assert!(
        flushed.iter().any(|(_, r)| *r == CloseReason::Idle),
        "sweeper should have flushed the idle buffer; got: {flushed:?}",
    );

    handle.shutdown().await.expect("graceful shutdown");
}

/// `shutdown()` must drain every still-open buffer with
/// `CloseReason::Shutdown`, even when no periodic tick has had time to
/// fire (1 hour interval). This is the SIGINT / SIGTERM contract.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sweeper_shutdown_flushes_remaining() {
    let sink = CollectSink::default();
    // Idle timeout deliberately huge so sweep_idle is a no-op even if it
    // happens to run once. Only the shutdown path should flush.
    let caps = SessionBufferCaps::default().with_idle_timeout(Duration::from_secs(3600));
    let router = router_with(caps, sink.clone());

    for sid in ["sess-a", "sess-b", "sess-c"] {
        router.ingest(vec![Ok(session_start(sid))]);
    }
    assert_eq!(router.open_buffers(), 3);

    let handle = spawn_idle_sweeper(Arc::clone(&router), Duration::from_secs(3600));
    handle.shutdown().await.expect("graceful shutdown");

    let flushed = sink.snapshot();
    assert_eq!(
        flushed.len(),
        3,
        "all 3 buffers must be flushed at shutdown"
    );
    assert!(
        flushed.iter().all(|(_, r)| *r == CloseReason::Shutdown),
        "shutdown path must use CloseReason::Shutdown; got: {flushed:?}",
    );
    assert_eq!(router.open_buffers(), 0);
}

/// Dropping the handle without calling `shutdown()` must still tear the
/// background task down cleanly: the oneshot Sender's Drop signals the
/// receiver, the task flushes with `CloseReason::Shutdown`, and the test
/// runtime cleans up without panic or leak.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn sweeper_shutdown_is_idempotent_via_drop() {
    let sink = CollectSink::default();
    let caps = SessionBufferCaps::default().with_idle_timeout(Duration::from_secs(3600));
    let router = router_with(caps, sink.clone());

    router.ingest(vec![Ok(session_start("sess-drop-1"))]);

    {
        let _handle = spawn_idle_sweeper(Arc::clone(&router), Duration::from_secs(3600));
        // Drop at end of scope without await.
    }

    // Give the detached task a chance to observe the dropped sender and
    // run its flush path. 200 ms is generous for what is essentially a
    // single oneshot::Receiver poll + a flush_all().
    for _ in 0..40 {
        if sink.len() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let flushed = sink.snapshot();
    assert_eq!(flushed.len(), 1, "dropped handle must still drain buffer");
    assert_eq!(flushed[0].1, CloseReason::Shutdown);
}

/// `shutdown()` must interrupt a long inter-tick sleep immediately rather
/// than waiting for the next periodic deadline. With time paused and the
/// interval set to 10 s, no virtual time is advanced — if the cancel
/// signal were ignored the test would hang and trip its timeout.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn sweeper_can_be_cancelled_mid_sleep() {
    let sink = CollectSink::default();
    let caps = SessionBufferCaps::default().with_idle_timeout(Duration::from_secs(60));
    let router = router_with(caps, sink);

    let handle = spawn_idle_sweeper(Arc::clone(&router), Duration::from_secs(10));

    // If the task only polled `ticker.tick()` and not the cancel channel,
    // this would hang until the test harness killed it.
    tokio::time::timeout(Duration::from_secs(1), handle.shutdown())
        .await
        .expect("shutdown must return without waiting for the next 10s tick")
        .expect("graceful shutdown");
}
