//! Local-data smoke tests against the developer's real `~/.copilot/session-state/`.
//!
//! These tests are `#[ignore]` by default; run with:
//!
//! ```bash
//! export AGENTPROF_LOCAL_FIXTURES_DIR=~/.copilot/session-state
//! cargo test -p agentprof-adapters --test copilot_smoke -- --include-ignored
//! ```
//!
//! No output is committed; this catches schema drift between Copilot CLI versions.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use agentprof_adapters::copilot::{CopilotAdapter, CopilotEvent};
use agentprof_core::adapter::Adapter;

fn local_fixtures_dir() -> Option<PathBuf> {
    std::env::var_os("AGENTPROF_LOCAL_FIXTURES_DIR").map(PathBuf::from)
}

#[test]
#[ignore = "requires AGENTPROF_LOCAL_FIXTURES_DIR=~/.copilot/session-state"]
fn smoke_parse_every_local_session() {
    let Some(dir) = local_fixtures_dir() else {
        eprintln!("AGENTPROF_LOCAL_FIXTURES_DIR not set; skipping.");
        return;
    };

    let adapter = CopilotAdapter;
    let sessions = adapter
        .discover_sessions(&dir)
        .unwrap_or_else(|e| panic!("discover failed on {}: {e}", dir.display()));

    eprintln!(
        "Smoke-testing {} sessions under {}",
        sessions.len(),
        dir.display()
    );

    let mut session_count = 0;
    let mut total_unknown = 0usize;
    let mut total_warnings = 0usize;

    for sref in sessions {
        let raw = adapter
            .load_session(&sref)
            .unwrap_or_else(|e| panic!("load_session failed on {}: {e}", sref.path.display()));
        session_count += 1;
        total_warnings += raw.parse_warnings.len();
        for ev in &raw.events {
            if matches!(ev, CopilotEvent::Unknown) {
                total_unknown += 1;
            }
        }
    }

    eprintln!(
        "Smoke results: {session_count} sessions, {total_warnings} warnings, {total_unknown} Unknown events"
    );

    assert_eq!(
        total_unknown, 0,
        "schema drift detected: {total_unknown} CopilotEvent::Unknown values; \
         update adr-0002-copilot-event-schema.md + add a variant"
    );
}
