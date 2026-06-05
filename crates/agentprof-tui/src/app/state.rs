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
    ///
    /// **Reserved for M1.6.** Currently unused — Flamegraph and Roi use
    /// dedicated `flame_viewport_top` / `roi_viewport_top` Cell fields,
    /// and Aggregate has no scrollable content. Kept on the struct for
    /// forward-compat (additive future views may need general-purpose
    /// scroll state).
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
    /// Vim-style `gg` two-key sequence in-progress flag.
    ///
    /// Set to `true` after a lone `g` press; cleared on either `gg`
    /// completion (jump to top), `G` (jump to bottom), or any other key
    /// press (be lenient: the other key still executes, we just exit the
    /// pending state). See [`dispatch`].
    pub(crate) pending_gg: bool,
    /// Optional full-screen detail view for the currently-selected turn.
    ///
    /// `Some` after the user presses `Enter` on a turn row in
    /// [`crate::views::flamegraph`]; cleared by `Esc` or `1` / `2` / `3`
    /// (which pop the detail view then switch top-level view).
    ///
    /// See [`crate::views::turn_detail::TurnDetailState`] for the inner
    /// per-detail-view state (selected tool index, expand set, pending-gg).
    pub detail_view: Option<crate::views::turn_detail::TurnDetailState>,
    /// Selected row index in the Models view (F1.7). `0` when the
    /// session has 0 or 1 models. Clamped to `model_metrics.len() - 1`
    /// in [`dispatch`].
    pub models_selected: usize,
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
            pending_gg: false,
            detail_view: None,
            models_selected: 0,
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
/// - `1`/`2`/`3`/`4` ALWAYS switch view (Flamegraph / Roi / Aggregate / Models)
/// - `t`/`c`/`s`/`p` cycle `RoiView` sort key (`TotalDur` / `Calls` / `SuccessRate` / `P50`);
///   only meaningful when view == Roi, ignored elsewhere
/// - `Tab` / `Shift-Tab` cycle views; vim `l` / `h` are aliases for the same
///   forward / backward cycle (F1.8 follow-up — parity with vim window
///   navigation)
/// - `↑` / `↓` or vim `k` / `j` scroll/select
/// - `G` jump to last selectable row; `gg` (two-key sequence) jump to first row
/// - `q` / Ctrl-C quit
/// - `?` toggle help overlay (any key closes)
#[allow(clippy::needless_pass_by_value)]
pub fn dispatch(state: &mut AppState<'_>, event: Event) -> Action {
    // Help overlay swallows everything; any key closes it.
    if state.help_open {
        if let Event::Key(_) = event {
            state.help_open = false;
            state.pending_gg = false;
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

    // Detail-view dispatch — when `state.detail_view` is `Some`, certain
    // keys go exclusively to the detail state. `1` / `2` / `3` pop detail
    // and fall through to the existing number-key view-switch dispatch
    // below. `?` pops the detail's own pending-gg and falls through to the
    // help-overlay toggle. `q` / Ctrl-C are intentionally handled BEFORE
    // this branch so they always quit. Unknown keys are swallowed (with
    // pending-gg cleared) to avoid leaking key state back into top-level
    // view handlers. See [`dispatch_detail`].
    if state.detail_view.is_some() {
        match dispatch_detail(state, &k) {
            DetailFlow::Handled => return Action::None,
            DetailFlow::FallThrough => {}
        }
    }

    // Vim-style G / gg motion. Handle BEFORE other key dispatch so the
    // pending-gg state stays consistent.
    //
    // `G` (Shift+g): crossterm sends `KeyCode::Char('G')` on most
    // terminals, but some emit `Char('g')` + `KeyModifiers::SHIFT`.
    // Accept both spellings.
    let is_capital_g = matches!(k.code, KeyCode::Char('G'))
        || (matches!(k.code, KeyCode::Char('g')) && k.modifiers.contains(KeyModifiers::SHIFT));
    let is_lowercase_g =
        matches!(k.code, KeyCode::Char('g')) && !k.modifiers.contains(KeyModifiers::SHIFT);
    if is_capital_g {
        scroll_to_bottom(state);
        state.pending_gg = false;
        return Action::None;
    }
    if is_lowercase_g {
        if state.pending_gg {
            scroll_to_top(state);
            state.pending_gg = false;
        } else {
            state.pending_gg = true;
        }
        return Action::None;
    }
    // Any non-g key clears pending_gg but still executes its own behavior
    // (lenient: avoid stranding the user in a half-typed gg state).
    state.pending_gg = false;

    // Number keys ALWAYS switch view (no Roi exception).
    // (Previously 1-4 re-sorted in Roi per spec §7; user reported that as
    // a discoverability bug. Sort keys are now t/c/s/p, see below.)
    if let KeyCode::Char(c @ ('1' | '2' | '3' | '4')) = k.code {
        state.view = match c {
            '1' => View::Flamegraph,
            '2' => View::Roi,
            '3' => View::Aggregate,
            '4' => View::Models,
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

    // Models view (F1.7): j/k/↑/↓/G navigate model rows; Esc returns to
    // Flamegraph (parity with TurnDetailView). Lowercase `g` is
    // intentionally NOT handled here — it falls through to the global
    // gg/G block above (handled earlier in this function) so the vim
    // two-key sequence stays consistent with other views.
    if state.view == View::Models {
        if let Some(action) = dispatch_models_view_key(state, &k) {
            return action;
        }
    }

    // Open detail view: Enter on Flamegraph with a valid turn selection.
    // (When `detail_view` is already `Some`, the detail-view dispatch
    // branch above intercepts Enter and routes it to `toggle_expand`.)
    if state.view == View::Flamegraph
        && state.detail_view.is_none()
        && matches!(k.code, KeyCode::Enter)
    {
        let idx = state.flame_selected;
        if let Some(turn) = state.episodes.turns.get(idx) {
            state.detail_view = Some(crate::views::turn_detail::TurnDetailState::new(
                turn.id.clone(),
            ));
        }
        return Action::None;
    }

    match k.code {
        // Tab / Shift-Tab cycle views. Vim `l` (right) / `h` (left) are
        // aliases for the same forward / backward cycle — parity with
        // vim window navigation and with the `j` / `k` aliases for
        // ↑ / ↓ scrolling already present below. Roi sort keys
        // (`t` / `c` / `s` / `p`) are matched earlier above and never
        // reach this arm, so `h` / `l` don't conflict with Roi sort.
        KeyCode::Tab | KeyCode::Char('l') => state.view = state.view.next(),
        KeyCode::BackTab | KeyCode::Char('h') => state.view = state.view.prev(),
        KeyCode::Char('?') => state.help_open = true,
        KeyCode::Up | KeyCode::Char('k') => scroll_up(state),
        KeyCode::Down | KeyCode::Char('j') => scroll_down(state),
        // KeyCode::Left | KeyCode::Right reserved for FlamegraphView horizontal scroll
        _ => {}
    }

    Action::None
}

/// Outcome of [`dispatch_detail`].
///
/// Used so the per-detail-view branch in [`dispatch`] can either fully
/// handle a key (`Handled`, the dispatcher returns `Action::None`) or pop
/// the detail view and ask the top-level dispatcher to keep processing
/// (`FallThrough`, used for `1`/`2`/`3` view switches and `?` help toggle).
#[derive(Debug, PartialEq, Eq)]
enum DetailFlow {
    Handled,
    FallThrough,
}

/// Detail-view key dispatch helper.
///
/// Precondition: `state.detail_view.is_some()`. Returns
/// [`DetailFlow::Handled`] when the key was consumed entirely by the
/// detail view; returns [`DetailFlow::FallThrough`] for `1` / `2` / `3`
/// (after popping `detail_view`) and `?` so the caller continues into the
/// top-level number-key / help-overlay handlers.
///
/// Borrow note: `count` is computed via a short immutable borrow of
/// `state.detail_view` + a disjoint read of `state.episodes` (split-borrow
/// safe). Each per-arm mutation uses a fresh `as_mut()` scope so the
/// `Esc` and `1/2/3` arms can clear `state.detail_view` without conflict.
fn dispatch_detail(state: &mut AppState<'_>, k: &crossterm::event::KeyEvent) -> DetailFlow {
    let count = state.detail_view.as_ref().map_or(0, |d| {
        state
            .episodes
            .turns
            .iter()
            .find(|t| t.id == d.turn_id)
            .map_or(0, |t| t.tool_calls.len())
    });
    // Mirror the top-level dual spelling for `G` (Shift+g): some
    // terminals emit `Char('G')` directly, others emit `Char('g')` +
    // `KeyModifiers::SHIFT`.
    let is_capital_g = matches!(k.code, KeyCode::Char('G'))
        || (matches!(k.code, KeyCode::Char('g')) && k.modifiers.contains(KeyModifiers::SHIFT));
    let is_lowercase_g =
        matches!(k.code, KeyCode::Char('g')) && !k.modifiers.contains(KeyModifiers::SHIFT);

    if is_capital_g {
        if let Some(d) = state.detail_view.as_mut() {
            d.jump_last(count);
            d.pending_gg = false;
        }
        return DetailFlow::Handled;
    }
    if is_lowercase_g {
        if let Some(d) = state.detail_view.as_mut() {
            if d.pending_gg {
                d.jump_first();
                d.pending_gg = false;
            } else {
                d.pending_gg = true;
            }
        }
        return DetailFlow::Handled;
    }

    match k.code {
        KeyCode::Esc => {
            state.detail_view = None;
            state.pending_gg = false;
            DetailFlow::Handled
        }
        KeyCode::Enter => {
            if let Some(d) = state.detail_view.as_mut() {
                d.toggle_expand();
                d.pending_gg = false;
            }
            DetailFlow::Handled
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(d) = state.detail_view.as_mut() {
                d.move_up();
                d.pending_gg = false;
            }
            DetailFlow::Handled
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(d) = state.detail_view.as_mut() {
                d.move_down(count);
                d.pending_gg = false;
            }
            DetailFlow::Handled
        }
        KeyCode::Char('1' | '2' | '3' | '4' | 'h' | 'l') => {
            // Pop detail; let the top-level number-key / Tab block switch
            // view. `h` / `l` are vim aliases for `Shift-Tab` / `Tab`
            // (cycle prev / next view) — mirroring the same parity in
            // the top-level dispatch arm.
            state.detail_view = None;
            state.pending_gg = false;
            DetailFlow::FallThrough
        }
        KeyCode::Char('?') => {
            // Let the top-level help-overlay toggle run.
            if let Some(d) = state.detail_view.as_mut() {
                d.pending_gg = false;
            }
            DetailFlow::FallThrough
        }
        _ => {
            if let Some(d) = state.detail_view.as_mut() {
                d.pending_gg = false;
            }
            DetailFlow::Handled
        }
    }
}

/// Dispatch for keys consumed by the Models view (F1.7).
///
/// Returns `Some(action)` when the key was handled (caller returns
/// immediately); returns `None` to fall through to the generic
/// dispatcher (Tab / help / etc.).
fn dispatch_models_view_key(
    state: &mut AppState<'_>,
    k: &crossterm::event::KeyEvent,
) -> Option<Action> {
    let count = state
        .report
        .model_metrics
        .as_ref()
        .map_or(0, std::collections::BTreeMap::len);
    match k.code {
        KeyCode::Esc => {
            state.view = View::Flamegraph;
            Some(Action::None)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.models_selected = state.models_selected.saturating_sub(1);
            Some(Action::None)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if count > 0 && state.models_selected + 1 < count {
                state.models_selected += 1;
            }
            Some(Action::None)
        }
        _ => None,
    }
}

fn scroll_up(state: &mut AppState<'_>) {
    match state.view {
        View::Roi => state.roi_selected = state.roi_selected.saturating_sub(1),
        View::Flamegraph => state.flame_selected = state.flame_selected.saturating_sub(1),
        View::Aggregate | View::Models => {
            // M1.5 Aggregate is a fixed 50/50 By-Mode + By-Hook split with
            // no scrollable element; ↑/↓ are intentionally no-ops here.
            // M1.6 may add a focused-pane concept allowing scroll within
            // the (potentially long) hook table.
            // F1.7: Models view dispatches Up/Down/k/j via
            // dispatch_models_view_key (consumed before reaching
            // scroll_up); this arm is reached only if a future code
            // path forwards arrow keys here. Aggregate intentionally
            // stays no-op.
        }
    }
}

fn scroll_down(state: &mut AppState<'_>) {
    match state.view {
        View::Roi => {
            // F1.11: all tools (work + user-blocking) are selectable in
            // the unified table; the user-blocking section is rendered
            // with DIM styling + a separator row, but every data row can
            // be reached via j/k. Pre-F1.11 the user-blocking sub-table
            // was a separate non-selectable widget — bug-ish UX since
            // users could see ask_user but not select it.
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
        View::Aggregate | View::Models => {
            // M1.5 Aggregate is a fixed 50/50 By-Mode + By-Hook split with
            // no scrollable element; ↑/↓ are intentionally no-ops here.
            // M1.6 may add a focused-pane concept allowing scroll within
            // the (potentially long) hook table.
            // F1.7: Models view dispatches Up/Down/k/j via
            // dispatch_models_view_key (consumed before reaching
            // scroll_down); this arm is reached only if a future code
            // path forwards arrow keys here. Aggregate intentionally
            // stays no-op.
        }
    }
}

fn scroll_to_top(state: &mut AppState<'_>) {
    match state.view {
        View::Roi => state.roi_selected = 0,
        View::Flamegraph => state.flame_selected = 0,
        View::Aggregate => {
            // Mirrors scroll_up/scroll_down: Aggregate has no scrollable
            // element in M1.5, so jump-to-top is a no-op.
        }
        View::Models => state.models_selected = 0,
    }
}

fn scroll_to_bottom(state: &mut AppState<'_>) {
    match state.view {
        View::Roi => {
            // F1.11: select last tool overall (work or user-blocking).
            // See scroll_down comment for the user-blocking selectability
            // rationale.
            state.roi_selected = state.report.tool_rank.len().saturating_sub(1);
        }
        View::Flamegraph => {
            state.flame_selected = state.report.turn_summary.len().saturating_sub(1);
        }
        View::Aggregate => {
            // No scrollable element in M1.5; no-op.
        }
        View::Models => {
            let count = state
                .report
                .model_metrics
                .as_ref()
                .map_or(0, std::collections::BTreeMap::len);
            state.models_selected = count.saturating_sub(1);
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
    fn view_switch_via_number_keys_1_2_3_4() {
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
        dispatch(&mut s, key(KeyCode::Char('4')));
        assert_eq!(s.view, View::Models);
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
        assert_eq!(s.view, View::Models);
        dispatch(&mut s, key(KeyCode::Tab));
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::BackTab));
        assert_eq!(s.view, View::Models);
    }

    // ──────────────────────────────────────────────────────────────────
    // Vim `h` / `l` view cycle aliases (F1.8 follow-up)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn l_cycles_view_forward_like_tab() {
        // `l` (vim right) must be an exact alias for `Tab`: cycle to the
        // next view in the same order (Flamegraph → Roi → Aggregate →
        // Models → Flamegraph).
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::Char('l')));
        assert_eq!(s.view, View::Roi);
        dispatch(&mut s, key(KeyCode::Char('l')));
        assert_eq!(s.view, View::Aggregate);
        dispatch(&mut s, key(KeyCode::Char('l')));
        assert_eq!(s.view, View::Models);
        dispatch(&mut s, key(KeyCode::Char('l')));
        assert_eq!(s.view, View::Flamegraph);
    }

    #[test]
    fn h_cycles_view_backward_like_shift_tab() {
        // `h` (vim left) must be an exact alias for `Shift-Tab`: cycle
        // to the previous view (Flamegraph → Models → Aggregate → Roi →
        // Flamegraph).
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::Char('h')));
        assert_eq!(s.view, View::Models);
        dispatch(&mut s, key(KeyCode::Char('h')));
        assert_eq!(s.view, View::Aggregate);
        dispatch(&mut s, key(KeyCode::Char('h')));
        assert_eq!(s.view, View::Roi);
        dispatch(&mut s, key(KeyCode::Char('h')));
        assert_eq!(s.view, View::Flamegraph);
    }

    #[test]
    fn h_l_work_from_every_view() {
        // Regression guard: ensure `h` / `l` are not accidentally
        // captured by per-view handlers (e.g. Roi's `t/c/s/p` sort keys
        // or the Models view dispatcher) before reaching the global
        // Tab-equivalent arm.
        let r = empty_report();
        let e = Episodes::new();
        for start in [View::Flamegraph, View::Roi, View::Aggregate, View::Models] {
            let mut s = AppState::new(&r, &e);
            s.view = start;
            dispatch(&mut s, key(KeyCode::Char('l')));
            assert_eq!(
                s.view,
                start.next(),
                "`l` from {start:?} should advance to next view"
            );
            let mut s = AppState::new(&r, &e);
            s.view = start;
            dispatch(&mut s, key(KeyCode::Char('h')));
            assert_eq!(
                s.view,
                start.prev(),
                "`h` from {start:?} should rewind to previous view"
            );
        }
    }

    #[test]
    fn l_from_detail_view_pops_and_cycles() {
        // While the TurnDetailView is open, `l` should pop the detail
        // (returning the user to the underlying view) AND cycle forward
        // — mirroring the existing `1` / `2` / `3` / `4` pop-and-switch
        // behavior so vim users don't get stuck in detail mode.
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new(
            "t1".to_string(),
        ));
        dispatch(&mut s, key(KeyCode::Char('l')));
        assert!(
            s.detail_view.is_none(),
            "`l` in detail view must pop the detail overlay"
        );
        assert_eq!(s.view, View::Roi, "view must cycle forward after popping");
    }

    #[test]
    fn h_from_detail_view_pops_and_cycles() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new(
            "t1".to_string(),
        ));
        dispatch(&mut s, key(KeyCode::Char('h')));
        assert!(
            s.detail_view.is_none(),
            "`h` in detail view must pop the detail overlay"
        );
        assert_eq!(
            s.view,
            View::Models,
            "view must cycle backward after popping"
        );
    }

    #[test]
    fn h_l_do_not_conflict_with_roi_sort_keys() {
        // Roi's sort keys are `t` / `c` / `s` / `p` — `h` and `l` are
        // explicitly NOT in that set. Verify pressing `h` / `l` in Roi
        // cycles view rather than mutating roi_sort.
        let r = one_row_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        let initial_sort = s.roi_sort;
        dispatch(&mut s, key(KeyCode::Char('l')));
        assert_eq!(s.view, View::Aggregate, "`l` in Roi must cycle view");
        assert_eq!(s.roi_sort, initial_sort, "`l` must not mutate Roi sort key");
        s.view = View::Roi;
        dispatch(&mut s, key(KeyCode::Char('h')));
        assert_eq!(s.view, View::Flamegraph, "`h` in Roi must cycle view");
        assert_eq!(s.roi_sort, initial_sort, "`h` must not mutate Roi sort key");
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
    fn roi_navigation_reaches_user_blocking_tools() {
        // F1.11 regression: pre-F1.11 the user-blocking tools were
        // un-selectable (rendered in a separate sub-table). After the
        // merge, j/k must traverse work tools and continue into the
        // user-blocking section.
        let mut r = empty_report();
        // 2 work tools.
        for n in ["bash", "read_file"] {
            r.tool_rank.push(ToolRankRow::new(
                n.into(),
                ToolSource::Builtin,
                1,
                1,
                0,
                0,
                0,
                chrono::Duration::milliseconds(10),
                chrono::Duration::milliseconds(5),
                chrono::Duration::milliseconds(5),
                chrono::Duration::milliseconds(5),
            ));
        }
        // 1 user-blocking tool — must be selectable.
        let mut ask_user_row = ToolRankRow::new(
            "ask_user".into(),
            ToolSource::Builtin,
            3,
            3,
            0,
            0,
            0,
            chrono::Duration::seconds(60),
            chrono::Duration::seconds(20),
            chrono::Duration::seconds(20),
            chrono::Duration::seconds(20),
        );
        ask_user_row.is_user_blocking = true;
        r.tool_rank.push(ask_user_row);

        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        // 0 → 1 → 2 (= ask_user). Saturate at 2.
        dispatch(&mut s, key(KeyCode::Down));
        assert_eq!(s.roi_selected, 1);
        dispatch(&mut s, key(KeyCode::Down));
        assert_eq!(
            s.roi_selected, 2,
            "j/Down must reach user-blocking tool (was unreachable pre-F1.11)"
        );
        dispatch(&mut s, key(KeyCode::Down));
        assert_eq!(s.roi_selected, 2, "must saturate at last user-blocking row");
        // G jumps straight to the last row (which is the user-blocking one).
        s.roi_selected = 0;
        dispatch(&mut s, key(KeyCode::Char('G')));
        assert_eq!(
            s.roi_selected, 2,
            "G must jump to last selectable row including user-blocking section"
        );
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

    // [Removed F1.11] `roi_selected_does_not_overshoot_work_partition`
    // — pre-F1.11 enforced the buggy behavior of clamping selection to
    // the work-only partition. F1.11 merged the user-blocking tools
    // into a single selectable table; see
    // `roi_navigation_reaches_user_blocking_tools` above for the
    // replacement positive test.

    #[test]
    fn aggregate_scroll_keys_are_noop() {
        let r = empty_report();
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        s.view = View::Aggregate;
        // Down × 5: should NOT mutate any scroll state visible elsewhere.
        // flame_viewport_top / roi_viewport_top must stay at default 0.
        for _ in 0..5 {
            dispatch(&mut s, key(KeyCode::Down));
        }
        assert_eq!(s.flame_viewport_top.get(), 0);
        assert_eq!(s.roi_viewport_top.get(), 0);
        assert_eq!(s.flame_selected, 0);
        assert_eq!(s.roi_selected, 0);
        // Up × 5: same.
        for _ in 0..5 {
            dispatch(&mut s, key(KeyCode::Up));
        }
        assert_eq!(s.flame_viewport_top.get(), 0);
        assert_eq!(s.roi_viewport_top.get(), 0);
    }

    // ============ Vim keybindings (j/k/G/gg) ============

    fn shift(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT))
    }

    fn three_row_roi_state() -> (AnalysisReport, Episodes) {
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
        (r, Episodes::new())
    }

    #[test]
    fn vim_j_moves_selection_down_like_arrow_down() {
        let (r, e) = three_row_roi_state();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        dispatch(&mut s, key(KeyCode::Char('j')));
        assert_eq!(s.roi_selected, 1);
        dispatch(&mut s, key(KeyCode::Char('j')));
        assert_eq!(s.roi_selected, 2);
    }

    #[test]
    fn vim_k_moves_selection_up_like_arrow_up() {
        let (r, e) = three_row_roi_state();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        s.roi_selected = 2;
        dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.roi_selected, 1);
        dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.roi_selected, 0);
        // Saturates at top.
        dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.roi_selected, 0);
    }

    #[test]
    fn capital_g_jumps_to_last_row() {
        let (r, e) = three_row_roi_state();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        dispatch(&mut s, key(KeyCode::Char('G')));
        assert_eq!(s.roi_selected, 2, "G should jump to last work-row index");
        // Also accept Shift+g spelling for terminals that don't fold it.
        s.roi_selected = 0;
        dispatch(&mut s, shift('g'));
        assert_eq!(s.roi_selected, 2);
    }

    #[test]
    fn double_g_jumps_to_first_row() {
        let (r, e) = three_row_roi_state();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        s.roi_selected = 2;
        dispatch(&mut s, key(KeyCode::Char('g')));
        assert!(s.pending_gg, "first g arms the gg sequence");
        assert_eq!(s.roi_selected, 2, "first g must not move the selection");
        dispatch(&mut s, key(KeyCode::Char('g')));
        assert!(!s.pending_gg, "second g clears the pending flag");
        assert_eq!(s.roi_selected, 0, "second g jumps to first row");
    }

    #[test]
    fn single_g_then_other_key_clears_pending_and_executes() {
        let (r, e) = three_row_roi_state();
        let mut s = AppState::new(&r, &e);
        s.view = View::Roi;
        dispatch(&mut s, key(KeyCode::Char('g')));
        assert!(s.pending_gg);
        // Non-g key: clears pending AND executes its own behavior.
        dispatch(&mut s, key(KeyCode::Char('j')));
        assert!(!s.pending_gg, "non-g key must clear pending_gg");
        assert_eq!(s.roi_selected, 1, "the j keystroke should still scroll");
        // Capital G also clears pending_gg.
        dispatch(&mut s, key(KeyCode::Char('g')));
        assert!(s.pending_gg);
        dispatch(&mut s, key(KeyCode::Char('G')));
        assert!(!s.pending_gg);
        assert_eq!(s.roi_selected, 2);
    }

    #[test]
    fn capital_g_on_flamegraph_jumps_to_last_turn() {
        use agentprof_core::analyzer::TurnSummaryRow;
        use agentprof_core::episode::TurnStatus;
        let mut r = empty_report();
        for i in 0..4 {
            r.turn_summary.push(TurnSummaryRow::new(
                format!("t{i}"),
                chrono::Utc::now(),
                None,
                TurnStatus::Open,
                None,
                None,
                None,
                0,
                0,
                0,
            ));
        }
        let e = Episodes::new();
        let mut s = AppState::new(&r, &e);
        assert_eq!(s.view, View::Flamegraph);
        dispatch(&mut s, key(KeyCode::Char('G')));
        assert_eq!(s.flame_selected, 3);
        dispatch(&mut s, key(KeyCode::Char('g')));
        dispatch(&mut s, key(KeyCode::Char('g')));
        assert_eq!(s.flame_selected, 0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod detail_view_dispatch_tests {
    use super::*;
    use crate::app::Event;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::AnalysisReport;
    use agentprof_core::episode::{
        CallRef, Episodes, Span as EpSpan, ToolCall, ToolCallStatus, ToolEpisode, Turn,
    };
    use agentprof_core::model::{SessionMeta, ToolSource};
    use chrono::{TimeZone, Utc};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn build_state_with_turn() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta);

        let mut episodes = Episodes::new();
        let span = EpSpan::new(
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 1).unwrap(),
        );
        let mut tc = ToolCall::new(span);
        tc.status = ToolCallStatus::Success;
        let mut tool_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        tool_ep.calls.push(tc);
        episodes.tools.insert("bash".into(), tool_ep);

        let mut turn = Turn::new(
            "T1".into(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
        );
        turn.ended_at = Some(Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 2).unwrap());
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        episodes.turns.push(turn);

        (report, episodes)
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::empty()))
    }

    #[test]
    fn detail_view_starts_none() {
        let (r, e) = build_state_with_turn();
        let s = AppState::new(&r, &e);
        assert!(s.detail_view.is_none());
    }

    #[test]
    fn enter_on_flamegraph_with_valid_selection_opens_detail() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        s.flame_selected = 0;
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(s.detail_view.is_some());
        assert_eq!(s.detail_view.as_ref().unwrap().turn_id, "T1");
    }

    #[test]
    fn esc_in_detail_closes_detail_preserves_flame_selected() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.flame_selected = 0;
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Esc));
        assert!(s.detail_view.is_none());
        assert_eq!(s.flame_selected, 0);
    }

    #[test]
    fn enter_in_detail_toggles_expansion() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(s.detail_view.as_ref().unwrap().expanded_tools.contains(&0));
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(!s.detail_view.as_ref().unwrap().expanded_tools.contains(&0));
    }

    #[test]
    fn jk_in_detail_navigate_tool_calls() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        let mut detail = crate::views::turn_detail::TurnDetailState::new("T1");
        detail.selected_tool_idx = 0;
        s.detail_view = Some(detail);
        // Only 1 call in this fixture, so move_down is no-op.
        let _ = dispatch(&mut s, key(KeyCode::Char('j')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 0);
        let _ = dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 0);
    }

    #[test]
    fn number_keys_in_detail_pop_then_switch_view() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Char('2')));
        assert!(s.detail_view.is_none(), "1/2/3/4 pops detail");
        assert_eq!(s.view, View::Roi, "and switches view");
    }

    #[test]
    fn number_key_4_pops_detail_then_switches_to_models() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Char('4')));
        assert!(s.detail_view.is_none(), "4 pops detail");
        assert_eq!(s.view, View::Models, "and switches to Models");
    }

    #[test]
    fn enter_on_flamegraph_invalid_selection_no_panic() {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta);
        let episodes = Episodes::new(); // no turns
        let mut s = AppState::new(&report, &episodes);
        s.view = View::Flamegraph;
        s.flame_selected = 0;
        let _ = dispatch(&mut s, key(KeyCode::Enter));
        assert!(s.detail_view.is_none(), "no-op when no turn at index");
    }

    #[test]
    fn q_quits_even_in_detail_view() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let act = dispatch(&mut s, key(KeyCode::Char('q')));
        assert!(matches!(act, Action::Quit));
    }

    fn build_state_with_three_tools() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta);
        let mut episodes = Episodes::new();
        let make_span = |s, e| {
            EpSpan::new(
                Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, s).unwrap(),
                Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, e).unwrap(),
            )
        };
        // Three different tool names so they live in separate ToolEpisode entries.
        let mut ep_bash = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        let mut tc1 = ToolCall::new(make_span(0, 1));
        tc1.status = ToolCallStatus::Success;
        ep_bash.calls.push(tc1);
        episodes.tools.insert("bash".into(), ep_bash);

        let mut ep_edit = ToolEpisode::new("edit".into(), ToolSource::Builtin);
        let mut tc2 = ToolCall::new(make_span(1, 2));
        tc2.status = ToolCallStatus::Success;
        ep_edit.calls.push(tc2);
        episodes.tools.insert("edit".into(), ep_edit);

        let mut ep_view = ToolEpisode::new("view".into(), ToolSource::Builtin);
        let mut tc3 = ToolCall::new(make_span(2, 3));
        tc3.status = ToolCallStatus::Success;
        ep_view.calls.push(tc3);
        episodes.tools.insert("view".into(), ep_view);

        let mut turn = Turn::new(
            "T1".into(),
            Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap(),
        );
        turn.ended_at = Some(Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 3).unwrap());
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        turn.tool_calls.push(CallRef::new("edit".into(), 0));
        turn.tool_calls.push(CallRef::new("view".into(), 0));
        episodes.turns.push(turn);

        (report, episodes)
    }

    #[test]
    fn question_in_detail_opens_help_keeps_detail() {
        let (r, e) = build_state_with_turn();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Char('?')));
        assert!(s.help_open, "? must open help overlay even from detail");
        assert!(s.detail_view.is_some(), "detail must survive help toggle");
    }

    #[test]
    fn capital_g_in_detail_jumps_to_last() {
        let (r, e) = build_state_with_three_tools();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        // Capital G via Char('G') (one common terminal emulator spelling).
        let _ = dispatch(&mut s, key(KeyCode::Char('G')));
        assert_eq!(
            s.detail_view.as_ref().unwrap().selected_tool_idx,
            2,
            "G must jump to last tool call (index count-1 in 3-tool fixture)"
        );
    }

    #[test]
    fn gg_two_press_in_detail_jumps_to_first() {
        let (r, e) = build_state_with_three_tools();
        let mut s = AppState::new(&r, &e);
        let mut detail = crate::views::turn_detail::TurnDetailState::new("T1");
        detail.selected_tool_idx = 2;
        s.detail_view = Some(detail);
        // First g: sets pending_gg.
        let _ = dispatch(&mut s, key(KeyCode::Char('g')));
        assert!(
            s.detail_view.as_ref().unwrap().pending_gg,
            "first g sets detail.pending_gg"
        );
        // Second g: jumps to first.
        let _ = dispatch(&mut s, key(KeyCode::Char('g')));
        assert_eq!(
            s.detail_view.as_ref().unwrap().selected_tool_idx,
            0,
            "gg jumps to first tool call"
        );
        assert!(
            !s.detail_view.as_ref().unwrap().pending_gg,
            "second g clears detail.pending_gg"
        );
    }

    #[test]
    fn g_then_non_g_in_detail_clears_pending_gg() {
        let (r, e) = build_state_with_three_tools();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        // First g sets pending_gg.
        let _ = dispatch(&mut s, key(KeyCode::Char('g')));
        assert!(s.detail_view.as_ref().unwrap().pending_gg);
        // Any non-g key should clear pending_gg (lenient invariant).
        let _ = dispatch(&mut s, key(KeyCode::Char('j')));
        assert!(
            !s.detail_view.as_ref().unwrap().pending_gg,
            "non-g key clears detail.pending_gg even if otherwise consumed"
        );
    }

    #[test]
    fn g_in_detail_does_not_set_top_level_pending_gg() {
        // Critical invariant: detail's vim G/gg is isolated from
        // AppState::pending_gg. Otherwise pressing g in detail then Esc
        // back to flamegraph would leave a stray pending_gg.
        let (r, e) = build_state_with_three_tools();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        let _ = dispatch(&mut s, key(KeyCode::Char('g')));
        assert!(
            !s.pending_gg,
            "top-level pending_gg must remain false when g is pressed in detail"
        );
        assert!(
            s.detail_view.as_ref().unwrap().pending_gg,
            "but detail's own pending_gg is set"
        );
    }

    #[test]
    fn jk_in_detail_navigate_three_tools_with_clamping() {
        let (r, e) = build_state_with_three_tools();
        let mut s = AppState::new(&r, &e);
        s.detail_view = Some(crate::views::turn_detail::TurnDetailState::new("T1"));
        // Start at 0 → j moves to 1.
        let _ = dispatch(&mut s, key(KeyCode::Char('j')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 1);
        // j again → 2 (last).
        let _ = dispatch(&mut s, key(KeyCode::Char('j')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 2);
        // j again → still 2 (clamped, count=3).
        let _ = dispatch(&mut s, key(KeyCode::Char('j')));
        assert_eq!(
            s.detail_view.as_ref().unwrap().selected_tool_idx,
            2,
            "j at last must clamp, not wrap or overflow"
        );
        // k back to 1.
        let _ = dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 1);
        // Two more k → clamp at 0.
        let _ = dispatch(&mut s, key(KeyCode::Char('k')));
        let _ = dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 0);
        // k at 0 → still 0.
        let _ = dispatch(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.detail_view.as_ref().unwrap().selected_tool_idx, 0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod models_view_dispatch_tests {
    use super::*;
    use crate::app::Event;
    use crate::views::View;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::{AnalysisReport, ModelUsage};
    use agentprof_core::episode::Episodes;
    use agentprof_core::model::SessionMeta;
    use chrono::Utc;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::BTreeMap;

    fn fixture() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let mut report = AnalysisReport::new(meta);
        let mut m = BTreeMap::new();
        for i in 0..3u64 {
            let mut u = ModelUsage::new();
            u.input_tokens = 100 - i * 10;
            m.insert(format!("model-{i}"), u);
        }
        report.model_metrics = Some(m);
        (report, Episodes::default())
    }

    fn k(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::empty()))
    }

    #[test]
    fn models_selected_starts_at_zero() {
        let (r, e) = fixture();
        let s = AppState::new(&r, &e);
        assert_eq!(s.models_selected, 0);
    }

    #[test]
    fn key_4_switches_to_models_view() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Flamegraph;
        let _ = dispatch(&mut s, k(KeyCode::Char('4')));
        assert_eq!(s.view, View::Models);
    }

    #[test]
    fn key_1_from_models_returns_to_flamegraph() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Models;
        let _ = dispatch(&mut s, k(KeyCode::Char('1')));
        assert_eq!(s.view, View::Flamegraph);
    }

    #[test]
    fn esc_in_models_returns_to_flamegraph() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Models;
        let _ = dispatch(&mut s, k(KeyCode::Esc));
        assert_eq!(s.view, View::Flamegraph);
    }

    #[test]
    fn j_in_models_advances_selection() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Models;
        let _ = dispatch(&mut s, k(KeyCode::Char('j')));
        assert_eq!(s.models_selected, 1);
    }

    #[test]
    fn k_in_models_saturates_at_zero() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Models;
        let _ = dispatch(&mut s, k(KeyCode::Char('k')));
        assert_eq!(s.models_selected, 0);
    }

    #[test]
    fn j_in_models_clamps_at_last() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Models;
        s.models_selected = 2; // last (fixture has 3 models)
        let _ = dispatch(&mut s, k(KeyCode::Char('j')));
        assert_eq!(s.models_selected, 2, "j must clamp at last");
    }

    #[test]
    fn capital_g_in_models_jumps_to_last() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Models;
        let _ = dispatch(&mut s, k(KeyCode::Char('G')));
        assert_eq!(s.models_selected, 2); // count=3 → last idx 2
    }

    #[test]
    fn tab_cycles_through_models() {
        let (r, e) = fixture();
        let mut s = AppState::new(&r, &e);
        s.view = View::Aggregate;
        let _ = dispatch(&mut s, k(KeyCode::Tab));
        assert_eq!(s.view, View::Models, "Aggregate → Models via Tab");
        let _ = dispatch(&mut s, k(KeyCode::Tab));
        assert_eq!(s.view, View::Flamegraph, "Models → Flamegraph via Tab");
    }
}
