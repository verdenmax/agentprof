//! [`AdapterDataSource`] — bridge from per-agent [`Adapter`] impls into the
//! cross-cutting [`SessionDataSource`] abstraction defined in
//! `agentprof-core`.
//!
//! This wrapper lets the dual-path composer (M2.1 T4) treat file adapters
//! and the `SQLite` store under one trait. Lifting the bridge into this
//! crate (rather than the CLI) keeps lib/bin separation per
//! `docs/architecture.md` §3.
//!
//! ## Pipeline shape
//!
//! Each [`SessionDataSource::load_session`] call runs the full
//! adapter pipeline that the CLI commands already use:
//!
//! ```text
//! Adapter::discover_sessions  →  (filter by id)
//!   →  Adapter::load_session  →  episode::derive_episodes
//!   →  analyzer::analyze      →  AnalysisReport
//! ```
//!
//! No new methods are added to the [`Adapter`] trait (per ADR-pending T3.1
//! review — keep the trait minimal so existing impls do not need to
//! change).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use agentprof_core::adapter::{Adapter, AdapterError, AgentKind, SessionRef as AdapterRef};
use agentprof_core::analyzer::analyze;
use agentprof_core::datasource::{DataSourceError, SessionDataSource, SessionRef as DataSourceRef};
use agentprof_core::episode::derive_episodes;

/// Wraps any [`Adapter`] implementation so it can be consumed via the
/// generic [`SessionDataSource`] trait.
///
/// Owns the on-disk session root because the [`Adapter`] trait itself is
/// stateless about location (it takes `root: &Path` per call). The CLI
/// resolves the root once (config / `--path` override / default) and hands
/// it to the wrapper at construction time.
///
/// `A` must be `Send + Sync + 'static` so the wrapper can be wrapped in
/// `Arc<dyn SessionDataSource>` and shared across blocking tasks.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use std::path::PathBuf;
/// use agentprof_adapters::{AdapterDataSource, copilot::CopilotAdapter};
/// use agentprof_core::datasource::SessionDataSource;
///
/// let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), PathBuf::from("/tmp/none"));
/// assert_eq!(ds.name(), "adapter:copilot");
/// ```
#[derive(Debug, Clone)]
pub struct AdapterDataSource<A: Adapter> {
    adapter: Arc<A>,
    root: PathBuf,
    name: &'static str,
}

impl<A: Adapter> AdapterDataSource<A> {
    /// Construct a new `AdapterDataSource` from an [`Adapter`] and the
    /// on-disk session root to scan.
    ///
    /// The `name()` returned by the resulting [`SessionDataSource`] is
    /// derived from `adapter.agent_kind()`:
    ///
    /// | `AgentKind` | `name()`           |
    /// |-------------|--------------------|
    /// | `Copilot`   | `"adapter:copilot"` |
    /// | `Claude`    | `"adapter:claude"`  |
    /// | `Codex`     | `"adapter:codex"`   |
    /// | _other_     | `"adapter:unknown"` |
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::path::PathBuf;
    /// use agentprof_adapters::{AdapterDataSource, copilot::CopilotAdapter};
    /// use agentprof_core::datasource::SessionDataSource;
    ///
    /// let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), PathBuf::from("/tmp/x"));
    /// assert_eq!(ds.name(), "adapter:copilot");
    /// ```
    #[must_use]
    pub fn new(adapter: Arc<A>, root: PathBuf) -> Self {
        let name = name_for(adapter.agent_kind());
        Self {
            adapter,
            root,
            name,
        }
    }

    /// On-disk session root this wrapper scans.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::path::PathBuf;
    /// use agentprof_adapters::{AdapterDataSource, copilot::CopilotAdapter};
    ///
    /// let root = PathBuf::from("/tmp/example");
    /// let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), root.clone());
    /// assert_eq!(ds.root(), root.as_path());
    /// ```
    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        self.root.as_path()
    }

    /// Load + analyze a single session given a pre-built [`AdapterRef`].
    ///
    /// Identical to [`SessionDataSource::load_session`] **except** it
    /// skips the up-front `discover_sessions` scan: the caller is
    /// expected to have obtained `sref` from a previous `discover`
    /// (cheap; one pass over the root) and now just wants to run the
    /// load → derive → analyze pipeline on that one session.
    ///
    /// This avoids the O(N²) hot loop in `db ingest --all` where the
    /// naïve trait route re-scans the entire root for every session
    /// id (M2.1 audit P1-3): for 100 sessions that meant 10,000
    /// first-line reads instead of 100. The CLI's ingest loop already
    /// has the [`AdapterRef`] in hand from its single up-front
    /// `discover` call, so it can pay the per-session cost exactly
    /// once via this method.
    ///
    /// The dual-path read code path (`SessionDataSource::load_session`)
    /// is **not** affected — there N is small (one session at a time)
    /// and re-discover is a deliberate freshness check.
    ///
    /// # Errors
    ///
    /// - [`DataSourceError::Adapter`] if the underlying adapter's
    ///   `load_session` fails (I/O, malformed JSONL beyond recovery).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use std::path::PathBuf;
    /// use agentprof_adapters::{AdapterDataSource, copilot::CopilotAdapter};
    /// use agentprof_core::adapter::Adapter as _;
    /// use agentprof_core::datasource::SessionDataSource;
    ///
    /// let ds = AdapterDataSource::new(
    ///     Arc::new(CopilotAdapter),
    ///     PathBuf::from("/home/me/.copilot/session-state"),
    /// );
    /// let refs = CopilotAdapter
    ///     .discover_sessions(ds.root())
    ///     .expect("discover");
    /// for sref in &refs {
    ///     let report = ds.load_session_by_ref(sref).expect("load");
    ///     let _ = report;
    /// }
    /// ```
    pub fn load_session_by_ref(
        &self,
        sref: &AdapterRef,
    ) -> Result<agentprof_core::analyzer::AnalysisReport, DataSourceError> {
        let raw = self
            .adapter
            .load_session(sref)
            .map_err(|e| wrap_adapter_err(self.name, e))?;
        let episodes = derive_episodes(&raw.events, &raw.meta);
        let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
        Ok(report)
    }

    /// Load `Episodes` given a pre-built [`AdapterRef`], skipping the
    /// `discover_sessions` scan. Symmetric to [`Self::load_session_by_ref`]
    /// for the episodes side; used by `cmd::db::ingest` to avoid the
    /// O(N²) hot loop on `db ingest --all`.
    ///
    /// # Errors
    ///
    /// - [`DataSourceError::Adapter`] if `Adapter::load_session(&sref)`
    ///   fails (I/O, malformed JSONL beyond recovery).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use std::path::PathBuf;
    /// use agentprof_adapters::{AdapterDataSource, copilot::CopilotAdapter};
    /// use agentprof_core::adapter::Adapter as _;
    ///
    /// let ds = AdapterDataSource::new(
    ///     Arc::new(CopilotAdapter),
    ///     PathBuf::from("/home/me/.copilot/session-state"),
    /// );
    /// let refs = CopilotAdapter.discover_sessions(ds.root()).expect("discover");
    /// for sref in &refs {
    ///     let _eps = ds.load_episodes_by_ref(sref).expect("load");
    /// }
    /// ```
    pub fn load_episodes_by_ref(
        &self,
        sref: &AdapterRef,
    ) -> Result<agentprof_core::episode::Episodes, DataSourceError> {
        let raw = self
            .adapter
            .load_session(sref)
            .map_err(|e| wrap_adapter_err(self.name, e))?;
        Ok(derive_episodes(&raw.events, &raw.meta))
    }
}

const fn name_for(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Copilot => "adapter:copilot",
        AgentKind::Claude => "adapter:claude",
        AgentKind::Codex => "adapter:codex",
        // `AgentKind` is `#[non_exhaustive]`; future variants fall back to a
        // generic label until a dedicated mapping is added.
        _ => "adapter:unknown",
    }
}

fn wrap_adapter_err(name: &'static str, err: AdapterError) -> DataSourceError {
    DataSourceError::Adapter {
        source: name,
        underlying: Box::new(err),
    }
}

fn system_time_to_ms(t: SystemTime) -> Option<i64> {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

fn adapter_ref_to_datasource_ref(sref: &AdapterRef, source_name: &'static str) -> DataSourceRef {
    // Re-read the first event line to extract the session's logical start
    // time so consumer ordering (e.g. `list` newest-first) is independent
    // of filesystem mtime, which varies across checkouts on CI runners
    // and was the root cause of M2.1 snapshot test flakiness on Win/Mac/Linux.
    // Cost: one extra BufReader::read_line per session (sub-millisecond);
    // for the typical 100-session workload this totals <100ms total.
    let started_at_ms =
        crate::copilot::paths::extract_session_start_ms_from_first_event(&sref.path);
    DataSourceRef::new(
        sref.id.clone(),
        sref.agent,
        started_at_ms,
        Some(sref.path.clone()),
        system_time_to_ms(sref.modified_at),
        source_name,
    )
}

impl<A: Adapter + 'static> SessionDataSource for AdapterDataSource<A> {
    fn name(&self) -> &'static str {
        self.name
    }

    fn discover(&self, since: Duration) -> Result<Vec<DataSourceRef>, DataSourceError> {
        let all = self
            .adapter
            .discover_sessions(self.root.as_path())
            .map_err(|e| wrap_adapter_err(self.name, e))?;
        let cutoff = SystemTime::now().checked_sub(since);
        let out = all
            .into_iter()
            .filter(|sref| cutoff.map_or(true, |c| sref.modified_at >= c))
            .map(|sref| adapter_ref_to_datasource_ref(&sref, self.name))
            .collect();
        Ok(out)
    }

    fn load_session(
        &self,
        id: &str,
    ) -> Result<agentprof_core::analyzer::AnalysisReport, DataSourceError> {
        let all = self
            .adapter
            .discover_sessions(self.root.as_path())
            .map_err(|e| wrap_adapter_err(self.name, e))?;
        let sref = all
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| DataSourceError::NotFound { id: id.to_owned() })?;
        let raw = self
            .adapter
            .load_session(&sref)
            .map_err(|e| wrap_adapter_err(self.name, e))?;
        let episodes = derive_episodes(&raw.events, &raw.meta);
        let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
        Ok(report)
    }

    fn load_episodes(
        &self,
        id: &str,
    ) -> Result<agentprof_core::episode::Episodes, DataSourceError> {
        let all = self
            .adapter
            .discover_sessions(self.root.as_path())
            .map_err(|e| wrap_adapter_err(self.name, e))?;
        let sref = all
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| DataSourceError::NotFound { id: id.to_owned() })?;
        let raw = self
            .adapter
            .load_session(&sref)
            .map_err(|e| wrap_adapter_err(self.name, e))?;
        Ok(derive_episodes(&raw.events, &raw.meta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_for_known_kinds() {
        assert_eq!(name_for(AgentKind::Copilot), "adapter:copilot");
        assert_eq!(name_for(AgentKind::Claude), "adapter:claude");
        assert_eq!(name_for(AgentKind::Codex), "adapter:codex");
    }

    #[test]
    fn system_time_to_ms_unix_epoch_is_zero() {
        assert_eq!(system_time_to_ms(SystemTime::UNIX_EPOCH), Some(0));
    }
}
