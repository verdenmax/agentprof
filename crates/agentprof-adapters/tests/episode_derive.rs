//! Integration test: run `derive_episodes` against every committed fixture and
//! snapshot the result.
//!
//! Placed under `agentprof-adapters/tests/` (not `agentprof-core/tests/`) to
//! avoid a dev-dependency cycle: this test needs both `agentprof-adapters`
//! (to load fixtures via `CopilotAdapter`) and `agentprof-core::episode`
//! (the function under test). See M1.3 spec §8 + ADR-0004 for rationale.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use insta::assert_json_snapshot;

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::Adapter;
use agentprof_core::episode::derive_episodes;

fn fixture(slug: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(slug)
}

fn load_and_derive(slug: &str) -> agentprof_core::episode::Episodes {
    let adapter = CopilotAdapter;
    let root = fixture(slug).parent().unwrap().to_path_buf();
    let sessions = adapter.discover_sessions(&root).expect("discover_sessions");
    let sref = sessions
        .into_iter()
        .find(|s| s.path.parent().unwrap().ends_with(slug))
        .unwrap_or_else(|| panic!("fixture {slug} not discovered"));
    let raw = adapter.load_session(&sref).expect("load_session");
    derive_episodes(&raw.events, &raw.meta)
}

macro_rules! episode_test {
    ($name:ident, $slug:expr) => {
        #[test]
        fn $name() {
            let episodes = load_and_derive($slug);
            assert_json_snapshot!(format!("episode__{}", $slug), episodes);
        }
    };
}

episode_test!(episode_minimal, "minimal");
episode_test!(episode_builtin_tools_only, "builtin-tools-only");
episode_test!(episode_with_mcp_calls, "with-mcp-calls");
episode_test!(episode_with_skill_invoked, "with-skill-invoked");
episode_test!(episode_with_hooks_heavy, "with-hooks-heavy");
episode_test!(episode_with_aborts, "with-aborts");
episode_test!(episode_with_mode_transitions, "with-mode-transitions");
episode_test!(episode_live_truncated, "live-truncated");
episode_test!(episode_orphan_events, "orphan-events");

// corrupt fixture: load may fail or produce warnings; we just assert no panic.
#[test]
fn episode_corrupt_does_not_panic() {
    let adapter = CopilotAdapter;
    let root = fixture("corrupt").parent().unwrap().to_path_buf();
    let sessions = adapter.discover_sessions(&root).expect("discover");
    if let Some(sref) = sessions
        .into_iter()
        .find(|s| s.path.parent().unwrap().ends_with("corrupt"))
    {
        if let Ok(raw) = adapter.load_session(&sref) {
            let _ = derive_episodes(&raw.events, &raw.meta);
        }
    }
}
