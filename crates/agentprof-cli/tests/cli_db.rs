//! Integration tests for the `agentprof db` subcommand family
//! (M2.1 T6.1–T6.4).
//!
//! Every test pins `--storage-path <tempdir>/test.sqlite` to avoid
//! polluting the user's real `~/.cache/agentprof/cache.sqlite` (the
//! M2.1 T5.x regression class). Tests that need fixture sessions
//! copy a single `minimal` fixture into a per-test isolated root so
//! they don't see whatever 20+ other fixtures the workspace ships.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn agentprof() -> Command {
    Command::cargo_bin("agentprof").expect("agentprof binary built")
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

/// Copy a single named fixture session directory into a fresh tempdir
/// so the test sees exactly one session.
fn isolated_single_session(slug: &str) -> TempDir {
    let src = fixtures_root().join(slug);
    assert!(
        src.is_dir(),
        "fixture {} not found at {}",
        slug,
        src.display()
    );
    let tmp = tempfile::tempdir().unwrap();
    let dst = tmp.path().join(slug);
    fs::create_dir_all(&dst).unwrap();
    for entry in fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
    }
    tmp
}

// -------- T6.1: init -------------------------------------------------

#[test]
fn db_init_creates_file_and_tables() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "init"])
        .assert()
        .success()
        .stdout(contains("db initialized at"));
    assert!(db_path.exists(), "DB file must exist after `db init`");

    // Idempotent: running again is also fine.
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "init"])
        .assert()
        .success();
}

// -------- T6.2: stats ------------------------------------------------

#[test]
fn db_stats_shows_zero_rows_on_fresh_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "init"])
        .assert()
        .success();
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "stats"])
        .assert()
        .success()
        .stdout(contains("sessions:            0"))
        .stdout(contains("tools_loaded:        0"))
        .stdout(contains("turn_buckets:        0"));
}

#[test]
fn db_stats_json_export_is_parseable() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "init"])
        .assert()
        .success();
    let out = agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "stats",
            "--export",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s.trim()).expect("stats json parses");
    assert_eq!(v["sessions"], 0);
    assert!(v["path"].is_string());
}

// -------- T6.3: ingest ----------------------------------------------

#[test]
fn db_ingest_increments_session_count() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    let fixture_root = isolated_single_session("minimal");

    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fixture_root.path().to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let out = agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "stats",
            "--export",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    assert_eq!(
        v["sessions"].as_i64().unwrap(),
        1,
        "ingest --all must surface exactly the one fixture session"
    );
}

#[test]
fn db_ingest_requires_scope() {
    // Neither --since nor --all nor --session → clap ArgGroup error.
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
        ])
        .assert()
        .failure()
        .stderr(contains("unrecognized").not());
}

// -------- T6.3: prune ------------------------------------------------

#[test]
fn db_prune_dry_run_outputs_count_without_deleting() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    let fixture_root = isolated_single_session("minimal");

    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fixture_root.path().to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    // Anything older than ~1 second qualifies; the fixture mtime is
    // months/years in the past, so it should count as prunable.
    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "prune",
            "--before",
            "1s",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(contains("would prune"));

    // Confirm count unchanged (dry-run must NOT delete).
    let out = agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "stats",
            "--export",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
    assert_eq!(v["sessions"].as_i64().unwrap(), 1);
}

#[test]
fn db_prune_actual_cascades_deletes_children() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    let fixture_root = isolated_single_session("with-mcp-calls");

    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fixture_root.path().to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    // Real prune of everything older than 1s.
    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "prune",
            "--before",
            "1s",
        ])
        .assert()
        .success()
        .stdout(contains("pruned"));

    // After prune: sessions == 0 AND child tables == 0 (FK CASCADE).
    let out = agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "stats",
            "--export",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
    assert_eq!(v["sessions"].as_i64().unwrap(), 0, "sessions must be 0");
    assert_eq!(
        v["tools_loaded"].as_i64().unwrap(),
        0,
        "tools_loaded must cascade to 0"
    );
    assert_eq!(
        v["turn_buckets"].as_i64().unwrap(),
        0,
        "turn_buckets must cascade to 0"
    );
}

// -------- T6.4: vacuum ----------------------------------------------

#[test]
fn db_vacuum_reports_before_after() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "init"])
        .assert()
        .success();
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "vacuum"])
        .assert()
        .success()
        .stdout(contains("vacuum complete"))
        .stdout(contains("before="))
        .stdout(contains("after="));
}

// -------- T6.4: export ----------------------------------------------

fn ingest_one_and_get_id(db_path: &std::path::Path) -> String {
    let fixture_root = isolated_single_session("minimal");
    // Extract the actual session id from the fixture's events.jsonl
    // before the TempDir gets shadowed by the ingest call below — the
    // Copilot adapter reads `session_id` from inside the JSONL stream
    // (falling back to the directory name only when absent), so the
    // dir name is *not* a reliable id.
    let events = fixture_root.path().join("minimal").join("events.jsonl");
    let first_line = fs::read_to_string(&events)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let id = serde_json::from_str::<serde_json::Value>(&first_line)
        .ok()
        .and_then(|v| {
            v.get("data")
                .and_then(|d| d.get("sessionId"))
                .and_then(|s| s.as_str())
                .map(str::to_owned)
        })
        .expect("minimal fixture must carry data.sessionId");
    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fixture_root.path().to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();
    id
}

#[test]
fn db_export_json_emits_valid_session_json() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    let id = ingest_one_and_get_id(&db_path);

    let out = agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "export",
            &id,
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s.trim()).expect("export json must parse");
    // Stored AnalysisReport is an object with at least a `meta` key.
    assert!(v.is_object(), "export must be a JSON object");
    assert!(
        v.as_object().unwrap().contains_key("meta"),
        "report must have meta field; saw keys: {:?}",
        v.as_object().unwrap().keys().collect::<Vec<_>>()
    );
}

#[test]
fn db_export_jsonl_emits_multiple_lines() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    let id = ingest_one_and_get_id(&db_path);

    let out = agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "export",
            &id,
            "--format",
            "jsonl",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = s.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        lines.len() >= 2,
        "jsonl must emit ≥ 2 lines, got {} from: {s:?}",
        lines.len()
    );
    for line in &lines {
        let _: serde_json::Value =
            serde_json::from_str(line).expect("each jsonl line must be valid JSON");
    }
}

#[test]
fn db_export_unknown_session_is_user_error() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.sqlite");
    agentprof()
        .args(["--storage-path", db_path.to_str().unwrap(), "db", "init"])
        .assert()
        .success();
    agentprof()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "export",
            "does-not-exist-id",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("not found"));
}
