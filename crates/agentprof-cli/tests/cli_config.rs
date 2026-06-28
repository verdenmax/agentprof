//! Integration tests for `agentprof config`. Env-isolated via `.env`
//! (`AGENTPROF_CONFIG` points each child process at a tempdir file).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("agentprof").expect("binary builds")
}

#[test]
fn config_path_reports_existing_file() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\nmode = \"cache\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains(cfg.to_str().unwrap()))
        .stdout(predicate::str::contains("[exists]"))
        .stdout(predicate::str::contains("(from $AGENTPROF_CONFIG)"));
}

#[test]
fn config_path_reports_missing_file() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("absent.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[not found]"));
}
