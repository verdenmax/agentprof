//! Dual-path read integration tests for `cmd::aggregate` (M2.1.1).
//!
//! Mirrors `cli_dualpath.rs` (which covers list) for the aggregate
//! command: silent / warn / no-cache parity / empty-episodes graceful
//! skip.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has parent")
        .join("agentprof-adapters/tests/fixtures/copilot")
}

#[test]
fn aggregate_silent_when_cache_in_sync() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("c.sqlite");
    let fx = fixture();

    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "aggregate",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--by",
            "tool",
            "--since",
            "9999d",
            "--export",
            "md",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("agentprof: warn:"),
        "expected silent stderr; got: {stderr}"
    );
}

#[test]
fn aggregate_warns_on_stale_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("c.sqlite");
    let fx = fixture();

    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute("UPDATE sessions SET raw_mtime = raw_mtime - 1000000", [])
        .unwrap();
    drop(conn);

    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "aggregate",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--by",
            "tool",
            "--since",
            "9999d",
            "--export",
            "md",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("agentprof: warn:") && stderr.contains("fields differ"),
        "expected divergence warn; got: {stderr}"
    );
}

#[test]
fn aggregate_no_cache_parity_with_dual_path_after_ingest() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("c.sqlite");
    let fx = fixture();

    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let dual = Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "aggregate",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--by",
            "tool",
            "--since",
            "9999d",
            "--export",
            "md",
        ])
        .output()
        .unwrap();
    let nocache = Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--no-cache",
            "aggregate",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--by",
            "tool",
            "--since",
            "9999d",
            "--export",
            "md",
        ])
        .output()
        .unwrap();

    let s_dual = String::from_utf8_lossy(&dual.stdout).into_owned();
    let s_nocache = String::from_utf8_lossy(&nocache.stdout).into_owned();
    assert_eq!(
        s_dual.trim(),
        s_nocache.trim(),
        "dual-path and --no-cache must produce byte-identical aggregate stdout"
    );
}

#[test]
fn aggregate_tolerates_empty_episodes_column() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("c.sqlite");
    let fx = fixture();

    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute("UPDATE sessions SET episodes_json = '{}'", [])
        .unwrap();
    drop(conn);

    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "aggregate",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--by",
            "tool",
            "--since",
            "9999d",
            "--export",
            "md",
        ])
        .assert()
        .success();
}

#[test]
fn aggregate_reports_corrupt_episodes_json_as_data_error() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("c.sqlite");
    let fx = fixture();

    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "db",
            "ingest",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute("UPDATE sessions SET episodes_json = 'not json'", [])
        .unwrap();
    drop(conn);

    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--storage-path",
            db_path.to_str().unwrap(),
            "aggregate",
            "--agent",
            "copilot",
            "--root",
            fx.to_str().unwrap(),
            "--by",
            "tool",
            "--since",
            "9999d",
            "--export",
            "md",
        ])
        .assert()
        .code(2)
        .stderr(contains("load_episodes"));
}
