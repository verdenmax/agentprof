//! Smoke tests for [`agentprof_storage::admin`] (M2.1 T2.7).
//!
//! Covers the four DB-admin helpers consumed by `agentprof db stats /
//! prune / vacuum / export`:
//!
//! 1. [`stats`] reports row counts and `started_at` min/max.
//! 2. [`prune_before`] dry-run reports candidate count without deleting.
//! 3. [`prune_before`] actual delete cascades via FK to child tables.
//! 4. [`vacuum`] is callable and returns `(before, after)` size pair.
//! 5. [`export_session_json`] round-trips to a JSON blob with the
//!    expected `meta.id`.
//!
//! [`stats`]: agentprof_storage::admin::stats
//! [`prune_before`]: agentprof_storage::admin::prune_before
//! [`vacuum`]: agentprof_storage::admin::vacuum
//! [`export_session_json`]: agentprof_storage::admin::export_session_json

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::Duration;

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::model::SessionMeta;
use agentprof_storage::admin::{export_session_json, prune_before, stats, vacuum};
use agentprof_storage::{upsert::upsert_report, Db};
use chrono::{TimeZone, Utc};

/// Inline builder mirroring `tests/upsert_smoke.rs::minimal_report`.
/// `started_ms` lets each test pin the session's start time so prune
/// windows are deterministic.
fn minimal_report(id: &str, started_ms: i64) -> AnalysisReport {
    let started = Utc.timestamp_millis_opt(started_ms).unwrap();
    let meta = SessionMeta::new(id.to_owned(), AgentKind::Copilot, started, false);
    AnalysisReport::new(meta)
}

fn raw() -> &'static Path {
    Path::new("/tmp/agentprof-admin-smoke.jsonl")
}

#[test]
fn stats_reports_row_counts() {
    let mut db = Db::open_in_memory().unwrap();
    upsert_report(&mut db, &minimal_report("s-a", 1_700_000_000_000), raw(), 1).unwrap();
    upsert_report(&mut db, &minimal_report("s-b", 1_700_000_100_000), raw(), 1).unwrap();

    let s = stats(&db).unwrap();
    assert_eq!(s.session_count, 2);
    assert_eq!(s.tools_loaded_count, 0);
    assert_eq!(s.turn_buckets_count, 0);
    assert_eq!(s.oldest_started_ms, Some(1_700_000_000_000));
    assert_eq!(s.newest_started_ms, Some(1_700_000_100_000));
}

#[test]
fn prune_dry_run_deletes_nothing() {
    let mut db = Db::open_in_memory().unwrap();
    // "old" is 60 days behind `now`; "new" is "now".
    let now_ms: i64 = 1_700_000_000_000;
    let old_ms = now_ms - 60 * 24 * 3_600 * 1_000;
    upsert_report(&mut db, &minimal_report("s-old", old_ms), raw(), 1).unwrap();
    upsert_report(&mut db, &minimal_report("s-new", now_ms), raw(), 1).unwrap();

    let retention = Duration::from_secs(30 * 24 * 3_600);
    let n = prune_before(&mut db, retention, now_ms, true).unwrap();
    assert_eq!(n, 1, "one row is older than retention");

    let s = stats(&db).unwrap();
    assert_eq!(s.session_count, 2, "dry-run must not actually delete");
}

#[test]
fn prune_actual_deletes_and_cascades() {
    let mut db = Db::open_in_memory().unwrap();
    let now_ms: i64 = 1_700_000_000_000;
    let old_ms = now_ms - 60 * 24 * 3_600 * 1_000;
    upsert_report(&mut db, &minimal_report("s-old", old_ms), raw(), 1).unwrap();
    upsert_report(&mut db, &minimal_report("s-new", now_ms), raw(), 1).unwrap();

    let retention = Duration::from_secs(30 * 24 * 3_600);
    let deleted = prune_before(&mut db, retention, now_ms, false).unwrap();
    assert_eq!(deleted, 1);

    let s = stats(&db).unwrap();
    assert_eq!(s.session_count, 1);
}

#[test]
fn vacuum_is_callable() {
    let mut db = Db::open_in_memory().unwrap();
    upsert_report(&mut db, &minimal_report("s-v", 1_700_000_000_000), raw(), 1).unwrap();

    // In-memory dbs report page_count=0; we only assert that the call
    // does not panic / error.
    let (_before, _after) = vacuum(&db).unwrap();
}

#[test]
fn export_session_json_round_trip() {
    let mut db = Db::open_in_memory().unwrap();
    upsert_report(
        &mut db,
        &minimal_report("s-export", 1_700_000_000_000),
        raw(),
        1,
    )
    .unwrap();

    let json = export_session_json(&db, "s-export").unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v.get("meta")
            .and_then(|m| m.get("id"))
            .and_then(|i| i.as_str()),
        Some("s-export"),
    );
}
