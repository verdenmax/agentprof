//! Integration tests for [`SqliteDataSource`] (M2.1 T2.6).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentprof_core::datasource::{DataSourceError, SessionDataSource};
use agentprof_storage::{Db, SqliteDataSource};

fn make_src() -> SqliteDataSource {
    let db = Db::open_in_memory().expect("open in-memory db");
    SqliteDataSource::new(Arc::new(Mutex::new(db)))
}

#[test]
fn sqlite_data_source_name() {
    let src = make_src();
    assert_eq!(src.name(), "sqlite");
}

#[test]
fn sqlite_data_source_discover_empty() {
    let src = make_src().with_now_fn(|| 1_700_000_000_000);
    let refs = src
        .discover(Duration::from_secs(7 * 86_400))
        .expect("discover should succeed on empty db");
    assert!(refs.is_empty(), "expected empty Vec, got {refs:?}");
}

#[test]
fn sqlite_data_source_load_session_missing_maps_to_not_found() {
    let src = make_src();
    let err = src
        .load_session("does-not-exist")
        .expect_err("missing id must fail");
    match err {
        DataSourceError::NotFound { id } => {
            assert_eq!(id, "does-not-exist");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}
