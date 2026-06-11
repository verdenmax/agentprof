//! Per-session event router with in-memory buffers and OOM caps (M2.2 T6.1).
//!
//! The router sits between [`crate::otlp::mapper`] (OTLP wire types →
//! [`TypedEvent`]) and the future persistence layer (M2.2 T7.1). It owns a
//! [`DashMap`] of [`SessionBuffer`] instances keyed by `session.id` and
//! decides when each buffer's contents are handed off to a [`FlushSink`].
//!
//! # Flush triggers (spec §5.3)
//!
//! 1. **Explicit** — a [`TypedEvent::SessionEnd`] arrives for the session;
//!    [`CloseReason::ExplicitEnd`].
//! 2. **OOM bytes** — `bytes_used` exceeds [`SessionBufferCaps::max_bytes`]
//!    (default 16 MiB); [`CloseReason::OomBytes`].
//! 3. **OOM events** — `events.len()` exceeds
//!    [`SessionBufferCaps::max_events`] (default 100 000);
//!    [`CloseReason::OomEvents`].
//! 4. **Idle** — `last_seen + idle_timeout < now` (default 5 min);
//!    [`CloseReason::Idle`] (driven by an external sweeper calling
//!    [`SessionRouter::sweep_idle`]; T6.2 wires the actual task).
//! 5. **Shutdown** — [`SessionRouter::flush_all`] drains every buffer with
//!    [`CloseReason::Shutdown`] on SIGINT / SIGTERM.
//!
//! # Soft-fail semantics (spec §5.5)
//!
//! - [`MapperError`] entries in the input vector are dropped with a
//!   `tracing::warn!` — one bad event must not poison the rest of the
//!   batch.
//! - Events without a resolvable `session_id` (i.e.
//!   [`TypedEvent::Unrecognized`]) are dropped silently — the mapper has
//!   already logged them.
//! - The router never panics on lookup misses;
//!   [`SessionRouter::close_buffer`] is idempotent (a missing buffer is a
//!   no-op `Ok(())`).
//!
//! # Concurrency
//!
//! Routing is **sync** (callers wrap it in `tokio::task::spawn_blocking`
//! if needed). `DashMap` provides entry-level locking; ingest acquires the
//! entry guard, mutates the buffer in place, drops the guard, and only
//! then calls [`SessionRouter::close_buffer`] (which takes a fresh
//! `remove()` lock). No `.await` is held across an entry guard.
//!
//! Background sweeper / shutdown wiring lands in M2.2 T6.2; full
//! `into_persistable` → `AnalysisReport` / `Episodes` conversion lands in
//! M2.2 T7.1. This module produces an opaque [`PersistableSession`] that
//! T7.1 will lower.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agentprof_core::episode::tool::ToolCallStatus;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tracing::warn;

use crate::otlp::error::{MapperError, RouterError};
use crate::otlp::typed::TypedEvent;

/// Session identifier — alias for `String` so call sites stay readable
/// without an extra newtype layer (matches [`TypedEvent::session_id`]'s
/// return type).
pub type SessionId = String;

/// Outcome of one [`FlushSink::flush`] call.
///
/// Wraps [`RouterError`] so a flush failure can be surfaced (and
/// `tracing::error!`-logged) without aborting the receiver.
pub type FlushResult = Result<(), RouterError>;

/// Sink invoked whenever the router closes a buffer.
///
/// Implementations persist the [`PersistableSession`] to disk
/// (M2.2 T7.1 will provide the SQLite-backed sink) or — in tests —
/// stash it in a collector vector.
///
/// # Examples
///
/// ```
/// use std::sync::{Arc, Mutex};
/// use agentprof_storage::otlp::router::{FlushSink, FlushResult, PersistableSession, SessionId};
///
/// struct Collect(Arc<Mutex<Vec<PersistableSession>>>);
/// impl FlushSink for Collect {
///     fn flush(&self, _: &SessionId, p: PersistableSession) -> FlushResult {
///         self.0.lock().expect("poisoned").push(p);
///         Ok(())
///     }
/// }
/// ```
pub trait FlushSink: Send + Sync + 'static {
    /// Persist (or otherwise consume) one closed session buffer.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError`] when the sink fails to consume the
    /// payload; the router logs and continues serving other sessions.
    fn flush(&self, session_id: &SessionId, persistable: PersistableSession) -> FlushResult;
}

/// Why a session buffer was closed.
///
/// Surfaced to [`FlushSink`] implementations so persistence can record
/// whether a flush corresponds to a clean `claude_code.session.end` or a
/// forced shutdown / OOM cap trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloseReason {
    /// [`TypedEvent::SessionEnd`] arrived for the session.
    ExplicitEnd,
    /// `bytes_used` exceeded [`SessionBufferCaps::max_bytes`].
    OomBytes,
    /// `events.len()` exceeded [`SessionBufferCaps::max_events`].
    OomEvents,
    /// [`SessionRouter::sweep_idle`] removed the buffer because
    /// `last_seen + idle_timeout < now`.
    Idle,
    /// [`SessionRouter::flush_all`] drained the buffer during shutdown.
    Shutdown,
    /// Router exceeded [`SessionBufferCaps::max_open_sessions`] and
    /// evicted this (least-recently-active) buffer to admit a new
    /// session. See [ADR-0022] D-1.
    ///
    /// [ADR-0022]: ../../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md
    CapacityEvict,
}

/// Per-buffer OOM ceilings and idle timeout.
///
/// Defaults match spec §5.3: 16 MiB bytes / 100 000 events / 5 min idle.
/// Constructed with [`SessionBufferCaps::default`]; the receiver wiring
/// layer overrides individual fields from
/// [`crate::otlp::config::OtlpServerConfig`].
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use agentprof_storage::otlp::router::SessionBufferCaps;
///
/// let caps = SessionBufferCaps::default()
///     .with_max_bytes(1 << 20)            // 1 MiB
///     .with_max_events(1_000)
///     .with_idle_timeout(Duration::from_secs(30));
/// assert_eq!(caps.max_events, 1_000);
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SessionBufferCaps {
    /// Force-flush threshold on `bytes_used` (rough estimate, see
    /// [`SessionBuffer`] field doc).
    pub max_bytes: usize,
    /// Force-flush threshold on `events.len()`.
    pub max_events: usize,
    /// Maximum gap between two ingests before a sweeper closes the
    /// buffer with [`CloseReason::Idle`].
    pub idle_timeout: Duration,
    /// Maximum number of distinct sessions tracked concurrently.
    /// When exceeded, the LRU buffer is evicted with
    /// [`CloseReason::CapacityEvict`]. Default 1024.
    pub max_open_sessions: usize,
}

impl Default for SessionBufferCaps {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_events: 100_000,
            idle_timeout: Duration::from_secs(5 * 60),
            max_open_sessions: 1024,
        }
    }
}

impl SessionBufferCaps {
    /// Returns `self` with `max_bytes` overridden — chainable builder
    /// over [`SessionBufferCaps::default`] for tests and CLI overrides
    /// (since the struct is `#[non_exhaustive]` and downstream crates
    /// cannot use struct-literal syntax).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::router::SessionBufferCaps;
    /// let caps = SessionBufferCaps::default().with_max_bytes(1024);
    /// assert_eq!(caps.max_bytes, 1024);
    /// ```
    #[must_use]
    pub const fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Returns `self` with `max_events` overridden.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::router::SessionBufferCaps;
    /// let caps = SessionBufferCaps::default().with_max_events(10);
    /// assert_eq!(caps.max_events, 10);
    /// ```
    #[must_use]
    pub const fn with_max_events(mut self, max_events: usize) -> Self {
        self.max_events = max_events;
        self
    }

    /// Returns `self` with `idle_timeout` overridden.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use agentprof_storage::otlp::router::SessionBufferCaps;
    /// let caps = SessionBufferCaps::default().with_idle_timeout(Duration::from_secs(1));
    /// assert_eq!(caps.idle_timeout, Duration::from_secs(1));
    /// ```
    #[must_use]
    pub const fn with_idle_timeout(mut self, idle_timeout: Duration) -> Self {
        self.idle_timeout = idle_timeout;
        self
    }

    /// Returns `self` with `max_open_sessions` overridden.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::router::SessionBufferCaps;
    /// let caps = SessionBufferCaps::default().with_max_open_sessions(8);
    /// assert_eq!(caps.max_open_sessions, 8);
    /// ```
    #[must_use]
    pub const fn with_max_open_sessions(mut self, n: usize) -> Self {
        self.max_open_sessions = n;
        self
    }
}

/// Closed-buffer payload handed to [`FlushSink::flush`].
///
/// Carries the (now-immutable) event stream plus session metadata so the
/// downstream persistence layer (M2.2 T7.1) can lower it to
/// `AnalysisReport` + `Episodes` rows.
///
/// Events are sorted by timestamp; any [`TypedEvent::ToolDecisionStart`]
/// that never saw a matching [`TypedEvent::ToolResult`] has been padded
/// with a synthetic [`TypedEvent::ToolResult`] of status
/// [`ToolCallStatus::OpenAtEndOfSession`] (spec §5.4 pairing rule).
#[derive(Debug)]
#[non_exhaustive]
pub struct PersistableSession {
    /// `session.id` this buffer was keyed under.
    pub session_id: SessionId,
    /// Why the buffer closed.
    pub close_reason: CloseReason,
    /// Time-sorted event stream including synthetic close-out
    /// [`TypedEvent::ToolResult`] entries for unpaired starts.
    pub events: Vec<TypedEvent>,
    /// Wall-clock start time (from the matched [`TypedEvent::SessionStart`]
    /// — `None` if no start was ever seen, which happens for buffers
    /// opened by a `UserPrompt` before the start log lands).
    pub started_at: Option<DateTime<Utc>>,
    /// Wall-clock end time (from the matched [`TypedEvent::SessionEnd`]
    /// — `None` for forced closes that never received an explicit end).
    pub ended_at: Option<DateTime<Utc>>,
}

/// Per-session in-memory event buffer.
///
/// Accumulates [`TypedEvent`]s for a single `session.id`, tracks open
/// `ToolDecisionStart` spans for pairing, and runs the OOM-cap accounting
/// checked by [`SessionRouter::ingest`] after every push.
///
/// All mutation goes through the parent [`SessionRouter`]; this struct is
/// public for `FlushSink` impls that need to inspect the closed state.
#[derive(Debug)]
pub struct SessionBuffer {
    session_id: SessionId,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    events: Vec<TypedEvent>,
    /// Unmatched [`TypedEvent::ToolDecisionStart`] entries keyed by
    /// `(tool_name, turn_id)` per spec §5.4 pairing algorithm. Each
    /// remaining entry at flush time becomes a synthetic
    /// `ToolResult { status: OpenAtEndOfSession }`.
    open_tool_calls: HashMap<(String, Option<String>), DateTime<Utc>>,
    /// Heuristic byte counter (struct size + per-variant string heap
    /// lengths). Not exact — used only for OOM trip decisions.
    bytes_used: usize,
    last_seen: Instant,
    last_event_ts: Option<DateTime<Utc>>,
    closed: bool,
}

impl SessionBuffer {
    fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            started_at: None,
            ended_at: None,
            events: Vec::new(),
            open_tool_calls: HashMap::new(),
            bytes_used: 0,
            last_seen: Instant::now(),
            last_event_ts: None,
            closed: false,
        }
    }

    fn push(&mut self, event: TypedEvent) {
        self.bytes_used = self.bytes_used.saturating_add(estimate_bytes(&event));
        self.last_seen = Instant::now();
        match &event {
            TypedEvent::SessionStart { started_at, .. } => {
                self.started_at = Some(*started_at);
                self.last_event_ts = Some(*started_at);
            }
            TypedEvent::SessionEnd { ended_at, .. } => {
                self.ended_at = Some(*ended_at);
                self.last_event_ts = Some(*ended_at);
            }
            TypedEvent::ToolDecisionStart {
                tool_name,
                turn_id,
                timestamp,
                ..
            } => {
                self.open_tool_calls
                    .insert((tool_name.clone(), turn_id.clone()), *timestamp);
                self.last_event_ts = Some(*timestamp);
            }
            TypedEvent::ToolResult {
                tool_name,
                turn_id,
                timestamp,
                ..
            } => {
                self.open_tool_calls
                    .remove(&(tool_name.clone(), turn_id.clone()));
                self.last_event_ts = Some(*timestamp);
            }
            TypedEvent::UserPrompt { timestamp, .. } | TypedEvent::TokenUsage { timestamp, .. } => {
                self.last_event_ts = Some(*timestamp);
            }
            TypedEvent::Unrecognized { .. } => {}
        }
        self.events.push(event);
    }

    /// Returns the `session.id` this buffer was keyed under.
    ///
    /// # Examples
    ///
    /// ```
    /// // SessionBuffer is constructed internally; consumers see it via
    /// // PersistableSession::session_id. See `FlushSink` doc example.
    /// # let _ = ();
    /// ```
    #[must_use]
    pub fn session_id(&self) -> &str {
        self.session_id.as_str()
    }

    /// Closes this buffer and produces the immutable [`PersistableSession`]
    /// snapshot consumed by [`FlushSink::flush`].
    ///
    /// Implements the pairing algorithm from spec §5.4: any
    /// [`TypedEvent::ToolDecisionStart`] still in `open_tool_calls` is
    /// closed with a synthetic [`TypedEvent::ToolResult`] of status
    /// [`ToolCallStatus::OpenAtEndOfSession`] timestamped at the buffer's
    /// last-known wall-clock event (`ended_at` if present, else the most
    /// recent ingested event's timestamp, else `Utc::now()`). The full
    /// event vector is then stable-sorted by timestamp.
    ///
    /// # Examples
    ///
    /// ```
    /// # use agentprof_storage::otlp::router::{SessionRouter, SessionBufferCaps,
    /// #     CloseReason, FlushSink, FlushResult, PersistableSession, SessionId};
    /// # use std::sync::{Arc, Mutex};
    /// # struct Sink(Arc<Mutex<Vec<PersistableSession>>>);
    /// # impl FlushSink for Sink {
    /// #     fn flush(&self, _: &SessionId, p: PersistableSession) -> FlushResult {
    /// #         self.0.lock().expect("poisoned").push(p); Ok(())
    /// #     }
    /// # }
    /// let bucket = Arc::new(Mutex::new(Vec::new()));
    /// let router = SessionRouter::new(
    ///     SessionBufferCaps::default(),
    ///     Arc::new(Sink(bucket.clone())),
    /// );
    /// // ... ingest events ...
    /// let _ = router.flush_all(CloseReason::Shutdown);
    /// ```
    #[must_use]
    pub fn into_persistable(mut self, reason: CloseReason) -> PersistableSession {
        let synthesis_ts = self
            .ended_at
            .or(self.last_event_ts)
            .unwrap_or_else(Utc::now);
        let session_id = self.session_id.clone();
        for ((tool_name, turn_id), _started_at) in self.open_tool_calls.drain() {
            self.events.push(TypedEvent::ToolResult {
                session_id: session_id.clone(),
                turn_id,
                tool_name,
                timestamp: synthesis_ts,
                status: ToolCallStatus::OpenAtEndOfSession,
            });
        }
        self.events.sort_by_key(event_timestamp);
        self.closed = true;
        PersistableSession {
            session_id,
            close_reason: reason,
            events: self.events,
            started_at: self.started_at,
            ended_at: self.ended_at,
        }
    }
}

const fn event_timestamp(e: &TypedEvent) -> DateTime<Utc> {
    match e {
        TypedEvent::SessionStart { started_at, .. } => *started_at,
        TypedEvent::UserPrompt { timestamp, .. }
        | TypedEvent::ToolDecisionStart { timestamp, .. }
        | TypedEvent::ToolResult { timestamp, .. }
        | TypedEvent::TokenUsage { timestamp, .. } => *timestamp,
        TypedEvent::SessionEnd { ended_at, .. } => *ended_at,
        TypedEvent::Unrecognized { .. } => DateTime::<Utc>::MIN_UTC,
    }
}

fn estimate_bytes(e: &TypedEvent) -> usize {
    let base = std::mem::size_of::<TypedEvent>();
    let heap = match e {
        TypedEvent::SessionStart {
            session_id,
            model,
            cwd,
            ..
        } => {
            session_id.len()
                + model.as_ref().map_or(0, String::len)
                + cwd.as_ref().map_or(0, |p| p.as_os_str().len())
        }
        TypedEvent::UserPrompt {
            session_id,
            turn_id,
            ..
        } => session_id.len() + turn_id.len(),
        TypedEvent::ToolDecisionStart {
            session_id,
            turn_id,
            tool_name,
            ..
        } => session_id.len() + turn_id.as_ref().map_or(0, String::len) + tool_name.len(),
        TypedEvent::ToolResult {
            session_id,
            turn_id,
            tool_name,
            status,
            ..
        } => {
            session_id.len()
                + turn_id.as_ref().map_or(0, String::len)
                + tool_name.len()
                + status_string_len(status)
        }
        TypedEvent::TokenUsage {
            session_id, model, ..
        } => session_id.len() + model.len(),
        TypedEvent::SessionEnd { session_id, .. } => session_id.len(),
        TypedEvent::Unrecognized { identity, .. } => identity.len(),
    };
    base + heap
}

fn status_string_len(s: &ToolCallStatus) -> usize {
    match s {
        ToolCallStatus::Failure {
            message: Some(m), ..
        } => m.len(),
        _ => 0,
    }
}

/// Routes [`TypedEvent`]s into per-session [`SessionBuffer`]s and
/// triggers [`FlushSink::flush`] on the four close conditions from spec
/// §5.3 (explicit end, OOM bytes, OOM events, idle / shutdown).
///
/// Cheap to clone via [`Arc`] — share one instance across the gRPC and
/// HTTP listener tasks. All methods are `&self` (interior mutability via
/// [`DashMap`]) and **sync** — async callers wrap with
/// `tokio::task::spawn_blocking` when ingesting from an async transport.
///
/// # Examples
///
/// ```
/// use std::sync::{Arc, Mutex};
/// use agentprof_storage::otlp::router::{
///     SessionRouter, SessionBufferCaps, FlushSink, FlushResult,
///     PersistableSession, SessionId, CloseReason,
/// };
///
/// struct Collect(Arc<Mutex<Vec<PersistableSession>>>);
/// impl FlushSink for Collect {
///     fn flush(&self, _: &SessionId, p: PersistableSession) -> FlushResult {
///         self.0.lock().expect("poisoned").push(p);
///         Ok(())
///     }
/// }
///
/// let bucket = Arc::new(Mutex::new(Vec::new()));
/// let router = SessionRouter::new(
///     SessionBufferCaps::default(),
///     Arc::new(Collect(bucket.clone())),
/// );
/// router.ingest(Vec::new());                       // no-op
/// let _ = router.flush_all(CloseReason::Shutdown); // drains nothing
/// assert!(bucket.lock().expect("poisoned").is_empty());
/// ```
pub struct SessionRouter {
    buffers: DashMap<SessionId, SessionBuffer>,
    cap: SessionBufferCaps,
    flush_sink: Arc<dyn FlushSink>,
    /// LRU ordering of currently-open sessions; front = oldest,
    /// back = newest. Touched on every `ingest` event so "recency"
    /// reflects activity, not just admission time. Guarded by
    /// `std::sync::Mutex` (no new dep); critical sections are short
    /// (push/pop a `String`, linear scan ≤ `max_open_sessions`).
    /// See [ADR-0022] D-1.
    ///
    /// [ADR-0022]: ../../../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md
    lru: Mutex<VecDeque<SessionId>>,
}

impl std::fmt::Debug for SessionRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lru_len = self.lru.lock().map_or(0, |g| g.len());
        f.debug_struct("SessionRouter")
            .field("buffers_open", &self.buffers.len())
            .field("lru_len", &lru_len)
            .field("cap", &self.cap)
            .finish_non_exhaustive()
    }
}

impl SessionRouter {
    /// Constructs an empty router with the given caps + sink.
    ///
    /// # Examples
    ///
    /// See the type-level example on [`SessionRouter`].
    #[must_use]
    pub fn new(cap: SessionBufferCaps, flush_sink: Arc<dyn FlushSink>) -> Self {
        Self {
            buffers: DashMap::new(),
            cap,
            flush_sink,
            lru: Mutex::new(VecDeque::new()),
        }
    }

    /// Number of currently open buffers — convenient for tests and
    /// diagnostics; not part of the receiver hot path.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::{Arc, Mutex};
    /// # use agentprof_storage::otlp::router::{SessionRouter, SessionBufferCaps,
    /// #     FlushSink, FlushResult, PersistableSession, SessionId};
    /// # struct Sink;
    /// # impl FlushSink for Sink {
    /// #     fn flush(&self, _: &SessionId, _: PersistableSession) -> FlushResult { Ok(()) }
    /// # }
    /// let router = SessionRouter::new(SessionBufferCaps::default(), Arc::new(Sink));
    /// assert_eq!(router.open_buffers(), 0);
    /// ```
    #[must_use]
    pub fn open_buffers(&self) -> usize {
        self.buffers.len()
    }

    /// Routes a batch of mapper outputs (one OTLP request → one batch).
    ///
    /// Per-event behavior:
    ///
    /// - `Err(MapperError)` → drop with `tracing::warn!` (soft-fail).
    /// - `Ok(TypedEvent::Unrecognized)` or any event whose `session_id()`
    ///   returns `None` → drop silently (already logged by the mapper).
    /// - Otherwise: find or create the matching [`SessionBuffer`], push
    ///   the event, then check the three immediate close triggers
    ///   (explicit end / OOM bytes / OOM events). When any fires the
    ///   buffer is flushed via [`SessionRouter::close_buffer`].
    ///
    /// # Examples
    ///
    /// See the type-level example on [`SessionRouter`].
    pub fn ingest(&self, events: Vec<Result<TypedEvent, MapperError>>) {
        for ev in events {
            let event = match ev {
                Ok(e) => e,
                Err(err) => {
                    warn!(
                        target: "agentprof::otlp::router",
                        error = %err,
                        "dropping mapper error before buffer push",
                    );
                    continue;
                }
            };
            let Some(sid) = event.session_id().map(str::to_owned) else {
                continue;
            };

            // ADR-0022 D-1: before admitting a NEW session, evict the
            // least-recently-active buffer if we are already at the cap.
            // Existing sessions skip this branch — they are already
            // counted, and admission control only applies to admissions.
            let is_new = !self.buffers.contains_key(&sid);
            if is_new {
                let victim = {
                    let mut lru = match self.lru.lock() {
                        Ok(g) => g,
                        // Recover from poisoning: the LRU index is
                        // strictly advisory; a panic on another thread
                        // must not take down the receiver.
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if self.buffers.len() >= self.cap.max_open_sessions {
                        lru.pop_front()
                    } else {
                        None
                    }
                };
                if let Some(evicted) = victim {
                    // Drop the lock before `close_buffer`, which re-acquires
                    // it via `evict_from_lru` on the close path.
                    let _ = self.close_buffer(&evicted, CloseReason::CapacityEvict);
                }
            }

            let close_reason: Option<CloseReason> = {
                let mut entry = self
                    .buffers
                    .entry(sid.clone())
                    .or_insert_with(|| SessionBuffer::new(sid.clone()));
                let buf = entry.value_mut();
                let is_end = matches!(event, TypedEvent::SessionEnd { .. });
                buf.push(event);
                let reason = if is_end {
                    Some(CloseReason::ExplicitEnd)
                } else if buf.bytes_used > self.cap.max_bytes {
                    Some(CloseReason::OomBytes)
                } else if buf.events.len() > self.cap.max_events {
                    Some(CloseReason::OomEvents)
                } else {
                    None
                };
                drop(entry);
                reason
            };

            // Touch LRU index — moves `sid` to the back (most-recent).
            self.touch_lru(&sid);

            if let Some(reason) = close_reason {
                let _ = self.close_buffer(&sid, reason);
            }
        }
    }

    /// Move `sid` to the back of the LRU index (most-recently-used).
    /// Adds `sid` if not present. Linear scan over the deque; cost is
    /// bounded by `cap.max_open_sessions` (default 1024).
    ///
    /// Recovers from a poisoned mutex — the LRU index is advisory and
    /// a panic on a sibling thread must not take down the receiver.
    fn touch_lru(&self, sid: &SessionId) {
        let mut lru = match self.lru.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(pos) = lru.iter().position(|s| s == sid) {
            lru.remove(pos);
        }
        lru.push_back(sid.clone());
    }

    /// Remove `sid` from the LRU index. No-op if absent. Recovers from
    /// a poisoned mutex (see [`SessionRouter::touch_lru`]).
    fn evict_from_lru(&self, sid: &SessionId) {
        let mut lru = match self.lru.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(pos) = lru.iter().position(|s| s == sid) {
            lru.remove(pos);
        }
    }

    /// Removes the buffer for `session_id` (if any) and forwards its
    /// [`PersistableSession`] to the configured [`FlushSink`].
    ///
    /// Idempotent: closing an unknown / already-closed session is a
    /// no-op `Ok(())`. Returning `Ok` lets concurrent callers
    /// (`sweep_idle` + `ingest` racing) both succeed without coordination.
    ///
    /// # Errors
    ///
    /// Returns whatever the [`FlushSink::flush`] implementation returned
    /// (typically [`RouterError::Storage`] when the persistence layer
    /// fails). The router itself never produces an error here.
    ///
    /// # Examples
    ///
    /// See the type-level example on [`SessionRouter`].
    pub fn close_buffer(&self, session_id: &SessionId, reason: CloseReason) -> FlushResult {
        let Some((_, buf)) = self.buffers.remove(session_id) else {
            // Keep the LRU consistent even if the buffer is already gone
            // (eviction races with explicit close).
            self.evict_from_lru(session_id);
            return Ok(());
        };
        self.evict_from_lru(session_id);
        let persistable = buf.into_persistable(reason);
        self.flush_sink.flush(session_id, persistable)
    }

    /// Closes every buffer whose `last_seen` is older than
    /// `cap.idle_timeout` and returns the closed session ids.
    ///
    /// Drives the spec §5.3 "Idle" flush trigger. M2.2 T6.2 will spawn a
    /// `tokio::time::interval` task that calls this on a fixed cadence;
    /// `Vec<SessionId>` lets the caller emit per-id metrics / log lines.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::{Arc, Mutex};
    /// # use std::time::Duration;
    /// # use agentprof_storage::otlp::router::{SessionRouter, SessionBufferCaps,
    /// #     FlushSink, FlushResult, PersistableSession, SessionId};
    /// # struct Sink;
    /// # impl FlushSink for Sink {
    /// #     fn flush(&self, _: &SessionId, _: PersistableSession) -> FlushResult { Ok(()) }
    /// # }
    /// let caps = SessionBufferCaps::default().with_idle_timeout(Duration::from_secs(1));
    /// let router = SessionRouter::new(caps, Arc::new(Sink));
    /// assert!(router.sweep_idle().is_empty());
    /// ```
    #[must_use = "discarded sweep_idle result loses the per-session metric signal"]
    pub fn sweep_idle(&self) -> Vec<SessionId> {
        let now = Instant::now();
        let timeout = self.cap.idle_timeout;
        let stale: Vec<SessionId> = self
            .buffers
            .iter()
            .filter(|e| now.duration_since(e.value().last_seen) > timeout)
            .map(|e| e.key().clone())
            .collect();
        for sid in &stale {
            let _ = self.close_buffer(sid, CloseReason::Idle);
        }
        stale
    }

    /// Drains every open buffer with the given `reason` and returns the
    /// per-flush results.
    ///
    /// Used by the receiver's shutdown path
    /// (`reason = CloseReason::Shutdown`); also handy in tests for a
    /// deterministic teardown. Concurrent ingest is safe — the caller is
    /// expected to stop the listeners first; any event arriving after
    /// `flush_all` simply opens a fresh buffer.
    ///
    /// # Examples
    ///
    /// See the type-level example on [`SessionRouter`].
    #[must_use = "discarded flush_all result hides per-session sink errors"]
    pub fn flush_all(&self, reason: CloseReason) -> Vec<FlushResult> {
        let ids: Vec<SessionId> = self.buffers.iter().map(|e| e.key().clone()).collect();
        ids.into_iter()
            .map(|id| self.close_buffer(&id, reason))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_default_matches_spec_5_3() {
        let c = SessionBufferCaps::default();
        assert_eq!(c.max_bytes, 16 * 1024 * 1024);
        assert_eq!(c.max_events, 100_000);
        assert_eq!(c.idle_timeout, Duration::from_secs(300));
    }

    #[test]
    fn close_reason_is_copy_and_distinguishable() {
        let a = CloseReason::OomBytes;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(CloseReason::Idle, CloseReason::Shutdown);
    }
}
