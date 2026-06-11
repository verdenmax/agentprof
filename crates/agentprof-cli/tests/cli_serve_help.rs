//! CLI surface tests for `agentprof serve` (M2.3 T2).
//!
//! Shallow surface checks: assert the clap-derived help exposes the
//! documented flags and that `serve` shows up in the top-level help.
//! Deep behavioral coverage (handlers, rendering, refresh) lands in
//! later M2.3 tasks.

#![cfg(feature = "web")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn serve_help_lists_core_flags() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(contains("--bind"))
        .stdout(contains("--storage-path"))
        .stdout(contains("--interval-default"))
        .stdout(contains("--no-open"));
}

#[test]
fn serve_subcommand_appears_in_top_level_help() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--help"])
        .assert()
        .success()
        .stdout(contains("serve"));
}
