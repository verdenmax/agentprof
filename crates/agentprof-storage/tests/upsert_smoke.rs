//! Smoke tests for [`agentprof_storage::upsert::upsert_report`] (M2.1 T2.4).
//!
//! Verifies the three core contracts:
//! 1. A minimal report inserts exactly one `sessions` row.
//! 2. A second `upsert_report` for the same id replaces in place
//!    (still one row).
//! 3. Child rows in `tools_loaded` are atomically replaced — a re-upsert
//!    with an empty `tool_rank` clears the previously inserted rows.
//!
//! The fixture builder lives inline (per the task brief) rather than in
//! a shared core helper.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::{tool_rank::USER_BLOCKING_TOOLS, AnalysisReport, ToolRankRow};
use agentprof_core::model::{SessionMeta, ToolSource};
use agentprof_storage::{upsert::upsert_report, Db};
use chrono::{Duration, TimeZone, Utc};
use tempfile::NamedTempFile;

fn minimal_report(id: &str) -> AnalysisReport {
    let meta = SessionMeta::new(
        id.to_owned(),
        AgentKind::Copilot,
        Utc.with_ymd_and_hms(2026, 6, 9, 12, 0, 0).unwrap(),
        false,
    );
    AnalysisReport::new(meta)
}

fn bash_tool_row() -> ToolRankRow {
    // ToolRankRow::new is #[doc(hidden)] but pub for cross-crate tests.
    let _ = USER_BLOCKING_TOOLS; // touch — silences unused-import on slimmer builds
    ToolRankRow::new(
        "bash".to_owned(),
        ToolSource::Builtin,
        1,
        1,
        0,
        0,
        0,
        Duration::milliseconds(42),
        Duration::milliseconds(42),
        Duration::milliseconds(42),
        Duration::milliseconds(42),
    )
}

#[test]
fn upsert_inserts_session_row() {
    let mut db = Db::open_in_memory().unwrap();
    let report = minimal_report("test-session-1");
    let raw = NamedTempFile::new().unwrap();

    let n = upsert_report(&mut db, &report, raw.path(), 1_700_000_000).unwrap();
    assert_eq!(n, 1, "upsert_report should return 1 for one parent row");

    let count: i64 = db
        .conn_for_test()
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = 'test-session-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);

    // Sanity-check the agent column got the Display form, not Debug.
    let agent: String = db
        .conn_for_test()
        .query_row(
            "SELECT agent FROM sessions WHERE id = 'test-session-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(agent, "copilot");
}

#[test]
fn upsert_is_idempotent_overwrite() {
    let mut db = Db::open_in_memory().unwrap();
    let report = minimal_report("test-session-2");
    let raw = NamedTempFile::new().unwrap();

    upsert_report(&mut db, &report, raw.path(), 1_700_000_000).unwrap();
    upsert_report(&mut db, &report, raw.path(), 1_700_000_999).unwrap();

    let count: i64 = db
        .conn_for_test()
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "second upsert must replace in place, not append");

    // The newer ingested_at should win (1_700_000_999 * 1000).
    let ingested: i64 = db
        .conn_for_test()
        .query_row("SELECT ingested_at FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ingested, 1_700_000_999_000);
}

#[test]
fn upsert_replaces_child_rows_atomically() {
    let mut db = Db::open_in_memory().unwrap();
    let raw = NamedTempFile::new().unwrap();

    // First pass: report has one tool → one tools_loaded row.
    let mut report = minimal_report("test-session-3");
    report.tool_rank.push(bash_tool_row());
    upsert_report(&mut db, &report, raw.path(), 1_700_000_000).unwrap();

    let n_before: i64 = db
        .conn_for_test()
        .query_row(
            "SELECT COUNT(*) FROM tools_loaded WHERE session_id = 'test-session-3'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_before, 1);

    // Second pass: tools cleared. Child rows must be gone (atomic
    // DELETE inside the same transaction as the parent re-INSERT).
    report.tool_rank.clear();
    upsert_report(&mut db, &report, raw.path(), 1_700_000_999).unwrap();

    let n_after: i64 = db
        .conn_for_test()
        .query_row(
            "SELECT COUNT(*) FROM tools_loaded WHERE session_id = 'test-session-3'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        n_after, 0,
        "child rows must be cleared when re-upserted with empty tool_rank"
    );

    // Parent row still exactly one.
    let parents: i64 = db
        .conn_for_test()
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = 'test-session-3'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(parents, 1);
}

#[test]
fn upsert_tolerates_missing_raw_path() {
    // raw_mtime falls back to 0 when fs::metadata fails; the upsert
    // itself must still succeed (the dual-path warning surface lives
    // at a higher layer in T8).
    let mut db = Db::open_in_memory().unwrap();
    let report = minimal_report("ghost-session");
    let missing = PathBuf::from("/definitely/does/not/exist/ghost.jsonl");

    upsert_report(&mut db, &report, &missing, 1_700_000_000).unwrap();

    let mtime: i64 = db
        .conn_for_test()
        .query_row(
            "SELECT raw_mtime FROM sessions WHERE id = 'ghost-session'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mtime, 0);
}
