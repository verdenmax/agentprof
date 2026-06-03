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
use crossterm::event::poll;
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal};

use crate::app::event::Event;
use crate::app::state::{dispatch, Action, AppState};
use crate::error::TuiError;
use crate::views::{aggregate, flamegraph, roi, View};

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

    /// Render one frame of the current view into the provided backend.
    ///
    /// Exposed for snapshot testing — `tests/views.rs` calls this directly
    /// with a `TestBackend` instead of running the live event loop.
    ///
    /// # Errors
    ///
    /// Returns [`TuiError::Io`] if `terminal.draw` fails.
    pub fn draw_frame<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), TuiError> {
        terminal.draw(|frame| self.render_into(frame))?;
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
            terminal.draw(|frame| self.render_into(frame))?;
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

    fn render_into(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if let Some(detail) = self.state.detail_view.as_ref() {
            crate::views::turn_detail::render_turn_detail(frame, area, detail, &self.state);
        } else {
            match self.state.view {
                View::Flamegraph => flamegraph::render(frame, area, &self.state),
                View::Roi => roi::render(frame, area, &self.state),
                View::Aggregate => aggregate::render(frame, area, &self.state),
            }
        }
        if self.state.help_open {
            draw_help_overlay(frame, area);
        }
    }
}

fn draw_help_overlay(frame: &mut Frame<'_>, full: Rect) {
    let w = full.width.min(60);
    let h = full.height.min(22);
    let x = full.x + (full.width.saturating_sub(w)) / 2;
    let y = full.y + (full.height.saturating_sub(h)) / 2;
    let area = Rect::new(x, y, w, h);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (any key closes) ")
        .style(Style::default().fg(Color::Yellow));
    let text = [
        "q / Ctrl-C       Quit",
        "1 / 2 / 3        Switch view (Flamegraph / Roi / Aggregate)",
        "Tab / S-Tab      Cycle views forward / backward",
        "↑ / ↓ or k / j   Scroll / select (viewport follows cursor)",
        "G                Jump to last row",
        "gg               Jump to first row (two-key vim sequence)",
        "t/c/s/p (Roi)    Cycle sort key (Total / Calls / Success% / p50)",
        "?                This help",
        "",
        "Flamegraph cell legend:",
        "  █ (colored)     Tool / hook / skill executing (color = ToolSource:",
        "                  cyan=Builtin, magenta=MCP, yellow=Skill)",
        "  ░ (dim gray)    LLM thinking time (no tool running; in-turn cost)",
        "  ·               Padding (turn ended; row shorter than longest turn)",
        "",
        "Deep flamegraph: `analyze --export speedscope` → speedscope.app",
    ]
    .join("\n");
    let para = Paragraph::new(text).block(block);
    frame.render_widget(Clear, area);
    frame.render_widget(para, area);
}
