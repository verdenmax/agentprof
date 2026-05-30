//! `RoiView` — interactive tool-rank table.
//!
//! Top section: "work tools" (rows where `is_user_blocking == false`),
//! sortable via t/c/s/p (cycle TotalDur/Calls/SuccessRate/P50). Bottom
//! section: user-blocking tools (e.g. `ask_user`), always sorted by total
//! duration descending. Bottom-most strip shows recent 5 calls of the
//! currently selected work tool.

use agentprof_core::analyzer::ToolRankRow;
use agentprof_core::episode::{Episodes, ToolCallStatus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span as TextSpan};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::state::{AppState, SortKey};
use crate::views::format::human_short;

/// Apply a [`SortKey`] to a slice of rows, returning a freshly sorted Vec.
///
/// Work tools and user-blocking tools are NOT split here — the caller does
/// the partition. This function is the pure-sort step that the test suite
/// pins.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::roi::sort_rows;
/// use agentprof_tui::app::state::SortKey;
/// // (empty in vec → empty out)
/// let v: Vec<agentprof_core::analyzer::ToolRankRow> = vec![];
/// assert!(sort_rows(&v, SortKey::Calls).is_empty());
/// ```
#[must_use]
pub fn sort_rows(rows: &[ToolRankRow], key: SortKey) -> Vec<ToolRankRow> {
    let mut v: Vec<ToolRankRow> = rows.to_vec();
    match key {
        SortKey::TotalDur => v.sort_by(|a, b| b.total_duration.cmp(&a.total_duration)),
        SortKey::Calls => v.sort_by(|a, b| b.call_count.cmp(&a.call_count)),
        SortKey::SuccessRate => v.sort_by(|a, b| {
            let ra = success_ratio(a);
            let rb = success_ratio(b);
            rb.partial_cmp(&ra)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.call_count.cmp(&a.call_count))
        }),
        SortKey::P50 => v.sort_by(|a, b| b.p50_duration.cmp(&a.p50_duration)),
    }
    v
}

#[allow(clippy::cast_precision_loss)]
fn success_ratio(r: &ToolRankRow) -> f64 {
    if r.call_count == 0 {
        0.0
    } else {
        r.success_count as f64 / r.call_count as f64
    }
}

/// Extract the most recent up-to-5 calls of `tool_name` from `episodes`.
///
/// Formats them as a `Vec` of `(turn_id, duration_str, status_glyph)`
/// tuples. "Most recent" = last entries by index in `ToolEpisode.calls`.
///
/// Used by the bottom "Selected: X" strip.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::Episodes;
/// use agentprof_tui::views::roi::recent_calls;
/// assert!(recent_calls("missing", &Episodes::new()).is_empty());
/// ```
#[must_use]
pub fn recent_calls<'a>(
    tool_name: &str,
    episodes: &'a Episodes,
) -> Vec<(&'a str, String, &'static str)> {
    let Some(ep) = episodes.tools.get(tool_name) else {
        return Vec::new();
    };
    let n = ep.calls.len();
    let start = n.saturating_sub(5);
    ep.calls[start..n]
        .iter()
        .map(|c| {
            let turn = c.turn_id.as_deref().unwrap_or("(no turn)");
            let dur = human_short(c.span.duration());
            let glyph: &'static str = match &c.status {
                ToolCallStatus::Success => "✓",
                ToolCallStatus::Failure { .. } => "✗",
                ToolCallStatus::OrphanSynthesizedStart => "○",
                ToolCallStatus::OpenAtEndOfSession => "·",
                _ => "?",
            };
            (turn, dur, glyph)
        })
        .collect()
}

/// Render `RoiView` into the given area.
///
/// Layout: vertical 3-chunk split — work-tools table (top, min 5 cells),
/// user-blocking table (4 cells), detail strip (3 cells).
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // top work-tools table
            Constraint::Length(4), // user-blocking section
            Constraint::Length(3), // detail strip
        ])
        .split(area);

    let (work, blocking): (Vec<_>, Vec<_>) = state
        .report
        .tool_rank
        .iter()
        .cloned()
        .partition(|r| !r.is_user_blocking);
    let sorted_work = sort_rows(&work, state.roi_sort);
    let sorted_blocking = sort_rows(&blocking, SortKey::TotalDur);

    render_work_table(frame, chunks[0], &sorted_work, state);
    render_blocking_table(frame, chunks[1], &sorted_blocking);
    render_detail_strip(frame, chunks[2], &sorted_work, state);
}

fn render_work_table(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[ToolRankRow],
    state: &AppState<'_>,
) {
    let title = format!(
        " RoiView (2/3) — Sort: {} ",
        match state.roi_sort {
            SortKey::TotalDur => "[t]total  c=calls  s=success%  p=p50",
            SortKey::Calls => "t=total  [c]calls  s=success%  p=p50",
            SortKey::SuccessRate => "t=total  c=calls  [s]success%  p=p50",
            SortKey::P50 => "t=total  c=calls  s=success%  [p]p50",
        }
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    let header = Row::new(vec![
        Cell::from("#"),
        Cell::from("Tool"),
        Cell::from("Source"),
        Cell::from("Calls"),
        Cell::from("OK"),
        Cell::from("Fail"),
        Cell::from("Total"),
        Cell::from("p50"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let trows: Vec<Row<'_>> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let style = if i == state.roi_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(truncate(&r.name, 32)),
                Cell::from(source_label(&r.source)),
                Cell::from(format!("{}", r.call_count)),
                Cell::from(format!("{}", r.success_count)),
                Cell::from(format!("{}", r.failure_count)),
                Cell::from(human_short(r.total_duration)),
                Cell::from(human_short(r.p50_duration)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        trows,
        [
            Constraint::Length(3),
            Constraint::Length(34),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(block);
    // Edge-triggered viewport: compute our own offset (instead of relying on
    // ratatui's "minimum offset to show selected" which bottom-anchors) so
    // the cursor can move freely within the viewport. Persist offset via
    // Cell on AppState so it stays put until the cursor leaves the window.
    //
    // Body row count = pane height minus 2 (top + bottom border) minus 1
    // (header row). max(1) to avoid zero-row edge case on tiny panes.
    let visible_rows = (area.height as usize).saturating_sub(3).max(1);
    let mut viewport_top = state.roi_viewport_top.get();
    if state.roi_selected < viewport_top {
        viewport_top = state.roi_selected;
    } else if state.roi_selected >= viewport_top + visible_rows {
        viewport_top = state.roi_selected + 1 - visible_rows;
    }
    let max_viewport_top = rows.len().saturating_sub(visible_rows);
    viewport_top = viewport_top.min(max_viewport_top);
    state.roi_viewport_top.set(viewport_top);
    // Pass our explicit offset to TableState (bypasses ratatui's auto-scroll).
    let mut table_state = TableState::default()
        .with_offset(viewport_top)
        .with_selected(Some(state.roi_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_blocking_table(frame: &mut Frame<'_>, area: Rect, rows: &[ToolRankRow]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" User-blocking (user think time) ");
    let trows: Vec<Row<'_>> = rows
        .iter()
        .map(|r| {
            Row::new(vec![
                Cell::from(truncate(&r.name, 32)),
                Cell::from(format!("{}", r.call_count)),
                Cell::from(human_short(r.total_duration)),
            ])
        })
        .collect();
    let table = Table::new(
        trows,
        [
            Constraint::Length(34),
            Constraint::Length(7),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from("Tool"),
            Cell::from("Calls"),
            Cell::from("Total"),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(block);
    frame.render_widget(table, area);
}

fn render_detail_strip(
    frame: &mut Frame<'_>,
    area: Rect,
    work: &[ToolRankRow],
    state: &AppState<'_>,
) {
    let title = work.get(state.roi_selected).map_or_else(
        || " Selected: (none) ".to_string(),
        |row| format!(" Selected: {} ", row.name),
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line<'_>> = work.get(state.roi_selected).map_or_else(
        || vec![Line::from("  (no row selected)")],
        |row| {
            let calls = recent_calls(&row.name, state.episodes);
            if calls.is_empty() {
                vec![Line::from("  (no calls)")]
            } else {
                let spans: Vec<TextSpan<'_>> = calls
                    .into_iter()
                    .map(|(turn, dur, glyph)| TextSpan::raw(format!("  {turn} ({dur}{glyph})")))
                    .collect();
                vec![Line::from(spans)]
            }
        },
    );
    frame.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn source_label(src: &agentprof_core::model::ToolSource) -> String {
    use agentprof_core::model::ToolSource as TS;
    match src {
        TS::Builtin => "builtin".to_string(),
        TS::Mcp { server } => format!("mcp/{server}"),
        TS::Skill { name } => format!("skill/{name}"),
        _ => "?".to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use agentprof_core::analyzer::ToolRankRow;
    use agentprof_core::episode::{Episodes, Span, ToolCall, ToolEpisode};
    use agentprof_core::model::ToolSource;
    use chrono::{TimeZone, Utc};

    fn row(
        name: &str,
        calls: usize,
        succ: usize,
        fail: usize,
        total_ms: i64,
        p50_ms: i64,
    ) -> ToolRankRow {
        ToolRankRow::new(
            name.into(),
            ToolSource::Builtin,
            calls,
            succ,
            fail,
            0, // orphan_count
            0, // user_requested_count
            chrono::Duration::milliseconds(total_ms),
            chrono::Duration::milliseconds(p50_ms),
            chrono::Duration::milliseconds(p50_ms),
            chrono::Duration::milliseconds(p50_ms),
        )
    }

    #[test]
    fn sort_rows_by_total_dur_desc() {
        let rows = vec![row("a", 10, 10, 0, 100, 10), row("b", 5, 5, 0, 1000, 200)];
        let sorted = sort_rows(&rows, SortKey::TotalDur);
        assert_eq!(sorted[0].name, "b");
        assert_eq!(sorted[1].name, "a");
    }

    #[test]
    fn sort_rows_by_calls_desc() {
        let rows = vec![row("a", 10, 10, 0, 100, 10), row("b", 5, 5, 0, 1000, 200)];
        let sorted = sort_rows(&rows, SortKey::Calls);
        assert_eq!(sorted[0].name, "a");
    }

    #[test]
    fn sort_rows_by_success_rate_then_calls_tiebreak() {
        // Both 100% success — tiebreak by call_count desc.
        let rows = vec![row("low", 3, 3, 0, 10, 1), row("high", 30, 30, 0, 10, 1)];
        let sorted = sort_rows(&rows, SortKey::SuccessRate);
        assert_eq!(sorted[0].name, "high"); // more calls wins tiebreak
    }

    #[test]
    fn sort_rows_by_p50_desc() {
        let rows = vec![row("slow", 1, 1, 0, 1, 999), row("fast", 1, 1, 0, 1, 1)];
        let sorted = sort_rows(&rows, SortKey::P50);
        assert_eq!(sorted[0].name, "slow");
    }

    #[test]
    fn recent_calls_truncates_to_5_most_recent() {
        let mut ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        for i in 0..7 {
            let mut c = ToolCall::new(Span::new(t0, t0 + chrono::Duration::milliseconds(10)));
            c.turn_id = Some(format!("t{i}"));
            ep.calls.push(c);
        }
        let mut episodes = Episodes::new();
        episodes.tools.insert("bash".into(), ep);
        let recent = recent_calls("bash", &episodes);
        assert_eq!(recent.len(), 5);
        // Most recent 5 = t2..t6.
        assert_eq!(recent[0].0, "t2");
        assert_eq!(recent[4].0, "t6");
    }

    #[test]
    fn recent_calls_for_missing_tool_returns_empty() {
        let episodes = Episodes::new();
        assert!(recent_calls("nonexistent", &episodes).is_empty());
    }

    #[test]
    fn truncate_long_name_appends_ellipsis() {
        assert_eq!(truncate("abcdefgh", 5), "abcd…");
        assert_eq!(truncate("abc", 5), "abc");
    }
}
