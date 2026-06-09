//! Round-trip tests for the M2.1.1 episodes_json column + helpers.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::Episodes;
use agentprof_core::model::SessionMeta;
use agentprof_storage::{
    query::load_episodes,
    upsert::{upsert_episodes, upsert_report},
    Db, SqliteError,
};
use chrono::{TimeZone, Utc};
use std::path::PathBuf;

fn minimal_report(id: &str) -> AnalysisReport {
    let meta = SessionMeta::new(
        id.to_owned(),
        AgentKind::Copilot,
        Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
        false,
    );
    AnalysisReport::new(meta)
}

#[test]
fn upsert_then_load_episodes_round_trip() {
    let mut db = Db::open_in_memory().unwrap();
    let r = minimal_report("rt-1");
    upsert_report(&mut db, &r, &PathBuf::from("/tmp/x.jsonl"), 1_700_000_000).unwrap();

    let eps = Episodes::default();
    let n = upsert_episodes(&mut db, "rt-1", &eps, 1_700_000_000).unwrap();
    assert_eq!(n, 1, "expected 1 row updated");

    let loaded = load_episodes(&db, "rt-1").unwrap();
    assert!(loaded.tools.is_empty());
    assert!(loaded.hooks.is_empty());
    assert!(loaded.turns.is_empty());
}

#[test]
fn load_episodes_returns_default_for_unmigrated_row() {
    let mut db = Db::open_in_memory().unwrap();
    let r = minimal_report("u-1");
    upsert_report(&mut db, &r, &PathBuf::from("/tmp/x.jsonl"), 1_700_000_000).unwrap();

    let loaded = load_episodes(&db, "u-1").unwrap();
    assert!(loaded.tools.is_empty());
}

#[test]
fn load_episodes_unknown_id_returns_rusqlite_no_rows() {
    let db = Db::open_in_memory().unwrap();
    let err = load_episodes(&db, "no-such-id").unwrap_err();
    match err {
        SqliteError::Rusqlite {
            source: rusqlite::Error::QueryReturnedNoRows,
            ..
        } => {}
        other => panic!("expected QueryReturnedNoRows, got {other:?}"),
    }
}

#[test]
fn upsert_episodes_idempotent_overwrite() {
    let mut db = Db::open_in_memory().unwrap();
    let r = minimal_report("io-1");
    upsert_report(&mut db, &r, &PathBuf::from("/tmp/x.jsonl"), 1_700_000_000).unwrap();
    let eps = Episodes::default();
    let n1 = upsert_episodes(&mut db, "io-1", &eps, 1_700_000_000).unwrap();
    let n2 = upsert_episodes(&mut db, "io-1", &eps, 1_700_000_999).unwrap();
    assert_eq!(n1, 1);
    assert_eq!(n2, 1, "second upsert should also update exactly 1 row");
}
