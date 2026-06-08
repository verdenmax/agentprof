//! View `[5]` — MCP Server Waste. Split-pane: server list left + tool
//! detail right (same layout pattern as Models view, key `4`).
//!
//! See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md` §7.4
//! and `docs/internals/adr-0015-mcp-waste-architecture.md` D-4.
//!
//! M1.6.5 T5.1 ships state types only; the split-pane `render` function
//! and key handling land in T5.2 / T5.3.

use ratatui::widgets::TableState;

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
