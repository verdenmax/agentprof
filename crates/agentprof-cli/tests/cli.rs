//! End-to-end CLI integration tests.
//!
//! Spawns the `agentprof` binary via `assert_cmd` against committed
//! fixtures under `crates/agentprof-adapters/tests/fixtures/copilot/`.
//!
//! Verifies:
//! - md happy path: structure + turn ids visible
//! - json happy path: parseable + expected top-level keys
//! - `--output` writes a file and acknowledges on stderr
//! - error paths: nonexistent root, unknown UUID → exit 1 with helpful
//!   diagnostics
//! - md insta snapshot for cross-turn-tool locks ADR-0005 D-2 fix
//!   visible in user-facing report

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

/// Path to the cross-turn-tool fixture directory.
///
/// We use the Path selector (not Uuid) because `SessionRef.id` is the
/// directory name set by `CopilotAdapter::discover_sessions`, NOT the
/// inner wire-format `sessionId` field. So `--session cross-turn-tool`
/// would be rejected by the `looks_like_uuid` heuristic, and
/// `--session 00000000-...-001000` (the inner UUID) doesn't match the
/// `SessionRef` set built by discovery. Path selector sidesteps both
/// problems and is what real users would type anyway.
fn cross_turn_path() -> PathBuf {
    fixtures_root().join("cross-turn-tool")
}

#[test]
fn analyze_md_to_stdout_succeeds_for_cross_turn_tool() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "md"])
        .assert()
        .success()
        .stdout(contains("# agentprof analyze"))
        .stdout(contains("## Session"))
        .stdout(contains("## Turn Summary"))
        .stdout(contains("turn-A"))
        .stdout(contains("turn-B"));
}

#[test]
fn analyze_json_to_stdout_is_valid_json() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&s).expect("output must be valid JSON");
    assert!(parsed.get("meta").is_some(), "missing meta");
    assert!(parsed.get("turn_summary").is_some(), "missing turn_summary");
    assert!(parsed.get("tool_rank").is_some(), "missing tool_rank");
    assert!(parsed.get("hook_rank").is_some(), "missing hook_rank");
    assert!(parsed.get("warnings").is_some(), "missing warnings");
    let turns = parsed["turn_summary"].as_array().unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["tool_call_count"], 1);
    assert_eq!(turns[1]["tool_call_count"], 0);
}

#[test]
fn analyze_writes_to_output_file_when_specified() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out_path = tmp.path().join("report.md");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "md", "--output"])
        .arg(&out_path)
        .assert()
        .success()
        .stderr(contains("wrote"))
        .stderr(contains("bytes"));
    let written = std::fs::read_to_string(&out_path).unwrap();
    assert!(written.starts_with("# agentprof analyze"));
    assert!(written.contains("turn-A"));
}

#[test]
fn analyze_nonexistent_root_exits_with_user_error() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "analyze",
            "--root",
            "/nonexistent/agentprof/path",
            "--session",
            "latest",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("session root not found"));
}

#[test]
fn analyze_unknown_session_uuid_exits_with_user_error_and_helpful_message() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--root"])
        .arg(fixtures_root())
        .args(["--session", "99999999-9999-9999-9999-999999999999"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("not found"))
        .stderr(contains("first 5 available"));
}

#[test]
fn analyze_md_snapshot_cross_turn_tool() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let md = String::from_utf8(out).unwrap();
    insta::assert_snapshot!("analyze_md__cross_turn_tool", md);
}
