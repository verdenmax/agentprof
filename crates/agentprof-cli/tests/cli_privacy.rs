//! E2E: `analyze --privacy` redacts PII; anonymize writes a sidecar.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use assert_cmd::Command;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

#[test]
fn analyze_privacy_redact_strips_pii_from_md() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "analyze", "--agent", "copilot", "--root"])
        .arg(fixtures_root())
        .args([
            "--session",
            "00000000-0000-0000-0000-000000000006",
            "--export",
            "md",
            "--privacy",
            "redact",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(!s.contains("/tmp/agentprof-fixture"), "cwd leaked:\n{s}");
    assert!(
        s.contains("<uuid-0>") || s.contains("<redacted>"),
        "no redaction marker:\n{s}"
    );
}

#[test]
fn analyze_privacy_anonymize_writes_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("r.json");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "analyze", "--agent", "copilot", "--root"])
        .arg(fixtures_root())
        .args([
            "--session",
            "00000000-0000-0000-0000-000000000006",
            "--export",
            "json",
            "--privacy",
            "anonymize",
            "--output",
        ])
        .arg(&report)
        .assert()
        .success();
    let sidecar = dir.path().join("agentprof-redaction-map.json");
    assert!(sidecar.exists(), "sidecar not written");
    let map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
    assert!(map.get("uuids").is_some(), "map missing uuids key");
}

#[test]
fn analyze_privacy_anonymize_redacts_html_meta() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("r.html");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "analyze", "--agent", "copilot", "--root"])
        .arg(fixtures_root())
        .args([
            "--session",
            "00000000-0000-0000-0000-000000000006",
            "--export",
            "html",
            "--privacy",
            "anonymize",
            "--output",
        ])
        .arg(&report)
        .assert()
        .success();
    let html = std::fs::read_to_string(&report).unwrap();
    // Part A redacts the meta the html *header* renders from (`report.meta`):
    // the original session id and started_at must not appear in the header.
    //
    // The fixture's session id is `...006` and it starts at 2026-05-26T14:00:01Z.
    // After `anonymize`, the id maps to `<uuid-0>` and started_at to the Unix
    // epoch (1970). We assert the header carries the redacted forms and that the
    // original session id is gone entirely.
    //
    // NOTE: per-turn timestamps + flamegraph frame turn-ids are a KNOWN deferred
    // leak (Part B warn) — `episodes` / turn rows are not yet redacted — so we do
    // NOT assert the whole document is free of the original date here.
    assert!(
        html.contains("Started: 1970-01-01"),
        "header started_at not redacted to epoch:\n{}",
        &html[..html.len().min(600)]
    );
    assert!(
        !html.contains("00000000-0000-0000-0000-000000000006"),
        "original session id leaked in html"
    );
    assert!(
        html.contains("uuid-0"),
        "expected anonymized session id placeholder in html header"
    );
}
