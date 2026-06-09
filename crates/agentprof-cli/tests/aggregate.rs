//! Integration tests for `agentprof aggregate` (M1.6.2).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

/// Root holding the 3 multi-sess-* fixtures plus other Copilot fixtures.
/// `aggregate` will walk every sub-dir that looks like a session, which
/// includes more than the 3 multi-sess fixtures; this is fine for tests
/// since each renderer only needs *some* bash row / day row to exist.
fn multi_sess_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

/// Isolated scratch root containing **only** the 3 `multi-sess-*` fixtures
/// (closes `m1.6.2-followup-i3-fixture-isolation`).
///
/// Without this, the aggregate snapshot tests (`aggregate_md_snapshot_by_*`,
/// `aggregate_html_snapshot_by_tool`) walk every directory under
/// `agentprof-adapters/tests/fixtures/copilot/` — 20+ fixtures including
/// `with-skill-invoked`, `with-span-overlap`, `with-mcp-calls`, etc. that
/// have nothing to do with the cross-session aggregate behaviour the
/// snapshots are pinning. Adding any future copilot fixture forced
/// regenerating these aggregate snapshots; B1 hit a milder version of this.
///
/// The scratch root copies just the 3 multi-sess-* directories into a
/// fresh `tempfile::TempDir`; copy (not symlink) avoids cross-platform
/// symlink-permission noise on CI and keeps the test deterministic.
/// Returned `TempDir` lives for the test's duration; drop cleans up.
fn isolated_multi_sess_root() -> tempfile::TempDir {
    use std::fs;
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot");
    let tmp = tempfile::tempdir().expect("create tempdir");
    for slug in ["multi-sess-a", "multi-sess-b", "multi-sess-c"] {
        let src_dir = src.join(slug);
        let dst_dir = tmp.path().join(slug);
        fs::create_dir_all(&dst_dir).expect("create dst dir");
        for entry in fs::read_dir(&src_dir).expect("read fixture dir") {
            let entry = entry.expect("read fixture entry");
            let dst = dst_dir.join(entry.file_name());
            fs::copy(entry.path(), &dst).expect("copy fixture file");
        }
    }
    tmp
}

#[test]
fn aggregate_by_tool_md_emits_header_and_rows() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "tool", "--since", "all"])
        .args(["--root"])
        .arg(multi_sess_root())
        .assert()
        .success()
        .stdout(contains("# agentprof aggregate"))
        .stdout(contains("By: tool"))
        .stdout(contains("bash"));
}

#[test]
fn aggregate_by_day_md_marks_low_utilization() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "day", "--since", "all"])
        .args(["--low-utilization-threshold", "99"])
        .args(["--root"])
        .arg(multi_sess_root())
        .assert()
        .success()
        .stdout(contains("⚠"));
}

#[test]
fn aggregate_by_mcp_server_json_round_trip() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--no-cache",
            "aggregate",
            "--by",
            "mcp-server",
            "--since",
            "all",
        ])
        .args(["--export", "json"])
        .args(["--root"])
        .arg(multi_sess_root())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let val: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
    assert_eq!(val["by"], "mcp_server");
    assert!(val["data"]["buckets"].is_array());
}

#[test]
fn aggregate_by_tool_csv_has_correct_header() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "tool", "--since", "all"])
        .args(["--export", "csv"])
        .args(["--root"])
        .arg(multi_sess_root())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let first_line = s.lines().next().expect("csv has ≥ 1 line");
    assert!(
        first_line.starts_with("name,source,call_count,"),
        "first line: {first_line}"
    );
}

#[test]
fn aggregate_by_tool_html_has_valid_html_no_script() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "tool", "--since", "all"])
        .args(["--export", "html"])
        .args(["--root"])
        .arg(multi_sess_root())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("<html"));
    assert!(s.contains("<table"));
    assert!(!s.contains("<script"));
}

#[test]
fn aggregate_zero_sessions_no_window_match_exits_zero() {
    let empty_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("agentprof-test-empty-aggregate-root");
    let _ = std::fs::create_dir_all(&empty_dir);
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "tool"])
        .args(["--root"])
        .arg(&empty_dir)
        .assert()
        .success()
        .stderr(contains("no sessions matching"));
}

#[test]
fn aggregate_nonexistent_root_exits_user_error() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "tool"])
        .args(["--root", "/nonexistent-agentprof-aggregate-dir"])
        .assert()
        .code(1)
        .stderr(contains("session root not found"));
}

#[test]
fn aggregate_invalid_threshold_exits_user_error() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "--no-cache",
            "aggregate",
            "--by",
            "day",
            "--low-utilization-threshold",
            "150",
        ])
        .args(["--root"])
        .arg(multi_sess_root())
        .assert()
        .code(1)
        .stderr(contains("--low-utilization-threshold"));
}

#[test]
fn aggregate_md_snapshot_by_tool() {
    let tmp = isolated_multi_sess_root();
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "tool", "--since", "all"])
        .args(["--root"])
        .arg(tmp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    insta::assert_snapshot!("aggregate_md__by_tool", s);
}

#[test]
fn aggregate_md_snapshot_by_day() {
    let tmp = isolated_multi_sess_root();
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "day", "--since", "all"])
        .args(["--root"])
        .arg(tmp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    insta::assert_snapshot!("aggregate_md__by_day", s);
}

#[test]
fn aggregate_html_snapshot_by_tool() {
    let tmp = isolated_multi_sess_root();
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--by", "tool", "--since", "all"])
        .args(["--export", "html"])
        .args(["--root"])
        .arg(tmp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut html = String::from_utf8(out).unwrap();
    let real_version = env!("CARGO_PKG_VERSION");
    html = html.replace(real_version, "0.0.0");
    // Strip the volatile rfc3339 timestamp in the footer.
    if let Some(idx) = html.find("v0.0.0 on ") {
        let tail_start = idx + "v0.0.0 on ".len();
        if let Some(end_rel) = html[tail_start..].find('\n') {
            let end = tail_start + end_rel;
            html.replace_range(tail_start..end, "<DATE>");
        }
    }
    insta::assert_snapshot!("aggregate_html__by_tool", html);
}

#[test]
fn aggregate_export_tui_requires_tty_not_unsupported() {
    // M1.6.3: --export tui is now supported but requires a TTY.
    // Piping stdin from /dev/null makes is_terminal() return false,
    // so we should exit with OutputError (3) and a TTY-related message,
    // NOT the M1.6.2 "tui not supported in M1.6.2" message.
    let mut cmd = Command::cargo_bin("agentprof").unwrap();
    cmd.args([
        "--no-cache",
        "aggregate",
        "--by",
        "tool",
        "--since",
        "30d",
        "--export",
        "tui",
    ])
    .write_stdin("");
    cmd.assert()
        .failure()
        .code(3)
        .stderr(contains("TTY").or(contains("tty")).or(contains("terminal")));
}
