//! Regression lock: `--no-cache` (single-path / adapter-only) stdout
//! is snapshotted via `insta`. Any future change to the bytes
//! `agentprof list --no-cache` / `agentprof aggregate --no-cache`
//! emit on the canonical fixture will trip a snapshot diff and
//! force an **explicit snapshot review** (`cargo insta review`).
//!
//! These snapshots are **not** a "v0.1.x baseline lock" (the audit
//! corrected the earlier framing) — they were captured during M2.1
//! T7.1 as the reference for the current single-path output and
//! exist to catch accidental regressions in that path, e.g. while
//! refactoring the dual-path composer. Intentional UX changes are
//! fine — just re-record the snapshot with `cargo insta review`
//! and explain in the commit / PR.

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
