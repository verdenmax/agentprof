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

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(format!(
            "agentprof-adapters/tests/fixtures/copilot/{name}/events.jsonl"
        ))
}

fn load_fixture(
    name: &str,
) -> (
    agentprof_core::analyzer::AnalysisReport,
    agentprof_core::episode::Episodes,
) {
    let path = fixture_path(name);
    let adapter = CopilotAdapter;
    let sref = SessionRef::new(
        name.to_string(),
        adapter.agent_kind(),
        path,
        std::time::SystemTime::UNIX_EPOCH,
        0,
        false,
    );
    let raw = adapter
        .load_session(&sref)
        .unwrap_or_else(|e| panic!("fixture {name} loads: {e}"));
    let episodes = derive_episodes(&raw.events, &raw.meta);
    let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
    (report, episodes)
}

/// Render a `ratatui::Buffer` to a newline-separated grid of cell symbols.
///
/// Avoids depending on `ratatui::Buffer`'s `Debug` impl (which is not part of
/// the public API contract and can change across minor versions).
fn buffer_to_symbol_grid(buffer: &ratatui::buffer::Buffer) -> String {
    let cells_per_row = buffer.area.width as usize;
    let mut text = String::with_capacity(buffer.content.len() + buffer.area.height as usize);
    for (i, cell) in buffer.content.iter().enumerate() {
        if i > 0 && i % cells_per_row == 0 {
            text.push('\n');
        }
        text.push_str(cell.symbol());
    }
    text
}

fn snapshot_view(fixture: &str, view: View, snap_name: &str) {
    let (report, episodes) = load_fixture(fixture);
    let mut runner = AppRunner::new(&report, &episodes);
    runner.set_view(view);
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    runner.draw_frame(&mut terminal).expect("draw");
    let buffer = terminal.backend().buffer().clone();
    insta::assert_snapshot!(snap_name, buffer_to_symbol_grid(&buffer));
}

// --- baseline: cross-turn-tool × 3 views ---

#[test]
fn snapshot_flamegraph_cross_turn_tool() {
    snapshot_view(
        "cross-turn-tool",
        View::Flamegraph,
        "flamegraph__cross_turn_tool",
    );
}

#[test]
fn snapshot_roi_cross_turn_tool() {
    snapshot_view("cross-turn-tool", View::Roi, "roi__cross_turn_tool");
}

#[test]
fn snapshot_aggregate_cross_turn_tool() {
    snapshot_view(
        "cross-turn-tool",
        View::Aggregate,
        "aggregate__cross_turn_tool",
    );
}

// --- view-specific code path coverage (5 new snapshots) ---

/// `with-aborts` → Flamegraph: exercises `Modifier::UNDERLINED` on aborted
/// turns (per `build_row`'s `TurnStatus::Aborted(_)` branch).
#[test]
fn snapshot_flamegraph_with_aborts() {
    snapshot_view("with-aborts", View::Flamegraph, "flamegraph__with_aborts");
}

/// `orphan-events` → Roi: exercises `ToolCallStatus::OrphanSynthesizedStart`
/// glyph `○` in `recent_calls`.
#[test]
fn snapshot_roi_orphan_events() {
    snapshot_view("orphan-events", View::Roi, "roi__orphan_events");
}

/// `with-mcp-calls` → Roi: exercises `source_label`'s `Mcp { server }`
/// branch → `"mcp/<server>"`.
#[test]
fn snapshot_roi_with_mcp_calls() {
    snapshot_view("with-mcp-calls", View::Roi, "roi__with_mcp_calls");
}

/// `with-skill-invoked` → Roi: exercises `source_label`'s `Skill { name }`
/// branch → `"skill/<name>"`.
#[test]
fn snapshot_roi_with_skill_invoked() {
    snapshot_view("with-skill-invoked", View::Roi, "roi__with_skill_invoked");
}

/// `with-mode-transitions` → Aggregate: exercises `group_by_mode` with
/// multiple Mode variants in one session.
#[test]
fn snapshot_aggregate_with_mode_transitions() {
    snapshot_view(
        "with-mode-transitions",
        View::Aggregate,
        "aggregate__with_mode_transitions",
    );
}

#[test]
fn cross_session_by_tool_snapshot() {
    use agentprof_core::analyzer::aggregate::{
        AggregateKey, AggregateReport, AnyAggregateReport, ToolBucket,
    };
    use agentprof_core::model::ToolSource;
    use agentprof_tui::views::aggregate::render_cross_session;
    use agentprof_tui::watch::AggSortKey;
    use chrono::Duration;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let bucket1 = ToolBucket::new(
        "bash".into(),
        ToolSource::Builtin,
        50,
        48,
        2,
        Duration::seconds(120),
        Duration::milliseconds(150),
        Duration::milliseconds(800),
        3,
    );
    let bucket2 = ToolBucket::new(
        "mcp__github__list_pulls".into(),
        ToolSource::Mcp {
            server: "github".into(),
        },
        10,
        10,
        0,
        Duration::seconds(30),
        Duration::milliseconds(2000),
        Duration::milliseconds(4500),
        2,
    );
    let inner: AggregateReport<ToolBucket> = AggregateReport::new(
        AggregateKey::Tool,
        Duration::days(30),
        3,
        0,
        Duration::seconds(300),
        vec![bucket1, bucket2],
    );
    let any = AnyAggregateReport::Tool(inner);

    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    term.draw(|f| render_cross_session(f, f.area(), &any, AggSortKey::TotalDuration, 0))
        .unwrap();
    let buf = term.backend().buffer().clone();
    insta::assert_snapshot!("cross_session_by_tool", buffer_to_symbol_grid(&buf));
}

#[test]
fn cross_session_by_day_snapshot() {
    use agentprof_core::analyzer::aggregate::{
        AggregateKey, AggregateReport, AnyAggregateReport, DayBucket,
    };
    use agentprof_tui::views::aggregate::render_cross_session;
    use agentprof_tui::watch::AggSortKey;
    use chrono::{Duration, NaiveDate};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let day = DayBucket::new(
        NaiveDate::from_ymd_opt(2026, 5, 30).unwrap(),
        2,
        Duration::seconds(180),
        Duration::seconds(60),
        0,
        33.3,
        true,
    );
    let inner: AggregateReport<DayBucket> = AggregateReport::new(
        AggregateKey::Day,
        Duration::days(7),
        2,
        0,
        Duration::seconds(180),
        vec![day],
    );
    let any = AnyAggregateReport::Day(inner);

    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    term.draw(|f| render_cross_session(f, f.area(), &any, AggSortKey::TotalDuration, 0))
        .unwrap();
    let buf = term.backend().buffer().clone();
    insta::assert_snapshot!("cross_session_by_day", buffer_to_symbol_grid(&buf));
}

// --- F1.7 Models view snapshots ---

#[test]
fn snapshot_models_with_data() {
    // Uses the F1.7 with-session-shutdown fixture which has 2 models'
    // modelMetrics. Locks the table render + sort + totals.
    snapshot_view("with-session-shutdown", View::Models, "models_with_data");
}

#[test]
fn snapshot_models_empty_state() {
    // Uses an existing fixture WITHOUT session.shutdown — Models view
    // falls through to the centered empty-state placeholder.
    snapshot_view("builtin-tools-only", View::Models, "models_empty_state");
}
