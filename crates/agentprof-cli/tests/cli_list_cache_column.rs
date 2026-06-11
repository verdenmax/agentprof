//! E2E: `list` shows Cache% column populated from session cache metrics.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn list_header_includes_cache_pct_column() {
    // We don't need a populated session DB — `list --root <empty>`
    // prints just the header.
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args([
            "list",
            "--agent",
            "copilot",
            "--root",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("Cache%"));
}
