//! Ingest pipeline stub (M2.2 T2.2).
//!
//! `IngestPipeline` is the single fan-in point through which the gRPC
//! ([`crate::otlp::server_grpc`]) and (future) HTTP receivers hand off
//! OTLP envelopes to the persistence layer. In T2.2 it is a **counters-only
//! stub**: every export call increments a per-signal `AtomicUsize` and
//! returns a fully-successful response. The real mapper +
//! per-session router (M2.2 T6.x / T7.1) will replace the body of the
//! `ingest_*` methods without changing their signatures.
//!
//! The OTLP message types used in the API surface come from a
//! tonic-generated, crate-private `proto` module produced by `build.rs`
//! (gated on the `otlp` feature) and re-included in
//! [`crate::otlp`]; the server stubs in [`crate::otlp::server_grpc`]
//! reference the same generated types so request / response plumbing
//! type-checks without an extra conversion layer.
//!
//! # Examples
//!
//! ```
//! use agentprof_storage::otlp::pipeline::IngestPipeline;
//! let p = IngestPipeline::noop_for_test();
//! assert_eq!(p.counts_for_test(), (0, 0, 0));
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::otlp::error::RouterError;
use crate::otlp::proto::logs::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use crate::otlp::proto::metrics::{ExportMetricsServiceRequest, ExportMetricsServiceResponse};
use crate::otlp::proto::trace::{ExportTraceServiceRequest, ExportTraceServiceResponse};

/// Fan-in point for OTLP ingest.
///
/// Holds per-signal counters bumped on every successful `ingest_*` call.
/// In the M2.2 T2.2 milestone the counters are the **only** observable
/// effect; the real session-router + storage layer plugs in at T7.1.
///
/// `#[non_exhaustive]` so subsequent milestones can add fields (e.g. a
/// `Arc<SessionRouter>`, a flush-spawn handle, or a metrics registry)
/// without breaking external matchers.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::pipeline::IngestPipeline;
/// use std::sync::Arc;
/// let p = Arc::new(IngestPipeline::noop_for_test());
/// assert_eq!(p.counts_for_test(), (0, 0, 0));
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
#[allow(clippy::struct_field_names)]
pub struct IngestPipeline {
    received_logs: AtomicUsize,
    received_metrics: AtomicUsize,
    received_traces: AtomicUsize,
}

impl IngestPipeline {
    /// Construct a no-op pipeline suitable for unit / smoke tests.
    ///
    /// Equivalent to [`IngestPipeline::default`]; provided under a
    /// `#[doc(hidden)]` factory name so the call site reads as intent at
    /// the test boundary.
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
        Self::default()
    }

    /// Read the current `(logs, metrics, traces)` counters.
    ///
    /// Intended for tests verifying that an ingest path actually fired.
    /// Loaded with [`Ordering::Relaxed`] — values are eventually consistent
    /// across threads and may lag a concurrent writer by one increment.
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
    pub fn counts_for_test(&self) -> (usize, usize, usize) {
        (
            self.received_logs.load(Ordering::Relaxed),
            self.received_metrics.load(Ordering::Relaxed),
            self.received_traces.load(Ordering::Relaxed),
        )
    }

    /// Ingest an OTLP `ExportLogsServiceRequest`.
    ///
    /// Stub implementation: increments the `received_logs` counter and
    /// returns a fully-successful response (no `partial_success`).
    ///
    /// Takes `Arc<Self>` by value so the future is `'static` and can be
    /// spawned by the tonic service impl. The router replacement in T7.1
    /// keeps this signature.
    ///
    /// # Errors
    ///
    /// Currently infallible. The `Result` is preserved so the T7.1 router
    /// can return [`RouterError::Storage`], [`RouterError::Mapper`], or
    /// [`RouterError::BufferOom`] without an API break.
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
    #[allow(clippy::unused_async)] // real router in T7.1 awaits on storage
    pub async fn ingest_logs(
        self: Arc<Self>,
        _req: ExportLogsServiceRequest,
    ) -> Result<ExportLogsServiceResponse, RouterError> {
        self.received_logs.fetch_add(1, Ordering::Relaxed);
        Ok(ExportLogsServiceResponse {
            partial_success: None,
        })
    }

    /// Ingest an OTLP `ExportMetricsServiceRequest`. See [`Self::ingest_logs`].
    ///
    /// # Errors
    ///
    /// Currently infallible; reserved for [`RouterError`] variants in T7.1.
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
    #[allow(clippy::unused_async)] // real router in T7.1 awaits on storage
    pub async fn ingest_metrics(
        self: Arc<Self>,
        _req: ExportMetricsServiceRequest,
    ) -> Result<ExportMetricsServiceResponse, RouterError> {
        self.received_metrics.fetch_add(1, Ordering::Relaxed);
        Ok(ExportMetricsServiceResponse {
            partial_success: None,
        })
    }

    /// Ingest an OTLP `ExportTraceServiceRequest`. See [`Self::ingest_logs`].
    ///
    /// # Errors
    ///
    /// Currently infallible; reserved for [`RouterError`] variants in T7.1.
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
    #[allow(clippy::unused_async)] // real router in T7.1 awaits on storage
    pub async fn ingest_traces(
        self: Arc<Self>,
        _req: ExportTraceServiceRequest,
    ) -> Result<ExportTraceServiceResponse, RouterError> {
        self.received_traces.fetch_add(1, Ordering::Relaxed);
        Ok(ExportTraceServiceResponse {
            partial_success: None,
        })
    }
}
