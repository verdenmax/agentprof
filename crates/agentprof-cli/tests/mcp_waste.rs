//! Integration tests for `agentprof mcp-waste` subcommand.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::str::contains;

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

#[test]
fn mcp_waste_md_default_renders_summary_header() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["mcp-waste", "--root"])
        .arg(fixture_root())
        .args(["--since", "all"])
        .assert()
        .success()
        .stdout(contains("# MCP Waste Report"))
        .stdout(contains("## Summary"))
        .stdout(contains("Always unused"))
        .get_output()
        .stdout
        .clone();
    // `generated YYYY-MM-DD` uses `chrono::Utc::now()` in the renderer;
    // redact the 10-char date to keep the snapshot stable across days.
    let s = String::from_utf8(out).unwrap();
    let redacted = redact_generated_date(&s);
    insta::assert_snapshot!("mcp_waste_md_default", redacted);
}

/// Replace `generated YYYY-MM-DD` with `generated [DATE]`. We do this
/// manually instead of pulling in `regex` because the substitution is
/// trivial and the workspace doesn't already depend on `regex`.
fn redact_generated_date(s: &str) -> String {
    let needle = "generated ";
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find(needle) {
        out.push_str(&rest[..idx + needle.len()]);
        let after = &rest[idx + needle.len()..];
        // YYYY-MM-DD = 10 chars; redact if the prefix looks date-shaped.
        if after.len() >= 10
            && after.as_bytes()[4] == b'-'
            && after.as_bytes()[7] == b'-'
            && after.as_bytes()[..10].iter().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    *b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
        {
            out.push_str("[DATE]");
            rest = &after[10..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn mcp_waste_json_default_is_valid_json() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["mcp-waste", "--root"])
        .arg(fixture_root())
        .args(["--since", "all", "--export", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&s).expect("valid json");
    assert!(parsed.get("sessions").is_some());
    assert!(parsed.get("per_server").is_some());
    assert!(parsed.get("never_called_tools").is_some());
}

#[test]
fn mcp_waste_html_includes_expected_table() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["mcp-waste", "--root"])
        .arg(fixture_root())
        .args(["--since", "all", "--export", "html"])
        .assert()
        .success()
        .stdout(contains("<!DOCTYPE html>"))
        .stdout(contains("MCP Waste Report"))
        .stdout(contains("Per-server cross-session"));
}

#[test]
fn mcp_waste_writes_output_file_when_output_flag_given() {
    let tmp = tempfile::tempdir().unwrap();
    let outpath = tmp.path().join("waste.md");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["mcp-waste", "--root"])
        .arg(fixture_root())
        .args(["--since", "all", "--output"])
        .arg(&outpath)
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
    let contents = std::fs::read_to_string(&outpath).unwrap();
    assert!(contents.starts_with("# MCP Waste Report"));
}

#[test]
fn mcp_waste_no_sessions_exits_data_error() {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["mcp-waste", "--root"])
        .arg(tmp.path())
        .args(["--since", "all"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn mcp_waste_top_flag_limits_table_rows() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["mcp-waste", "--root"])
        .arg(fixture_root())
        .args(["--since", "all", "--top", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(
        s.contains("(top 0)"),
        "expected '(top 0)' in output, got:\n{s}"
    );
}
