//! Process exit-code taxonomy shared across all subcommands.
//!
//! Lifted out of `cmd::analyze` per full-review CLI #10
//! (`exitkind-location`). The old location was historical — `analyze`
//! was the first subcommand to define structured exit codes, and
//! later `list` / `aggregate` / `watch` imported `ExitKind` from it
//! despite having no other dependency. Pulling it up to its own
//! `cmd::exit` module makes the dependency graph match the conceptual
//! one: subcommands depend on a shared exit taxonomy, not on each
//! other.
//!
//! Mapped to process exit codes per `docs/architecture.md` §8.1:
//!
//! - `UserError = 1` — invalid args, session not found, bad config.
//! - `DataError = 2` — adapter / DB / mapper could not parse session data.
//! - `OutputError = 3` — any "could not deliver the result" failure:
//!   file write, non-TTY TUI start, JSON / HTML render, OTLP listener
//!   bind, TUI runtime, OTLP server task exit, external service call.
//!   The name predates the broader use case but is preserved for
//!   stability — see `docs/architecture.md` §8.1 historical note.
//!
//! `main()`'s `classify_error` downcasts the `anyhow::Error` chain to
//! `ExitKind` to pick the process exit code; the
//! [`ExitKind::into_anyhow`] helper wraps a user-facing message into
//! an `anyhow::Error` carrying the exit kind as `.context()`.

/// Process exit-code taxonomy.
#[allow(clippy::enum_variant_names)] // names spec'd in docs/architecture.md
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum ExitKind {
    /// User error: invalid args, session not found.
    #[error("user error")]
    UserError = 1,
    /// Data error: session file could not be parsed by the adapter.
    #[error("data error")]
    DataError = 2,
    /// I/O error during output write, or TUI invoked from a non-TTY.
    #[error("output error")]
    OutputError = 3,
}

impl ExitKind {
    /// Wrap a user-facing message into an `anyhow::Error` whose
    /// downcast target is `ExitKind`. `main()`'s `classify_error`
    /// extracts this to pick the process exit code.
    pub(crate) fn into_anyhow(self, msg: String) -> anyhow::Error {
        anyhow::Error::msg(msg).context(self)
    }
}
