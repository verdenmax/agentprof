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
    /// other variants fall back to `TotalDuration`).
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
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct WatchViewState {
    /// Sort key for the cross-session aggregate view.
    pub agg_sort: AggSortKey,
    /// Selected bucket row index in the aggregate table.
    pub agg_selected: usize,
    /// `?` toggles the help overlay.
    pub help_overlay: bool,
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
                views::aggregate::render(frame, body_area, &transient);
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
                    if let WatchData::Single {
                        report, episodes, ..
                    } = &self.data
                    {
                        let mut transient = AppState::new(report, episodes);
                        transient.help_open = self.view_state.help_overlay;
                        match dispatch(&mut transient, ev) {
                            Action::Quit => return Ok(()),
                            Action::None => {
                                self.view_state.help_overlay = transient.help_open;
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
        use crossterm::event::{KeyCode, KeyEvent};
        let Event::Key(KeyEvent { code, .. }) = ev else {
            return false;
        };
        if matches!(code, KeyCode::Char('?')) {
            self.view_state.help_overlay = !self.view_state.help_overlay;
            return true;
        }
        if !matches!(self.data, WatchData::Cross(_)) {
            return false;
        }
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
            KeyCode::Up => {
                self.view_state.agg_selected = self.view_state.agg_selected.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                self.view_state.agg_selected = self.view_state.agg_selected.saturating_add(1);
                true
            }
            _ => false,
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
}
