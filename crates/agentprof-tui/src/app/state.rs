//! `AppState` + `dispatch` — the TUI's pure-logic state machine.
//!
//! All input flows through [`dispatch`] which mutates [`AppState`] and
//! returns an [`Action`]. The render loop in `app::AppRunner` (T6) reads
//! the resulting state to draw each frame.
//!
//! `AppState` borrows `&AnalysisReport` and `&Episodes`; both come from the
//! CLI (it owns the parse output). The TUI never owns these — it only
//! renders, never mutates.

use std::collections::HashMap;

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::Episodes;
use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::event::Event;
use crate::views::View;

/// Sort key for [`crate::views::roi`]'s tool table.
///
/// Cycled by keys `t` / `c` / `s` / `p` when the active view is [`View::Roi`].
/// Default is [`SortKey::TotalDur`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// Sort by `total_duration` descending.
    #[default]
    TotalDur,
    /// Sort by `call_count` descending.
    Calls,
    /// Sort by success ratio (`success_count / call_count`) descending;
    /// tiebreak by `call_count` descending.
    SuccessRate,
    /// Sort by `p50_duration` descending.
    P50,
}

/// Action returned by [`dispatch`] for the run loop to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Continue the loop; no special action.
    None,
    /// Exit the loop cleanly with status 0.
    Quit,
}

/// Pure-data view of the TUI.
///
/// Borrowed `&AnalysisReport` / `&Episodes` come from the CLI (single
/// ownership). The state owns only its own scroll positions, sort key, and
/// selected indices.
#[non_exhaustive]
#[derive(Debug)]
pub struct AppState<'a> {
    /// Currently active view.
    pub view: View,
    /// Per-view (vertical, horizontal) scroll offset.
    pub scroll: HashMap<View, (u16, u16)>,
    /// Active sort key for `RoiView`.
    pub roi_sort: SortKey,
    /// Selected row in `RoiView`'s work-tools table.
    pub roi_selected: usize,
    /// Selected turn index in `FlamegraphView`.
    pub flame_selected: usize,
    /// Help overlay open. While true, all input keys close the overlay
    /// instead of being interpreted.
    pub help_open: bool,
    /// Viewport offset for Flamegraph (interior mutability — render
    /// computes edge-triggered viewport without requiring `&mut state`
    /// through the call chain).
    pub flame_viewport_top: std::cell::Cell<usize>,
    /// Viewport offset for `RoiView` work table (same rationale).
    pub roi_viewport_top: std::cell::Cell<usize>,
    /// Source report (turn / tool / hook rollups).
    pub report: &'a AnalysisReport,
    /// Source episodes (per-call timing for `FlamegraphView`).
    pub episodes: &'a Episodes,
}

impl<'a> AppState<'a> {
    /// Construct an initial state on [`View::Flamegraph`] with default
    /// scroll / sort / selection.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::model::SessionMeta;
    /// use agentprof_tui::app::state::AppState;
    /// use agentprof_tui::views::View;
    /// use chrono::Utc;
    /// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    /// let report = AnalysisReport::new(meta);
    /// let episodes = Episodes::new();
    /// let state = AppState::new(&report, &episodes);
    /// assert_eq!(state.view, View::Flamegraph);
    /// ```
    #[must_use]
    pub fn new(report: &'a AnalysisReport, episodes: &'a Episodes) -> Self {
        Self {
            view: View::Flamegraph,
            scroll: HashMap::new(),
            roi_sort: SortKey::default(),
            roi_selected: 0,
            flame_selected: 0,
            help_open: false,
            flame_viewport_top: std::cell::Cell::new(0),
            roi_viewport_top: std::cell::Cell::new(0),
            report,
            episodes,
        }
    }
}

/// State-machine entry point.
///
/// Returns [`Action::Quit`] when the user presses `q` / Ctrl-C. All other
/// input mutates `state` in place.
///
/// Key bindings (M1.5 post-shipping UX fix — supersedes spec §7's
/// conflict-resolution rule):
/// - `1`/`2`/`3` ALWAYS switch view (Flamegraph / Roi / Aggregate)
/// - `t`/`c`/`s`/`p` cycle `RoiView` sort key (`TotalDur` / `Calls` / `SuccessRate` / `P50`);
///   only meaningful when view == Roi, ignored elsewhere
/// - `Tab` / `Shift-Tab` cycle views
/// - `↑` / `↓` scroll/select
/// - `q` / Ctrl-C quit
/// - `?` toggle help overlay (any key closes)
#[allow(clippy::needless_pass_by_value)]
pub fn dispatch(state: &mut AppState<'_>, event: Event) -> Action {
    // Help overlay swallows everything; any key closes it.
    if state.help_open {
        if let Event::Key(_) = event {
            state.help_open = false;
        }
        return Action::None;
    }

    // Resize / Tick are intentionally dropped here — ratatui re-reads the
    // terminal dimensions on every draw, so we don't need to mirror them
    // on AppState. Tick is reserved for a future periodic-refresh use case.
    let Event::Key(k) = event else {
        return Action::None;
    };

    // Global Ctrl-C / q quits regardless of focus.
    if matches!(k.code, KeyCode::Char('c')) && k.modifiers.contains(KeyModifiers::CONTROL) {
        return Action::Quit;
    }
    if matches!(k.code, KeyCode::Char('q')) {
        return Action::Quit;
    }

    // Number keys ALWAYS switch view (no Roi exception).
    // (Previously 1-4 re-sorted in Roi per spec §7; user reported that as
    // a discoverability bug. Sort keys are now t/c/s/p, see below.)
    if let KeyCode::Char(c @ ('1' | '2' | '3')) = k.code {
        state.view = match c {
            '1' => View::Flamegraph,
            '2' => View::Roi,
            '3' => View::Aggregate,
            _ => state.view, // unreachable; appeases match
        };
        return Action::None;
    }

    // Letter keys cycle RoiView sort (only meaningful when view == Roi;
    // ignored elsewhere so they don't accidentally fire side-effects).
    if state.view == View::Roi {
        if let KeyCode::Char(c @ ('t' | 'c' | 's' | 'p')) = k.code {
            state.roi_sort = match c {
                't' => SortKey::TotalDur,
                'c' => SortKey::Calls,
                's' => SortKey::SuccessRate,
                'p' => SortKey::P50,
                _ => state.roi_sort, // unreachable
            };
            state.roi_selected = 0;
            return Action::None;
        }
    }

    match k.code {
        KeyCode::Tab => state.view = state.view.next(),
        KeyCode::BackTab => state.view = state.view.prev(),
        KeyCode::Char('?') => state.help_open = true,
        KeyCode::Up => scroll_up(state),
        KeyCode::Down => scroll_down(state),
        // KeyCode::Left | KeyCode::Right reserved for FlamegraphView horizontal scroll
        _ => {}
    }

    Action::None
}

fn scroll_up(state: &mut AppState<'_>) {
    match state.view {
        View::Roi => state.roi_selected = state.roi_selected.saturating_sub(1),
        View::Flamegraph => state.flame_selected = state.flame_selected.saturating_sub(1),
        View::Aggregate => {
            let (v, _) = state.scroll.entry(View::Aggregate).or_insert((0, 0));
            *v = v.saturating_sub(1);
        }
    }
}

fn scroll_down(state: &mut AppState<'_>) {
    match state.view {
        View::Roi => {
            let max = state.report.tool_rank.len().saturating_sub(1);
            if state.roi_selected < max {
                state.roi_selected += 1;
            }
        }
        View::Flamegraph => {
            let max = state.report.turn_summary.len().saturating_sub(1);
            if state.flame_selected < max {
                state.flame_selected += 1;
            }
        }
        View::Aggregate => {
            let (v, _) = state.scroll.entry(View::Aggregate).or_insert((0, 0));
            *v = v.saturating_add(1);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::ToolRankRow;
    use agentprof_core::model::{SessionMeta, ToolSource};
    use chrono::Utc;
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    fn empty_report() -> AnalysisReport {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        AnalysisReport::new(meta)
    }

    fn one_row_report() -> AnalysisReport {
        let mut r = empty_report();
        r.tool_rank.push(ToolRankRow::new(
            "bash".into(),
            ToolSource::Builtin,
            1,
            1,
            0,
            0,
            0,
            chrono::Duration::milliseconds(10),
            chrono::Duration::milliseconds(10),
            chrono::Duration::milliseconds(10),
            chrono::Duration::milliseconds(10),
        ));
        r
    }

    #[test]
    fn view_switch_via_number_keys_1_2_3() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::Char('2')));
        assert_eq!(s.view, View::Roi);
        // From Roi, '1' now switches view (no longer re-sorts).
        dispatch(&mut s, key(KeyCode::Char('1')));
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::Char('3')));
        assert_eq!(s.view, View::Aggregate);
        dispatch(&mut s, key(KeyCode::Char('2')));
        assert_eq!(s.view, View::Roi);
    }

    #[test]
    fn tab_cycles_views_forward_and_backward() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        dispatch(&mut s, key(KeyCode::Tab));
        assert_eq!(s.view, View::Roi);
        dispatch(&mut s, key(KeyCode::Tab));
        assert_eq!(s.view, View::Aggregate);
        dispatch(&mut s, key(KeyCode::Tab));
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::BackTab));
        assert_eq!(s.view, View::Aggregate);
    }

    #[test]
    fn scroll_saturates_at_top_and_bottom() {
        let r = one_row_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        // Up at index 0 saturates.
        dispatch(&mut s, key(KeyCode::Up));
        assert_eq!(s.roi_selected, 0);
        // Down to the only row (index 0) — already at max (len=1, max=0).
        dispatch(&mut s, key(KeyCode::Down));
        assert_eq!(s.roi_selected, 0);
    }

    #[test]
    fn roi_sort_key_cycle_letter_keys() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        assert_eq!(s.roi_sort, SortKey::TotalDur);
        dispatch(&mut s, key(KeyCode::Char('c')));
        assert_eq!(s.roi_sort, SortKey::Calls);
        dispatch(&mut s, key(KeyCode::Char('s')));
        assert_eq!(s.roi_sort, SortKey::SuccessRate);
        dispatch(&mut s, key(KeyCode::Char('p')));
        assert_eq!(s.roi_sort, SortKey::P50);
        dispatch(&mut s, key(KeyCode::Char('t')));
        assert_eq!(s.roi_sort, SortKey::TotalDur);
    }

    #[test]
    fn roi_selected_row_navigation() {
        let mut r = empty_report();
        for n in ["a", "b", "c"] {
            r.tool_rank.push(ToolRankRow::new(
                n.into(),
                ToolSource::Builtin,
                1,
                1,
                0,
                0,
                0,
                chrono::Duration::milliseconds(1),
                chrono::Duration::milliseconds(1),
                chrono::Duration::milliseconds(1),
                chrono::Duration::milliseconds(1),
            ));
        }
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        dispatch(&mut s, key(KeyCode::Down));
        assert_eq!(s.roi_selected, 1);
        dispatch(&mut s, key(KeyCode::Down));
        assert_eq!(s.roi_selected, 2);
        // Saturate at max=2 (len=3).
        dispatch(&mut s, key(KeyCode::Down));
        assert_eq!(s.roi_selected, 2);
        dispatch(&mut s, key(KeyCode::Up));
        assert_eq!(s.roi_selected, 1);
    }

    #[test]
    fn ctrl_c_emits_quit_action() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        assert_eq!(dispatch(&mut s, ctrl('c')), Action::Quit);
    }

    #[test]
    fn help_overlay_swallows_other_keys() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        dispatch(&mut s, key(KeyCode::Char('?')));
        assert!(s.help_open);
        // Press '2' while help open — should close help, NOT switch view.
        dispatch(&mut s, key(KeyCode::Char('2')));
        assert!(!s.help_open);
        assert_eq!(s.view, View::Flamegraph);
        // Even Ctrl-C must be swallowed (not quit) while help is open —
        // this guards the help-check-before-quit-check precedence in dispatch().
        s.help_open = true;
        assert_eq!(dispatch(&mut s, ctrl('c')), Action::None);
        assert!(!s.help_open);
        // And 'q' — same precedence rule.
        s.help_open = true;
        assert_eq!(dispatch(&mut s, key(KeyCode::Char('q'))), Action::None);
        assert!(!s.help_open);
    }

    #[test]
    fn quit_q_emits_quit_action() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        assert_eq!(dispatch(&mut s, key(KeyCode::Char('q'))), Action::Quit);
    }

    #[test]
    fn number_keys_in_roi_view_switch_not_sort() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        // Number keys now ALWAYS switch view, even from Roi.
        dispatch(&mut s, key(KeyCode::Char('1')));
        assert_eq!(s.view, View::Flamegraph);
        assert_eq!(s.roi_sort, SortKey::TotalDur); // sort unchanged
    }

    #[test]
    fn letter_sort_keys_ignored_outside_roi() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        // Currently on Flamegraph (default). Press 't' — should NOT change sort.
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::Char('t')));
        assert_eq!(s.view, View::Flamegraph); // view unchanged
        assert_eq!(s.roi_sort, SortKey::TotalDur); // sort unchanged (still default)
    }

    #[test]
    fn flame_viewport_top_defaults_to_zero() {
        let r = empty_report();
        let e = Episodes::new();
        let s = AppState::new(&r, &e);
        assert_eq!(s.flame_viewport_top.get(), 0);
        assert_eq!(s.roi_viewport_top.get(), 0);
    }

    #[test]
    fn viewport_state_is_cell_based_interior_mutability() {
        // Smoke test: Cell::set works on a shared reference.
        let r = empty_report();
        let e = Episodes::new();
        let s = AppState::new(&r, &e);
        let s_ref = &s; // shared reference, not mutable
        s_ref.flame_viewport_top.set(42);
        s_ref.roi_viewport_top.set(7);
        assert_eq!(s.flame_viewport_top.get(), 42);
        assert_eq!(s.roi_viewport_top.get(), 7);
    }
}
