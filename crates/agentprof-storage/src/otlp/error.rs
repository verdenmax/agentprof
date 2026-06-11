//! Error types for the OTLP receiver subsystem (M2.2).
//!
//! Three error families partition the OTLP lifecycle:
//!
//! - [`OtlpServerError`] — top-level: bind / TLS init / transport lifecycle.
//!   Returned by listener-start APIs in later M2.2 tasks.
//! - [`MapperError`] — per-event, **recoverable**: a malformed OTLP signal is
//!   collected into a warnings vector and the surrounding session keeps
//!   ingesting. `#[derive(Clone)]` so warnings can be batched into
//!   `Vec<MapperError>` for later surfacing.
//! - [`RouterError`] — per-session-router: persistence failures + mapper
//!   propagation + buffer-OOM tripwire.
//!
//! Per the workspace error-model rule (`docs/architecture.md` §16 / iron rule
//! #1), this crate is a library and uses [`thiserror`] exclusively.
//!
//! # Examples
//!
//! ```
//! use agentprof_storage::otlp::error::MapperError;
//! let w = MapperError::MissingResourceAttr { name: "session.id" };
//! assert!(w.to_string().contains("session.id"));
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;

/// Top-level error for OTLP server lifecycle (bind / TLS init / transport).
///
/// Returned by the listener-start APIs introduced in M2.2 T2.2 (gRPC) and
/// T3.1 (HTTP). The variant set is `#[non_exhaustive]` so future failure
/// kinds (e.g. proxy-protocol parse errors) can be added without breaking
/// downstream matchers.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::error::OtlpServerError;
/// let e = OtlpServerError::Config("both listeners disabled".into());
/// assert!(e.to_string().contains("config validation"));
/// ```
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum OtlpServerError {
    /// Failed to bind on the configured listener address.
    #[error("bind error on {addr}: {source}")]
    Bind {
        /// Address we tried to bind on.
        addr: SocketAddr,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },
    /// TLS configuration (cert / key load + parse) failed.
    #[error("tls config error: {message}")]
    TlsConfig {
        /// Human-readable diagnostic.
        message: String,
    },
    /// Tonic transport-level error (server lifecycle).
    #[error("tonic transport error: {0}")]
    Tonic(#[from] tonic::transport::Error),
    /// Hyper / axum HTTP server error.
    #[error("http server error: {0}")]
    Http(String),
    /// I/O error reading a config-supplied PEM file.
    #[error("io error at {path}: {source}")]
    Io {
        /// File path.
        path: PathBuf,
        /// Underlying `io::Error`.
        #[source]
        source: std::io::Error,
    },
    /// Config validation rejected the resolved settings.
    #[error("config validation: {0}")]
    Config(String),
    /// Internal lifecycle failure not attributable to user config or I/O.
    ///
    /// Currently emitted by [`crate::otlp::sweeper::SweeperHandle::shutdown`]
    /// when the background sweeper task panics or is aborted before the
    /// `JoinHandle` resolves cleanly — neither should happen in normal
    /// operation, but the variant gives the shutdown path a typed escape
    /// hatch rather than swallowing the failure.
    #[error("internal otlp lifecycle error: {0}")]
    Internal(String),
}

/// Errors raised by the OTLP → `TypedEvent` mapper (per-event, recoverable).
///
/// These are collected into a `Vec<MapperError>` warnings buffer by the
/// per-session router (M2.2 T6.1+); a single bad envelope must not stop
/// the ingest. `#[derive(Clone)]` lets callers stash, clone, and report.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::error::MapperError;
/// let e = MapperError::BadTimestamp("nanos overflow".into());
/// assert!(e.to_string().contains("invalid timestamp"));
/// ```
#[derive(thiserror::Error, Debug, Clone)]
#[non_exhaustive]
pub enum MapperError {
    /// Required resource attribute missing from the OTLP envelope.
    #[error("missing required resource attribute: {name}")]
    MissingResourceAttr {
        /// Attribute name we looked for (e.g. `"session.id"`).
        name: &'static str,
    },
    /// Timestamp field couldn't be parsed.
    #[error("invalid timestamp: {0}")]
    BadTimestamp(String),
    /// Recognized signal kind but unknown event name (e.g. unrecognized `claude_code.*`).
    #[error("unknown event name: {0}")]
    UnknownEventName(String),
    /// Recognized event name but payload shape mismatched.
    #[error("payload shape mismatch for {event_name}: {message}")]
    PayloadMismatch {
        /// The event name we were parsing.
        event_name: String,
        /// What went wrong.
        message: String,
    },
    /// `session.id` exceeded the 256-byte cap (ADR-0022 D-5).
    ///
    /// The mapper rejects the offending record before any router buffer is
    /// allocated so an attacker cannot consume memory via pathologically
    /// long session ids.
    #[error("session.id too long on {signal:?} signal: {len} bytes (cap 256)")]
    SessionIdTooLong {
        /// Which OTLP signal carried the offending record.
        signal: crate::otlp::typed::SignalKind,
        /// Actual byte length of the rejected session id.
        len: usize,
    },
}

/// Errors raised by the per-session router during ingest / flush.
///
/// Propagates storage failures and mapper failures up to the listener layer
/// and trips a dedicated [`RouterError::BufferOom`] variant when a session
/// buffer exceeds its byte / event ceiling before a flush completes.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::error::RouterError;
/// let e = RouterError::BufferOom {
///     session_id: "sess-1".into(),
///     bytes: 1 << 20,
///     events: 4096,
/// };
/// assert!(e.to_string().contains("OOM"));
/// ```
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum RouterError {
    /// Persistence layer error during a buffer flush.
    #[error("storage error: {0}")]
    Storage(#[from] crate::SqliteError),
    /// Mapper error encountered while routing an event.
    #[error("mapper error: {0}")]
    Mapper(#[from] MapperError),
    /// Buffer hit OOM cap before flush completed.
    #[error("session {session_id} buffer hit OOM cap (bytes={bytes}, events={events})")]
    BufferOom {
        /// The offending session.
        session_id: String,
        /// Estimated bytes at OOM trip.
        bytes: usize,
        /// Event count at OOM trip.
        events: usize,
    },
}
