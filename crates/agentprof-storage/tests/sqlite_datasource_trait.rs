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

#[test]
fn sqlite_load_episodes_returns_default_for_seeded_row() {
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::AnalysisReport;
    use agentprof_core::episode::Episodes;
    use agentprof_core::model::SessionMeta;
    use agentprof_storage::upsert::upsert_report;
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;

    let mut db = Db::open_in_memory().unwrap();
    let meta = SessionMeta::new(
        "ep-1".into(),
        AgentKind::Copilot,
        Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
        false,
    );
    let r = AnalysisReport::new(meta);
    upsert_report(&mut db, &r, &PathBuf::from("/tmp/x.jsonl"), 1_700_000_000).unwrap();

    let src = SqliteDataSource::new(Arc::new(Mutex::new(db)));
    let eps: Episodes = src.load_episodes("ep-1").unwrap();
    assert!(eps.tools.is_empty());
}

#[test]
fn sqlite_load_episodes_unknown_id_maps_to_not_found() {
    let src = make_src();
    match src.load_episodes("nope") {
        Err(DataSourceError::NotFound { id }) => assert_eq!(id, "nope"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}
