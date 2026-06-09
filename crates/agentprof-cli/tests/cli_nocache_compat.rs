//! Backward-compat snapshot lock: `--no-cache` must produce stdout
//! byte-identical to the v0.1.x single-path (adapter-only) baseline.
//!
//! Any future regression in the single-path code path (the path taken
//! when storage is disabled via `--no-cache`) will flag as an `insta`
//! snapshot diff. This locks the M2.1 invariant that dual-path adoption
//! never alters legacy adapter-only output (M2.1 T7.1).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use insta::assert_snapshot;

const FIXTURE: &str = "../agentprof-adapters/tests/fixtures/copilot";

#[test]
fn list_no_cache_matches_v0_1_x_baseline() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--no-cache",
            "list",
            "--agent",
            "copilot",
            "--root",
            FIXTURE,
            "--since",
            "9999d",
            "--limit",
            "10",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_snapshot!("list_no_cache_stable", stdout);
}

#[test]
fn aggregate_by_tool_no_cache_matches_v0_1_x_baseline() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--no-cache",
            "aggregate",
            "--agent",
            "copilot",
            "--root",
            FIXTURE,
            "--by",
            "tool",
            "--since",
            "9999d",
            "--export",
            "md",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_snapshot!("aggregate_no_cache_stable", stdout);
}
