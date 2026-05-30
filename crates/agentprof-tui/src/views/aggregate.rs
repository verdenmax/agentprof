//! `AggregateView` — single-session aggregates.
//!
//! Two sections:
//! 1. **By Mode** — group `TurnSummaryRow`s by `Mode` (Interactive / Plan /
//!    Autopilot / Unknown / None); per-group totals: turns, duration, output
//!    tokens, tool calls.
//! 2. **By Hook** — direct render of `AnalysisReport.hook_rank`.
//!
//! M1.5 is single-session only; cross-session aggregate is M1.6 with the
//! `aggregate` subcommand.

use std::collections::BTreeMap;

use agentprof_core::analyzer::TurnSummaryRow;
use agentprof_core::episode::Mode;
use chrono::Duration;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::state::AppState;
use crate::views::format::human_short;

/// Per-mode aggregation produced by [`group_by_mode`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ModeBucket {
    /// Display label for the mode ("interactive", "plan", "autopilot",
    /// `Unknown(x)` → "unknown:x", `None` → "—").
    pub label: String,
    /// Number of turns falling into this mode.
    pub turns: usize,
    /// Sum of turn durations (skips turns with no `duration`).
    pub total_duration: Duration,
    /// Sum of `output_tokens` (None entries excluded).
    pub total_output_tokens: u64,
    /// Sum of `tool_call_count`.
    pub total_tool_calls: usize,
}

/// Group `TurnSummaryRow`s by `Mode`. Result is sorted by `turns` descending.
///
/// `Mode` is `#[non_exhaustive]`; the wildcard arm labels future variants
/// as `"?"` so a new wire value never panics.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::aggregate::group_by_mode;
/// assert!(group_by_mode(&[]).is_empty());
/// ```
#[must_use]
pub fn group_by_mode(rows: &[TurnSummaryRow]) -> Vec<ModeBucket> {
    let mut map: BTreeMap<String, ModeBucket> = BTreeMap::new();
    for row in rows {
        let label = match &row.mode {
            Some(Mode::Interactive) => "interactive".to_string(),
            Some(Mode::Plan) => "plan".to_string(),
            Some(Mode::Autopilot) => "autopilot".to_string(),
            Some(Mode::Unknown(s)) => format!("unknown:{s}"),
            // Mode is #[non_exhaustive]; fall back for any future variant.
            Some(_) => "?".to_string(),
            None => "—".to_string(),
        };
        let entry = map.entry(label.clone()).or_insert_with(|| ModeBucket {
            label,
            turns: 0,
            total_duration: Duration::zero(),
            total_output_tokens: 0,
            total_tool_calls: 0,
        });
        entry.turns += 1;
        if let Some(d) = row.duration {
            entry.total_duration += d;
        }
        if let Some(t) = row.output_tokens {
            entry.total_output_tokens += u64::from(t);
        }
        entry.total_tool_calls += row.tool_call_count;
    }
    let mut v: Vec<ModeBucket> = map.into_values().collect();
    v.sort_by(|a, b| b.turns.cmp(&a.turns));
    v
}

/// Render `AggregateView` into the given area.
///
/// Layout: vertical 50/50 split — top is "By Mode" table (computed via
/// [`group_by_mode`]), bottom is "By Hook" table (direct render of
/// `AnalysisReport.hook_rank`, no grouping).
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_by_mode(frame, chunks[0], state);
    render_by_hook(frame, chunks[1], state);
}

fn render_by_mode(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Aggregate (3/3) — By Mode (single session) ");
    let buckets = group_by_mode(&state.report.turn_summary);
    let header = Row::new(vec![
        Cell::from("Mode"),
        Cell::from("Turns"),
        Cell::from("Total dur"),
        Cell::from("Out tokens"),
        Cell::from("Tool calls"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row<'_>> = buckets
        .iter()
        .map(|b| {
            Row::new(vec![
                Cell::from(b.label.clone()),
                Cell::from(format!("{}", b.turns)),
                Cell::from(human_short(b.total_duration)),
                Cell::from(format!("{}", b.total_output_tokens)),
                Cell::from(format!("{}", b.total_tool_calls)),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(block);
    frame.render_widget(table, area);
}

fn render_by_hook(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let block = Block::default().borders(Borders::ALL).title(" By Hook ");
    let header = Row::new(vec![
        Cell::from("Hook"),
        Cell::from("Calls"),
        Cell::from("Success"),
        Cell::from("Fail"),
        Cell::from("p50"),
        Cell::from("Total"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row<'_>> = state
        .report
        .hook_rank
        .iter()
        .map(|h| {
            Row::new(vec![
                Cell::from(h.name.clone()),
                Cell::from(format!("{}", h.call_count)),
                Cell::from(format!("{}", h.success_count)),
                Cell::from(format!("{}", h.failure_count)),
                Cell::from(human_short(h.p50_duration)),
                Cell::from(human_short(h.total_duration)),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(block);
    frame.render_widget(table, area);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use agentprof_core::analyzer::TurnSummaryRow;
    use agentprof_core::episode::TurnStatus;
    use chrono::Utc;

    fn turn_row(
        mode: Option<Mode>,
        dur_ms: i64,
        out_tokens: Option<u32>,
        tool_calls: usize,
    ) -> TurnSummaryRow {
        TurnSummaryRow::new(
            format!("t{dur_ms}"),
            Utc::now(),
            Some(Duration::milliseconds(dur_ms)),
            TurnStatus::Completed,
            None, // model
            mode,
            out_tokens,
            tool_calls,
            0, // hook_call_count
            0, // skill_call_count
        )
    }

    #[test]
    fn group_by_mode_buckets_correctly() {
        let rows = vec![
            turn_row(Some(Mode::Interactive), 100, Some(50), 2),
            turn_row(Some(Mode::Interactive), 200, Some(100), 3),
            turn_row(Some(Mode::Plan), 50, None, 0),
            turn_row(None, 30, Some(10), 1),
        ];
        let g = group_by_mode(&rows);
        // Sorted by turns desc: interactive (2), plan (1), — (1) — last two
        // can be in either order; assert interactive is first.
        assert_eq!(g[0].label, "interactive");
        assert_eq!(g[0].turns, 2);
        assert_eq!(g[0].total_duration, Duration::milliseconds(300));
        assert_eq!(g[0].total_output_tokens, 150);
        assert_eq!(g[0].total_tool_calls, 5);
    }

    #[test]
    fn group_by_mode_empty_input_yields_empty() {
        assert!(group_by_mode(&[]).is_empty());
    }

    #[test]
    fn group_by_mode_unknown_variant_labeled_with_payload() {
        let rows = vec![turn_row(Some(Mode::Unknown("custom".into())), 10, None, 0)];
        let g = group_by_mode(&rows);
        assert_eq!(g[0].label, "unknown:custom");
    }
}
