//! TUI snapshot tests — 1 fixture × 3 views = 3 snapshots.
//!
//! Uses `ratatui::backend::TestBackend` (no terminal needed; works in CI).
//! Loads the same `cross-turn-tool` fixture the CLI tests use, derives
//! Episodes, runs the analyzer, then renders each view into a 100×30
//! `TestBackend` and snapshots the resulting buffer.
//!
//! Updating snapshots: `INSTA_UPDATE=always cargo test -p agentprof-tui --tests`
//! followed by `cargo insta review` to inspect.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::{Adapter, SessionRef};
use agentprof_core::analyzer::analyze;
use agentprof_core::episode::derive_episodes;
use agentprof_tui::views::View;
use agentprof_tui::AppRunner;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot/cross-turn-tool/events.jsonl")
}

fn load_fixture() -> (
    agentprof_core::analyzer::AnalysisReport,
    agentprof_core::episode::Episodes,
) {
    let path = fixture_path();
    let adapter = CopilotAdapter;
    let sref = SessionRef::new(
        "cross-turn-tool".into(),
        adapter.agent_kind(),
        path,
        std::time::SystemTime::UNIX_EPOCH,
        0,
        false,
    );
    let raw = adapter.load_session(&sref).expect("fixture loads");
    let episodes = derive_episodes(&raw.events, &raw.meta);
    let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
    (report, episodes)
}

fn snapshot_view(view: View, name: &str) {
    let (report, episodes) = load_fixture();
    let mut runner = AppRunner::new(&report, &episodes);
    runner.set_view(view);
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    runner.draw_frame(&mut terminal).expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let cells_per_row = buffer.area.width as usize;
    let mut text = String::with_capacity(buffer.content.len() + buffer.area.height as usize);
    for (i, cell) in buffer.content.iter().enumerate() {
        if i > 0 && i % cells_per_row == 0 {
            text.push('\n');
        }
        text.push_str(cell.symbol());
    }
    insta::assert_snapshot!(name, text);
}

#[test]
fn snapshot_flamegraph_cross_turn_tool() {
    snapshot_view(View::Flamegraph, "flamegraph__cross_turn_tool");
}

#[test]
fn snapshot_roi_cross_turn_tool() {
    snapshot_view(View::Roi, "roi__cross_turn_tool");
}

#[test]
fn snapshot_aggregate_cross_turn_tool() {
    snapshot_view(View::Aggregate, "aggregate__cross_turn_tool");
}
