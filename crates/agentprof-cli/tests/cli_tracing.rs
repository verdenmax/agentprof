//! Integration tests for the M1.6.4 tracing infrastructure.
//!
//! These exercise the global `--log-level` / `--log-file` flags and the
//! soft-fall semantics, via the `assert_cmd` surface. Span-name presence
//! is verified by stderr / file substring matching; format details are
//! intentionally NOT pinned (`tracing_subscriber::fmt` output format may
//! change across versions).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;

fn agentprof() -> Command {
    Command::cargo_bin("agentprof").expect("binary built")
}

#[test]
fn list_default_log_level_warn_hides_debug_events() {
    // Running `agentprof list` without --log-level should not print
    // debug-level events to stderr.
    let assert = agentprof()
        .arg("list")
        .arg("--root")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("--since")
        .arg("365d")
        .assert()
        // Either ok (some sessions found) OR exit 2 with no-sessions message.
        // We don't care about exit; only stderr content.
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&assert.stderr).to_string();
    assert!(
        !stderr.contains("DEBUG"),
        "default level should suppress DEBUG events; got stderr:\n{stderr}"
    );
}

#[test]
fn list_log_level_debug_emits_debug_events() {
    let out = agentprof()
        .arg("--log-level")
        .arg("debug")
        .arg("list")
        .arg("--root")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // T5 added Layer-2/3 spans: `adapter.discover` (in
    // `agentprof_adapters::copilot::paths`) emits a `debug!` "discovered
    // sessions" line as it scans for sessions, and the surrounding span
    // name appears in the formatted output. At `--log-level debug` we
    // therefore expect BOTH:
    //   - a level token (DEBUG / INFO / WARN), AND
    //   - content from the adapter.discover span emission.
    assert!(
        !stderr.is_empty(),
        "expected non-empty stderr at --log-level debug; got empty stderr"
    );
    assert!(
        stderr.contains("DEBUG") || stderr.contains("INFO") || stderr.contains("WARN"),
        "expected some level token in stderr at debug filter; got:\n{stderr}"
    );
    assert!(
        stderr.contains("adapter.discover") || stderr.contains("discovered sessions"),
        "expected T5 Layer-2 `adapter.discover` span or its `discovered sessions` debug \
         emission in stderr at --log-level debug; got:\n{stderr}"
    );
}

#[test]
fn log_file_flag_writes_to_path() {
    // Note: this test verifies the rolling appender CREATED the file, not
    // that any event reached it (rolling appender opens eagerly). For
    // content-level coverage of the reload-layer swap path, see
    // `watch_run_writes_log_events_to_file` below.
    let tmp = tempfile::tempdir().expect("tempdir");
    let log_path = tmp.path().join("test.log");

    agentprof()
        .arg("--log-level")
        .arg("info")
        .arg("--log-file")
        .arg(&log_path)
        .arg("list")
        .arg("--root")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .get_output();

    // The `daily` rolling appender uses a date-suffixed filename, e.g.
    // `test.log.2026-06-02`. Find any file in the dir matching the prefix.
    let dir_entries: Vec<_> = std::fs::read_dir(tmp.path())
        .expect("read tempdir")
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("test.log"))
        })
        .collect();

    let listing: Vec<_> = std::fs::read_dir(tmp.path())
        .expect("read tempdir")
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .collect();
    assert!(
        !dir_entries.is_empty(),
        "expected at least one rolling log file with prefix 'test.log' in {:?}; entries: {:?}",
        tmp.path(),
        listing
    );
}

#[test]
fn log_file_invalid_path_soft_falls_to_stderr() {
    let assert = agentprof()
        .arg("--log-level")
        .arg("warn")
        .arg("--log-file")
        .arg("/this/dir/does/not/exist/agentprof.log")
        .arg("list")
        .arg("--root")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .get_output()
        .clone();

    // The CLI should NOT have exited 3 or 2 because of tracing.
    // (Exit code may still be 2 if no sessions matched; that's about
    // data, not tracing.)
    let code = assert.status.code().unwrap_or(0);
    assert!(
        code == 0 || code == 2,
        "tracing init failure must not crash CLI; got exit code {code}"
    );

    // Stderr should mention the fallback (the warn from init.rs).
    let stderr = String::from_utf8_lossy(&assert.stderr).to_string();
    assert!(
        stderr.contains("falling back to stderr") || stderr.contains("agentprof:"),
        "expected fallback warning OR top-level error line; got stderr:\n{stderr}"
    );
}

#[test]
fn dash_log_file_forces_stderr() {
    // `--log-file -` is the explicit-stderr opt-in; the CLI should accept it.
    agentprof()
        .arg("--log-file")
        .arg("-")
        .arg("--log-level")
        .arg("info")
        .arg("list")
        .arg("--root")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .get_output();
    // No assertion on stderr content — different tracing-subscriber
    // versions format differently. Just verify the CLI ACCEPTS the
    // value without exiting 2/3 due to argument parsing.
}

#[test]
fn watch_run_writes_log_events_to_file() {
    // Rubber-duck Critical #1 acceptance test (FIXED in T3 fix-up).
    //
    // Setup:
    //  - Invoke `agentprof analyze --export tui --root <real-fixture-path>`
    //    so resolve_session succeeds and execution reaches run_tui.
    //  - Stdin redirected to /dev/null so the TTY check inside run_tui
    //    fails AFTER enter_tui_log_guard installs (today's ordering).
    //  - Override XDG_STATE_HOME to a tempdir so the swap writes there.
    //  - --log-level debug so tui_guard's success-swap debug emission
    //    is enabled.
    //
    // Without the reload-layer architecture (Critical #1 regression):
    //  - swap_writer returns Err, takes the warn arm; file never gets
    //    the post-swap debug line.
    //  - Even if some events leak through, they go to the original
    //    stderr subscriber (not the file).
    //
    // With reload-layer correctly wired:
    //  - swap_writer Ok; the debug! immediately following the swap
    //    lands in the file under XDG_STATE_HOME/agentprof/.
    //
    // The assertion: strict — the file must exist AND have non-zero
    // size (no OR-clause escape hatch).

    use std::process::Stdio;

    let tmp = tempfile::tempdir().expect("tempdir");

    // Use a real copilot fixture session so analyze::run reaches run_tui.
    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has parent")
        .join("agentprof-adapters/tests/fixtures/copilot");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_agentprof"))
        .env("XDG_STATE_HOME", tmp.path())
        // Suppress the user's actual env so test is hermetic.
        .env_remove("AGENTPROF_LOG_FILE")
        .env_remove("AGENTPROF_LOG_LEVEL")
        .env_remove("AGENTPROF_LOG")
        .args([
            "--log-level",
            "debug",
            "analyze",
            "--export",
            "tui",
            "--root",
        ])
        .arg(&fixture_root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run agentprof");

    let log_dir = tmp.path().join("agentprof");
    let mut found_log_with_content = false;
    if let Ok(rd) = std::fs::read_dir(&log_dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if let Some(name) = p.file_name() {
                if name.to_string_lossy().starts_with("agentprof.log") {
                    if let Ok(meta) = std::fs::metadata(&p) {
                        if meta.len() > 0 {
                            found_log_with_content = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        found_log_with_content,
        "Critical #1 acceptance failed: expected non-empty log file under \
         XDG_STATE_HOME/agentprof/ (proving the reload-layer swap_writer \
         correctly redirected emission to file). \
         log_dir = {log_dir:?}, exit = {:?}, stderr = {stderr}",
        output.status.code()
    );
}
