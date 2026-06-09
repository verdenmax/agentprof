//! Integration tests for `copilot::paths` and `CopilotAdapter`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use agentprof_adapters::copilot::paths;
use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::{Adapter, AdapterError, AgentKind};

fn write_session(root: &Path, id: &str, events_body: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("events.jsonl"), events_body).unwrap();
}

#[test]
fn discover_finds_sessions_in_uuid_subdirs() {
    let tmp = tempfile::tempdir().unwrap();
    write_session(tmp.path(), "0190abc1-1111-7000-8000-000000000001", "{}\n");
    write_session(tmp.path(), "0190abc1-2222-7000-8000-000000000002", "{}\n");

    let sessions = paths::discover_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 2, "expected 2 sessions, got {sessions:?}");
    for s in &sessions {
        assert_eq!(s.agent, AgentKind::Copilot);
        assert!(s.path.ends_with("events.jsonl"));
        assert!(!s.is_live);
    }
}

#[test]
fn discover_skips_subdirs_without_events_jsonl() {
    let tmp = tempfile::tempdir().unwrap();
    write_session(tmp.path(), "has-events", "{}\n");
    fs::create_dir_all(tmp.path().join("empty-dir")).unwrap();

    let sessions = paths::discover_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "has-events");
}

#[test]
fn discover_sorts_descending_by_mtime() {
    let tmp = tempfile::tempdir().unwrap();
    write_session(tmp.path(), "older", "{}\n");
    sleep(Duration::from_millis(20));
    write_session(tmp.path(), "newer", "{}\n");

    let sessions = paths::discover_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].id, "newer", "newest should be first");
    assert_eq!(sessions[1].id, "older");
    assert!(sessions[0].modified_at >= sessions[1].modified_at);
}

#[test]
fn discover_marks_is_live_when_inuse_lock_present() {
    let tmp = tempfile::tempdir().unwrap();
    write_session(tmp.path(), "live-session", "{}\n");
    fs::write(tmp.path().join("live-session").join("inuse.12345.lock"), "").unwrap();
    write_session(tmp.path(), "idle-session", "{}\n");

    let sessions = paths::discover_sessions(tmp.path()).unwrap();
    let live = sessions.iter().find(|s| s.id == "live-session").unwrap();
    let idle = sessions.iter().find(|s| s.id == "idle-session").unwrap();
    assert!(
        live.is_live,
        "session with inuse.*.lock must be is_live=true"
    );
    assert!(!idle.is_live);
}

#[test]
fn discover_returns_root_not_found_for_missing_path() {
    let res = paths::discover_sessions(Path::new("/nonexistent/agentprof/path/zz9"));
    assert!(matches!(res, Err(AdapterError::RootNotFound { .. })));
}

#[test]
fn load_session_returns_raw_session_for_fixture_minimal() {
    let adapter = CopilotAdapter;
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("copilot");
    let sessions = paths::discover_sessions(&fixture_root).unwrap();
    // Locate by trailing path segment instead of `s.id`: as of the M2.1
    // id-namespace fix, `SessionRef.id` is the canonical UUID from
    // events.jsonl, not the directory name.
    let minimal = sessions
        .iter()
        .find(|s| {
            s.path
                .parent()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n == "minimal")
        })
        .expect("minimal fixture should be discovered");

    let raw = adapter.load_session(minimal).unwrap();
    assert!(
        !raw.events.is_empty(),
        "minimal fixture should yield events"
    );
    assert_eq!(adapter.agent_kind(), AgentKind::Copilot);
}
