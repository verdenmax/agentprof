//! TUI-aware tracing sink switch. Before entering ratatui's alt-screen,
//! call [`enter_tui_log_guard`] (passing the [`TracingHandle`] returned
//! from [`super::init_tracing`] in `main`) to swap the writer to a
//! rolling file under `$XDG_STATE_HOME/agentprof/agentprof.log` (unless
//! the user explicitly pinned one or forced stderr). On Drop the guard
//! prints the log path to stdout — this runs AFTER `terminal::leave()`,
//! so the line is visible in the user's shell.
//!
//! Soft-falls (ADR-0010 D-13):
//! - If `cfg.force_stderr` is true (user passed `--log-file -`), do
//!   NOT auto-switch — preserve the user's explicit choice.
//! - If `cfg.writer` is already a `File`, do NOT swap (the subscriber
//!   was installed with the file writer in `main`); just store the path
//!   so Drop prints it.
//! - If XDG state dir resolution fails OR file open fails OR the
//!   reload-handle modify fails, emit a warning and return a no-op
//!   guard (the subscriber stays as-is — likely on stderr, with the
//!   documented alt-screen-corruption risk).

use std::path::PathBuf;

use super::config::{LogConfig, LogWriter};
use super::init::{try_build_file_writer, TracingHandle};

/// Returned by [`enter_tui_log_guard`]; on Drop prints
/// `"agentprof: trace log at <path>"` to stdout iff a file writer is
/// active. Idempotent (no-op on second drop). Does NOT swap the writer
/// back on Drop — once a TUI session has written to a file, post-TUI
/// emission continues there too (consistent location for debugging).
#[non_exhaustive]
pub struct TuiLogGuard {
    log_path: Option<PathBuf>,
}

impl std::fmt::Debug for TuiLogGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TuiLogGuard")
            .field("log_path", &self.log_path)
            .finish()
    }
}

impl Drop for TuiLogGuard {
    fn drop(&mut self) {
        if let Some(p) = self.log_path.take() {
            // After ratatui::terminal::leave() runs, stdout is the user's
            // shell again — println! is safe and visible.
            println!("agentprof: trace log at {}", p.display());
        }
    }
}

/// Auto-switch the active tracing writer to a rolling file under
/// `$XDG_STATE_HOME/agentprof/agentprof.log` before entering a TUI,
/// unless `cfg` already pins one.
///
/// `tracing_handle` MUST be the same handle returned from
/// [`super::init_tracing`] at startup — it carries the `reload::Handle`
/// used to swap the writer.
///
/// Returns a no-op [`TuiLogGuard`] if the user explicitly forced stderr
/// (via `--log-file -`) or if anything in the swap path fails (a
/// warning is emitted in that case).
///
/// # Examples
///
/// ```text
/// // bin-crate: see tests/cli_tracing.rs for executable coverage.
/// fn run_tui_cmd(cfg: &LogConfig, h: &TracingHandle) {
///     let _log_guard = enter_tui_log_guard(cfg, h);
///     terminal::enter();
///     // ... ratatui app loop ...
///     terminal::leave();
///     // _log_guard drops here, after terminal::leave; prints path.
/// }
/// ```
#[must_use]
pub fn enter_tui_log_guard(cfg: &LogConfig, tracing_handle: &TracingHandle) -> TuiLogGuard {
    // User explicitly chose stderr — respect it (alt-screen corruption is
    // on them).
    if cfg.force_stderr {
        return TuiLogGuard { log_path: None };
    }

    // User explicitly set a file — main already installed the file writer
    // in the subscriber; don't swap. Just store the path for Drop.
    if let LogWriter::File(p) = &cfg.writer {
        return TuiLogGuard {
            log_path: Some(p.clone()),
        };
    }

    // Default Stderr path → try to swap to XDG state dir file.
    let Some(path) = resolve_xdg_log_path() else {
        tracing::warn!(
            "could not resolve XDG_STATE_HOME or $HOME/.local/state; \
             leaving tracing on stderr (may corrupt TUI alt-screen)"
        );
        return TuiLogGuard { log_path: None };
    };

    let (writer, guard) = match try_build_file_writer(&path) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::warn!(
                log_file = %path.display(),
                error = %e,
                "failed to open TUI log file; leaving tracing on stderr \
                 (may corrupt TUI alt-screen)"
            );
            return TuiLogGuard { log_path: None };
        }
    };

    match tracing_handle.swap_writer(writer, Some(guard)) {
        Ok(()) => TuiLogGuard {
            log_path: Some(path),
        },
        Err(e) => {
            tracing::warn!(
                error = %e,
                "TracingHandle::swap_writer failed; leaving tracing on stderr"
            );
            TuiLogGuard { log_path: None }
        }
    }
}

fn resolve_xdg_log_path() -> Option<PathBuf> {
    use directories::BaseDirs;
    let base = BaseDirs::new()?;
    let state_dir = base.state_dir().map_or_else(
        || base.cache_dir().to_path_buf(),
        std::path::Path::to_path_buf,
    );
    Some(state_dir.join("agentprof").join("agentprof.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_stderr_guard_is_no_op() {
        let cfg = LogConfig {
            level_filter: "warn".into(),
            writer: LogWriter::Stderr,
            full_paths: false,
            force_stderr: true,
        };
        let handle = TracingHandle::empty_for_test();
        let g = enter_tui_log_guard(&cfg, &handle);
        assert!(g.log_path.is_none());
        drop(g);
    }

    #[test]
    fn explicit_file_guard_carries_path_for_drop_message() {
        let cfg = LogConfig {
            level_filter: "warn".into(),
            writer: LogWriter::File(PathBuf::from("/tmp/explicit.log")),
            full_paths: false,
            force_stderr: false,
        };
        let handle = TracingHandle::empty_for_test();
        let g = enter_tui_log_guard(&cfg, &handle);
        assert_eq!(
            g.log_path.as_deref(),
            Some(std::path::Path::new("/tmp/explicit.log"))
        );
    }

    #[test]
    fn no_op_when_swap_fails_due_to_no_reload_handle() {
        // Sanity: when handle has no reload (init soft-fell), the guard
        // should also no-op rather than crash.
        let cfg = LogConfig::default(); // Stderr writer; not force_stderr
        let handle = TracingHandle::empty_for_test();
        let _g = enter_tui_log_guard(&cfg, &handle);
        // Just verifying no panic.
    }
}
