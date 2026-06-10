//! CLI surface tests for `agentprof ingest-otlp` (M2.2 T8.1).
//!
//! These are deliberately shallow — they only assert the clap-derived
//! help text exposes the documented flags and that the obvious
//! "no listener at all" validation triggers. Deep end-to-end
//! transport + persistence coverage is owned by T9.1.

#![cfg(feature = "otlp")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn ingest_otlp_help_lists_required_flags() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["ingest-otlp", "--help"])
        .assert()
        .success()
        .stdout(contains("--grpc"))
        .stdout(contains("--http"))
        .stdout(contains("--bearer-token"))
        .stdout(contains("--tls-cert"))
        .stdout(contains("--tls-key"))
        .stdout(contains("--client-ca"))
        .stdout(contains("--max-session-bytes"))
        .stdout(contains("--max-session-events"))
        .stdout(contains("--idle-seconds"))
        .stdout(contains("--store"));
}

#[test]
fn ingest_otlp_rejects_both_listeners_disabled() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["ingest-otlp", "--no-grpc", "--no-http"])
        .assert()
        .failure()
        .stderr(contains("at least one of"));
}
