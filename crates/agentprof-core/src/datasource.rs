//! Abstraction over session data sources: file adapters, `SQLite` cache/store,
//! and the dual-path composer (cli only).
//!
//! Symmetric to the existing [`crate::adapter::Adapter`] trait: where
//! `Adapter` knows how to turn a single on-disk session log into an
//! [`crate::analyzer::AnalysisReport`], [`SessionDataSource`] is the
//! higher-level "where do reports come from?" abstraction — it can wrap
//! a file adapter, a `SQLite` store, an OTLP receiver (M2.2), or a
//! composer that fans out to several at once.
//!
//! See `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`
//! §3.2 for the design rationale; ADR-0017 (drafted in M2.1 T8.1)
//! captures the decision record.

use std::path::PathBuf;
use std::time::Duration;

use crate::adapter::AgentKind;
use crate::analyzer::AnalysisReport;

/// A source of analyzed session data.
///
/// Implemented by file adapters (parsing `events.jsonl`), the `SQLite`
/// cache/store, and the dual-path composer that fans out to both.
///
/// Implementors **must** be `Send + Sync` so that the CLI may share a
/// single instance across blocking tasks.
///
/// # Examples
///
/// ```ignore
/// use agentprof_core::datasource::SessionDataSource;
/// fn discover_recent(src: &dyn SessionDataSource) {
///     let refs = src.discover(std::time::Duration::from_secs(7 * 86_400));
///     println!("{} sessions in last 7d", refs.map(|v| v.len()).unwrap_or(0));
/// }
/// ```
///
/// # Errors
///
/// Each method documents the [`DataSourceError`] variants it may return.
pub trait SessionDataSource: Send + Sync {
    /// Human-readable name for warnings and traces
    /// (e.g. `"adapter:copilot"`, `"sqlite"`, `"dual"`).
    fn name(&self) -> &'static str;

    /// Enumerate sessions modified within the time window.
    ///
    /// Cheap; must **not** load full reports. Returns lightweight
    /// [`SessionRef`] summaries.
    ///
    /// # Errors
    ///
    /// Returns [`DataSourceError::Adapter`] or [`DataSourceError::Storage`]
    /// depending on the implementor's backing surface.
    fn discover(&self, since: Duration) -> Result<Vec<SessionRef>, DataSourceError>;

    /// Load full [`AnalysisReport`] for a single session id.
    ///
    /// # Errors
    ///
    /// - [`DataSourceError::NotFound`] if the id is unknown to this source.
    /// - [`DataSourceError::Adapter`] / [`DataSourceError::Storage`] when
    ///   the underlying parse or query fails.
    fn load_session(&self, id: &str) -> Result<AnalysisReport, DataSourceError>;
}

/// Lightweight summary of a session — used by
/// [`SessionDataSource::discover`] to avoid full-report load on listing
/// operations.
///
/// All metadata fields are optional so that store-only entries (no
/// originating file path) and adapters that cannot determine a
/// `started_at` cheaply can still emit refs.
///
/// # Examples
///
/// Field shape (constructed from inside `agentprof-core` or via a
/// future builder — `#[non_exhaustive]` forbids cross-crate struct
/// literals):
///
/// ```ignore
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::datasource::SessionRef;
///
/// let r = SessionRef {
///     id: "abc".into(),
///     agent: AgentKind::Copilot,
///     started_at_ms: None,
///     raw_path: None,
///     raw_mtime_ms: None,
///     source: "adapter:copilot",
/// };
/// assert_eq!(r.agent, AgentKind::Copilot);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    /// Stable session identifier (e.g. Copilot uuid).
    pub id: String,
    /// Which agent produced this session.
    pub agent: AgentKind,
    /// Unix epoch (ms) when the session began. `None` if unknown.
    pub started_at_ms: Option<i64>,
    /// Original source path (`events.jsonl`) — `None` for store-only entries.
    pub raw_path: Option<PathBuf>,
    /// Source file mtime (ms) — used by dual-path freshness compare.
    pub raw_mtime_ms: Option<i64>,
    /// Where this ref came from (`"adapter:<name>"` / `"sqlite"` / `"dual"`).
    pub source: &'static str,
}

impl SessionRef {
    /// Construct a new [`SessionRef`] from its raw fields.
    ///
    /// Cross-crate callers (`agentprof-storage`, `agentprof-adapters`)
    /// must go through this constructor because [`SessionRef`] is
    /// `#[non_exhaustive]` and direct struct-literal construction is
    /// forbidden outside this crate.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::datasource::SessionRef;
    ///
    /// let r = SessionRef::new(
    ///     "s1".into(),
    ///     AgentKind::Copilot,
    ///     Some(1_700_000_000_000),
    ///     None,
    ///     None,
    ///     "adapter:copilot",
    /// );
    /// assert_eq!(r.id, "s1");
    /// ```
    #[must_use]
    pub const fn new(
        id: String,
        agent: AgentKind,
        started_at_ms: Option<i64>,
        raw_path: Option<PathBuf>,
        raw_mtime_ms: Option<i64>,
        source: &'static str,
    ) -> Self {
        Self {
            id,
            agent,
            started_at_ms,
            raw_path,
            raw_mtime_ms,
            source,
        }
    }
}

/// Errors returned by [`SessionDataSource`] implementations.
///
/// # Examples
///
/// ```
/// use agentprof_core::datasource::DataSourceError;
/// let e = DataSourceError::NotFound { id: "abc".into() };
/// assert_eq!(e.to_string(), "session not found: abc");
/// ```
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum DataSourceError {
    /// The given session id was not found in this source.
    #[error("session not found: {id}")]
    NotFound {
        /// Session id that was requested.
        id: String,
    },

    /// An underlying adapter (file-system) error.
    #[error("adapter error in {source}: {underlying}")]
    Adapter {
        /// Symbolic source name (matches [`SessionDataSource::name`]).
        source: &'static str,
        /// The wrapped error.
        #[source]
        underlying: Box<dyn std::error::Error + Send + Sync>,
    },

    /// An underlying storage (`SQLite`) error.
    #[error("storage error in {source}: {underlying}")]
    Storage {
        /// Symbolic source name (matches [`SessionDataSource::name`]).
        source: &'static str,
        /// The wrapped error.
        #[source]
        underlying: Box<dyn std::error::Error + Send + Sync>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ref_eq_and_clone() {
        let r1 = SessionRef {
            id: "s1".into(),
            agent: AgentKind::Copilot,
            started_at_ms: None,
            raw_path: None,
            raw_mtime_ms: None,
            source: "test",
        };
        let r2 = r1.clone();
        assert_eq!(r1, r2);
    }

    #[test]
    fn datasource_error_not_found_display() {
        let e = DataSourceError::NotFound { id: "x".into() };
        assert_eq!(e.to_string(), "session not found: x");
    }
}
