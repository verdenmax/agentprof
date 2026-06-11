//! `ingest-otlp` CLI surface for the M2.4 capacity flags.
//!
//! Asserts that `--help` lists all 4 new flags and that `--max-open-sessions 16
//! --max-logs-request-bytes 1024 ...` does not error out at CLI parse time.
//! Deep behavior is covered by storage-crate integration tests (`otlp_caps_smoke`,
//! `otlp_router_lru`); this file only locks the operator-facing flag surface.

#![cfg(feature = "otlp")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ingest_otlp_help_lists_new_caps_flags() {
    Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args(["ingest-otlp", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--max-logs-request-bytes"))
        .stdout(predicate::str::contains("--max-metrics-request-bytes"))
        .stdout(predicate::str::contains("--max-traces-request-bytes"))
        .stdout(predicate::str::contains("--max-open-sessions"));
}

#[test]
fn ingest_otlp_accepts_new_caps_flags() {
    // We pass --no-grpc --no-http to make the run exit immediately with a
    // user error (instead of binding for real) — we only care that the cap
    // flags PARSE without error. Stderr will contain the usual "at least
    // one of --grpc / --http" message.
    Command::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args([
            "ingest-otlp",
            "--no-grpc",
            "--no-http",
            "--max-logs-request-bytes",
            "1048576",
            "--max-metrics-request-bytes",
            "524288",
            "--max-traces-request-bytes",
            "1048576",
            "--max-open-sessions",
            "16",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("at least one of"));
}
