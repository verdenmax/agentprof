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

/// `list` writes no sidecar, so anonymize and redact are identical output.
#[test]
fn list_privacy_anonymize_matches_redact() {
    let root = copilot_fixtures_root();
    let mk = |lvl: &str| {
        Command::cargo_bin("agentprof")
            .expect("cargo_bin")
            .args(["--no-cache", "list", "--agent", "copilot", "--root"])
            .arg(&root)
            .args(["--since", "all", "--limit", "100", "--privacy", lvl])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    assert_eq!(mk("redact"), mk("anonymize"), "list: redact == anonymize");
}
