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
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
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
    v.sort_by_key(|b| std::cmp::Reverse(b.turns));
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

    // F1.18 — empty-state: when the session has no turns recorded, show
    // a centered explanatory placeholder instead of an empty table
    // (which would just render header + blank rows).
    if buckets.is_empty() {
        render_empty_state(
            frame,
            area,
            block,
            "(no turns recorded for this session yet)",
        );
        return;
    }

    // F1.15 — totals for percent columns. Sum across buckets (parallel
    // to the RoiView `total_all_ms` computation) so the column denominators
    // match the rendered table. `total_turns` equals
    // `state.report.turn_summary.len()` (every turn falls into some
    // bucket including the `—` no-mode bucket), but summing
    // bucket-by-bucket keeps the computation local + obvious.
    let total_turns: usize = buckets.iter().map(|b| b.turns).sum();
    let total_all_ms: i64 = buckets
        .iter()
        .map(|b| b.total_duration.num_milliseconds().max(0))
        .fold(0_i64, i64::saturating_add);

    let header = Row::new(vec![
        Cell::from("Mode"),
        Cell::from("Turns"),
        Cell::from("Turns%"),
        Cell::from("Total dur"),
        Cell::from("Dur%"),
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
                Cell::from(crate::views::roi::format_ok_pct(b.turns, total_turns)),
                Cell::from(human_short(b.total_duration)),
                Cell::from(crate::views::roi::format_total_pct(
                    b.total_duration,
                    total_all_ms,
                )),
                Cell::from(crate::views::models::format_token_u64_short(
                    b.total_output_tokens,
                )),
                Cell::from(format!("{}", b.total_tool_calls)),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(11),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(11),
        ],
    )
    .header(header)
    .block(block);
    frame.render_widget(table, area);
}

fn render_by_hook(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" By Hook (single session — hook events) ");

    // F1.18 — empty-state: many sessions have no hook events
    // (Copilot sessions without configured hooks). Show a centered
    // explanatory placeholder instead of just a header + blank rows.
    if state.report.hook_rank.is_empty() {
        render_empty_state(
            frame,
            area,
            block,
            "(no hook events recorded for this session)",
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("Hook"),
        Cell::from("Calls"),
        Cell::from("OK"),
        Cell::from("Fail"),
        Cell::from("OK%"),
        Cell::from("p50"),
        Cell::from("Total"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row<'_>> = state
        .report
        .hook_rank
        .iter()
        .map(|h| {
            // F1.16 — failure-severity coloring on the Hook cell only
            // (parallel to F1.13 RoiView Tool cell coloring).
            // Reuses the same `views::roi::failure_severity_color`
            // helper so the thresholds (>50% Red, >=3 calls + any fail
            // Yellow) stay consistent across views.
            let hook_cell_style =
                crate::views::roi::failure_severity_color(h.call_count, h.failure_count)
                    .map_or_else(Style::default, |c| {
                        Style::default().fg(c).add_modifier(Modifier::BOLD)
                    });
            Row::new(vec![
                Cell::from(h.name.clone()).style(hook_cell_style),
                Cell::from(format!("{}", h.call_count)),
                Cell::from(format!("{}", h.success_count)),
                Cell::from(format!("{}", h.failure_count)),
                Cell::from(crate::views::roi::format_ok_pct(
                    h.success_count,
                    h.call_count,
                )),
                Cell::from(human_short(h.p50_duration)),
                Cell::from(human_short(h.total_duration)),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(block);
    frame.render_widget(table, area);
}

/// Render a centered placeholder line inside the given bordered `block`
/// when a By-Mode / By-Hook section has no data (F1.18).
///
/// Used by [`render_by_mode`] and [`render_by_hook`] for graceful
/// empty-state messaging — beats showing a header followed by blank
/// rows (which historically led to the user wondering "is the view
/// broken or is there just no data?"). Matches the F1.7 Models view
/// empty-state pattern.
fn render_empty_state(frame: &mut Frame<'_>, area: Rect, block: Block<'_>, message: &str) {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // Vertically center the message inside the inner area.
    let lines_to_pad_top = usize::from(inner.height.saturating_sub(1) / 2);
    let mut lines: Vec<Line<'_>> = Vec::with_capacity(lines_to_pad_top + 1);
    for _ in 0..lines_to_pad_top {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(message.to_string()).style(Style::default().add_modifier(Modifier::DIM)));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

/// Cross-session aggregate render (M1.6.3).
///
/// Renders an [`agentprof_core::analyzer::aggregate::AnyAggregateReport`]
/// as a full-area bucket table preceded by a 3-line header. Selection
/// highlighting is the inverted row at `selected`. Sort key cycles in
/// the runner — this fn just applies the already-chosen sort.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::{
///     AggregateKey, AggregateReport, AnyAggregateReport, ToolBucket,
/// };
/// use agentprof_tui::views::aggregate::render_cross_session;
/// use agentprof_tui::watch::AggSortKey;
/// use chrono::Duration;
/// use ratatui::backend::TestBackend;
/// use ratatui::Terminal;
///
/// let inner: AggregateReport<ToolBucket> = AggregateReport::new(
///     AggregateKey::Tool, None, 0, 0, Duration::zero(), Vec::new(),
/// );
/// let any = AnyAggregateReport::Tool(inner);
/// let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
/// term.draw(|f| render_cross_session(f, f.area(), &any, AggSortKey::TotalDuration, 0)).unwrap();
/// ```
pub fn render_cross_session(
    frame: &mut Frame<'_>,
    area: Rect,
    report: &agentprof_core::analyzer::aggregate::AnyAggregateReport,
    sort: crate::watch::AggSortKey,
    selected: usize,
) {
    use agentprof_core::analyzer::aggregate::AnyAggregateReport as A;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    render_cross_header(frame, chunks[0], report);

    match report {
        A::Tool(r) => render_tool_buckets(frame, chunks[1], r, sort, selected),
        A::McpServer(r) => render_mcp_buckets(frame, chunks[1], r, sort, selected),
        A::Day(r) => render_day_buckets(frame, chunks[1], r, sort, selected),
        A::Model(r) => render_model_buckets(frame, chunks[1], r, sort, selected),
        _ => {
            let p = Paragraph::new(
                "This aggregate type is not supported by the current TUI build. \
                 Please update agentprof.",
            );
            frame.render_widget(p, chunks[1]);
        }
    }
}

fn render_cross_header(
    frame: &mut Frame<'_>,
    area: Rect,
    r: &agentprof_core::analyzer::aggregate::AnyAggregateReport,
) {
    use agentprof_core::analyzer::aggregate::AnyAggregateReport as A;
    let (by, sessions, since) = match r {
        A::Tool(x) => ("tool", x.session_count, x.since),
        A::McpServer(x) => ("mcp-server", x.session_count, x.since),
        A::Day(x) => ("day", x.session_count, x.since),
        A::Model(x) => ("model", x.session_count, x.since),
        _ => ("?", 0_usize, None),
    };
    // Wave C / D2: `since` is Option<Duration>; None means "all time".
    // Wave D2 (`m1.6.3-t1-followup-subday-window-label`) uses
    // `human_short` for sub-day windows so `--since 6h` shows
    // `window 6h` instead of the truncated `window 0d` we'd get from
    // `num_days()` (integer-truncating to 0 for everything < 24h).
    let window_label = since.map_or_else(|| "all".to_string(), crate::views::format::human_short);
    let block = Block::default().borders(Borders::ALL).title(format!(
        " Aggregate watch — by {by} | {sessions} sessions | window {window_label} ",
    ));
    let p = Paragraph::new("Keys: c/t/s/p sort  ↑/↓ select  q quit").block(block);
    frame.render_widget(p, area);
}

fn render_tool_buckets(
    frame: &mut Frame<'_>,
    area: Rect,
    r: &agentprof_core::analyzer::aggregate::AggregateReport<
        agentprof_core::analyzer::aggregate::ToolBucket,
    >,
    sort: crate::watch::AggSortKey,
    selected: usize,
) {
    use crate::watch::AggSortKey as S;
    let mut buckets: Vec<&_> = r.buckets.iter().collect();
    match sort {
        S::Calls => buckets.sort_by_key(|b| std::cmp::Reverse(b.call_count)),
        S::TotalDuration => buckets.sort_by_key(|b| std::cmp::Reverse(b.total_duration)),
        S::Sessions => buckets.sort_by_key(|b| std::cmp::Reverse(b.session_count)),
        S::Percentile50 => buckets.sort_by_key(|b| std::cmp::Reverse(b.p50_duration)),
    }
    let header = Row::new(vec![
        Cell::from("Tool"),
        Cell::from("Source"),
        Cell::from("Calls"),
        Cell::from("Total"),
        Cell::from("p50"),
        Cell::from("p95"),
        Cell::from("Sess"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row<'_>> = buckets
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(b.name.clone()),
                Cell::from(source_label(&b.source)),
                Cell::from(format!("{}", b.call_count)),
                Cell::from(human_short(b.total_duration)),
                Cell::from(human_short(b.p50_duration)),
                Cell::from(human_short(b.p95_duration)),
                Cell::from(format!("{}", b.session_count)),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(30),
            Constraint::Length(12),
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Tool buckets "),
    );
    frame.render_widget(table, area);
}

fn source_label(s: &agentprof_core::model::ToolSource) -> String {
    use agentprof_core::model::ToolSource;
    match s {
        ToolSource::Builtin => "builtin".to_string(),
        ToolSource::Mcp { server } => format!("mcp:{server}"),
        ToolSource::Skill { name } => format!("skill:{name}"),
        _ => "?".to_string(),
    }
}

fn render_mcp_buckets(
    frame: &mut Frame<'_>,
    area: Rect,
    r: &agentprof_core::analyzer::aggregate::AggregateReport<
        agentprof_core::analyzer::aggregate::McpServerBucket,
    >,
    sort: crate::watch::AggSortKey,
    selected: usize,
) {
    use crate::watch::AggSortKey as S;
    let mut buckets: Vec<&_> = r.buckets.iter().collect();
    match sort {
        S::Calls => buckets.sort_by_key(|b| std::cmp::Reverse(b.call_count)),
        S::Sessions => buckets.sort_by_key(|b| std::cmp::Reverse(b.session_count)),
        _ => buckets.sort_by_key(|b| std::cmp::Reverse(b.total_duration)),
    }
    let header = Row::new(vec![
        Cell::from("Server"),
        Cell::from("Calls"),
        Cell::from("Total"),
        Cell::from("Sess"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row<'_>> = buckets
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(b.server.clone()),
                Cell::from(format!("{}", b.call_count)),
                Cell::from(human_short(b.total_duration)),
                Cell::from(format!("{}", b.session_count)),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(30),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" MCP server buckets "),
    );
    frame.render_widget(table, area);
}

fn render_day_buckets(
    frame: &mut Frame<'_>,
    area: Rect,
    r: &agentprof_core::analyzer::aggregate::AggregateReport<
        agentprof_core::analyzer::aggregate::DayBucket,
    >,
    sort: crate::watch::AggSortKey,
    selected: usize,
) {
    // Day buckets are intrinsically chronological — see AggSortKey doc.
    let _ = sort;
    let header = Row::new(vec![
        Cell::from("Date"),
        Cell::from("Sess"),
        Cell::from("Wall"),
        Cell::from("Tool"),
        Cell::from("Util%"),
        Cell::from("Low?"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row<'_>> = r
        .buckets
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(b.date.to_string()),
                Cell::from(format!("{}", b.session_count)),
                Cell::from(human_short(b.total_wall_duration)),
                Cell::from(human_short(b.total_tool_duration)),
                Cell::from(format!("{:.1}", b.utilization_pct)),
                Cell::from(if b.is_low_utilization { "yes" } else { "no" }),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(5),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Day buckets "),
    );
    frame.render_widget(table, area);
}

fn render_model_buckets(
    frame: &mut Frame<'_>,
    area: Rect,
    r: &agentprof_core::analyzer::aggregate::AggregateReport<
        agentprof_core::analyzer::aggregate::ModelBucket,
    >,
    sort: crate::watch::AggSortKey,
    selected: usize,
) {
    use crate::watch::AggSortKey as S;
    let mut buckets: Vec<&_> = r.buckets.iter().collect();
    match sort {
        S::Sessions => buckets.sort_by_key(|b| std::cmp::Reverse(b.session_count)),
        _ => buckets.sort_by_key(|b| std::cmp::Reverse(b.total_duration)),
    }
    let header = Row::new(vec![
        Cell::from("Model"),
        Cell::from("Sess"),
        Cell::from("Total"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row<'_>> = buckets
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(b.model.clone()),
                Cell::from(format!("{}", b.session_count)),
                Cell::from(human_short(b.total_duration)),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(40),
            Constraint::Length(6),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Model buckets "),
    );
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

    // ──────────────────────────────────────────────────────────────────
    // F1.18 — empty-state rendering
    // ──────────────────────────────────────────────────────────────────

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_empty_state_centers_message_dim() {
        let block = Block::default().borders(Borders::ALL).title(" test-title ");
        let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
        term.draw(|f| {
            let area = f.area();
            render_empty_state(f, area, block, "(no data yet)");
        })
        .unwrap();
        let buffer = term.backend().buffer().clone();
        // Find the row containing the placeholder.
        let mut found_row: Option<u16> = None;
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                line.push_str(buffer[(x, y)].symbol());
            }
            if line.contains("(no data yet)") {
                found_row = Some(y);
                break;
            }
        }
        let Some(row) = found_row else {
            panic!("placeholder row must be rendered");
        };
        // Vertically roughly centered (within ±2 rows of middle).
        let mid = buffer.area.height / 2;
        let dist = row.abs_diff(mid);
        assert!(
            dist <= 2,
            "placeholder should be near vertical center (row={row}, mid={mid})"
        );
    }
}
