//! Integration tests for [`agentprof_cli::data_source::DualPathDataSource`].
//!
//! These tests exercise the composer with stub [`SessionDataSource`]
//! impls (no on-disk fixtures, no `SQLite`) to validate:
//!
//! 1. `name() == "dual"` regardless of inner source composition.
//! 2. With `storage = None`, discover passes the adapter result
//!    through unchanged (degenerate dual-path).
//! 3. When adapter and storage disagree on `raw_mtime_ms`, the merged
//!    output uses the adapter's value AND a warning is recorded.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Mutex;
use std::time::Duration;

use agentprof_cli::data_source::DualPathDataSource;
use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::datasource::{DataSourceError, SessionDataSource, SessionRef};

/// Minimal stub: replays a fixed list of refs and never serves loads.
struct Fake {
    name: &'static str,
    refs: Mutex<Vec<SessionRef>>,
}

impl Fake {
    const fn new(name: &'static str, refs: Vec<SessionRef>) -> Self {
        Self {
            name,
            refs: Mutex::new(refs),
        }
    }
}

impl SessionDataSource for Fake {
    fn name(&self) -> &'static str {
        self.name
    }

    fn discover(&self, _since: Duration) -> Result<Vec<SessionRef>, DataSourceError> {
        Ok(self.refs.lock().unwrap().clone())
    }

    fn load_session(&self, id: &str) -> Result<AnalysisReport, DataSourceError> {
        Err(DataSourceError::NotFound { id: id.to_string() })
    }

    fn load_episodes(
        &self,
        id: &str,
    ) -> Result<agentprof_core::episode::Episodes, DataSourceError> {
        Err(DataSourceError::NotFound { id: id.to_string() })
    }
}

#[test]
fn name_is_dual() {
    let adapter = Fake::new("adapter:copilot", vec![]);
    let dual = DualPathDataSource::new(Box::new(adapter), None);
    assert_eq!(dual.name(), "dual");
}

#[test]
fn single_path_falls_back_when_no_storage() {
    let adapter = Fake::new("adapter:copilot", vec![]);
    let dual = DualPathDataSource::new(Box::new(adapter), None);
    let refs = dual
        .discover(Duration::from_secs(86_400))
        .expect("discover");
    assert!(refs.is_empty());
    assert!(dual.drain_warnings().is_empty());
}

#[test]
fn merge_records_warning_on_diverging_mtime() {
    let storage_ref = SessionRef::new(
        "shared-id".to_string(),
        AgentKind::Copilot,
        Some(1000),
        None,
        Some(1000),
        "sqlite",
    );
    let adapter_ref = SessionRef::new(
        "shared-id".to_string(),
        AgentKind::Copilot,
        Some(2000),
        None,
        Some(2000),
        "adapter:copilot",
    );

    let adapter = Fake::new("adapter:copilot", vec![adapter_ref]);
    let storage = Fake::new("sqlite", vec![storage_ref]);
    let dual = DualPathDataSource::new(Box::new(adapter), Some(Box::new(storage)));

    let refs = dual
        .discover(Duration::from_secs(86_400))
        .expect("discover");
    assert_eq!(refs.len(), 1);
    let merged = &refs[0];
    assert_eq!(merged.id, "shared-id");
    assert_eq!(merged.raw_mtime_ms, Some(2000), "adapter must win");
    assert_eq!(merged.source, "adapter:copilot");

    let warns = dual.drain_warnings();
    assert_eq!(warns.len(), 1, "exactly one divergence warning");
    let w = &warns[0];
    assert_eq!(w.session_id, "shared-id");
    assert!(w.adapter_won);
    assert!(w.differing_fields.contains(&"raw_mtime_ms"));
    assert!(w.differing_fields.contains(&"started_at_ms"));

    // drain is idempotent — second call is empty
    assert!(dual.drain_warnings().is_empty());
}

#[test]
fn diff_fields_treats_one_side_none_as_no_opinion() {
    // Regression guard for the M2.1 audit M2 finding: when one side
    // (typically the adapter on a fresh discover) hasn't computed a
    // value yet, the diff must treat that as "no opinion" rather
    // than disagreement. Otherwise every fresh scan would emit a
    // spurious divergence warning.
    //
    // We exercise diff_fields indirectly through the DualPathDataSource
    // pipeline because diff_fields itself is module-private.
    let storage_ref = SessionRef::new(
        "shared-id".to_string(),
        AgentKind::Copilot,
        Some(5000), // storage has a real started_at_ms
        None,
        Some(5000), // and a real raw_mtime_ms
        "sqlite",
    );
    let adapter_ref = SessionRef::new(
        "shared-id".to_string(),
        AgentKind::Copilot,
        None, // adapter hasn't bothered to parse startTime
        None,
        None, // …and not bothered to stat() the file either
        "adapter:copilot",
    );

    let adapter = Fake::new("adapter:copilot", vec![adapter_ref]);
    let storage = Fake::new("sqlite", vec![storage_ref]);
    let dual = DualPathDataSource::new(Box::new(adapter), Some(Box::new(storage)));

    let refs = dual
        .discover(Duration::from_secs(86_400))
        .expect("discover");
    assert_eq!(refs.len(), 1, "merge dedupes by id");

    let warns = dual.drain_warnings();
    assert!(
        warns.is_empty(),
        "None on either side must NOT trigger a divergence warning, got: {warns:?}"
    );
}

/// Fake with configurable `load_episodes` behavior, for T4.1 (M2.1.1).
struct EpFake {
    name: &'static str,
    episodes: Option<agentprof_core::episode::Episodes>,
}

impl SessionDataSource for EpFake {
    fn name(&self) -> &'static str {
        self.name
    }

    fn discover(&self, _since: Duration) -> Result<Vec<SessionRef>, DataSourceError> {
        Ok(vec![])
    }

    fn load_session(&self, _id: &str) -> Result<AnalysisReport, DataSourceError> {
        Err(DataSourceError::NotFound { id: "x".into() })
    }

    fn load_episodes(
        &self,
        id: &str,
    ) -> Result<agentprof_core::episode::Episodes, DataSourceError> {
        self.episodes.as_ref().map_or_else(
            || Err(DataSourceError::NotFound { id: id.to_owned() }),
            |e| Ok(e.clone()),
        )
    }
}

#[test]
fn dualpath_load_episodes_tries_storage_first() {
    let adapter = EpFake {
        name: "adapter:test",
        episodes: None,
    };
    let storage = EpFake {
        name: "sqlite",
        episodes: Some(agentprof_core::episode::Episodes::default()),
    };
    let dp = DualPathDataSource::new(Box::new(adapter), Some(Box::new(storage)));
    let eps = dp.load_episodes("anything").expect("storage hit");
    assert!(eps.tools.is_empty());
}

#[test]
fn dualpath_load_episodes_falls_back_to_adapter_when_storage_missing_id() {
    let mut sentinel = agentprof_core::episode::Episodes::default();
    sentinel.tools.insert(
        "sentinel".to_owned(),
        agentprof_core::episode::ToolEpisode::new(
            "sentinel".to_owned(),
            agentprof_core::model::ToolSource::Builtin,
        ),
    );

    let adapter = EpFake {
        name: "adapter:test",
        episodes: Some(sentinel),
    };
    let storage = EpFake {
        name: "sqlite",
        episodes: None,
    };
    let dp = DualPathDataSource::new(Box::new(adapter), Some(Box::new(storage)));
    let eps = dp.load_episodes("anything").expect("fall back to adapter");
    assert!(
        eps.tools.contains_key("sentinel"),
        "adapter branch should have been hit"
    );
}
