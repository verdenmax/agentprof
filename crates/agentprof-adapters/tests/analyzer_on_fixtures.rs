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
analyzer_test!(
    analysis_tool_and_skill_same_turn,
    "tool-and-skill-same-turn"
);
analyzer_test!(analysis_two_skills_one_turn, "two-skills-one-turn");
analyzer_test!(analysis_orphan_skill_mix, "orphan-skill-mix");

// ============== B-7 (M1.6.4 follow-up wave, 2026-06-03) ========
// Regression-lock for the `b5c1429` FlamegraphView fix. Snapshots
// the analyzer output (so `tool_rank.ask_user.is_user_blocking`
// stays flagged) and asserts the derived-episode invariants that
// the renderer's `max_dur` filter relies on (exactly 1 turn
// is_user_blocking + duration ratio).
analyzer_test!(
    analysis_with_ask_user_mid_session,
    "with-ask-user-mid-session"
);

#[test]
fn with_ask_user_mid_session_episode_invariants() {
    let adapter = CopilotAdapter;
    let root = fixture("with-ask-user-mid-session")
        .parent()
        .unwrap()
        .to_path_buf();
    let sessions = adapter.discover_sessions(&root).expect("discover");
    let sref = sessions
        .into_iter()
        .find(|s| {
            s.path
                .parent()
                .unwrap()
                .ends_with("with-ask-user-mid-session")
        })
        .expect("fixture discovered");
    let raw = adapter.load_session(&sref).expect("load_session");
    let episodes = derive_episodes(&raw.events, &raw.meta);

    assert_eq!(episodes.turns.len(), 3, "fixture has 3 turns");
    let blocking: Vec<&_> = episodes
        .turns
        .iter()
        .filter(|t| t.is_user_blocking())
        .collect();
    assert_eq!(
        blocking.len(),
        1,
        "exactly one turn must be is_user_blocking (the ask_user one)"
    );

    // Duration ratio: blocking turn >= 10× the longest non-blocking
    // turn. Actual fixture is ~121× — the loose bound keeps the
    // assertion robust against small future adjustments.
    let dur_ms = |t: &agentprof_core::episode::Turn| -> i64 {
        t.ended_at
            .map_or(0, |e| (e - t.started_at).num_milliseconds())
    };
    let blocking_dur = dur_ms(blocking[0]);
    let non_blocking_max = episodes
        .turns
        .iter()
        .filter(|t| !t.is_user_blocking())
        .map(dur_ms)
        .max()
        .expect("at least one non-blocking turn");
    assert!(
        blocking_dur >= non_blocking_max * 10,
        "blocking turn duration {blocking_dur} ms must be >= 10× non-blocking max {non_blocking_max} ms"
    );
}

// ──────────────────────────────────────────────────────────────────────
// B1 — end-to-end regression guards: fixtures with success:false events
// must produce non-zero failure_count downstream. Closes the silent bug
// that hid behind the always-zero failure_count between M1.2 and B1.
// See ADR-0013 + spec §7.3.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn b1_with_mcp_calls_has_tool_failure() {
    let report = load_and_analyze("with-mcp-calls");
    let total_tool_failures: usize = report.tool_rank.iter().map(|r| r.failure_count).sum();
    assert!(
        total_tool_failures >= 1,
        "with-mcp-calls fixture has 1 tool.execution_complete success=false event; \
         expected >= 1 tool failure, got {total_tool_failures}. \
         Regression of the M1.2 always-zero bug?"
    );
}

#[test]
fn b1_multi_sess_c_has_tool_failure() {
    let report = load_and_analyze("multi-sess-c");
    let total_tool_failures: usize = report.tool_rank.iter().map(|r| r.failure_count).sum();
    assert!(
        total_tool_failures >= 1,
        "multi-sess-c fixture has 1 tool.execution_complete success=false event; \
         expected >= 1 tool failure, got {total_tool_failures}."
    );
}

#[test]
fn b1_with_hooks_heavy_has_hook_failure() {
    let report = load_and_analyze("with-hooks-heavy");
    let total_hook_failures: usize = report.hook_rank.iter().map(|r| r.failure_count).sum();
    assert!(
        total_hook_failures >= 1,
        "with-hooks-heavy fixture has 2 hook.end success=false events; \
         expected >= 1 hook failure, got {total_hook_failures}."
    );
}

#[test]
fn b1_with_aborts_has_hook_failure() {
    let report = load_and_analyze("with-aborts");
    let total_hook_failures: usize = report.hook_rank.iter().map(|r| r.failure_count).sum();
    assert!(
        total_hook_failures >= 1,
        "with-aborts fixture has 1 hook.end success=false event; \
         expected >= 1 hook failure, got {total_hook_failures}."
    );
}
