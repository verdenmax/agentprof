//! Async background task that drives [`SessionRouter`] flush triggers
//! (M2.2 T6.2).
//!
//! [`SessionRouter`] is a synchronous, [`Arc`]-shareable type whose flush
//! triggers (`sweep_idle`, `flush_all`) are themselves synchronous
//! methods. The receiver runtime however needs:
//!
//! 1. **Periodic** invocation of [`SessionRouter::sweep_idle`] — closing
//!    buffers whose `last_seen + idle_timeout < now` is otherwise a no-op
//!    because nothing else calls it.
//! 2. **Graceful shutdown** — on SIGINT / SIGTERM (or the receiver's
//!    own server-stop hook) every still-open buffer must be drained
//!    with [`CloseReason::Shutdown`] *before* the process exits, so the
//!    persistence layer sees them.
//!
//! Both responsibilities belong in a single tokio task so the listener
//! lifecycle only has one handle to await on. This module is that task.
//!
//! # Wire diagram
//!
//! ```text
//!   receiver-main                          spawn_idle_sweeper
//!         │                                       │
//!         │      Arc<SessionRouter>               │
//!         ├──────────────────────────────────────▶│
//!         │                                       │ tokio::spawn
//!         │           SweeperHandle  ◀────────────┤
//!         │                                       │
//!         │                                       │   loop {
//!         │                                       │     select! {
//!         │                                       │       _ = ticker.tick()  → sweep_idle()
//!         │                                       │       _ = cancel_rx      → break
//!         │                                       │     }
//!         │                                       │   }
//!         │                                       │   flush_all(Shutdown)
//!         │  handle.shutdown().await ────────────▶│   (task ends; join resolves)
//!         ▼                                       ▼
//! ```
//!
//! # Cancellation mechanism
//!
//! A [`tokio::sync::oneshot`] channel is used as the cancel signal.
//! Two paths feed it:
//!
//! - Explicit: [`SweeperHandle::shutdown`] takes the `Sender` out and
//!   `send`s on it.
//! - Implicit: dropping the [`SweeperHandle`] drops the `Sender`, which
//!   the task observes as `Receiver` resolving to `Err(_)`. Either way
//!   the task takes the cancel branch of the `select!`, flushes, and
//!   exits.
//!
//! No external crate is pulled in (no `tokio-util::sync::CancellationToken`)
//! because the oneshot is sufficient and `tokio::sync::oneshot` is already
//! used elsewhere in this crate.
//!
//! # Examples
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use std::time::Duration;
//! # use agentprof_storage::otlp::router::{
//! #     FlushSink, FlushResult, PersistableSession, SessionBufferCaps,
//! #     SessionId, SessionRouter,
//! # };
//! # use agentprof_storage::otlp::sweeper::spawn_idle_sweeper;
//! # struct Sink;
//! # impl FlushSink for Sink {
//! #     fn flush(&self, _: &SessionId, _: PersistableSession) -> FlushResult { Ok(()) }
//! # }
//! # async fn run() -> Result<(), agentprof_storage::otlp::OtlpServerError> {
//! let router = Arc::new(SessionRouter::new(
//!     SessionBufferCaps::default(),
//!     Arc::new(Sink),
//! ));
//!
//! // Sweep every 30 s; tweak via OtlpServerConfig in real wiring.
//! let handle = spawn_idle_sweeper(Arc::clone(&router), Duration::from_secs(30));
//!
//! // ... run listeners ...
//!
//! handle.shutdown().await?; // drains every open buffer with Shutdown
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use tokio::select;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};

use super::error::OtlpServerError;
use super::router::{CloseReason, SessionRouter};

/// Owning handle to a running idle-sweeper task.
///
/// Returned by [`spawn_idle_sweeper`]. Two ways to stop the task:
///
/// - [`SweeperHandle::shutdown`] (preferred) — sends the cancel signal,
///   awaits the [`JoinHandle`], and surfaces any task-join failure as
///   [`OtlpServerError::Internal`].
/// - Dropping the handle — the inner oneshot `Sender` drops, the
///   background task observes it via `Receiver::Err`, runs the same
///   flush path, and exits. The detached `JoinHandle` will be reaped by
///   the runtime.
///
/// # Examples
///
/// See the module-level example.
#[derive(Debug)]
pub struct SweeperHandle {
    /// `None` after [`Self::shutdown`] has been called; `Some(_)`
    /// otherwise. Dropping a `Some(_)` sender is what makes the
    /// drop-path cancellation work.
    cancel: Option<oneshot::Sender<()>>,
    join: JoinHandle<()>,
}

impl SweeperHandle {
    /// Cancels the sweeper, awaits its flush-all pass, and returns once
    /// the task has fully exited.
    ///
    /// This is idempotent only in the "explicit-then-drop" sense:
    /// calling `shutdown` consumes `self`, so a second call is a
    /// compile-time error. Dropping a handle that has not been
    /// `shutdown` runs the same cancel path implicitly but does **not**
    /// await the join — the task drains in the background.
    ///
    /// # Errors
    ///
    /// Returns [`OtlpServerError::Internal`] if the sweeper task panicked
    /// or was aborted before completing its flush. Under normal
    /// operation the task only awaits `ticker.tick()` and the cancel
    /// channel (neither panic), so this is exceptional.
    ///
    /// # Examples
    ///
    /// See the module-level example.
    pub async fn shutdown(mut self) -> Result<(), OtlpServerError> {
        if let Some(tx) = self.cancel.take() {
            // `send` only fails when the receiver was dropped, which
            // means the task has already exited (e.g. via a panic). In
            // that case the `await` below will surface the JoinError.
            let _ = tx.send(());
        }
        self.join
            .await
            .map_err(|e| OtlpServerError::Internal(format!("sweeper join failed: {e}")))
    }
}

/// Spawns a background task that calls
/// [`SessionRouter::sweep_idle`] every `interval_dur` and drains the
/// router with [`CloseReason::Shutdown`] on cancellation.
///
/// The returned [`SweeperHandle`] owns the task; see its docs for the
/// two supported teardown patterns.
///
/// `interval_dur` should be **shorter** than the router's
/// [`SessionBufferCaps::idle_timeout`][crate::otlp::router::SessionBufferCaps::idle_timeout]
/// so that idle detection happens promptly; a typical receiver wiring
/// uses 30 s sweeper with 5 min idle timeout (~10 ticks per timeout).
///
/// Tick behaviour is [`MissedTickBehavior::Skip`]: if the sweep itself
/// runs longer than `interval_dur`, the next tick fires once and the
/// missed deadlines are dropped (no thundering herd of catch-up sweeps).
///
/// # Panics
///
/// `interval(Duration::ZERO)` would panic; the caller must pass a
/// non-zero duration. The receiver config layer enforces this when
/// resolving [`crate::otlp::config::OtlpServerConfig`].
///
/// # Examples
///
/// See the module-level example.
#[must_use = "dropping the SweeperHandle stops the sweeper; bind to `_handle` if intentional"]
pub fn spawn_idle_sweeper(router: Arc<SessionRouter>, interval_dur: Duration) -> SweeperHandle {
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let mut ticker = interval(interval_dur);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            select! {
                // `biased` keeps cancellation strictly higher-priority
                // than ticks, so a `shutdown` racing with a tick edge
                // never gets stuck doing one more sweep before exit.
                biased;
                _ = &mut cancel_rx => break,
                _ = ticker.tick() => {
                    // sweep_idle is `#[must_use]` for metric capture;
                    // we don't expose metrics yet (M2.2 T8 wires them),
                    // so discard with a binding name for clarity.
                    let _swept = router.sweep_idle();
                }
            }
        }
        let _results = router.flush_all(CloseReason::Shutdown);
    });
    SweeperHandle {
        cancel: Some(cancel_tx),
        join,
    }
}

#[cfg(test)]
mod tests {
    //! Smoke tests; full lifecycle coverage lives in
    //! `tests/otlp_sweeper.rs` (integration tests that need
    //! `tokio = { features = ["test-util"] }`).
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::otlp::router::{
        FlushResult, FlushSink, PersistableSession, SessionBufferCaps, SessionId,
    };

    struct NoopSink;
    impl FlushSink for NoopSink {
        fn flush(&self, _: &SessionId, _: PersistableSession) -> FlushResult {
            Ok(())
        }
    }

    #[test]
    fn handle_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<SweeperHandle>();
        assert_sync::<SweeperHandle>();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_returns_ok_on_clean_exit() {
        let router = Arc::new(SessionRouter::new(
            SessionBufferCaps::default(),
            Arc::new(NoopSink),
        ));
        let handle = spawn_idle_sweeper(router, Duration::from_secs(3600));
        handle.shutdown().await.expect("clean shutdown");
    }
}
