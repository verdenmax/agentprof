//! Smoke tests for [`agentprof_storage::query`] (M2.1 T2.5).
//!
//! Covers the two read entry points:
//! - `query_sessions_since` — window filter + DESC ordering on `started_at`.
//! - `load_session` — round-trip a stored report; missing id surfaces as
//!   `SqliteError::Rusqlite { source: QueryReturnedNoRows, .. }`.
//!
//! The fixture builder is inlined per the task brief (tests stay
//! independent across T2.4 / T2.5).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::model::SessionMeta;
use agentprof_storage::query::{load_session, query_sessions_since};
use agentprof_storage::upsert::upsert_report;
use agentprof_storage::{Db, SqliteError};
use chrono::{TimeZone, Utc};
use tempfile::NamedTempFile;

fn report_with_started_at_ms(id: &str, started_at_ms: i64) -> AnalysisReport {
    let started = Utc.timestamp_millis_opt(started_at_ms).single().unwrap();
    let meta = SessionMeta::new(id.to_owned(), AgentKind::Copilot, started, false);
    AnalysisReport::new(meta)
}

#[test]
fn query_since_returns_recent_only() {
    let mut db = Db::open_in_memory().unwrap();
    let raw = NamedTempFile::new().unwrap();

    let now_ms: i64 = 1_700_000_000_000;
    let week_secs: u64 = 7 * 86_400;

    // "old" is 30 days back — should be excluded by a 7d window.
    let old = report_with_started_at_ms("old", now_ms - 30 * 86_400 * 1000);
    // "new" is 1 day back — included.
    let new = report_with_started_at_ms("new", now_ms - 86_400 * 1000);

    upsert_report(&mut db, &old, raw.path(), 1_700_000_000).unwrap();
    upsert_report(&mut db, &new, raw.path(), 1_700_000_000).unwrap();

    let refs = query_sessions_since(&db, Duration::from_secs(week_secs), now_ms).unwrap();
    assert_eq!(refs.len(), 1, "only the recent session must come back");
    assert_eq!(refs[0].id, "new");
    assert_eq!(refs[0].agent, AgentKind::Copilot);
    assert_eq!(refs[0].source, "sqlite");
    assert!(refs[0].raw_path.is_some());
    assert!(refs[0].raw_mtime_ms.is_some());
}

#[test]
fn query_since_orders_descending() {
    let mut db = Db::open_in_memory().unwrap();
    let raw = NamedTempFile::new().unwrap();

    let now_ms: i64 = 1_700_000_000_000;
    let a = report_with_started_at_ms("a-oldest", now_ms - 3 * 3600 * 1000);
    let b = report_with_started_at_ms("b-middle", now_ms - 2 * 3600 * 1000);
    let c = report_with_started_at_ms("c-newest", now_ms - 3600 * 1000);

    // Insert out of order to make sure ordering comes from SQL, not insert order.
    upsert_report(&mut db, &b, raw.path(), 1_700_000_000).unwrap();
    upsert_report(&mut db, &a, raw.path(), 1_700_000_000).unwrap();
    upsert_report(&mut db, &c, raw.path(), 1_700_000_000).unwrap();

    let refs = query_sessions_since(&db, Duration::from_secs(86_400), now_ms).unwrap();
    let ids: Vec<&str> = refs.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["c-newest", "b-middle", "a-oldest"]);
}

#[test]
fn load_session_round_trip() {
    let mut db = Db::open_in_memory().unwrap();
    let raw = NamedTempFile::new().unwrap();

    let report = report_with_started_at_ms("rt-1", 1_700_000_000_000);
    upsert_report(&mut db, &report, raw.path(), 1_700_000_000).unwrap();

    let loaded = load_session(&db, "rt-1").unwrap();
    assert_eq!(loaded.meta.id, "rt-1");
    assert_eq!(loaded.meta.agent, AgentKind::Copilot);
}

#[test]
fn load_session_missing_returns_not_found() {
    let db = Db::open_in_memory().unwrap();
    let err = load_session(&db, "does-not-exist").unwrap_err();
    match err {
        SqliteError::Rusqlite { source, .. } => {
            assert!(
                matches!(source, rusqlite::Error::QueryReturnedNoRows),
                "expected QueryReturnedNoRows, got: {source:?}"
            );
        }
        other => panic!("expected SqliteError::Rusqlite, got: {other:?}"),
    }
}
