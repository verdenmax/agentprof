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
