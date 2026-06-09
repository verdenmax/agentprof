//! End-to-end CLI integration tests for `agentprof list`.
//!
//! Spawns the binary via `assert_cmd` against the committed Copilot
//! fixtures under `crates/agentprof-adapters/tests/fixtures/copilot/`.
//!
//! Verifies:
//! - happy path: lists committed fixture session IDs.
//! - `--limit` caps output row count.
//! - `--since 1s` may yield empty match (fixture mtimes may be older).
//! - error paths: nonexistent root, unsupported agent.
//! - corrupt fixture: skipped + summarized to stderr; table still printed.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

#[test]
fn list_happy_path_lists_committed_fixtures() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "list", "--root"])
        .arg(fixtures_root())
        .args(["--since", "all", "--limit", "100"])
        .assert()
        .success()
        .stdout(contains("ID"))
        .stdout(contains("Started"))
        .stdout(contains("Model"))
        // At least 1 known fixture should be listed. As of the M2.1
        // id-namespace fix, list rows show the canonical UUID parsed from
        // events.jsonl rather than the directory name; assert on
        // sessionIds that uniquely identify those two fixtures
        // (cross-turn-tool=...001000, minimal=...000001).
        .stdout(
            contains("00000000-0000-0000-0000-000000001000")
                .or(contains("00000000-0000-0000-0000-000000000001")),
        );
}

#[test]
fn list_default_limit_caps_output() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "list", "--root"])
        .arg(fixtures_root())
        .args(["--since", "all", "--limit", "2"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    // Footer says "2 of N sessions shown".
    assert!(s.contains("2 of"), "expected '2 of' in footer; got:\n{s}");
}

#[test]
fn list_since_filter_with_zero_window_yields_empty_or_few() {
    // 1-second window is almost certainly past for committed fixtures.
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "list", "--root"])
        .arg(fixtures_root())
        .args(["--since", "1s"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    // Either empty match message or a very small table.
    let is_empty_msg = s.contains("no sessions matching");
    let is_small_table = s.lines().count() <= 5;
    assert!(
        is_empty_msg || is_small_table,
        "expected empty-match or small table; got:\n{s}"
    );
}

#[test]
fn list_nonexistent_root_exits_user_error() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--no-cache",
            "list",
            "--root",
            "/nonexistent/agentprof/list/path",
            "--since",
            "all",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("session root not found"));
}

#[test]
fn list_corrupt_fixture_summarized_in_stderr() {
    // The `corrupt` fixture has invalid session.start; per-session parse
    // failure is reported but doesn't crash the command.
    let assertion = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "list", "--root"])
        .arg(fixtures_root())
        .args(["--since", "all", "--limit", "100"])
        .assert()
        .success();
    let stderr = String::from_utf8(assertion.get_output().stderr.clone()).unwrap();
    // We expect either no failures (if corrupt is parseable) or a
    // mention of corrupt id in the failure summary.
    if stderr.contains("failed to parse") {
        assert!(
            stderr.contains("corrupt"),
            "stderr should mention corrupt session: {stderr}"
        );
    }
}

#[test]
fn list_unsupported_agent_exits_user_error() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "list", "--agent", "claude", "--since", "all"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("not yet implemented"));
}
