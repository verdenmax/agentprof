//! Integration test: run `analyze()` against every committed fixture
//! and snapshot the result.
//!
//! Mirrors the structure of `episode_derive.rs`; placed here (not in
//! `agentprof-core/tests/`) to avoid the dev-dep cycle (adapters depend
//! on core, not vice versa).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use insta::assert_json_snapshot;

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::Adapter;
use agentprof_core::analyzer::analyze;
use agentprof_core::episode::derive_episodes;

fn fixture(slug: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(slug)
}

fn load_and_analyze(slug: &str) -> agentprof_core::analyzer::AnalysisReport {
    let adapter = CopilotAdapter;
    let root = fixture(slug).parent().unwrap().to_path_buf();
    let sessions = adapter.discover_sessions(&root).expect("discover_sessions");
    let sref = sessions
        .into_iter()
        .find(|s| s.path.parent().unwrap().ends_with(slug))
        .unwrap_or_else(|| panic!("fixture {slug} not discovered"));
    let raw = adapter.load_session(&sref).expect("load_session");
    let episodes = derive_episodes(&raw.events, &raw.meta);
    analyze(&episodes, &raw.meta, &raw.parse_warnings)
}

macro_rules! analyzer_test {
    ($name:ident, $slug:expr) => {
        #[test]
        fn $name() {
            let report = load_and_analyze($slug);
            assert_json_snapshot!(format!("analysis__{}", $slug), report);
        }
    };
}

analyzer_test!(analysis_minimal, "minimal");
analyzer_test!(analysis_builtin_tools_only, "builtin-tools-only");
analyzer_test!(analysis_with_mcp_calls, "with-mcp-calls");
analyzer_test!(analysis_with_skill_invoked, "with-skill-invoked");
analyzer_test!(analysis_with_hooks_heavy, "with-hooks-heavy");
analyzer_test!(analysis_with_aborts, "with-aborts");
analyzer_test!(analysis_with_mode_transitions, "with-mode-transitions");
analyzer_test!(analysis_live_truncated, "live-truncated");
analyzer_test!(analysis_orphan_events, "orphan-events");
analyzer_test!(analysis_cross_turn_tool, "cross-turn-tool");
analyzer_test!(
    analysis_with_post_tool_use_hooks,
    "with-post-tool-use-hooks"
);
