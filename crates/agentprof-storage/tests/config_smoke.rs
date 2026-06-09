//! Smoke tests for [`agentprof_storage::config`] defaults & XDG resolution.
//!
//! These tests mutate process-global environment variables, so they are
//! serialized through a [`Mutex`] to avoid cross-test races.

use std::path::PathBuf;
use std::sync::Mutex;

use agentprof_storage::config::{StorageConfig, StorageMode};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Snapshot/restore a single env var for the duration of one test.
struct EnvGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prev }
    }
    fn unset(key: &'static str) -> Self {
        let prev = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
fn default_mode_is_cache() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let cfg = StorageConfig::default();
    assert_eq!(cfg.mode, StorageMode::Cache);
    assert_eq!(cfg.auto_prune_days, 30);
}

#[test]
fn cache_default_path_under_xdg_cache_home() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _g = EnvGuard::set("XDG_CACHE_HOME", "/tmp/test-xdg-cache");
    let p = StorageConfig::default_path_for(StorageMode::Cache);
    assert_eq!(
        p,
        PathBuf::from("/tmp/test-xdg-cache/agentprof/cache.sqlite")
    );
}

#[test]
fn store_default_path_under_xdg_data_home() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _g = EnvGuard::set("XDG_DATA_HOME", "/tmp/test-xdg-data");
    // Ensure cache var doesn't accidentally bleed in.
    let _g2 = EnvGuard::unset("XDG_CACHE_HOME");
    let p = StorageConfig::default_path_for(StorageMode::Store);
    assert_eq!(
        p,
        PathBuf::from("/tmp/test-xdg-data/agentprof/store.sqlite")
    );
}
