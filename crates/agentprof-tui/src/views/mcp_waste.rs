//! View `[5]` — MCP Server Waste. Split-pane: server list left + tool
//! detail right (same layout pattern as Models view, key `4`).
//!
//! See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md` §7.4
//! and `docs/internals/adr-0015-mcp-waste-architecture.md` D-4.
//!
//! M1.6.5 T5.1 ships state types; T5.2 adds the split-pane [`render`]
//! function (banner + 40/60 horizontal split). Key handling +
//! `AppRunner` registration land in T5.3.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use agentprof_core::model::WasteReport;

/// Render the MCP Waste view into `area`. Split-pane: left = servers
/// summary table; right = tools for the cursor-selected server.
///
/// Top banner shows the data-source provenance and loaded/unused totals;
/// the body splits 40% / 60% horizontally between the server summary
/// (left) and the tools of the cursor-selected server (right). The
/// server cursor is highlighted with `REVERSED` style; the tools pane
/// auto-updates for the selected server and respects the
/// `unused_only` filter on [`McpWasteState`].
///
/// # Examples
///
/// ```ignore
/// // Render is exercised end-to-end via TUI integration tests; this
/// // signature documentation is sufficient for callers.
/// use agentprof_core::model::WasteReport;
/// use agentprof_tui::views::mcp_waste::{render, McpWasteState};
/// let report = WasteReport::default();
/// let mut state = McpWasteState::new();
/// // render(frame, area, &report, &mut state);
/// let _ = (&report, &mut state);
/// ```
pub fn render(f: &mut ratatui::Frame, area: Rect, report: &WasteReport, state: &mut McpWasteState) {
    // Top banner (data source + totals).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let banner = banner_lines(report);
    let banner_p = ratatui::widgets::Paragraph::new(banner)
        .block(Block::default().borders(Borders::ALL).title(" MCP Waste "));
    f.render_widget(banner_p, outer[0]);

    // Body split: 40% left server table / 60% right tool table.
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(outer[1]);

    render_server_table(f, body[0], report, state);
    render_tool_table(f, body[1], report, state);
}

fn banner_lines(r: &WasteReport) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    let ds = match r.data_source {
        agentprof_core::model::WasteDataSource::None => "no data".to_string(),
        agentprof_core::model::WasteDataSource::Wire => "wire notices".to_string(),
        agentprof_core::model::WasteDataSource::Config => "mcp.json".to_string(),
        agentprof_core::model::WasteDataSource::Both => "wire + mcp.json".to_string(),
        // `WasteDataSource` is `#[non_exhaustive]`; future variants render as "unknown".
        _ => "unknown".to_string(),
    };
    let fully = r.server_waste.iter().filter(|s| s.is_fully_unused).count();
    vec![Line::from(vec![Span::raw(format!(
        "Source: {ds}   Loaded: {}   Unused: {}   Fully-unused servers: {fully}",
        r.total_loaded_tool_count, r.total_unused_tool_count
    ))])]
}

fn render_server_table(
    f: &mut ratatui::Frame,
    area: Rect,
    report: &WasteReport,
    state: &mut McpWasteState,
) {
    let rows: Vec<Row> = report
        .server_waste
        .iter()
        .map(|sw| {
            Row::new(vec![
                Cell::from(sw.server.clone()),
                Cell::from(format!("{}/{}", sw.loaded_count, sw.unused_count)),
                Cell::from(if sw.is_fully_unused { "!" } else { "" }),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(8),
            Constraint::Length(2),
        ],
    )
    .header(
        Row::new(vec!["Server", "L/U", "F"]).style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Servers ({}) ", report.server_waste.len())),
    )
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_stateful_widget(table, area, &mut state.server_cursor);
}

// `state: &mut` kept for symmetry with `render_server_table` and for the
// future filter-cursor state introduced in T5.3.
#[allow(clippy::needless_pass_by_ref_mut)]
fn render_tool_table(
    f: &mut ratatui::Frame,
    area: Rect,
    report: &WasteReport,
    state: &mut McpWasteState,
) {
    let selected = state.server_cursor.selected().unwrap_or(0);
    let title = report.server_waste.get(selected).map_or_else(
        || " (no server) ".to_string(),
        |sw| format!(" Tools in {} ", sw.server),
    );

    let rows: Vec<Row> = report
        .server_waste
        .get(selected)
        .map(|sw| {
            sw.tools
                .iter()
                .filter(|t| !state.unused_only || t.call_count == 0)
                .map(|t| {
                    Row::new(vec![
                        Cell::from(t.tool_name.clone()),
                        Cell::from(t.call_count.to_string()),
                        Cell::from(format!("{:?}", t.loaded_source)),
                    ])
                })
                .collect()
        })
        .unwrap_or_default();

    let footer = if state.unused_only {
        " (alphabetical; unused-only filter: ON — press u to toggle) "
    } else {
        " (alphabetical; unused-only filter: off — press u to toggle) "
    };

    let table = Table::new(
        rows,
        [
            Constraint::Min(30),
            Constraint::Length(6),
            Constraint::Length(18),
        ],
    )
    .header(
        Row::new(vec!["Tool", "Calls", "Source"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(table, area);
    // Footer line rendered below the table border via a separate paragraph
    // would require splitting `area`; for simplicity, the footer is logged
    // in the bottom-bar status (see T5.4).
    let _ = footer;
}

/// Per-view scrolling + focus state for the MCP Waste view, persisted on
/// [`crate::app::state::AppState`].
///
/// Holds the left-pane server cursor and the right-pane "unused tools
/// only" filter toggle. The actual render function consumes this state
/// in T5.2; key handling lives in `app::state::dispatch` (T5.3).
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::mcp_waste::McpWasteState;
///
/// let mut s = McpWasteState::new();
/// assert_eq!(s.server_cursor.selected(), Some(0));
/// assert!(!s.unused_only);
///
/// s.cursor_down(3);
/// assert_eq!(s.server_cursor.selected(), Some(1));
///
/// s.toggle_unused_only();
/// assert!(s.unused_only);
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct McpWasteState {
    /// Cursor on the left server-summary table (one row per MCP server).
    pub server_cursor: TableState,
    /// Whether the right pane filters to tools with `call_count == 0`.
    pub unused_only: bool,
}

impl McpWasteState {
    /// Construct a fresh state with the server cursor positioned on row 0
    /// and the unused-only filter disabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::views::mcp_waste::McpWasteState;
    ///
    /// let s = McpWasteState::new();
    /// assert_eq!(s.server_cursor.selected(), Some(0));
    /// assert!(!s.unused_only);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        let mut server_cursor = TableState::default();
        server_cursor.select(Some(0));
        Self {
            server_cursor,
            unused_only: false,
        }
    }

    /// Toggle the right-pane "unused tools only" filter.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::views::mcp_waste::McpWasteState;
    ///
    /// let mut s = McpWasteState::new();
    /// s.toggle_unused_only();
    /// assert!(s.unused_only);
    /// s.toggle_unused_only();
    /// assert!(!s.unused_only);
    /// ```
    pub fn toggle_unused_only(&mut self) {
        self.unused_only = !self.unused_only;
    }

    /// Move the server cursor down by one, bounded by `server_count`.
    ///
    /// No-ops when `server_count == 0`. When `server_count > 0`, the cursor
    /// is clamped to `server_count - 1`.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::views::mcp_waste::McpWasteState;
    ///
    /// let mut s = McpWasteState::new();
    /// s.cursor_down(3);
    /// assert_eq!(s.server_cursor.selected(), Some(1));
    /// s.cursor_down(3);
    /// assert_eq!(s.server_cursor.selected(), Some(2));
    /// s.cursor_down(3);
    /// assert_eq!(s.server_cursor.selected(), Some(2));
    /// ```
    pub fn cursor_down(&mut self, server_count: usize) {
        if server_count == 0 {
            return;
        }
        let cur = self.server_cursor.selected().unwrap_or(0);
        let next = (cur + 1).min(server_count.saturating_sub(1));
        self.server_cursor.select(Some(next));
    }

    /// Move the server cursor up by one, saturating at 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::views::mcp_waste::McpWasteState;
    ///
    /// let mut s = McpWasteState::new();
    /// s.cursor_down(5);
    /// s.cursor_down(5);
    /// assert_eq!(s.server_cursor.selected(), Some(2));
    /// s.cursor_up();
    /// assert_eq!(s.server_cursor.selected(), Some(1));
    /// s.cursor_up();
    /// s.cursor_up();
    /// assert_eq!(s.server_cursor.selected(), Some(0));
    /// ```
    pub fn cursor_up(&mut self) {
        let cur = self.server_cursor.selected().unwrap_or(0);
        self.server_cursor.select(Some(cur.saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_row_zero_filter_off() {
        let s = McpWasteState::new();
        assert_eq!(s.server_cursor.selected(), Some(0));
        assert!(!s.unused_only);
    }

    #[test]
    fn cursor_down_clamps_to_last_row() {
        let mut s = McpWasteState::new();
        for _ in 0..10 {
            s.cursor_down(3);
        }
        assert_eq!(s.server_cursor.selected(), Some(2));
    }

    #[test]
    fn cursor_down_zero_count_is_noop() {
        let mut s = McpWasteState::new();
        s.cursor_down(0);
        assert_eq!(s.server_cursor.selected(), Some(0));
    }

    #[test]
    fn cursor_up_saturates_at_zero() {
        let mut s = McpWasteState::new();
        s.cursor_up();
        s.cursor_up();
        assert_eq!(s.server_cursor.selected(), Some(0));
    }

    #[test]
    fn toggle_unused_only_flips() {
        let mut s = McpWasteState::new();
        s.toggle_unused_only();
        assert!(s.unused_only);
        s.toggle_unused_only();
        assert!(!s.unused_only);
    }
}
