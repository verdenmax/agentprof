//! Dual-path data source.
//!
//! Fans out [`SessionDataSource`] calls to a primary file
//! [`agentprof_core::adapter::Adapter`]
//! **and** an optional `SQLite` store, merges the discovered refs by
//! session id, and records per-session warnings whenever the two
//! sources disagree on metadata.
//!
//! ## Conflict policy — **adapter wins**
//!
//! When the same `session_id` appears in both the adapter (live file
//! system) and the storage (previously-ingested snapshot), the adapter
//! is treated as the **source of truth**. The dual-path source returns
//! the adapter's [`SessionRef`] and emits a [`DualPathWarning`]
//! listing the diverging fields so the CLI can surface them on stderr.
//!
//! Storage-only sessions (id not seen by the adapter — likely because
//! the on-disk log was rotated away) are preserved and returned as-is.
//!
//! See `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`
//! §3.2 for the design rationale.
//!
//! ## Adapter-name aliasing
//!
//! [`SessionRef`] exists in two namespaces inside `agentprof-core`:
//! [`agentprof_core::adapter::SessionRef`] (M1.x file discovery) and
//! [`agentprof_core::datasource::SessionRef`] (M2.1 cross-source
//! abstraction). This module deals exclusively in the **datasource**
//! variant.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::datasource::{DataSourceError, SessionDataSource, SessionRef};

/// Composer that queries an [`Adapter`]-backed
/// [`SessionDataSource`] **and** (optionally) a `SQLite`-backed
/// [`SessionDataSource`], merging their results.
///
/// Both inner sources are stored as boxed trait objects so the
/// composer is concrete (avoids generic-over-two-types signatures in
/// the CLI).
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use std::path::PathBuf;
/// use agentprof_adapters::{AdapterDataSource, copilot::CopilotAdapter};
/// use agentprof_core::datasource::SessionDataSource;
/// use agentprof_cli::data_source::DualPathDataSource;
///
/// let adapter = AdapterDataSource::new(Arc::new(CopilotAdapter), PathBuf::from("/tmp/none"));
/// let dual = DualPathDataSource::new(Box::new(adapter), None);
/// assert_eq!(dual.name(), "dual");
/// ```
///
/// [`Adapter`]: agentprof_core::adapter::Adapter
pub struct DualPathDataSource {
    adapter: Box<dyn SessionDataSource>,
    storage: Option<Box<dyn SessionDataSource>>,
    warnings: Mutex<Vec<DualPathWarning>>,
}

/// A single metadata divergence detected between the adapter and the
/// storage for one session id.
///
/// Emitted by [`DualPathDataSource::discover`] whenever the internal
/// `diff_fields` helper returns a non-empty list. The CLI drains
/// these via [`DualPathDataSource::drain_warnings`] after a
/// `discover` call and surfaces them on stderr (`agentprof list` /
/// `aggregate`).
///
/// `#[non_exhaustive]` so future fields (e.g. `divergence_kind`) can
/// be added without a major bump.
///
/// # Examples
///
/// ```
/// use agentprof_cli::data_source::DualPathWarning;
/// // Warnings are produced internally by `DualPathDataSource::discover`
/// // and drained via `DualPathDataSource::drain_warnings`. The shape:
/// fn describe(w: &DualPathWarning) -> String {
///     format!("{}: {:?}", w.session_id, w.differing_fields)
/// }
/// # let _ = describe;
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DualPathWarning {
    /// Session id whose adapter/storage views disagree.
    pub session_id: String,
    /// Names of the [`SessionRef`] fields that differ. Currently one of
    /// `"raw_mtime_ms"`, `"started_at_ms"`, `"raw_path"`.
    pub differing_fields: Vec<&'static str>,
    /// Always `true` in M2.1 — the adapter is the source of truth.
    /// Reserved for a future policy knob.
    pub adapter_won: bool,
}

impl DualPathDataSource {
    /// Construct a new dual-path source.
    ///
    /// `adapter` is required; `storage` is optional — passing `None`
    /// degrades the composer to a pass-through of `adapter` (still
    /// `name() == "dual"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::path::PathBuf;
    /// use agentprof_adapters::{AdapterDataSource, copilot::CopilotAdapter};
    /// use agentprof_cli::data_source::DualPathDataSource;
    ///
    /// let adapter = AdapterDataSource::new(Arc::new(CopilotAdapter), PathBuf::from("/tmp/none"));
    /// let dual = DualPathDataSource::new(Box::new(adapter), None);
    /// # let _ = dual;
    /// ```
    #[must_use]
    pub fn new(
        adapter: Box<dyn SessionDataSource>,
        storage: Option<Box<dyn SessionDataSource>>,
    ) -> Self {
        Self {
            adapter,
            storage,
            warnings: Mutex::new(Vec::new()),
        }
    }

    /// Drain accumulated [`DualPathWarning`]s, returning them and
    /// leaving the internal buffer empty.
    ///
    /// Intended to be called by the CLI after a `discover()` round so
    /// that warnings can be surfaced on stderr exactly once.
    ///
    /// Recovers gracefully from a poisoned internal mutex (returns
    /// the inner buffer in whatever state it was left in) — does not
    /// panic.
    #[must_use]
    pub fn drain_warnings(&self) -> Vec<DualPathWarning> {
        let mut guard = self
            .warnings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }
}

impl SessionDataSource for DualPathDataSource {
    fn name(&self) -> &'static str {
        "dual"
    }

    fn discover(&self, since: Duration) -> Result<Vec<SessionRef>, DataSourceError> {
        let adapter_refs = self.adapter.discover(since)?;
        let Some(storage) = self.storage.as_deref() else {
            return Ok(adapter_refs);
        };
        let storage_refs = storage.discover(since)?;
        Ok(merge_refs(adapter_refs, storage_refs, &self.warnings))
    }

    fn load_session(&self, id: &str) -> Result<AnalysisReport, DataSourceError> {
        if let Some(storage) = self.storage.as_deref() {
            match storage.load_session(id) {
                Ok(rep) => return Ok(rep),
                Err(DataSourceError::NotFound { .. }) => {
                    // fall through to adapter
                }
                Err(e) => return Err(e),
            }
        }
        self.adapter.load_session(id)
    }
}

/// Merge `adapter_refs` over `storage_refs` by session id.
///
/// Algorithm:
///
/// 1. Seed a `HashMap<id, SessionRef>` with every storage entry.
/// 2. For each adapter entry, look up the same id in the map.
///    - **Match found**: compute `diff_fields`. If non-empty, push a
///      [`DualPathWarning`] (`adapter_won = true`). Overwrite the map
///      entry with the adapter ref.
///    - **No match**: insert the adapter ref.
/// 3. Collect the map values and sort by `started_at_ms` descending
///    (entries with `None` sort last).
///
/// The function is free-standing (not a method) so it can be unit-
/// tested without constructing a full `DualPathDataSource`.
fn merge_refs(
    adapter_refs: Vec<SessionRef>,
    storage_refs: Vec<SessionRef>,
    warnings: &Mutex<Vec<DualPathWarning>>,
) -> Vec<SessionRef> {
    let mut by_id: HashMap<String, SessionRef> =
        HashMap::with_capacity(adapter_refs.len().saturating_add(storage_refs.len()));
    for sref in storage_refs {
        by_id.insert(sref.id.clone(), sref);
    }
    for adapter_ref in adapter_refs {
        if let Some(existing) = by_id.get(&adapter_ref.id) {
            let diffs = diff_fields(&adapter_ref, existing);
            if !diffs.is_empty() {
                let mut guard = warnings
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.push(DualPathWarning {
                    session_id: adapter_ref.id.clone(),
                    differing_fields: diffs,
                    adapter_won: true,
                });
            }
        }
        by_id.insert(adapter_ref.id.clone(), adapter_ref);
    }
    let mut out: Vec<SessionRef> = by_id.into_values().collect();
    out.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
    out
}

/// Compare metadata fields between two [`SessionRef`]s and return the
/// names of those that disagree.
///
/// Compared fields: `raw_mtime_ms`, `started_at_ms`, `raw_path`. The
/// `id`, `agent`, and `source` fields are intentionally excluded
/// (`id` is the join key; `agent` would indicate a corrupt store;
/// `source` is expected to differ — that is the whole point).
fn diff_fields(a: &SessionRef, b: &SessionRef) -> Vec<&'static str> {
    let mut diffs = Vec::new();
    if a.raw_mtime_ms != b.raw_mtime_ms {
        diffs.push("raw_mtime_ms");
    }
    if a.started_at_ms != b.started_at_ms {
        diffs.push("started_at_ms");
    }
    if a.raw_path != b.raw_path {
        diffs.push("raw_path");
    }
    diffs
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentprof_core::adapter::AgentKind;

    fn make_ref(id: &str, mtime: Option<i64>, source: &'static str) -> SessionRef {
        SessionRef::new(
            id.to_string(),
            AgentKind::Copilot,
            mtime,
            None,
            mtime,
            source,
        )
    }

    #[test]
    fn diff_fields_detects_mtime_change() {
        let a = make_ref("s1", Some(2000), "adapter:copilot");
        let b = make_ref("s1", Some(1000), "sqlite");
        let diffs = diff_fields(&a, &b);
        assert!(diffs.contains(&"raw_mtime_ms"));
        assert!(diffs.contains(&"started_at_ms"));
    }

    #[test]
    fn diff_fields_empty_for_identical() {
        let a = make_ref("s1", Some(1000), "adapter:copilot");
        let b = make_ref("s1", Some(1000), "sqlite");
        assert!(diff_fields(&a, &b).is_empty());
    }

    #[test]
    fn merge_refs_sorts_desc() {
        let warnings = Mutex::new(Vec::new());
        let adapter = vec![
            make_ref("a", Some(100), "adapter:copilot"),
            make_ref("b", Some(300), "adapter:copilot"),
        ];
        let storage = vec![make_ref("c", Some(200), "sqlite")];
        let merged = merge_refs(adapter, storage, &warnings);
        let ids: Vec<&str> = merged.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }
}
