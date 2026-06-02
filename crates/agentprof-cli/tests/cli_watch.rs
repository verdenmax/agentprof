//! Integration tests for `agentprof watch` (M1.6.3).
//!
//! All tests use non-TTY stdin/stdout so the watcher loop never starts —
//! they only exercise argument parsing + early validation. The live
//! watcher is covered by manual smoke per spec D-14.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

#[test]
fn watch_help_lists_aggregate_subcommand() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch", "--help"]);
    cmd.assert().success().stdout(contains("aggregate"));
}

#[test]
fn watch_non_tty_exits_output_error() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch"]).write_stdin("");
    cmd.assert()
        .failure()
        .code(3)
        .stderr(contains("TTY").or(contains("tty")).or(contains("terminal")));
}

#[test]
fn watch_aggregate_non_tty_exits_output_error() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch", "aggregate", "--by", "tool"])
        .write_stdin("");
    cmd.assert()
        .failure()
        .code(3)
        .stderr(contains("TTY").or(contains("tty")).or(contains("terminal")));
}

#[test]
fn watch_aggregate_invalid_by_value_exits_with_clap_error() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch", "aggregate", "--by", "garbage"]);
    // clap default failure exit is 2.
    cmd.assert().failure().code(2);
}

#[test]
fn watch_aggregate_rejects_export_flag() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch", "aggregate", "--by", "tool", "--export", "csv"])
        .write_stdin("");
    cmd.assert()
        .failure()
        .code(1)
        .stderr(contains("does not accept --export"));
}

#[test]
fn watch_aggregate_rejects_output_flag() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args([
        "watch",
        "aggregate",
        "--by",
        "tool",
        "--output",
        "agentprof-watch-test-output.txt",
    ])
    .write_stdin("");
    cmd.assert()
        .failure()
        .code(1)
        .stderr(contains("does not accept --export"));
}
