//! Integration tests for [`agentprof_adapters::AdapterDataSource`].

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::sync::Arc;
use std::time::Duration;

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_adapters::AdapterDataSource;
use agentprof_core::datasource::{DataSourceError, SessionDataSource};

#[test]
fn name_is_adapter_copilot() {
    let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), std::path::PathBuf::from("/tmp/x"));
    assert_eq!(ds.name(), "adapter:copilot");
}

#[test]
fn discover_on_empty_dir_returns_empty_vec() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), tmp.path().to_path_buf());
    let refs = ds
        .discover(Duration::from_secs(7 * 86_400))
        .expect("discover");
    assert!(
        refs.is_empty(),
        "empty dir should yield no sessions, got {refs:?}"
    );
}

#[test]
fn load_session_unknown_id_is_not_found() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), tmp.path().to_path_buf());
    match ds.load_session("nonexistent-uuid") {
        Err(DataSourceError::NotFound { id }) => assert_eq!(id, "nonexistent-uuid"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn adapter_load_episodes_returns_derived_for_fixture() {
    use agentprof_adapters::{copilot::CopilotAdapter, AdapterDataSource};
    use agentprof_core::SessionDataSource;
    use std::sync::Arc;

    let fixture_root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/copilot");
    let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), fixture_root);

    let id = "00000000-0000-0000-0000-000000001000";
    let eps = ds.load_episodes(id).expect("load");
    assert!(
        !eps.tools.is_empty(),
        "cross-turn-tool fixture should have ≥1 tool"
    );
}

#[test]
fn adapter_load_episodes_unknown_id_is_not_found() {
    use agentprof_adapters::{copilot::CopilotAdapter, AdapterDataSource};
    use agentprof_core::{DataSourceError, SessionDataSource};
    use std::sync::Arc;

    let tmp = tempfile::tempdir().expect("tempdir");
    let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), tmp.path().to_path_buf());
    match ds.load_episodes("no-such-id") {
        Err(DataSourceError::NotFound { id }) => assert_eq!(id, "no-such-id"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn adapter_load_episodes_by_ref_skips_discover() {
    use agentprof_adapters::{copilot::CopilotAdapter, AdapterDataSource};
    use agentprof_core::adapter::Adapter as _;
    use std::sync::Arc;

    let fixture_root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/copilot");
    let ds = AdapterDataSource::new(Arc::new(CopilotAdapter), fixture_root);

    let refs = CopilotAdapter
        .discover_sessions(ds.root())
        .expect("discover");
    let sref = refs
        .into_iter()
        .find(|r| r.id == "00000000-0000-0000-0000-000000001000")
        .expect("cross-turn-tool fixture");
    let eps = ds.load_episodes_by_ref(&sref).expect("load");
    assert!(!eps.tools.is_empty());
}
