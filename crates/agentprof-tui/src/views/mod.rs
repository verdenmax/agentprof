//! View enum + per-view render dispatch.
//!
//! M1.5 ships three views: [`View::Flamegraph`], [`View::Roi`],
//! [`View::Aggregate`]. The submodules implement `render(frame, area, state)`
//! and are wired together by `app::AppRunner` (T6).

/// The three views shipped in M1.5.
///
/// Selected by the user via keys `1` / `2` / `3` (with `Tab` / `Shift-Tab`
/// cycling). The current variant lives on `app::state::AppState::view`.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::View;
/// assert_eq!(View::Flamegraph.next(), View::Roi);
/// assert_eq!(View::Aggregate.next(), View::Flamegraph);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    /// Per-turn horizontal gantt of tool calls; rendered by `views::flamegraph`.
    Flamegraph,
    /// Interactive tool-rank table; rendered by `views::roi`.
    Roi,
    /// Single-session aggregate (By Mode + By Hook); rendered by `views::aggregate`.
    Aggregate,
}

impl View {
    /// Cycle forward: Flamegraph → Roi → Aggregate → Flamegraph.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Flamegraph => Self::Roi,
            Self::Roi => Self::Aggregate,
            Self::Aggregate => Self::Flamegraph,
        }
    }

    /// Cycle backward: Flamegraph → Aggregate → Roi → Flamegraph.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Flamegraph => Self::Aggregate,
            Self::Roi => Self::Flamegraph,
            Self::Aggregate => Self::Roi,
        }
    }
}

// Submodules added by T3–T5.
pub mod aggregate;
pub mod flamegraph;
pub mod roi;
