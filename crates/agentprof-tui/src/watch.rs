//! M1.6.3 — owned-data `WatchRunner` for `agentprof watch` and
//! `agentprof aggregate --export tui`.
//!
//! Coexists with M1.5's borrow-based [`crate::AppRunner`]. Whereas
//! `AppRunner` borrows `&AnalysisReport` + `&Episodes` from the CLI,
//! `WatchRunner` **owns** its data so the reload closure can swap the
//! whole snapshot on each filesystem-event tick without lifetime
//! contortions.
//!
//! ## Architecture
//!
//! - [`WatchData`] enum carries either single-session (analyze TUI) or
//!   cross-session (aggregate TUI) data.
//! - [`WatchRunner`] holds a `WatchData` + persistent [`WatchViewState`]
//!   (sort key, selected row, help overlay), plus an optional
//!   `mpsc::Receiver<RefreshKind>` + reload closure pair.
//! - On each loop iteration: redraw → `try_recv` (non-blocking) → if a
//!   refresh arrived, call `reload()` → swap data → continue. Then
//!   poll crossterm 250 ms for key input.
//! - Errors from `reload()` set `last_error` for the footer banner and
//!   keep the watch loop alive (D-13 in the M1.6.3 spec).

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration as StdDuration;

use agentprof_core::analyzer::aggregate::AnyAggregateReport;
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::Episodes;
use agentprof_core::model::SessionMeta;
use crossterm::event::poll;
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};

use crate::app::event::Event;
use crate::app::state::{dispatch, Action, AppState};
use crate::error::TuiError;
use crate::views;

/// Data snapshot for one render frame.
///
/// `Single` is the single-session shape (mirrors analyze TUI inputs);
/// `Cross` is the cross-session aggregate shape (mirrors `aggregate`
/// subcommand output).
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use agentprof_tui::watch::WatchData;
/// use chrono::Utc;
///
/// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
/// let _ = WatchData::Single {
///     report: AnalysisReport::new(meta.clone()),
///     episodes: Episodes::new(),
///     meta,
/// };
/// ```
#[derive(Debug)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum WatchData {
    /// Single-session TUI surface.
    Single {
        /// Per-session analyzer output.
        report: AnalysisReport,
        /// Per-session episodes (needed by flamegraph view).
        episodes: Episodes,
        /// Session metadata (id, agent, `started_at`).
        meta: SessionMeta,
    },
    /// Cross-session aggregate TUI surface.
    Cross(AnyAggregateReport),
}

/// Kind of refresh event sent from the cli-side file watcher.
///
/// # Examples
///
/// ```
/// use agentprof_tui::watch::RefreshKind;
/// assert_eq!(format!("{:?}", RefreshKind::DataChanged), "DataChanged");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RefreshKind {
    /// One or more watched files changed.
    DataChanged,
}

/// Error returned by a reload closure (M1.6.3).
///
/// Surfaced into the footer banner; does NOT terminate the watch loop.
///
/// # Examples
///
/// ```
/// use agentprof_tui::watch::ReloadError;
/// use std::path::PathBuf;
/// let e = ReloadError::SessionGone { path: PathBuf::from("/tmp/x") };
/// assert!(e.to_string().contains("session disappeared"));
/// ```
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ReloadError {
    /// Session directory or events file was deleted.
    #[error("session disappeared at {path}")]
    SessionGone {
        /// Path that vanished.
        path: PathBuf,
    },
    /// Underlying reload pipeline (parse + derive + analyze) failed.
    /// Carries the formatted error chain (anyhow chain → String).
    #[error("reload failed: {0}")]
    Pipeline(String),
}

/// Sort key for the cross-session aggregate table.
///
/// Day buckets are always rendered in chronological order; the sort key is
/// ignored entirely for `--by day`.
///
/// # Examples
///
/// ```
/// use agentprof_tui::watch::AggSortKey;
/// assert_eq!(AggSortKey::default(), AggSortKey::TotalDuration);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AggSortKey {
    /// Sort by per-bucket `call_count` descending.
    Calls,
    /// Sort by per-bucket `total_duration` descending.
    #[default]
    TotalDuration,
    /// Sort by per-bucket `session_count` descending.
    Sessions,
    /// Sort by per-bucket `p50_duration` descending (Tool only;
    /// MCP-server / Model variants fall back to `TotalDuration`.
    /// Day buckets are always chronological and ignore the sort key entirely).
    Percentile50,
}

/// Persistent view state (survives reloads).
///
/// # Examples
///
/// ```
/// use agentprof_tui::watch::WatchViewState;
/// let s = WatchViewState::default();
/// assert_eq!(s.agg_selected, 0);
/// assert!(!s.help_overlay);
/// assert!(!s.pending_gg);
/// assert!(s.detail_view.is_none());
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct WatchViewState {
    /// Sort key for the cross-session aggregate view.
    pub agg_sort: AggSortKey,
    /// Selected bucket row index in the aggregate table.
    pub agg_selected: usize,
    /// `?` toggles the help overlay.
    pub help_overlay: bool,
    /// Vim-style `gg` two-key sequence in-progress flag. Mirrors
    /// `AppState::pending_gg` for the cross-session aggregate view.
    pub pending_gg: bool,
    /// Mirrors [`crate::app::state::AppState::detail_view`] across
    /// `WatchRunner`'s transient-`AppState` reconstruction (every
    /// frame and every key dispatch). Cleared by
    /// `WatchRunner::do_reload` when the cached `turn_id` is no
    /// longer present in the reloaded [`Episodes`] (see ADR-0011 D-14).
    pub detail_view: Option<crate::views::turn_detail::TurnDetailState>,
    /// Mirrors [`crate::app::state::AppState::models_selected`] across
    /// `WatchRunner`'s transient `AppState` reconstruction. F1.7.
    pub models_selected: usize,
    /// Currently selected view in watch mode. Round-tripped across
    /// the transient `AppState` every key dispatch + render. Defaults
    /// to [`crate::views::View::Aggregate`] for backward compat with
    /// M1.6.3 (watch mode originally shipped aggregate-only). F1.7
    /// enables view switching via keys `1`/`2`/`3`/`4` once `view`
    /// persists across events.
    pub view: crate::views::View,
}

impl Default for WatchViewState {
    fn default() -> Self {
        Self {
            agg_sort: AggSortKey::default(),
            agg_selected: 0,
            help_overlay: false,
            pending_gg: false,
            detail_view: None,
            models_selected: 0,
            view: crate::views::View::Aggregate,
        }
    }
}

/// Owned-data live-refresh TUI runner.
///
/// Constructed in 2 modes:
/// - [`Self::new_static`] — no refresh channel; one-shot render loop.
/// - [`Self::with_watcher`] — refresh channel + reload closure; live mode.
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use agentprof_tui::watch::{WatchData, WatchRunner};
/// use chrono::Utc;
///
/// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
/// let data = WatchData::Single {
///     report: AnalysisReport::new(meta.clone()),
///     episodes: Episodes::new(),
///     meta,
/// };
/// let runner = WatchRunner::new_static(data);
/// assert_eq!(runner.refresh_count(), 0);
/// ```
pub struct WatchRunner {
    data: WatchData,
    view_state: WatchViewState,
    refresh_rx: Option<Receiver<RefreshKind>>,
    reload: Option<Box<dyn FnMut() -> Result<WatchData, ReloadError>>>,
    last_error: Option<String>,
    refresh_count: u32,
}

impl WatchRunner {
    /// Construct a runner in **static mode** (no refresh channel).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::model::SessionMeta;
    /// use agentprof_tui::watch::{WatchData, WatchRunner};
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    /// let data = WatchData::Single {
    ///     report: AnalysisReport::new(meta.clone()),
    ///     episodes: Episodes::new(),
    ///     meta,
    /// };
    /// let runner = WatchRunner::new_static(data);
    /// assert!(runner.last_error().is_none());
    /// ```
    #[must_use]
    pub fn new_static(data: WatchData) -> Self {
        Self {
            data,
            view_state: WatchViewState::default(),
            refresh_rx: None,
            reload: None,
            last_error: None,
            refresh_count: 0,
        }
    }

    /// Construct a runner with a refresh channel + reload closure.
    ///
    /// The CLI is responsible for keeping the underlying file-watcher
    /// alive (typically via a `Debouncer` value bound to the same scope
    /// as the `run` call). Dropping the watcher hangs up the channel,
    /// which the runner treats as "no more refreshes" (graceful).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::mpsc::channel;
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::model::SessionMeta;
    /// use agentprof_tui::watch::{ReloadError, WatchData, WatchRunner};
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    /// let data = WatchData::Single {
    ///     report: AnalysisReport::new(meta.clone()),
    ///     episodes: Episodes::new(),
    ///     meta: meta.clone(),
    /// };
    /// let (_tx, rx) = channel();
    /// let reload: Box<dyn FnMut() -> Result<WatchData, ReloadError>> = Box::new(move || {
    ///     Ok(WatchData::Single {
    ///         report: AnalysisReport::new(meta.clone()),
    ///         episodes: Episodes::new(),
    ///         meta: meta.clone(),
    ///     })
    /// });
    /// let _runner = WatchRunner::with_watcher(data, rx, reload);
    /// ```
    #[must_use]
    pub fn with_watcher(
        data: WatchData,
        refresh_rx: Receiver<RefreshKind>,
        reload: Box<dyn FnMut() -> Result<WatchData, ReloadError>>,
    ) -> Self {
        Self {
            data,
            view_state: WatchViewState::default(),
            refresh_rx: Some(refresh_rx),
            reload: Some(reload),
            last_error: None,
            refresh_count: 0,
        }
    }

    /// Returns successful-reload count (informational footer counter).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::model::SessionMeta;
    /// use agentprof_tui::watch::{WatchData, WatchRunner};
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    /// let runner = WatchRunner::new_static(WatchData::Single {
    ///     report: AnalysisReport::new(meta.clone()),
    ///     episodes: Episodes::new(),
    ///     meta,
    /// });
    /// assert_eq!(runner.refresh_count(), 0);
    /// ```
    #[must_use]
    pub const fn refresh_count(&self) -> u32 {
        self.refresh_count
    }

    /// Returns the last reload error (rendered in footer banner), if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::model::SessionMeta;
    /// use agentprof_tui::watch::{WatchData, WatchRunner};
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    /// let runner = WatchRunner::new_static(WatchData::Single {
    ///     report: AnalysisReport::new(meta.clone()),
    ///     episodes: Episodes::new(),
    ///     meta,
    /// });
    /// assert!(runner.last_error().is_none());
    /// ```
    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Borrow the current data snapshot (test helper).
    #[doc(hidden)]
    #[must_use]
    pub const fn data(&self) -> &WatchData {
        &self.data
    }

    /// Render one frame (snapshot-test helper).
    ///
    /// # Errors
    ///
    /// Returns [`TuiError::Io`] if the underlying `terminal.draw` fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::model::SessionMeta;
    /// use agentprof_tui::watch::{WatchData, WatchRunner};
    /// use chrono::Utc;
    /// use ratatui::backend::TestBackend;
    /// use ratatui::Terminal;
    ///
    /// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    /// let runner = WatchRunner::new_static(WatchData::Single {
    ///     report: AnalysisReport::new(meta.clone()),
    ///     episodes: Episodes::new(),
    ///     meta,
    /// });
    /// let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    /// runner.draw_frame(&mut term).unwrap();
    /// ```
    pub fn draw_frame<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), TuiError> {
        terminal.draw(|frame| self.render_into(frame))?;
        Ok(())
    }

    fn render_into(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let (body_area, footer_area) = if self.last_error.is_some() && area.height > 1 {
            (
                Rect::new(area.x, area.y, area.width, area.height - 1),
                Some(Rect::new(area.x, area.y + area.height - 1, area.width, 1)),
            )
        } else {
            (area, None)
        };

        match &self.data {
            WatchData::Single {
                report, episodes, ..
            } => {
                let mut transient = AppState::new(report, episodes);
                transient.help_open = self.view_state.help_overlay;
                transient.models_selected = self.view_state.models_selected;
                transient.view = self.view_state.view;
                // Read `detail_view` directly from the persistent
                // `view_state` — render is read-only so no clone-in
                // round trip is needed (unlike `dispatch`, which mutates
                // a transient `AppState` and writes back).
                if let Some(detail) = self.view_state.detail_view.as_ref() {
                    crate::views::turn_detail::render_turn_detail(
                        frame, body_area, detail, &transient,
                    );
                } else {
                    // F1.7.1 — full 4-view dispatch in Single mode.
                    // Pre-F1.7.1 only `View::Models` had its own arm
                    // and Flamegraph/Roi/Aggregate fell through to
                    // `aggregate::render` — pressing 1/2/3 updated
                    // `view_state.view` correctly (F1.7 T10) but the
                    // render stayed on Aggregate. Mirrors the
                    // [`AppRunner::render_into`] match exactly so the
                    // two runner paths render identically.
                    match transient.view {
                        crate::views::View::Flamegraph => {
                            crate::views::flamegraph::render(frame, body_area, &transient);
                        }
                        crate::views::View::Roi => {
                            crate::views::roi::render(frame, body_area, &transient);
                        }
                        crate::views::View::Aggregate => {
                            views::aggregate::render(frame, body_area, &transient);
                        }
                        crate::views::View::Models => {
                            crate::views::models::render(frame, body_area, &transient);
                        }
                    }
                }
                // F1.7.1 — render the help overlay if toggled. Pre-F1.7.1
                // the `?` keystroke flipped `view_state.help_overlay`
                // (gated to Single mode by TUI #3) but no render path
                // consumed it — the overlay was state-with-no-display.
                if self.view_state.help_overlay {
                    crate::app::draw_help_overlay(frame, body_area);
                }
            }
            WatchData::Cross(any) => {
                views::aggregate::render_cross_session(
                    frame,
                    body_area,
                    any,
                    self.view_state.agg_sort,
                    self.view_state.agg_selected,
                );
            }
        }

        if let (Some(area), Some(msg)) = (footer_area, self.last_error.as_deref()) {
            let p = Paragraph::new(format!("⚠ reload error: {msg}"))
                .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
            frame.render_widget(p, area);
        }
    }

    /// Main event loop. Returns on `q` / Ctrl-C / `Action::Quit` / Err.
    ///
    /// Reload-closure failures are caught and stored in `last_error` —
    /// the watch loop continues running. Terminal I/O errors propagate.
    ///
    /// # Errors
    ///
    /// Returns [`TuiError::Io`] on `terminal.draw` or `poll` failure.
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), TuiError> {
        loop {
            terminal.draw(|frame| self.render_into(frame))?;

            // Drain pending refreshes (collapse N => 1 reload).
            let mut got_refresh = false;
            if let Some(rx) = &self.refresh_rx {
                while let Ok(_kind) = rx.try_recv() {
                    got_refresh = true;
                }
            }
            if got_refresh {
                self.do_reload();
                continue;
            }

            // Block up to 250 ms for the next input event.
            if poll(StdDuration::from_millis(250))? {
                let raw = crossterm::event::read()?;
                if let Some(ev) = Event::from_crossterm(raw) {
                    if self.handle_watch_key(&ev) {
                        continue;
                    }
                    // Generic q / Ctrl-C / help dispatch via M1.5 state machine
                    // (Single mode only; Cross mode handles its own q above).
                    //
                    // TUI #2 — Single mode "transient AppState" pattern:
                    // every keystroke constructs a fresh AppState from the
                    // persistent `report` + `episodes`, then copies the
                    // **round-tripped fields** back into `view_state`
                    // after dispatch. Round-tripped fields (must match
                    // both directions):
                    //   - help_open (F1)
                    //   - detail_view (F1)
                    //   - models_selected (F1.7)
                    //   - view (F1.7 T10 — was broken pre-F1.7)
                    //
                    // **NOT round-tripped** (intentionally — these state
                    // pieces reset between keystrokes in watch mode):
                    //   - flame_selected, flame_viewport_top
                    //   - roi_selected, roi_viewport_top, roi_sort
                    //   - pending_gg
                    // Reason: watch mode is intended to surface "what's
                    // happening NOW" — persistent per-view selection
                    // would conflict with the auto-reload semantics
                    // (data underneath the cursor changes between
                    // reloads). Documented here for tui-2 review.
                    // When/if a future polish round wants per-view
                    // selection persistence, add the missing fields
                    // here AND in WatchViewState.
                    if let WatchData::Single {
                        report, episodes, ..
                    } = &self.data
                    {
                        let mut transient = AppState::new(report, episodes);
                        transient.help_open = self.view_state.help_overlay;
                        transient
                            .detail_view
                            .clone_from(&self.view_state.detail_view);
                        transient.models_selected = self.view_state.models_selected;
                        transient.view = self.view_state.view;
                        match dispatch(&mut transient, ev) {
                            Action::Quit => return Ok(()),
                            Action::None => {
                                self.view_state.help_overlay = transient.help_open;
                                self.view_state.detail_view = transient.detail_view;
                                self.view_state.models_selected = transient.models_selected;
                                self.view_state.view = transient.view;
                            }
                        }
                    } else if ev.is_ctrl_c()
                        || matches!(
                            &ev,
                            Event::Key(k) if k.code == crossterm::event::KeyCode::Char('q')
                        )
                    {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn do_reload(&mut self) {
        if let Some(cb) = self.reload.as_mut() {
            match cb() {
                Ok(new_data) => {
                    self.data = new_data;
                    self.refresh_count = self.refresh_count.saturating_add(1);
                    self.last_error = None;

                    // Drop detail_view if its cached turn_id no longer
                    // exists in the reloaded Episodes. ADR-0011 D-14.
                    //
                    // - Single → Single: check turn_id presence; if
                    //   gone, drop + red-banner footer.
                    // - Single → Cross (mode change) or Cross →
                    //   anything: Cross mode doesn't support the
                    //   detail view at all, so any stale detail_view
                    //   is silently dropped without a footer message.
                    match &self.data {
                        WatchData::Single { episodes, .. } => {
                            if let Some(dv) = self.view_state.detail_view.as_ref() {
                                let still_present =
                                    episodes.turns.iter().any(|t| t.id == dv.turn_id);
                                if !still_present {
                                    let id = dv.turn_id.clone();
                                    self.view_state.detail_view = None;
                                    self.last_error =
                                        Some(format!("turn {id} disappeared after reload"));
                                }
                            }
                        }
                        WatchData::Cross(_) => {
                            // Silent drop on Cross — no banner. Mode change
                            // (Single→Cross) is its own visual signal: the
                            // entire view morphs from per-turn flamegraph
                            // to cross-session aggregate. A red-banner
                            // "turn disappeared" would be redundant and
                            // misleading (the turn didn't disappear; the
                            // whole single-session context did).
                            self.view_state.detail_view = None;
                        }
                    }
                }
                Err(e) => {
                    self.last_error = Some(e.to_string());
                }
            }
        }
    }

    /// Returns true if the key was consumed by watch-specific state
    /// (sort cycling, selection, help, Cross-mode quit). Caller then
    /// `continue`s without forwarding to general dispatch.
    fn handle_watch_key(&mut self, ev: &Event) -> bool {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = ev
        else {
            return false;
        };
        if matches!(code, KeyCode::Char('?')) {
            // TUI #3 — gate the help-overlay toggle on Single mode.
            // Pre-fix, Cross mode also toggled `view_state.help_overlay`
            // (line 605) but `render_into`'s Cross arm has no help-overlay
            // render path. The "? help" hint advertised by
            // `render_cross_header` was promising functionality that
            // didn't exist — the keystroke went into a black hole.
            // Returning `false` for Cross mode lets the keystroke fall
            // through to subsequent handlers; if no handler claims it,
            // the user gets explicit no-op feedback (vs silent state
            // mutation). When Cross-mode help overlay is implemented,
            // remove this gate.
            if matches!(self.data, WatchData::Cross(_)) {
                return false;
            }
            self.view_state.help_overlay = !self.view_state.help_overlay;
            self.view_state.pending_gg = false;
            return true;
        }
        if !matches!(self.data, WatchData::Cross(_)) {
            return false;
        }
        // Vim G / gg motion (cross-session aggregate only). Handle before
        // the sort/select match so pending_gg state stays consistent.
        let is_capital_g = matches!(code, KeyCode::Char('G'))
            || (matches!(code, KeyCode::Char('g')) && modifiers.contains(KeyModifiers::SHIFT));
        let is_lowercase_g =
            matches!(code, KeyCode::Char('g')) && !modifiers.contains(KeyModifiers::SHIFT);
        if is_capital_g {
            self.view_state.agg_selected = self.cross_bucket_count().saturating_sub(1);
            self.view_state.pending_gg = false;
            return true;
        }
        if is_lowercase_g {
            if self.view_state.pending_gg {
                self.view_state.agg_selected = 0;
                self.view_state.pending_gg = false;
            } else {
                self.view_state.pending_gg = true;
            }
            return true;
        }
        // Any non-g key clears pending_gg but still executes its own behavior.
        self.view_state.pending_gg = false;
        match code {
            KeyCode::Char('c') => {
                self.view_state.agg_sort = AggSortKey::Calls;
                true
            }
            KeyCode::Char('t') => {
                self.view_state.agg_sort = AggSortKey::TotalDuration;
                true
            }
            KeyCode::Char('s') => {
                self.view_state.agg_sort = AggSortKey::Sessions;
                true
            }
            KeyCode::Char('p') => {
                self.view_state.agg_sort = AggSortKey::Percentile50;
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.view_state.agg_selected = self.view_state.agg_selected.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.cross_bucket_count().saturating_sub(1);
                if self.view_state.agg_selected < max {
                    self.view_state.agg_selected += 1;
                }
                true
            }
            _ => false,
        }
    }

    /// Bucket count of the current `WatchData::Cross` payload (0 for
    /// `Single`). Used to clamp `agg_selected` for j/k/G/gg motion.
    fn cross_bucket_count(&self) -> usize {
        use agentprof_core::analyzer::aggregate::AnyAggregateReport as A;
        let WatchData::Cross(any) = &self.data else {
            return 0;
        };
        match any {
            A::Tool(r) => r.buckets.len(),
            A::McpServer(r) => r.buckets.len(),
            A::Day(r) => r.buckets.len(),
            A::Model(r) => r.buckets.len(),
            // AnyAggregateReport is `#[non_exhaustive]`; future variants
            // surface as a zero count (selection clamps to 0).
            _ => 0,
        }
    }
}

// =================== test-only helpers ===================

#[doc(hidden)]
impl WatchRunner {
    /// Inject a reload error (test helper — sets footer banner state
    /// without requiring an actual reload-closure run).
    #[doc(hidden)]
    pub fn set_last_error_for_test(&mut self, msg: impl Into<String>) {
        self.last_error = Some(msg.into());
    }

    /// Toggle the help overlay (test helper).
    #[doc(hidden)]
    pub fn toggle_help_for_test(&mut self) {
        self.view_state.help_overlay = !self.view_state.help_overlay;
    }

    /// Returns help overlay state (test helper).
    #[doc(hidden)]
    #[must_use]
    pub const fn help_overlay_for_test(&self) -> bool {
        self.view_state.help_overlay
    }

    /// Borrow the persistent view state (test helper).
    #[doc(hidden)]
    #[must_use]
    pub const fn view_state(&self) -> &WatchViewState {
        &self.view_state
    }

    /// Mutably borrow the persistent view state (test helper —
    /// lets tests seed `detail_view` etc.).
    #[doc(hidden)]
    pub const fn view_state_mut(&mut self) -> &mut WatchViewState {
        &mut self.view_state
    }

    /// Install or replace the reload callback (test helper —
    /// lets `new_static`-constructed runners receive a reload fn
    /// without needing the full `with_watcher` channel plumbing).
    #[doc(hidden)]
    pub fn set_reload(&mut self, cb: Box<dyn FnMut() -> Result<WatchData, ReloadError>>) {
        self.reload = Some(cb);
    }

    /// Trigger one synchronous reload through the installed callback
    /// (test helper — bypasses the refresh channel).
    #[doc(hidden)]
    pub fn do_reload_for_test(&mut self) {
        self.do_reload();
    }

    /// Single non-blocking iteration of `run` (test helper). Drains the
    /// refresh channel and calls `do_reload` if needed; returns without
    /// polling for keystrokes.
    ///
    /// # Errors
    ///
    /// Returns [`TuiError::Io`] if `terminal.draw` fails.
    #[doc(hidden)]
    pub fn run_one_iteration_for_test<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> Result<(), TuiError> {
        terminal.draw(|frame| self.render_into(frame))?;
        let mut got_refresh = false;
        if let Some(rx) = &self.refresh_rx {
            while let Ok(_kind) = rx.try_recv() {
                got_refresh = true;
            }
        }
        if got_refresh {
            self.do_reload();
        }
        Ok(())
    }

    /// Drive a single [`Event`] through the same Single-mode dispatch
    /// path that [`WatchRunner::run`] uses for keystrokes (test helper).
    ///
    /// Mirrors the clone-in / write-back round trip of `run`: builds a
    /// transient [`AppState`], seeds `help_open` + `detail_view` +
    /// `models_selected` from `self.view_state`, calls [`dispatch`], and
    /// on `Action::None` writes the mutated fields back. Cross-mode
    /// payloads are a no-op (matches `run`).
    ///
    /// Returns `true` if [`dispatch`] returned [`Action::Quit`].
    #[doc(hidden)]
    pub fn dispatch_event_for_test(&mut self, ev: Event) -> bool {
        if let WatchData::Single {
            report, episodes, ..
        } = &self.data
        {
            let mut transient = AppState::new(report, episodes);
            transient.help_open = self.view_state.help_overlay;
            transient
                .detail_view
                .clone_from(&self.view_state.detail_view);
            transient.models_selected = self.view_state.models_selected;
            transient.view = self.view_state.view;
            match dispatch(&mut transient, ev) {
                Action::Quit => return true,
                Action::None => {
                    self.view_state.help_overlay = transient.help_open;
                    self.view_state.detail_view = transient.detail_view;
                    self.view_state.models_selected = transient.models_selected;
                    self.view_state.view = transient.view;
                }
            }
        }
        false
    }

    /// Test helper: invoke `handle_watch_key` and return its bool.
    /// Used by tui-3 regression tests for the `?` gate behavior.
    #[doc(hidden)]
    pub fn handle_watch_key_for_test(&mut self, ev: &Event) -> bool {
        self.handle_watch_key(ev)
    }
}
