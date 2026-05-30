//! Panic-safe terminal lifecycle.
//!
//! [`install_panic_hook`] MUST be called before [`enter`] so that any panic
//! during the TUI run is intercepted, the terminal is restored to cooked mode,
//! and the original panic hook is invoked to print the panic message.
//! [`leave`] is idempotent — calling it twice or after an `enter` failure is
//! safe.
//!
//! See `docs/internals/adr-0006-panic-safe-tui.md` for the rationale.

use std::io::{self, IsTerminal, Stdout};
use std::sync::Once;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::error::TuiError;

/// Concrete `Terminal` type returned by [`enter`].
pub type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

static PANIC_HOOK_INSTALLED: Once = Once::new();

/// Install a panic hook that restores the terminal before re-emitting the
/// panic. Idempotent — calling more than once is a no-op (uses [`Once`]).
///
/// MUST be called before [`enter`]; otherwise a panic during `run()` will
/// leave the terminal in raw mode + alternate screen, breaking the user's
/// shell.
///
/// # Examples
///
/// ```
/// agentprof_tui::app::terminal::install_panic_hook();
/// agentprof_tui::app::terminal::install_panic_hook(); // idempotent
/// ```
pub fn install_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Best-effort terminal restore; ignore errors since we are
            // already panicking. Then invoke the original hook so the user
            // sees the panic message in cooked mode.
            let mut stdout = io::stdout();
            let _ = disable_raw_mode();
            let _ = execute!(stdout, LeaveAlternateScreen);
            original(info);
        }));
    });
}

/// Enter raw mode + alternate screen and build a ratatui [`Terminal`].
///
/// # Errors
///
/// Returns [`TuiError::NotATerminal`] if stdout is not a TTY (e.g. piped to
/// a file). Returns [`TuiError::Io`] if `enable_raw_mode` or
/// `EnterAlternateScreen` fail. On `EnterAlternateScreen` or `Terminal::new`
/// failure after raw mode was enabled, the function performs a best-effort
/// `disable_raw_mode` cleanup before returning the error.
///
/// # Examples
///
/// ```no_run
/// use agentprof_tui::app::terminal::{enter, install_panic_hook, leave};
/// install_panic_hook();
/// let mut term = enter()?;
/// // ... draw frames ...
/// leave(&mut term)?;
/// # Ok::<(), agentprof_tui::error::TuiError>(())
/// ```
pub fn enter() -> Result<TuiTerminal, TuiError> {
    if !io::stdout().is_terminal() {
        return Err(TuiError::NotATerminal);
    }
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(e) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(e.into());
    }
    let backend = CrosstermBackend::new(stdout);
    match Terminal::new(backend) {
        Ok(t) => Ok(t),
        Err(e) => {
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            let _ = disable_raw_mode();
            Err(e.into())
        }
    }
}

/// Restore the terminal: leave alternate screen + disable raw mode. Best-effort,
/// idempotent. Errors during cleanup are reported but never propagated by the
/// CLI (we are exiting anyway).
///
/// # Errors
///
/// Returns the first error encountered if any of `disable_raw_mode` /
/// `LeaveAlternateScreen` / `show_cursor` fail. All three steps are
/// attempted regardless — failures do not short-circuit subsequent cleanup.
/// Callers typically log and ignore (the process is about to exit).
///
/// # Examples
///
/// ```no_run
/// use agentprof_tui::app::terminal::{enter, leave};
/// let mut term = enter()?;
/// leave(&mut term)?;
/// # Ok::<(), agentprof_tui::error::TuiError>(())
/// ```
pub fn leave(terminal: &mut TuiTerminal) -> Result<(), TuiError> {
    // Best-effort cleanup: keep going even if one step fails so we don't
    // leave the terminal in a partially-restored state. We return the
    // first error encountered (if any) so the CLI can log it.
    let r1 = disable_raw_mode();
    let r2 = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let r3 = terminal.show_cursor();
    r1.map_err(TuiError::from)
        .and_then(|()| r2.map_err(TuiError::from))
        .and_then(|()| r3.map_err(TuiError::from))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn install_panic_hook_is_idempotent() {
        install_panic_hook();
        install_panic_hook();
        install_panic_hook();
    }

    #[test]
    fn enter_returns_not_a_terminal_when_stdout_is_piped() {
        // The Rust test harness captures stdout, which means
        // `io::stdout().is_terminal()` returns false. So this is a real
        // negative integration test — under `cargo test`, enter() must
        // refuse to set raw mode.
        let err = enter().unwrap_err();
        assert!(matches!(err, TuiError::NotATerminal));
    }
}
