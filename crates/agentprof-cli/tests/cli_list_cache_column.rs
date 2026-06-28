//! E2E: `list` renders the `Cache%` column header.
//!
//! Spawns the binary via `assert_cmd` against the committed Copilot
//! fixtures under `crates/agentprof-adapters/tests/fixtures/copilot/`.
//!
//! Regression guard: an empty `--root` short-circuits to the
//! "(no sessions matching …)" branch in `cmd::list` and never reaches
//! `format_table`, so the 8-column header (which contains `Cache%`) is
//! only emitted once at least one real session matches. We therefore
//! point at the committed fixtures with `--since all` — their mtimes
//! predate any bounded `--since` window, so a relative window like the
//! default `7d` would filter every fixture out.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;

/// Absolute path to the committed Copilot fixtures directory.
fn copilot_fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace crates dir")
        .join("agentprof-adapters/tests/fixtures/copilot")
}

#[test]
fn list_header_includes_cache_pct_column() {
    // Needs ≥1 real session: the table header is only printed once `list`
    // discovers a matching session (empty root → "(no sessions …)" with no
    // table). `--no-cache` keeps us on the pure filesystem path and
    // `--since all` neutralizes committed-fixture mtime staleness.
    Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args(["--no-cache", "list", "--agent", "copilot", "--root"])
        .arg(copilot_fixtures_root())
        .args(["--since", "all", "--limit", "100"])
        .assert()
        .success()
        .stdout(contains("Cache%"));
}
