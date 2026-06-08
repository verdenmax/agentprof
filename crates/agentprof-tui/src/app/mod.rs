//! Event loop, view switching, terminal lifecycle.
//!
//! [`AppRunner`] is the public entry point. The CLI installs the panic hook
//! via [`terminal::install_panic_hook`], enters the alternate screen via
//! [`terminal::enter`], hands the resulting [`terminal::TuiTerminal`] to
//! [`AppRunner::run`], then calls [`terminal::leave`] on the result (Ok or
//! Err).
//!
//! [`AppRunner`] does NOT enter/leave the terminal itself — that ownership
//! stays with the CLI so the panic hook + `?` early-return both restore
//! correctly.

use std::time::Duration;

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::Episodes;
use agentprof_core::model::WasteReport;
use crossterm::event::poll;
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal};

use crate::app::event::Event;
use crate::app::state::{dispatch, Action, AppState};
use crate::error::TuiError;
use crate::views::{aggregate, flamegraph, mcp_waste, roi, View};

pub mod event;
pub mod state;
pub mod terminal;

/// Public TUI entry point.
///
/// Owns nothing — borrows the analyzer output + episodes from the CLI. The
/// [`run`](Self::run) method takes the terminal by mutable reference so the
/// caller retains ownership for cleanup.
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use agentprof_tui::AppRunner;
/// use chrono::Utc;
/// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
/// let report = AnalysisReport::new(meta);
/// let episodes = Episodes::new();
/// let runner = AppRunner::new(&report, &episodes);
/// // `run(&mut term)` is the live event loop; `draw_frame(&mut term)` is the
/// // one-shot render used by snapshot tests.
/// let _ = runner;
/// ```
pub struct AppRunner<'a> {
    state: AppState<'a>,
}

impl<'a> AppRunner<'a> {
    /// Construct an `AppRunner` borrowing the analyzer output.
    #[must_use]
    pub fn new(report: &'a AnalysisReport, episodes: &'a Episodes) -> Self {
        Self {
            state: AppState::new(report, episodes),
        }
    }

    /// Construct an `AppRunner` with an additional borrowed [`WasteReport`]
    /// so the [`View::McpWaste`] split-pane has data to render.
    ///
    /// Additive non-breaking sibling of [`Self::new`] — callers that don't
    /// have / don't need waste data (e.g. `cmd::watch`) keep using
    /// [`Self::new`] and the `McpWaste` view shows a "data not provided"
    /// banner.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::model::{SessionMeta, WasteReport};
    /// use agentprof_tui::AppRunner;
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    /// let report = AnalysisReport::new(meta);
    /// let episodes = Episodes::new();
    /// let waste = WasteReport::default();
    /// let runner = AppRunner::new_with_waste(&report, &episodes, &waste);
    /// let _ = runner;
    /// ```
    #[must_use]
    pub fn new_with_waste(
        report: &'a AnalysisReport,
        episodes: &'a Episodes,
        waste: &'a WasteReport,
    ) -> Self {
        let mut runner = Self::new(report, episodes);
        runner.state.waste_report = Some(waste);
        runner
    }

    /// Render one frame of the current view into the provided backend.
    ///
    /// Exposed for snapshot testing — `tests/views.rs` calls this directly
    /// with a `TestBackend` instead of running the live event loop.
    ///
    /// Takes `&mut self` because some views (e.g. M1.6.5 [`View::McpWaste`])
    /// hold ratatui `TableState` cursors that `render_stateful_widget`
    /// mutates each frame.
    ///
    /// # Errors
    ///
    /// Returns [`TuiError::Io`] if `terminal.draw` fails.
    pub fn draw_frame<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), TuiError> {
        terminal.draw(|frame| Self::render_into(&mut self.state, frame))?;
        Ok(())
    }

    /// Force the active view (test helper).
    ///
    /// Snapshot tests use this to render a specific view without going
    /// through the dispatch state machine. Production callers should not
    /// reach for it — use the key bindings in [`crate::app::state::dispatch`]
    /// instead.
    #[doc(hidden)]
    pub fn set_view(&mut self, view: View) {
        self.state.view = view;
    }

    /// Borrow the state (for inspection in tests).
    #[doc(hidden)]
    #[must_use]
    pub const fn state(&self) -> &AppState<'a> {
        &self.state
    }

    /// Main event loop. Returns on `q` / Ctrl-C / Err.
    ///
    /// # Errors
    ///
    /// Returns [`TuiError::Io`] on any terminal/draw/poll failure.
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), TuiError> {
        loop {
            terminal.draw(|frame| Self::render_into(&mut self.state, frame))?;
            // 250 ms tick keeps the UI responsive without burning CPU.
            if poll(Duration::from_millis(250))? {
                let raw = crossterm::event::read()?;
                if let Some(ev) = Event::from_crossterm(raw) {
                    match dispatch(&mut self.state, ev) {
                        Action::Quit => return Ok(()),
                        Action::None => {}
                    }
                }
            }
        }
    }

    fn render_into(state: &mut AppState<'a>, frame: &mut Frame<'_>) {
        let area = frame.area();
        if let Some(detail) = state.detail_view.as_ref() {
            crate::views::turn_detail::render_turn_detail(frame, area, detail, state);
        } else {
            match state.view {
                View::Flamegraph => flamegraph::render(frame, area, state),
                View::Roi => roi::render(frame, area, state),
                View::Aggregate => aggregate::render(frame, area, state),
                View::Models => crate::views::models::render(frame, area, state),
                View::McpWaste => {
                    // M1.6.5 T5.3: real split-pane render when a
                    // `WasteReport` was threaded in via
                    // `AppRunner::new_with_waste`; fallback banner
                    // otherwise (e.g. `cmd::watch` constructs via
                    // `AppRunner::new` and never computes waste).
                    if let Some(waste) = state.waste_report {
                        mcp_waste::render(frame, area, waste, &mut state.mcp_waste_state);
                    } else {
                        let block = Block::default().borders(Borders::ALL).title(" MCP Waste ");
                        let p = Paragraph::new(
                            "MCP waste data not provided to TUI\n\
                             (run `agentprof analyze --export tui` to enable this view)",
                        )
                        .block(block);
                        frame.render_widget(p, area);
                    }
                }
            }
        }
        if state.help_open {
            draw_help_overlay(frame, area);
        }
    }
}

/// Render the help overlay (centered yellow-border block listing
/// key bindings + view legends). Called by [`AppRunner::render_into`]
/// when `state.help_open` and by [`crate::watch::WatchRunner::render_into`]
/// when `view_state.help_overlay` is set in Single mode (F1.7.1).
///
/// `pub(crate)` rather than truly public — the overlay is an
/// implementation detail of the runner layer; external callers
/// shouldn't render it themselves.
pub(crate) fn draw_help_overlay(frame: &mut Frame<'_>, full: Rect) {
    let w = full.width.min(60);
    let h = full.height.min(40);
    let x = full.x + (full.width.saturating_sub(w)) / 2;
    let y = full.y + (full.height.saturating_sub(h)) / 2;
    let area = Rect::new(x, y, w, h);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (any key closes) ")
        .style(Style::default().fg(Color::Yellow));
    let text = [
        "q / Ctrl-C       Quit",
        "1 / 2 / 3 / 4 / 5    Switch view (Flamegraph / Roi / Aggregate / Models / McpWaste)",
        "Tab / S-Tab      Cycle views forward / backward",
        "h / l            Vim aliases for S-Tab / Tab (prev / next view)",
        "↑ / ↓ or k / j   Scroll / select (viewport follows cursor)",
        "G                Jump to last row",
        "gg               Jump to first row (two-key vim sequence)",
        "t/c/s/p (Roi)    Cycle sort key (Total / Calls / Success% / p50)",
        "u (McpWaste)     Toggle unused-tools-only filter (M1.6.5)",
        "?                This help",
        "",
        "Detail view (Flamegraph → Enter):",
        "  Enter           Toggle args expand",
        "  Esc             Return to flamegraph",
        "  j / k / G / gg  Navigate tool calls",
        "  1 / 2 / 3 / 4 / 5   Pop detail + switch view",
        "  h / l           Pop detail + cycle prev / next view",
        "",
        "Flamegraph cell legend:",
        "  █ (colored)     Tool / hook / skill executing (color = ToolSource:",
        "                  cyan=Builtin, magenta=MCP, yellow=Skill)",
        "  ░ (dim gray)    LLM thinking time (no tool running; in-turn cost)",
        "  ·               Padding (turn ended; row shorter than longest turn)",
        "",
        "Flamegraph T-id color (F1.10 — leftmost 5-char column):",
        "  T-id (red)       Aborted turn (also underlined as backup)",
        "  T-id (gray)      Open / in-flight turn (no ended_at yet)",
        "  T-id (blue)      Thinking-only closed turn (no tool calls)",
        "  T-id (default)   Completed turn with tool calls",
        "  T-id (yellow)    Pending — turn has stuck tool call (F2.2)",
        "",
        "RoiView Tool color (F1.13 + F2.3 — Tool cell only):",
        "  Tool (red)       > 50% failure rate (likely broken)",
        "  Tool (yellow)    Any failure on busy tool (>= 3 calls), OR",
        "                   tool has pending call(s) (F2.3 — see footer)",
        "  Tool (default)   No failures, no pending, or too few calls",
        "  * (rank #)       User-waiting tool (DIM, e.g. ask_user)",
        "",
        "Watch footer banner:",
        "  ⚠ <tool> pending for <elapsed>  Tool stuck (F2.3, watch mode)",
        "  ⚠ reload error: ...             File reload failed",
        "",
        "Deep flamegraph: `analyze --export speedscope` → speedscope.app",
    ]
    .join("\n");
    let para = Paragraph::new(text).block(block);
    frame.render_widget(Clear, area);
    frame.render_widget(para, area);
}
