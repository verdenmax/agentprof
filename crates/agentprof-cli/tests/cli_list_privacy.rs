//! E2E: `list --privacy` redacts per-row identifiers (F-10 T4).
//!
//! Spawns the binary via `assert_cmd` against the committed Copilot
//! fixtures. `--no-cache` keeps the pure filesystem path; `--since all`
//! neutralizes committed-fixture mtime staleness so rows actually render.
//!
//! At `--privacy redact` the displayed session id becomes `<uuid-N>` and
//! the model collapses to its family (`claude-opus-4.7-1m-internal` →
//! `claude-opus`); `--privacy none` is byte-identical to omitting the flag.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// Absolute path to the committed Copilot fixtures directory.
fn copilot_fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace crates dir")
        .join("agentprof-adapters/tests/fixtures/copilot")
}

#[test]
fn list_privacy_redact_replaces_ids_and_collapses_model() {
    let out = Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args(["--no-cache", "list", "--agent", "copilot", "--root"])
        .arg(copilot_fixtures_root())
        .args(["--since", "all", "--limit", "100", "--privacy", "redact"])
        .assert()
        .success()
        .stdout(contains("<uuid-0>"))
        .stdout(contains("claude-sonnet"))
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).expect("utf8");
    assert!(
        !text.contains("claude-sonnet-4.6"),
        "redact should collapse model to family; got:\n{text}"
    );
    assert!(
        !text.contains("multi-sess-a"),
        "redact should replace original session id; got:\n{text}"
    );
}

#[test]
fn list_privacy_none_is_byte_identical_to_no_flag() {
    let root = copilot_fixtures_root();
    let no_flag = Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args(["--no-cache", "list", "--agent", "copilot", "--root"])
        .arg(&root)
        .args(["--since", "all", "--limit", "100"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let none = Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args(["--no-cache", "list", "--agent", "copilot", "--root"])
        .arg(&root)
        .args(["--since", "all", "--limit", "100", "--privacy", "none"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(no_flag, none, "--privacy none must not alter output");
}

/// Audit leak B: at `anonymize`, the `Started` column is zeroed to epoch so a
/// raw 2026-era date never appears; `redact` keeps the real timestamp.
#[test]
fn list_privacy_anonymize_zeroes_started_at_redact_keeps() {
    let root = copilot_fixtures_root();
    let mk = |lvl: &str| {
        let out = Command::cargo_bin("agentprof")
            .expect("cargo_bin")
            .args(["--no-cache", "list", "--agent", "copilot", "--root"])
            .arg(&root)
            .args(["--since", "all", "--limit", "100", "--privacy", lvl])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(out).expect("utf8")
    };
    let anon = mk("anonymize");
    let red = mk("redact");
    assert!(
        anon.contains("1970-01-01"),
        "anonymize should zero Started to epoch; got:\n{anon}"
    );
    assert!(
        !anon.contains("2026-"),
        "anonymize leaked a raw started_at date; got:\n{anon}"
    );
    assert!(
        red.contains("2026-"),
        "redact must keep the real started_at; got:\n{red}"
    );
}

#[test]
fn list_privacy_redacts_empty_root_diagnostic() {
    let empty = TempDir::new().expect("tempdir");
    let raw_root = empty.path().display().to_string();
    let out = Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args(["--no-cache", "list", "--agent", "copilot", "--root"])
        .arg(empty.path())
        .args(["--since", "all", "--privacy", "anonymize"])
        .assert()
        .success()
        .stdout(contains("<redacted>"))
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).expect("utf8");
    assert!(
        !text.contains(&raw_root),
        "privacy mode should not print raw empty root path; got:\n{text}"
    );
}
