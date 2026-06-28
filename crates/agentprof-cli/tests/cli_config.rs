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

#[test]
fn edit_creates_template_when_absent() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env("EDITOR", "true") // unix no-op editor, exits 0
        .env_remove("VISUAL")
        .args(["config", "edit"])
        .assert()
        .success();
    // Template content written (not just an empty file).
    assert!(std::fs::read_to_string(&cfg)
        .unwrap()
        .contains("agentprof configuration"));
}

#[test]
fn edit_without_editor_env_errors() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(["config", "edit"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("set $VISUAL or $EDITOR"));
}

#[test]
fn edit_preserves_existing_file() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\nmode = \"store\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env("EDITOR", "true")
        .env_remove("VISUAL")
        .args(["config", "edit"])
        .assert()
        .success();
    // edit must open the file as-is, never re-template it.
    assert_eq!(
        std::fs::read_to_string(&cfg).unwrap(),
        "[storage]\nmode = \"store\"\n"
    );
}

#[test]
fn edit_empty_visual_falls_through_to_editor() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    // An empty $VISUAL must NOT shadow a valid $EDITOR (regression: an empty
    // var was once spawned as `""`, yielding a misleading exit 3).
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env("VISUAL", "")
        .env("EDITOR", "true")
        .args(["config", "edit"])
        .assert()
        .success();
}

#[test]
fn edit_nonzero_editor_exits_user_error() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env("EDITOR", "false") // runs but exits 1
        .env_remove("VISUAL")
        .args(["config", "edit"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("editor exited with status"));
}

#[test]
fn edit_prefers_visual_over_editor() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    // VISUAL=true (exit 0) preferred over EDITOR=false (exit 1) => success.
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env("VISUAL", "true")
        .env("EDITOR", "false")
        .args(["config", "edit"])
        .assert()
        .success();
}

// Locks the `show` resolve-error path (exit 2): TOML parses, but a block
// fails to resolve via its real resolver. Distinct from a parse failure.
#[cfg(feature = "otlp")]
#[test]
fn show_rejects_unresolvable_otlp_block() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[otlp]\nsession_idle_timeout = \"not-a-duration\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid [otlp] config"));
}

// Locks the editor spawn-failure path (exit 3) — distinct from a runnable
// editor returning non-zero (exit 1).
#[test]
fn edit_spawn_failure_is_output_error() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env("EDITOR", "/nonexistent/xyz-editor-binary")
        .env_remove("VISUAL")
        .args(["config", "edit"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("failed to launch editor"));
}
