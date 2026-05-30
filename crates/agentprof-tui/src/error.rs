//! Crate-level error type for `agentprof-tui`.
//!
//! All fallible TUI operations return `Result<_, TuiError>`. Wrap I/O errors
//! from the terminal backend (`crossterm`) and any unexpected ratatui draw
//! errors. The error is `#[non_exhaustive]` to allow additive variants.

use thiserror::Error;

/// Crate-level error returned by `agentprof-tui` public functions.
///
/// # Examples
///
/// ```
/// use agentprof_tui::error::TuiError;
/// let e = TuiError::Io(std::io::Error::other("simulated"));
/// assert!(format!("{e}").contains("terminal io"));
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TuiError {
    /// I/O failure from the terminal backend (crossterm) — entering raw
    /// mode, drawing a frame, polling events, etc.
    #[error("terminal io: {0}")]
    Io(#[from] std::io::Error),

    /// Stdout is not a TTY. Surfaced by `app::terminal::enter` so the CLI
    /// can convert it to `ExitKind::OutputError` with a helpful message.
    #[error("stdout is not a terminal; --export tui requires a TTY")]
    NotATerminal,
}
