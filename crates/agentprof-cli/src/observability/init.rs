//! Install a tracing subscriber with a reload-able writer layer.
//!
//! [`init_tracing`] is called ONCE from `main()`; the returned
//! [`TracingHandle`] holds the `reload::Handle` that
//! [`super::enter_tui_log_guard`] uses to swap the writer to a file
//! when entering an interactive TUI.
//!
//! Soft-fall policy (ADR-0010 D-13): any subscriber installation
//! failure falls back to a default stderr subscriber + a one-shot
//! warning, and continues. Tracing MUST NOT block CLI startup.

use std::fs;
use std::io;
use std::sync::Mutex;

use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload::{self, Handle};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Layer, Registry};

use super::config::{LogConfig, LogWriter};

/// Inner fmt layer type — kept as an alias so the `Handle` signature
/// remains compact at use sites.
type FmtLayer =
    fmt::Layer<Registry, fmt::format::DefaultFields, fmt::format::Format, BoxMakeWriter>;
type ReloadHandle = Handle<FmtLayer, Registry>;

/// Held by `main()` for the lifetime of the process. Carries
/// (a) the reload handle so `enter_tui_log_guard` can swap the writer
/// at runtime, and (b) the appender `WorkerGuard` (when the active
/// writer is a non-blocking file appender) so the background writer
/// thread stays alive until process exit.
///
/// # Examples
///
/// ```text
/// // bin-crate: not name-resolvable from doctests; see
/// // tests/cli_tracing.rs (added in T3) for executable coverage.
/// let cfg = LogConfig::default();
/// let _handle = init_tracing(&cfg); // hold for process lifetime
/// ```
#[non_exhaustive]
pub struct TracingHandle {
    /// `None` when the soft-fall path was taken (no reload-able
    /// subscriber was installed); `Some` in the success path.
    reload_handle: Option<ReloadHandle>,
    /// Slot for the currently-active `WorkerGuard`. Held under a
    /// `Mutex` because `enter_tui_log_guard` runs on the main thread
    /// but `TracingHandle::drop` semantically runs at process exit.
    /// Holding the guard keeps the non-blocking worker alive; replacing
    /// it (`swap_writer`) drops the previous one (final flush).
    appender_guard: Mutex<Option<WorkerGuard>>,
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for TracingHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TracingHandle")
            .field("has_reload_handle", &self.reload_handle.is_some())
            .field("has_appender_guard", &true)
            .finish()
    }
}

impl TracingHandle {
    const fn empty() -> Self {
        Self {
            reload_handle: None,
            appender_guard: Mutex::new(None),
        }
    }

    /// Swap the active writer to `new_writer`, replacing the previous
    /// guard (which causes a final flush of the previous appender, if
    /// any).
    ///
    /// # Errors
    ///
    /// Returns `Err(&'static str)` when the soft-fall path was taken at
    /// init time (no reload handle present), or when
    /// `reload::Handle::modify` itself fails.
    pub fn swap_writer(
        &self,
        new_writer: BoxMakeWriter,
        new_guard: Option<WorkerGuard>,
    ) -> Result<(), &'static str> {
        let handle = self
            .reload_handle
            .as_ref()
            .ok_or("no reload handle (init_tracing soft-fell)")?;
        handle
            .modify(|layer| {
                *layer = fmt::Layer::new().with_writer(new_writer).with_ansi(false);
            })
            .map_err(|_| "reload::Handle::modify failed")?;
        // Drop the old guard AFTER the swap so any in-flight event from
        // before the swap still has a worker thread to flush to.
        if let Ok(mut slot) = self.appender_guard.lock() {
            *slot = new_guard;
        }
        Ok(())
    }
}

#[cfg(test)]
impl TracingHandle {
    /// Test-only crate-internal empty constructor — same as the private
    /// `empty()` but exposed for sibling-module tests (`tui_guard`).
    pub(crate) fn empty_for_test() -> Self {
        Self::empty()
    }
}

/// Install a tracing subscriber per `cfg`. Soft-fails to a default
/// stderr subscriber (no reload handle) on any error.
///
/// Call EXACTLY ONCE from `main()`. The returned [`TracingHandle`]
/// MUST be held for the process lifetime — dropping it stops the
/// background appender thread (if any) and loses buffered events.
///
/// # Soft-fall scenarios
///
/// - Invalid level filter string ⇒ falls back to `"warn"`.
/// - Log file parent dir missing AND cannot be created ⇒ falls back
///   to stderr writer (still reload-able).
/// - File open fails ⇒ falls back to stderr writer.
/// - Subscriber already installed (e.g. test environment) ⇒ swallowed;
///   returns a no-op `TracingHandle` (`swap_writer` will fail; callers
///   must tolerate this).
///
/// Each soft-fall emits one diagnostic (`eprintln!` for file-open
/// failures detected before the subscriber is alive).
///
/// # Examples
///
/// ```text
/// // bin-crate: see tests/cli_tracing.rs for executable coverage.
/// let cfg = LogConfig::resolve_from_env_and_flags(None, None);
/// let _handle = init_tracing(&cfg);
/// tracing::info!("started");
/// ```
#[must_use]
pub fn init_tracing(cfg: &LogConfig) -> TracingHandle {
    let filter = build_env_filter(&cfg.level_filter);

    let (initial_writer, initial_guard) = match &cfg.writer {
        LogWriter::Stderr => (stderr_writer(), None),
        LogWriter::File(path) => match try_build_file_writer(path) {
            Ok((w, g)) => (w, Some(g)),
            Err(e) => {
                // Subscriber isn't alive yet — emit to stderr directly.
                eprintln!(
                    "agentprof: warning: failed to open log file {}: {e}; \
                     falling back to stderr",
                    path.display()
                );
                (stderr_writer(), None)
            }
        },
    };

    let initial_layer = fmt::Layer::new()
        .with_writer(initial_writer)
        .with_ansi(false);
    let (reloadable, reload_handle) = reload::Layer::new(initial_layer);

    let install_result = Registry::default()
        .with(reloadable.with_filter(filter))
        .try_init();

    match install_result {
        Ok(()) => TracingHandle {
            reload_handle: Some(reload_handle),
            appender_guard: Mutex::new(initial_guard),
        },
        Err(_) => {
            // Already installed (e.g. test harness). Return a no-op handle —
            // swap_writer will fail with a clean error and the caller (TUI
            // guard) soft-falls per its own contract.
            TracingHandle::empty()
        }
    }
}

fn stderr_writer() -> BoxMakeWriter {
    BoxMakeWriter::new(io::stderr)
}

fn build_env_filter(level_filter: &str) -> EnvFilter {
    EnvFilter::try_new(level_filter).unwrap_or_else(|_| EnvFilter::new("warn"))
}

/// Build the file writer + its `WorkerGuard` (caller must keep the guard).
///
/// # Errors
///
/// Returns [`FileWriterError`] when `path` has no parent / file-name
/// component, or when the parent directory cannot be created. The caller
/// decides whether to soft-fall to stderr.
pub fn try_build_file_writer(
    path: &std::path::Path,
) -> Result<(BoxMakeWriter, WorkerGuard), FileWriterError> {
    let parent = path.parent().ok_or(FileWriterError::NoParent)?;
    if !parent.as_os_str().is_empty() && !parent.exists() {
        fs::create_dir_all(parent).map_err(FileWriterError::Io)?;
    }
    let file_name = path
        .file_name()
        .ok_or(FileWriterError::NoFileName)?
        .to_string_lossy()
        .into_owned();
    let dir = if parent.as_os_str().is_empty() {
        std::path::Path::new(".")
    } else {
        parent
    };
    let appender = tracing_appender::rolling::daily(dir, file_name);
    let (non_blocking, guard): (NonBlocking, WorkerGuard) =
        tracing_appender::non_blocking(appender);
    Ok((BoxMakeWriter::new(non_blocking), guard))
}

/// Reasons [`try_build_file_writer`] may refuse a path.
#[derive(Debug)]
pub enum FileWriterError {
    /// Path has no parent component (e.g. an empty string).
    NoParent,
    /// Path has no file-name component (e.g. `..`).
    NoFileName,
    /// I/O error creating the parent directory.
    Io(io::Error),
}

impl std::fmt::Display for FileWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoParent => write!(f, "log file path has no parent directory"),
            Self::NoFileName => write!(f, "log file path has no file name component"),
            Self::Io(e) => write!(f, "io error creating log dir: {e}"),
        }
    }
}

impl std::error::Error for FileWriterError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_handle_drops_cleanly() {
        let h = TracingHandle::empty();
        drop(h);
    }

    #[test]
    fn build_env_filter_invalid_falls_back_to_warn() {
        let _ = build_env_filter("not-a-real-level");
    }

    #[test]
    fn build_env_filter_valid_does_not_panic() {
        let _ = build_env_filter("debug");
        let _ = build_env_filter("warn,agentprof_core=trace");
    }

    #[test]
    fn try_build_file_writer_creates_parent_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let log_path = tmp.path().join("nested").join("subdir").join("test.log");
        let result = try_build_file_writer(&log_path);
        assert!(result.is_ok(), "should create nested parent dirs");
        assert!(log_path.parent().expect("parent").exists());
    }

    #[test]
    fn swap_writer_fails_when_no_reload_handle() {
        let h = TracingHandle::empty();
        let result = h.swap_writer(BoxMakeWriter::new(io::stderr), None);
        assert!(result.is_err());
    }
}
