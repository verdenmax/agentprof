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

#[test]
fn show_marks_file_values_and_defaults() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    // Only `mode` is set in the file; auto_prune_days stays default.
    std::fs::write(&cfg, "[storage]\nmode = \"store\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mode = \"store\"  (from file)"))
        .stdout(predicate::str::contains("auto_prune_days = 30  (default)"));
}

#[test]
fn show_without_file_is_all_defaults() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("absent.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[not found]"))
        .stdout(predicate::str::contains("mode = \"cache\"  (default)"));
}

#[test]
fn show_rejects_malformed_config() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\nmode = = broken\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .code(2) // ExitKind::DataError
        .stderr(predicate::str::contains("failed to parse"));
}

// Guards the otlp+serve source-flag→line mappings (the highest-risk surface
// that a swap would compile cleanly past). Requires both features for the
// `[otlp]`/`[serve]` blocks to parse.
#[cfg(all(feature = "otlp", feature = "web"))]
#[test]
fn show_marks_otlp_and_serve_overrides() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(
        &cfg,
        "[otlp]\nlisten_token = \"sek\"\n[serve]\nauto_open = false\n",
    )
    .unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("listen_token = \"sek\"  (from file)"))
        // Non-overridden neighbor stays (default) — guards against a swap.
        .stdout(predicate::str::contains("listen_http = \"127.0.0.1:4318\"  (default)"))
        .stdout(predicate::str::contains("auto_open = false  (from file)"));
}

#[test]
fn init_writes_template_and_creates_parent() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("nested").join("config.toml"); // parent absent
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "init"])
        .assert()
        .success();
    assert!(cfg.exists());
    // The template must itself parse cleanly via `show`.
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success();
}

#[test]
fn init_refuses_existing_without_force() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "init"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("already exists"));
    // Refusing must NOT touch the file (guards against truncate-then-refuse).
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "[storage]\n");
}

#[test]
fn init_force_overwrites() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\nmode = \"store\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "init", "--force"])
        .assert()
        .success();
    let written = std::fs::read_to_string(&cfg).unwrap();
    assert!(written.contains("agentprof configuration"));
}
