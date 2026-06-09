//! M2.1 T4.3: confirm the three new global flags
//! (`--no-cache`, `--storage-path`, `--quiet`) are accepted by clap on
//! every subcommand. We probe `list` with a deliberately bogus `--root`
//! so the command itself fails *after* successful flag parsing; an
//! "unknown argument" clap error would surface as exit code 2 + an
//! `error:` line on stderr, which is what we'd actually be guarding
//! against.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn agentprof() -> Command {
    Command::cargo_bin("agentprof").expect("agentprof binary built")
}

#[test]
fn no_cache_flag_accepted() {
    agentprof()
        .args([
            "--no-cache",
            "list",
            "--agent",
            "copilot",
            "--root",
            "/nonexistent-agentprof-t43",
        ])
        .assert()
        .failure()
        .stderr(contains("unrecognized").not())
        .stderr(contains("unexpected argument").not());
}

#[test]
fn quiet_flag_accepted() {
    agentprof()
        .args([
            "--quiet",
            "list",
            "--agent",
            "copilot",
            "--root",
            "/nonexistent-agentprof-t43",
        ])
        .assert()
        .failure()
        .stderr(contains("unrecognized").not())
        .stderr(contains("unexpected argument").not());
}

#[test]
fn storage_path_flag_accepted() {
    agentprof()
        .args([
            "--storage-path",
            "/nonexistent-agentprof-t43/foo.sqlite",
            "list",
            "--agent",
            "copilot",
            "--root",
            "/nonexistent-agentprof-t43",
        ])
        .assert()
        .failure()
        .stderr(contains("unrecognized").not())
        .stderr(contains("unexpected argument").not());
}

#[test]
fn all_three_flags_compose() {
    agentprof()
        .args([
            "--no-cache",
            "--quiet",
            "--storage-path",
            "/nonexistent-agentprof-t43/foo.sqlite",
            "list",
            "--agent",
            "copilot",
            "--root",
            "/nonexistent-agentprof-t43",
        ])
        .assert()
        .failure()
        .stderr(contains("unrecognized").not())
        .stderr(contains("unexpected argument").not());
}

#[test]
fn flags_listed_in_help_output() {
    let out = agentprof().args(["list", "--help"]).output().expect("help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--no-cache"), "stdout:\n{stdout}");
    assert!(stdout.contains("--storage-path"), "stdout:\n{stdout}");
    assert!(stdout.contains("--quiet"), "stdout:\n{stdout}");
}
