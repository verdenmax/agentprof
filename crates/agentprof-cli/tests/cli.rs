//! End-to-end CLI integration tests.
//!
//! Spawns the `agentprof` binary via `assert_cmd` against committed
//! fixtures under `crates/agentprof-adapters/tests/fixtures/copilot/`.
//!
//! Verifies:
//! - md happy path: structure + turn ids visible
//! - json happy path: parseable + expected top-level keys + ADR-0005 D-2 invariant
//! - `--output` writes a file and acknowledges on stderr
//! - error paths:
//!   - exit 1 (UserError): nonexistent root, unknown UUID, unsupported agent
//!   - exit 2 (DataError): malformed events.jsonl with no session.start
//!   - exit 3 (OutputError): `--output` to a path under a non-existent dir
//! - md insta snapshot for cross-turn-tool locks ADR-0005 D-2 fix
//!   visible in user-facing report

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

/// Path to the cross-turn-tool fixture directory.
///
/// We use the Path selector (not Uuid) because `SessionRef.id` is the
/// directory name set by `CopilotAdapter::discover_sessions`, NOT the
/// inner wire-format `sessionId` field. So `--session cross-turn-tool`
/// would be rejected by the `looks_like_uuid` heuristic, and
/// `--session 00000000-...-001000` (the inner UUID) doesn't match the
/// `SessionRef` set built by discovery. Path selector sidesteps both
/// problems and is what real users would type anyway.
fn cross_turn_path() -> PathBuf {
    fixtures_root().join("cross-turn-tool")
}

#[test]
fn analyze_md_to_stdout_succeeds_for_cross_turn_tool() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "md"])
        .assert()
        .success()
        .stdout(contains("# agentprof analyze"))
        .stdout(contains("## Session"))
        .stdout(contains("## Turn Summary"))
        .stdout(contains("turn-A"))
        .stdout(contains("turn-B"));
}

#[test]
fn analyze_json_to_stdout_is_valid_json() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&s).expect("output must be valid JSON");
    assert!(parsed.get("meta").is_some(), "missing meta");
    assert!(parsed.get("turn_summary").is_some(), "missing turn_summary");
    assert!(parsed.get("tool_rank").is_some(), "missing tool_rank");
    assert!(parsed.get("hook_rank").is_some(), "missing hook_rank");
    assert!(parsed.get("warnings").is_some(), "missing warnings");
    let turns = parsed["turn_summary"].as_array().unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["tool_call_count"], 1);
    assert_eq!(turns[1]["tool_call_count"], 0);
}

#[test]
fn analyze_writes_to_output_file_when_specified() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out_path = tmp.path().join("report.md");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "md", "--output"])
        .arg(&out_path)
        .assert()
        .success()
        .stderr(contains("wrote"))
        .stderr(contains("bytes"));
    let written = std::fs::read_to_string(&out_path).unwrap();
    assert!(written.starts_with("# agentprof analyze"));
    assert!(written.contains("turn-A"));
}

#[test]
fn analyze_nonexistent_root_exits_with_user_error() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args([
            "analyze",
            "--root",
            "/nonexistent/agentprof/path",
            "--session",
            "latest",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("session root not found"));
}

#[test]
fn analyze_unknown_session_uuid_exits_with_user_error_and_helpful_message() {
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--root"])
        .arg(fixtures_root())
        .args(["--session", "99999999-9999-9999-9999-999999999999"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("not found"))
        .stderr(contains("first 5 available"));
}

#[test]
fn analyze_md_snapshot_cross_turn_tool() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let md = String::from_utf8(out).unwrap();
    insta::assert_snapshot!("analyze_md__cross_turn_tool", md);
}

#[test]
fn analyze_unparseable_session_exits_with_data_error() {
    // Write a malformed events.jsonl that has no session.start, so the
    // adapter's load_session returns AdapterError::MissingSessionStart.
    // run() maps this to ExitKind::DataError = 2.
    //
    // (The committed `corrupt` fixture has a valid session.start + one
    // bad line, which the parser tolerates with a warning and still
    // returns Ok — that's a different fixture for a different scenario.
    // Here we need a fixture that fully fails to parse, so we synthesize
    // one inline rather than committing another *.jsonl.)
    let tmp = tempfile::TempDir::new().unwrap();
    let session_dir = tmp.path().join("broken-session");
    std::fs::create_dir(&session_dir).unwrap();
    let events_path = session_dir.join("events.jsonl");
    std::fs::write(
        &events_path,
        // No session.start; a single user.message-shaped line.
        r#"{"type":"user.message","data":{"content":"hi","source":"cli","attachments":[],"interactionId":"x"},"id":"00000000-0000-0000-0000-000000000099","timestamp":"2026-05-29T00:00:00Z","parentId":null}
"#,
    )
    .unwrap();

    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(&events_path)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("data error"))
        .stderr(contains("loading session"));
}

#[test]
fn analyze_output_to_unwritable_path_exits_with_output_error() {
    // --output to a path under a non-existent parent dir. std::fs::write
    // fails with ENOENT; run() maps this to ExitKind::OutputError = 3.
    let unwritable = PathBuf::from("/nonexistent/agentprof/output/report.md");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "md", "--output"])
        .arg(&unwritable)
        .assert()
        .failure()
        .code(3)
        .stderr(contains("output error"))
        .stderr(contains("writing"));
}

#[test]
fn analyze_unsupported_agent_exits_with_friendly_message() {
    // --agent claude should produce a friendly "not yet implemented"
    // message (not the previous cryptic "no adapter wired"). Regression
    // guard for the audit-a3-claude-codex-unfriendly-error fix.
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--agent", "claude"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("not yet implemented"))
        .stderr(contains("M1.4 ships copilot only"));
}

#[test]
fn analyze_minimal_fixture_populates_turn_metadata_in_json() {
    // Regression test for turn-metadata-extraction: the `minimal` fixture
    // has an assistant.message with model='gpt-5-mini' + outputTokens=10.
    // Before turn-metadata-extraction, Turn.model and Turn.output_tokens
    // were always None (despite the data being in the wire format).
    //
    // This locks the end-to-end pipeline: parser reads the field →
    // CopilotEvent::payload_model/output_tokens returns Some →
    // derive_episodes' on_assistant_message writes to Turn →
    // analyzer's turn_summary copies into TurnSummaryRow →
    // json renderer serializes → JSON contains the real values.
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(fixtures_root().join("minimal"))
        .args(["--export", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&s).expect("output must be valid JSON");

    let turns = parsed["turn_summary"].as_array().unwrap();
    assert_eq!(turns.len(), 1, "minimal has exactly 1 turn");
    assert_eq!(
        turns[0]["model"], "gpt-5-mini",
        "Turn.model must come from assistant.message.data.model"
    );
    assert_eq!(
        turns[0]["output_tokens"], 10,
        "Turn.output_tokens must come from assistant.message.data.output_tokens"
    );
}

#[test]
fn analyze_export_tui_flag_parses_and_short_circuits_under_non_tty() {
    // `assert_cmd` redirects stdout — so the spawned binary sees a non-tty
    // stdout and must exit with OutputError (3) plus a helpful message
    // before attempting to enter raw mode.
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "tui"])
        .assert()
        .failure()
        .code(3)
        .stderr(contains("requires both stdin and stdout to be TTYs"));
}

#[test]
fn analyze_export_value_help_lists_tui() {
    // Sanity: `--help` text mentions 'tui' as a valid value.
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(
        s.contains("tui"),
        "expected --help to list 'tui' as --export value, got:\n{s}"
    );
}

#[test]
fn analyze_export_tui_with_output_flag_warns() {
    // Polish #4: --export tui with --output should warn on stderr that
    // --output is ignored (still exits with the stdin/stdout TTY error
    // because assert_cmd pipes them).
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(cross_turn_path())
        .args(["--export", "tui", "--output", "/tmp/should-be-ignored.txt"])
        .assert()
        .failure()
        .code(3)
        .stderr(contains("--output is ignored with --export tui"))
        .stderr(contains("requires both stdin and stdout to be TTYs"));
}
