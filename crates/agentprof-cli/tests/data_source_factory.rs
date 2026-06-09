//! Integration tests for [`agentprof_cli::data_source_factory::build_data_source`].

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use agentprof_cli::data_source_factory::build_data_source;
use agentprof_core::datasource::SessionDataSource;
use agentprof_storage::config::{StorageConfig, StorageMode};
use tempfile::TempDir;

fn storage_cfg(tmp: &TempDir) -> StorageConfig {
    let mut cfg = StorageConfig::default();
    cfg.mode = StorageMode::Cache;
    cfg.path = tmp.path().join("c.sqlite");
    cfg.auto_prune_days = 30;
    cfg
}

#[test]
fn build_returns_dual_when_storage_enabled() {
    let tmp = TempDir::new().unwrap();
    let cfg = storage_cfg(&tmp);

    let ds = build_data_source("copilot", &PathBuf::from("/nonexistent"), &cfg, false).unwrap();
    assert_eq!(SessionDataSource::name(&*ds), "dual");
}

#[test]
fn build_returns_adapter_when_no_cache_set() {
    let tmp = TempDir::new().unwrap();
    let cfg = storage_cfg(&tmp);

    let ds = build_data_source("copilot", &PathBuf::from("/nonexistent"), &cfg, true).unwrap();
    assert!(SessionDataSource::name(&*ds).starts_with("adapter:"));
}

#[test]
fn build_rejects_unsupported_agent() {
    let tmp = TempDir::new().unwrap();
    let cfg = storage_cfg(&tmp);

    let err = build_data_source("claude", &PathBuf::from("/x"), &cfg, false)
        .err()
        .expect("expected error for unsupported agent");
    assert!(err.to_string().contains("unsupported agent"));
}
