//! E2E: `aggregate` `--by` `model`/`day` emits 4 cache columns
//! (`CacheCr` / `CacheRd` / `Hit%` / `NetSaved`) per ADR-0023 D-3/D-5;
//! `--by` `tool` and `--by` `mcp-server` deliberately omit them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

/// Root pointing at the entire Copilot fixture set; M2.5 fixture
/// `with-session-shutdown` (T5) lives here and provides cache
/// activity, but the column headers must show up even when no bucket
/// reports any cache activity.
fn copilot_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

fn run(by: &str) -> assert_cmd::assert::Assert {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--no-cache",
            "aggregate",
            "--by",
            by,
            "--export",
            "md",
            "--agent",
            "copilot",
            "--since",
            "all",
            "--root",
        ])
        .arg(copilot_fixture_root())
        .assert()
        .success()
}

#[test]
fn aggregate_by_model_md_has_cache_cols() {
    run("model")
        .stdout(contains("CacheCr"))
        .stdout(contains("CacheRd"))
        .stdout(contains("Hit%"))
        .stdout(contains("NetSaved"));
}

#[test]
fn aggregate_by_day_md_has_cache_cols() {
    run("day").stdout(contains("CacheCr"));
}

#[test]
fn aggregate_by_tool_md_omits_cache_cols() {
    run("tool")
        .stdout(contains("CacheCr").not())
        .stdout(contains("CacheRd").not());
}

#[test]
fn aggregate_by_mcp_server_md_omits_cache_cols() {
    run("mcp-server")
        .stdout(contains("CacheCr").not())
        .stdout(contains("CacheRd").not());
}
