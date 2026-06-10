//! Ingest pipeline: OTLP wire → mapper → router → storage sink (M2.2 T7.1).
//!
//! [`IngestPipeline`] is the single fan-in point through which the gRPC
//! ([`crate::otlp::server_grpc`]) and HTTP ([`crate::otlp::server_http`])
//! receivers hand off OTLP envelopes to the persistence layer.
//!
//! Per-call flow:
//!
//! 1. The transport hands us a decoded `ExportXServiceRequest`.
//! 2. We run [`crate::otlp::mapper`] to lower it to
//!    `Vec<Result<TypedEvent, MapperError>>`.
//! 3. Mapper errors are counted into `error_count` and dropped (the router
//!    will `tracing::warn!` them too — counting here gives a transport-level
//!    metric without re-walking the batch).
//! 4. The vector is forwarded to [`crate::otlp::router::SessionRouter::ingest`],
//!    which buckets by `session.id`, applies OOM caps, and synchronously
//!    flushes through the configured [`crate::otlp::router::FlushSink`].
//! 5. We return an empty success response — at this point the events are
//!    either buffered in memory (most common, awaiting more activity / a
//!    `SessionEnd` / an idle sweep) or already persisted by
//!    [`crate::otlp::sink_storage::StorageFlushSink`].
//!
//! The pipeline never panics; recoverable failures stay in the warning /
//! counter channels. Storage errors raised by the sink are logged by the
//! router as `FlushResult` failures and never propagate up to the
//! transport — a single bad row must not turn into a 500 for unrelated
//! sessions.
//!
//! # Examples
//!
//! ```
//! use agentprof_storage::otlp::pipeline::IngestPipeline;
//! let p = IngestPipeline::noop_for_test();
//! assert_eq!(p.counts_for_test(), (0, 0, 0));
//! assert_eq!(p.error_count_for_test(), 0);
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::otlp::error::OtlpServerError;
use crate::otlp::mapper;
use crate::otlp::proto::logs::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use crate::otlp::proto::metrics::{ExportMetricsServiceRequest, ExportMetricsServiceResponse};
use crate::otlp::proto::trace::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use crate::otlp::router::{
    FlushResult, FlushSink, PersistableSession, SessionBufferCaps, SessionId, SessionRouter,
};

/// Fan-in point for OTLP ingest.
///
/// Owns an [`Arc<SessionRouter>`] which in turn drives the configured
/// [`FlushSink`] (typically [`crate::otlp::sink_storage::StorageFlushSink`]
/// in production, an in-memory collector in tests). Per-signal call
/// counters and a unified mapper-error counter remain for tests and
/// future metrics export (M2.2 T8).
///
/// `#[non_exhaustive]` so subsequent milestones can add fields (metrics
/// handles, custom flush hooks) without breaking external matchers.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::pipeline::IngestPipeline;
/// use std::sync::Arc;
/// let p = Arc::new(IngestPipeline::noop_for_test());
/// assert_eq!(p.counts_for_test(), (0, 0, 0));
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct IngestPipeline {
    /// Per-session router that owns buffers + drives the flush sink.
    router: Arc<SessionRouter>,
    /// Successful `ingest_logs` invocations (one increment per OTLP request).
    logs_count: AtomicU64,
    /// Successful `ingest_metrics` invocations.
    metrics_count: AtomicU64,
    /// Successful `ingest_traces` invocations.
    traces_count: AtomicU64,
    /// Per-event mapper errors observed across all signals.
    error_count: AtomicU64,
}

/// In-memory [`FlushSink`] that drops every flush silently.
///
/// Used by [`IngestPipeline::noop_for_test`] so smoke tests that only
/// exercise the transport layer don't need a real storage handle.
#[derive(Debug, Default)]
struct NoopFlushSink;

impl FlushSink for NoopFlushSink {
    fn flush(&self, _session_id: &SessionId, _persistable: PersistableSession) -> FlushResult {
        Ok(())
    }
}

impl IngestPipeline {
    /// Construct an `IngestPipeline` around an existing router.
    ///
    /// Counters start at zero. Wire the router with whatever
    /// [`FlushSink`] the caller wants (typically
    /// [`crate::otlp::sink_storage::StorageFlushSink`] in production).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// use agentprof_storage::otlp::router::{SessionBufferCaps, SessionRouter, FlushSink,
    ///     FlushResult, PersistableSession, SessionId};
    ///
    /// struct Sink;
    /// impl FlushSink for Sink {
    ///     fn flush(&self, _: &SessionId, _: PersistableSession) -> FlushResult { Ok(()) }
    /// }
    /// let router = Arc::new(SessionRouter::new(SessionBufferCaps::default(), Arc::new(Sink)));
    /// let _ = IngestPipeline::new(router);
    /// ```
    #[must_use]
    pub const fn new(router: Arc<SessionRouter>) -> Self {
        Self {
            router,
            logs_count: AtomicU64::new(0),
            metrics_count: AtomicU64::new(0),
            traces_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        }
    }

    /// Construct a pipeline backed by a router with the default caps and
    /// a no-op flush sink.
    ///
    /// Used by the gRPC / HTTP smoke tests in this crate (and by transport
    /// integration tests in other crates) that exercise the listener
    /// plumbing without caring about persistence.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// let _ = IngestPipeline::noop_for_test();
    /// ```
    #[doc(hidden)]
    #[must_use]
    pub fn noop_for_test() -> Self {
        let sink: Arc<dyn FlushSink> = Arc::new(NoopFlushSink);
        let router = Arc::new(SessionRouter::new(SessionBufferCaps::default(), sink));
        Self::new(router)
    }

    /// Borrow the inner router. Useful in tests that want to flush /
    /// inspect open buffer counts after firing some requests.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// let p = IngestPipeline::noop_for_test();
    /// assert_eq!(p.router_for_test().open_buffers(), 0);
    /// ```
    #[doc(hidden)]
    #[must_use]
    pub const fn router_for_test(&self) -> &Arc<SessionRouter> {
        &self.router
    }

    /// Read the current `(logs, metrics, traces)` per-signal counters.
    ///
    /// Loaded with [`Ordering::Relaxed`] — values are eventually
    /// consistent across threads and may lag a concurrent writer by one
    /// increment.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// let p = IngestPipeline::noop_for_test();
    /// assert_eq!(p.counts_for_test(), (0, 0, 0));
    /// ```
    #[doc(hidden)]
    #[must_use]
    pub fn counts_for_test(&self) -> (u64, u64, u64) {
        (
            self.logs_count.load(Ordering::Relaxed),
            self.metrics_count.load(Ordering::Relaxed),
            self.traces_count.load(Ordering::Relaxed),
        )
    }

    /// Read the cumulative mapper-error counter.
    ///
    /// Incremented once per `Err(MapperError)` observed by `ingest_*`,
    /// across all three signals. Loaded with [`Ordering::Relaxed`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// let p = IngestPipeline::noop_for_test();
    /// assert_eq!(p.error_count_for_test(), 0);
    /// ```
    #[doc(hidden)]
    #[must_use]
    pub fn error_count_for_test(&self) -> u64 {
        self.error_count.load(Ordering::Relaxed)
    }

    /// Ingest one OTLP `ExportLogsServiceRequest`.
    ///
    /// Runs [`mapper::map_logs`], counts mapper errors, and forwards the
    /// batch (errors included — the router drops them with a warning) to
    /// [`SessionRouter::ingest`]. Returns the OTLP empty-success response
    /// the transport will encode back to the client.
    ///
    /// Takes `Arc<Self>` so the future is `'static` and can be spawned
    /// by the tonic service impl.
    ///
    /// # Errors
    ///
    /// Currently infallible — the router never propagates per-event
    /// failures upward (storage errors are logged by the sink layer).
    /// The `Result` is preserved for future signals (e.g. surfacing
    /// fatal storage failures so the transport can return 503).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn run() {
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// use std::sync::Arc;
    /// let p = Arc::new(IngestPipeline::noop_for_test());
    /// let req = Default::default();
    /// let _ = p.ingest_logs(req).await;
    /// # }
    /// ```
    #[allow(clippy::unused_async)] // signature kept async so transport callers don't churn
    pub async fn ingest_logs(
        self: Arc<Self>,
        req: ExportLogsServiceRequest,
    ) -> Result<ExportLogsServiceResponse, OtlpServerError> {
        let mapped = mapper::map_logs(&req);
        let bad = mapped.iter().filter(|r| r.is_err()).count();
        if bad > 0 {
            self.error_count.fetch_add(bad as u64, Ordering::Relaxed);
        }
        self.router.ingest(mapped);
        self.logs_count.fetch_add(1, Ordering::Relaxed);
        Ok(ExportLogsServiceResponse {
            partial_success: None,
        })
    }

    /// Ingest one OTLP `ExportMetricsServiceRequest`. See [`Self::ingest_logs`].
    ///
    /// # Errors
    ///
    /// Currently infallible; see [`Self::ingest_logs`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn run() {
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// use std::sync::Arc;
    /// let p = Arc::new(IngestPipeline::noop_for_test());
    /// let req = Default::default();
    /// let _ = p.ingest_metrics(req).await;
    /// # }
    /// ```
    #[allow(clippy::unused_async)]
    pub async fn ingest_metrics(
        self: Arc<Self>,
        req: ExportMetricsServiceRequest,
    ) -> Result<ExportMetricsServiceResponse, OtlpServerError> {
        let mapped = mapper::map_metrics(&req);
        let bad = mapped.iter().filter(|r| r.is_err()).count();
        if bad > 0 {
            self.error_count.fetch_add(bad as u64, Ordering::Relaxed);
        }
        self.router.ingest(mapped);
        self.metrics_count.fetch_add(1, Ordering::Relaxed);
        Ok(ExportMetricsServiceResponse {
            partial_success: None,
        })
    }

    /// Ingest one OTLP `ExportTraceServiceRequest`. See [`Self::ingest_logs`].
    ///
    /// # Errors
    ///
    /// Currently infallible; see [`Self::ingest_logs`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn run() {
    /// use agentprof_storage::otlp::pipeline::IngestPipeline;
    /// use std::sync::Arc;
    /// let p = Arc::new(IngestPipeline::noop_for_test());
    /// let req = Default::default();
    /// let _ = p.ingest_traces(req).await;
    /// # }
    /// ```
    #[allow(clippy::unused_async)]
    pub async fn ingest_traces(
        self: Arc<Self>,
        req: ExportTraceServiceRequest,
    ) -> Result<ExportTraceServiceResponse, OtlpServerError> {
        let mapped = mapper::map_traces(&req);
        let bad = mapped.iter().filter(|r| r.is_err()).count();
        if bad > 0 {
            self.error_count.fetch_add(bad as u64, Ordering::Relaxed);
        }
        self.router.ingest(mapped);
        self.traces_count.fetch_add(1, Ordering::Relaxed);
        Ok(ExportTraceServiceResponse {
            partial_success: None,
        })
    }
}
