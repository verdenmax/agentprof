//! View enum + per-view render dispatch.
//!
//! M1.5 shipped three views: [`View::Flamegraph`], [`View::Roi`],
//! [`View::Aggregate`]. F1.7 adds [`View::Models`] (key `4`). M1.6.5 adds
//! [`View::McpWaste`] (key `5`). The submodules implement
//! `render(frame, area, state)` and are wired together by `app::AppRunner`
//! (T6).

/// The views shipped by the TUI.
///
/// Selected by the user via keys `1` / `2` / `3` / `4` / `5` (with `Tab` /
/// `Shift-Tab` cycling). The current variant lives on
/// `app::state::AppState::view`.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::View;
/// assert_eq!(View::Flamegraph.next(), View::Roi);
/// assert_eq!(View::Roi.next(), View::Aggregate);
/// assert_eq!(View::Aggregate.next(), View::Models);
/// assert_eq!(View::Models.next(), View::McpWaste);
/// assert_eq!(View::McpWaste.next(), View::Flamegraph);
/// assert_eq!(View::McpWaste.prev(), View::Models);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum View {
    /// Per-turn horizontal gantt of tool calls; rendered by `views::flamegraph`.
    Flamegraph,
    /// Interactive tool-rank table; rendered by `views::roi`.
    Roi,
    /// Single-session aggregate (By Mode + By Hook); rendered by `views::aggregate`.
    Aggregate,
    /// Session-level per-model token rollup; rendered by `views::models` (F1.7).
    Models,
    /// MCP server waste split-pane; rendered by `views::mcp_waste` (M1.6.5).
    McpWaste,
}

impl View {
    /// Cycle forward: Flamegraph → Roi → Aggregate → Models → `McpWaste` → Flamegraph.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Flamegraph => Self::Roi,
            Self::Roi => Self::Aggregate,
            Self::Aggregate => Self::Models,
            Self::Models => Self::McpWaste,
            Self::McpWaste => Self::Flamegraph,
        }
    }

    /// Cycle backward: Flamegraph → `McpWaste` → Models → Aggregate → Roi → Flamegraph.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Flamegraph => Self::McpWaste,
            Self::Roi => Self::Flamegraph,
            Self::Aggregate => Self::Roi,
            Self::Models => Self::Aggregate,
            Self::McpWaste => Self::Models,
        }
    }
}

// Submodules
pub mod aggregate;
pub mod flamegraph;
pub mod format;
pub mod mcp_waste; // M1.6.5
pub mod models; // F1.7
pub mod roi;
pub mod turn_detail;
