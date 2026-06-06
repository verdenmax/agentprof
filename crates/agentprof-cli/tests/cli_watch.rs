//! Integration tests for `agentprof watch` (M1.6.3).
//!
//! All tests use non-TTY stdin/stdout so the watcher loop never starts —
//! they only exercise argument parsing + early validation. The live
//! watcher is covered by manual smoke per spec D-14.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
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
        // Wave D3 (`m1.6.3-t2-followup-tighten-cli-watch-predicate`):
        // tightened from `contains("TTY").or("tty").or("terminal")`
        // to the literal substring the error actually contains. Both
        // watch + aggregate `check_tty_for_tui` errors include the
        // plural "TTYs" verbatim — pinning that is precise without
        // being brittle (any future error rewording that drops the
        // word entirely should re-trip this test by intent).
        .stderr(contains("TTYs"));
}

#[test]
fn watch_aggregate_non_tty_exits_output_error() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch", "aggregate", "--by", "tool"])
        .write_stdin("");
    cmd.assert().failure().code(3).stderr(contains("TTYs"));
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

// ──────────────────────────────────────────────────────────────────────
// Wave D3 (`m1.6.3-t2-followup-clap-arg-ordering`): top-level --debounce-ms
// / --agent / --root / --session are now `global = true`, so they accept
// either position relative to the `aggregate` subcommand. Both invocations
// should reach the TTY check (which fails with exit 3 + "TTYs" on a
// non-TTY pipe) — proves clap parsed the flag without an UNKNOWN error.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn watch_aggregate_accepts_debounce_after_subcommand() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch", "aggregate", "--by", "tool", "--debounce-ms", "500"])
        .write_stdin("");
    cmd.assert().failure().code(3).stderr(contains("TTYs"));
}

#[test]
fn watch_aggregate_accepts_debounce_before_subcommand() {
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args(["watch", "--debounce-ms", "500", "aggregate", "--by", "tool"])
        .write_stdin("");
    cmd.assert().failure().code(3).stderr(contains("TTYs"));
}
