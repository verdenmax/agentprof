//! `RoiView` — interactive tool-rank table.
//!
//! Single table showing all tools (work + user-blocking) in one
//! selectable list. Work tools (e.g. `bash`, `read_file`) come first,
//! sortable via t/c/s/p (cycle TotalDur/Calls/SuccessRate/P50).
//! User-blocking tools (e.g. `ask_user`) come after a visual separator,
//! rendered with DIM modifier + always sorted by total duration desc
//! (they don't participate in the sort cycle — see
//! [`USER_BLOCKING_TOOLS`](agentprof_core::analyzer::tool_rank::USER_BLOCKING_TOOLS)).
//! The bottom-most strip shows recent 5 calls of the currently selected
//! work tool, or a "you waited N times" summary for user-blocking
//! selections.
//!
//! Pre-F1.11 the user-blocking section was a separate non-selectable
//! sub-table — users could see `ask_user` but couldn't navigate to it
//! with j/k. F1.11 merges into a single selectable table; the visual
//! separator + DIM styling preserves the semantic distinction without
//! the selectability paper-cut.

use agentprof_core::analyzer::ToolRankRow;
use agentprof_core::episode::{Episodes, ToolCallStatus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
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

/// Render `OK%` as a 0-100 integer percent string (or `—` for zero-call
/// rows).
///
/// F1.12 — surfaces the success-rate datum visible in the sort
/// cycle (`s` key) directly in the table without forcing users to do
/// mental arithmetic over Calls / OK columns.
///
/// Truncates rather than rounds so a single failure on a hot tool
/// (e.g. 99/100) reads `99%`, not `100%`. This makes Yellow-coding in
/// F1.13 monotone with the displayed value.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::roi::format_ok_pct;
/// assert_eq!(format_ok_pct(100, 100), "100%");
/// assert_eq!(format_ok_pct(99, 100), "99%");
/// assert_eq!(format_ok_pct(0, 0), "—");
/// assert_eq!(format_ok_pct(0, 5), "0%");
/// ```
#[must_use]
pub fn format_ok_pct(success: usize, calls: usize) -> String {
    if calls == 0 {
        return "—".to_string();
    }
    // Truncating integer division → 99/100 = 99, not 100.
    let pct = success.saturating_mul(100) / calls;
    format!("{pct}%")
}

/// Render the percent of session total tool duration occupied by this
/// tool's rows (F1.12).
///
/// `total_all_ms` is the sum of all `ToolRankRow.total_duration` across
/// the entire `tool_rank`, pre-computed once by the caller.
///
/// Always shows at least `0%` (vs the dash used by [`format_ok_pct`])
/// — a tool with measurable duration but < 0.5 % of the session is
/// still meaningful information. For session totals of 0 (no tools
/// recorded), every cell reads `—`.
///
/// # Examples
///
/// ```
/// use chrono::Duration;
/// use agentprof_tui::views::roi::format_total_pct;
/// assert_eq!(format_total_pct(Duration::seconds(30), 60_000), "50%");
/// assert_eq!(format_total_pct(Duration::seconds(1), 100_000), "1%");
/// assert_eq!(format_total_pct(Duration::seconds(10), 0), "—");
/// // < 0.5% rounds to 0 but stays visible.
/// assert_eq!(format_total_pct(Duration::milliseconds(1), 100_000), "0%");
/// ```
#[must_use]
pub fn format_total_pct(tool_total: chrono::Duration, total_all_ms: i64) -> String {
    if total_all_ms <= 0 {
        return "—".to_string();
    }
    let ms = tool_total.num_milliseconds().max(0);
    // Truncating integer division — matches `format_ok_pct` convention.
    let pct = ms.saturating_mul(100) / total_all_ms;
    format!("{pct}%")
}

/// Build the unified selectable rank ordering used by [`render`] (F1.11).
///
/// Returns `(work_sorted, blocking_sorted)`. Selection order in the
/// unified table is `work_sorted` first then `blocking_sorted` — the
/// caller (render and `roi_selected_row`) treats indices `0..work.len()`
/// as work rows and `work.len()..work.len()+blocking.len()` as
/// user-blocking rows.
///
/// Work tools sort according to `sort_key`; user-blocking tools always
/// sort by `TotalDur` desc (they don't participate in the user's sort
/// cycle — they're a sidebar in the same table).
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::roi::partition_and_sort;
/// use agentprof_tui::app::state::SortKey;
/// let rows: Vec<agentprof_core::analyzer::ToolRankRow> = vec![];
/// let (work, blocking) = partition_and_sort(&rows, SortKey::TotalDur);
/// assert!(work.is_empty());
/// assert!(blocking.is_empty());
/// ```
#[must_use]
pub fn partition_and_sort(
    rows: &[ToolRankRow],
    sort_key: SortKey,
) -> (Vec<ToolRankRow>, Vec<ToolRankRow>) {
    let (work, blocking): (Vec<_>, Vec<_>) =
        rows.iter().cloned().partition(|r| !r.is_user_blocking);
    (
        sort_rows(&work, sort_key),
        sort_rows(&blocking, SortKey::TotalDur),
    )
}

/// Resolve a unified-table selection index to its corresponding tool row
/// (F1.11), if any.
///
/// `selected` is the 0-based position in the unified ordering described
/// by [`partition_and_sort`]. Returns `None` when `selected` is out of
/// range (e.g. empty `tool_rank`).
///
/// Companion to [`is_selection_user_blocking`] for the detail strip.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::roi::roi_selected_row;
/// let rows: Vec<agentprof_core::analyzer::ToolRankRow> = vec![];
/// assert!(roi_selected_row(&rows, &rows, 0).is_none());
/// ```
#[must_use]
pub fn roi_selected_row<'a>(
    work_sorted: &'a [ToolRankRow],
    blocking_sorted: &'a [ToolRankRow],
    selected: usize,
) -> Option<&'a ToolRankRow> {
    if selected < work_sorted.len() {
        work_sorted.get(selected)
    } else {
        blocking_sorted.get(selected - work_sorted.len())
    }
}

/// Returns `true` when the unified-table selection points at a
/// user-blocking row (F1.11).
///
/// Used by the detail strip to switch between "recent 5 calls" (work
/// tools) and "you waited N times" (user-blocking tools) renderings.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::roi::is_selection_user_blocking;
/// let rows: Vec<agentprof_core::analyzer::ToolRankRow> = vec![];
/// // Out-of-range selection returns false (safe default).
/// assert!(!is_selection_user_blocking(&rows, &rows, 99));
/// ```
#[must_use]
pub const fn is_selection_user_blocking(
    work_sorted: &[ToolRankRow],
    blocking_sorted: &[ToolRankRow],
    selected: usize,
) -> bool {
    selected >= work_sorted.len() && selected < work_sorted.len() + blocking_sorted.len()
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

/// Render `RoiView` into the given area (F1.11 unified table).
///
/// Layout: vertical 2-chunk split — unified tool table (top, min 5
/// cells) and detail strip (3 cells). Pre-F1.11 had a third middle
/// chunk for the user-blocking sub-table; that's now merged into the
/// top table with a visual separator + DIM styling.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // unified tool table (work + user-blocking)
            Constraint::Length(3), // detail strip
        ])
        .split(area);

    let (sorted_work, sorted_blocking) =
        partition_and_sort(&state.report.tool_rank, state.roi_sort);
    render_unified_table(frame, chunks[0], &sorted_work, &sorted_blocking, state);
    render_detail_strip(frame, chunks[1], &sorted_work, &sorted_blocking, state);
}

#[allow(clippy::too_many_lines)]
fn render_unified_table(
    frame: &mut Frame<'_>,
    area: Rect,
    sorted_work: &[ToolRankRow],
    sorted_blocking: &[ToolRankRow],
    state: &AppState<'_>,
) {
    let title = format!(
        " RoiView (2/3) — Sort: {} · DIM = user-waiting ",
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
        Cell::from("OK%"),
        Cell::from("Total"),
        Cell::from("Tot%"),
        Cell::from("p50"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    // F1.12 — compute session total tool duration once for the Tot%
    // column. Includes ALL tools (work + user-blocking) so the
    // percentages sum to 100 % across the entire table. Saturating arith
    // for defensive overflow guard (sum-of-Durations across many calls
    // could in pathological data exceed i64::MAX).
    let total_all_ms: i64 = sorted_work
        .iter()
        .chain(sorted_blocking.iter())
        .map(|r| r.total_duration.num_milliseconds().max(0))
        .fold(0_i64, i64::saturating_add);

    // F1.11 — render order: work rows → optional separator row →
    // user-blocking rows. Separator row uses DIM modifier and a divider
    // glyph to make the section break obvious; it's NOT selectable
    // (navigation flows through it transparently because the selection
    // index is computed against the data slice, not the render slice).
    //
    // For the selection→render index mapping: if `state.roi_selected` is
    // in `0..work.len()`, the render index is the same. If it falls in
    // `work.len()..work.len()+blocking.len()`, the render index is
    // `selected + 1` (skip past the separator). The TableState::offset
    // we compute below uses render indices.
    let work_n = sorted_work.len();
    let blocking_n = sorted_blocking.len();
    let has_separator = work_n > 0 && blocking_n > 0;
    let selected_render_idx = if state.roi_selected < work_n {
        state.roi_selected
    } else if has_separator {
        state.roi_selected + 1
    } else {
        state.roi_selected
    };

    let mut trows: Vec<Row<'_>> = Vec::with_capacity(work_n + blocking_n + 1);
    for (i, r) in sorted_work.iter().enumerate() {
        let style = if i == selected_render_idx {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        trows.push(tool_row(i, r, total_all_ms, style));
    }
    if has_separator {
        // Plain DIM gray divider row — pure dashes in the Tool column.
        // The "user-waiting" semantic is conveyed by the title bar's
        // "DIM = user-waiting" note + the `*` marker in the # column of
        // user-blocking rows + the DIM styling itself. Avoiding verbose
        // text here keeps the divider working on narrow terminals.
        trows.push(
            Row::new(vec![
                Cell::from(""),
                Cell::from("─".repeat(60)),
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
                Cell::from(""),
            ])
            .style(
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ),
        );
    }
    let blocking_render_offset = if has_separator { work_n + 1 } else { work_n };
    for (i, r) in sorted_blocking.iter().enumerate() {
        let render_idx = blocking_render_offset + i;
        let mut style = Style::default().add_modifier(Modifier::DIM);
        if render_idx == selected_render_idx {
            style = style.add_modifier(Modifier::REVERSED);
        }
        // The "#" column for user-blocking rows shows `*` to distinguish
        // them from the 1-based work row numbering and make their
        // "out-of-rank" status visually obvious.
        trows.push(
            Row::new(vec![
                Cell::from("*"),
                Cell::from(truncate(&r.name, 32)),
                Cell::from(source_label(&r.source)),
                Cell::from(format!("{}", r.call_count)),
                Cell::from(format!("{}", r.success_count)),
                Cell::from(format!("{}", r.failure_count)),
                Cell::from(format_ok_pct(r.success_count, r.call_count)),
                Cell::from(human_short(r.total_duration)),
                Cell::from(format_total_pct(r.total_duration, total_all_ms)),
                Cell::from(human_short(r.p50_duration)),
            ])
            .style(style),
        );
    }

    let table = Table::new(
        trows,
        [
            Constraint::Length(3),
            Constraint::Length(32),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(7),
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
    if selected_render_idx < viewport_top {
        viewport_top = selected_render_idx;
    } else if selected_render_idx >= viewport_top + visible_rows {
        viewport_top = selected_render_idx + 1 - visible_rows;
    }
    let total_render_rows = work_n + blocking_n + usize::from(has_separator);
    let max_viewport_top = total_render_rows.saturating_sub(visible_rows);
    viewport_top = viewport_top.min(max_viewport_top);
    state.roi_viewport_top.set(viewport_top);
    // Pass our explicit offset to TableState (bypasses ratatui's auto-scroll).
    let mut table_state = TableState::default()
        .with_offset(viewport_top)
        .with_selected(Some(selected_render_idx));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn tool_row<'a>(idx: usize, r: &ToolRankRow, total_all_ms: i64, style: Style) -> Row<'a> {
    Row::new(vec![
        Cell::from(format!("{}", idx + 1)),
        Cell::from(truncate(&r.name, 32)),
        Cell::from(source_label(&r.source)),
        Cell::from(format!("{}", r.call_count)),
        Cell::from(format!("{}", r.success_count)),
        Cell::from(format!("{}", r.failure_count)),
        Cell::from(format_ok_pct(r.success_count, r.call_count)),
        Cell::from(human_short(r.total_duration)),
        Cell::from(format_total_pct(r.total_duration, total_all_ms)),
        Cell::from(human_short(r.p50_duration)),
    ])
    .style(style)
}

fn render_detail_strip(
    frame: &mut Frame<'_>,
    area: Rect,
    sorted_work: &[ToolRankRow],
    sorted_blocking: &[ToolRankRow],
    state: &AppState<'_>,
) {
    let selected_row = roi_selected_row(sorted_work, sorted_blocking, state.roi_selected);
    let is_blocking = is_selection_user_blocking(sorted_work, sorted_blocking, state.roi_selected);

    let title = selected_row.map_or_else(
        || " Selected: (none) ".to_string(),
        |row| {
            if is_blocking {
                format!(" Selected: {} (user-waiting) ", row.name)
            } else {
                format!(" Selected: {} ", row.name)
            }
        },
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line<'_>> = match selected_row {
        None => vec![Line::from("  (no row selected)")],
        Some(row) if is_blocking => vec![Line::from(format!(
            "  You waited {} time{} totaling {} (not counted in agent work)",
            row.call_count,
            if row.call_count == 1 { "" } else { "s" },
            human_short(row.total_duration),
        ))],
        Some(row) => {
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
        }
    };
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

    // ──────────────────────────────────────────────────────────────────
    // F1.11 — unified table partitioning + selection mapping tests
    // ──────────────────────────────────────────────────────────────────

    fn blocking_row(name: &str, calls: usize, total_ms: i64) -> ToolRankRow {
        let per_call = if calls == 0 {
            total_ms
        } else {
            total_ms / i64::try_from(calls).unwrap_or(i64::MAX)
        };
        let mut r = row(name, calls, calls, 0, total_ms, per_call);
        r.is_user_blocking = true;
        r
    }

    #[test]
    fn partition_and_sort_separates_user_blocking_correctly() {
        let rows = vec![
            row("bash", 10, 10, 0, 100, 10),
            blocking_row("ask_user", 3, 5000),
            row("read_file", 5, 5, 0, 50, 8),
        ];
        let (work, blocking) = partition_and_sort(&rows, SortKey::TotalDur);
        assert_eq!(work.len(), 2);
        assert_eq!(blocking.len(), 1);
        // Work sorted by TotalDur desc.
        assert_eq!(work[0].name, "bash");
        assert_eq!(work[1].name, "read_file");
        assert_eq!(blocking[0].name, "ask_user");
        assert!(blocking[0].is_user_blocking);
    }

    #[test]
    fn partition_and_sort_handles_empty_inputs() {
        let rows: Vec<ToolRankRow> = vec![];
        let (work, blocking) = partition_and_sort(&rows, SortKey::Calls);
        assert!(work.is_empty());
        assert!(blocking.is_empty());
    }

    #[test]
    fn roi_selected_row_indexes_work_then_blocking() {
        // selection 0..work_len → work rows.
        // selection work_len..work_len+blocking_len → blocking rows.
        // selection >= total → None.
        let work = vec![row("bash", 1, 1, 0, 10, 5), row("read", 1, 1, 0, 5, 5)];
        let blocking = vec![blocking_row("ask_user", 1, 1000)];
        assert_eq!(
            roi_selected_row(&work, &blocking, 0).map(|r| r.name.as_str()),
            Some("bash")
        );
        assert_eq!(
            roi_selected_row(&work, &blocking, 1).map(|r| r.name.as_str()),
            Some("read")
        );
        // Index 2 → first blocking row (no separator counted in selection).
        assert_eq!(
            roi_selected_row(&work, &blocking, 2).map(|r| r.name.as_str()),
            Some("ask_user")
        );
        // Out of range.
        assert!(roi_selected_row(&work, &blocking, 3).is_none());
        assert!(roi_selected_row(&work, &blocking, 99).is_none());
    }

    #[test]
    fn roi_selected_row_for_empty_returns_none() {
        let empty: Vec<ToolRankRow> = vec![];
        assert!(roi_selected_row(&empty, &empty, 0).is_none());
    }

    #[test]
    fn is_selection_user_blocking_only_true_for_blocking_indices() {
        let work = vec![row("bash", 1, 1, 0, 10, 5), row("read", 1, 1, 0, 5, 5)];
        let blocking = vec![
            blocking_row("ask_user", 1, 1000),
            blocking_row("ask_other", 1, 500),
        ];
        // Work indices.
        assert!(!is_selection_user_blocking(&work, &blocking, 0));
        assert!(!is_selection_user_blocking(&work, &blocking, 1));
        // Blocking indices.
        assert!(is_selection_user_blocking(&work, &blocking, 2));
        assert!(is_selection_user_blocking(&work, &blocking, 3));
        // Out-of-range = false (defensive).
        assert!(!is_selection_user_blocking(&work, &blocking, 4));
        assert!(!is_selection_user_blocking(&work, &blocking, 99));
    }

    #[test]
    fn is_selection_user_blocking_false_when_no_blocking_tools() {
        let work = vec![row("bash", 1, 1, 0, 10, 5)];
        let blocking: Vec<ToolRankRow> = vec![];
        assert!(!is_selection_user_blocking(&work, &blocking, 0));
        assert!(!is_selection_user_blocking(&work, &blocking, 1));
    }

    // ──────────────────────────────────────────────────────────────────
    // F1.12 — OK% + Total% column formatters
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn format_ok_pct_renders_perfect_one_hundred() {
        assert_eq!(format_ok_pct(100, 100), "100%");
        assert_eq!(format_ok_pct(7, 7), "100%");
        assert_eq!(format_ok_pct(1, 1), "100%");
    }

    #[test]
    fn format_ok_pct_truncates_rather_than_rounds() {
        // 99/100 = 99, NOT 100 (would mislead users into thinking the
        // tool was perfect when it wasn't). Truncation also keeps the
        // value monotone with F1.13 color thresholds.
        assert_eq!(format_ok_pct(99, 100), "99%");
        assert_eq!(format_ok_pct(2, 3), "66%"); // 0.666... truncates
        assert_eq!(format_ok_pct(199, 200), "99%");
    }

    #[test]
    fn format_ok_pct_zero_calls_renders_dash() {
        // Distinct from `0%` which means "called, all failed".
        assert_eq!(format_ok_pct(0, 0), "—");
    }

    #[test]
    fn format_ok_pct_zero_success_with_calls_renders_zero() {
        assert_eq!(format_ok_pct(0, 5), "0%");
        assert_eq!(format_ok_pct(0, 1), "0%");
    }

    #[test]
    fn format_total_pct_renders_basic_percentages() {
        use chrono::Duration as D;
        // 30/60 = 50 %
        assert_eq!(format_total_pct(D::seconds(30), 60_000), "50%");
        // 1/100 = 1 %
        assert_eq!(format_total_pct(D::seconds(1), 100_000), "1%");
    }

    #[test]
    fn format_total_pct_zero_session_total_renders_dash() {
        use chrono::Duration as D;
        assert_eq!(format_total_pct(D::seconds(10), 0), "—");
        assert_eq!(format_total_pct(D::seconds(10), -1), "—");
    }

    #[test]
    fn format_total_pct_tiny_fraction_still_shows_zero() {
        use chrono::Duration as D;
        // < 0.5 % truncates to 0 but stays visible as "0%" (vs "—").
        assert_eq!(format_total_pct(D::milliseconds(1), 100_000), "0%");
    }
}
