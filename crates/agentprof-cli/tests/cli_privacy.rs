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
