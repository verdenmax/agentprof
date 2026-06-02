//! Tracing initialization and TUI-aware sink switching for the
//! `agentprof` binary.
//!
//! See `docs/internals/adr-0010-tracing-infrastructure.md` for the full
//! design.
//!
//! Public surface (link-local to the bin):
//! - [`LogConfig`] / [`LogWriter`] / [`LogConfig::resolve_from_env_and_flags`]
//! - [`TracingHandle`] / [`init_tracing`]
//! - [`TuiLogGuard`] / [`enter_tui_log_guard`]
//! - [`maybe_hash_path`]

pub mod config;
pub mod init;
pub mod tui_guard;

// `LogWriter` is part of the documented public surface but is referenced
// only via full paths inside the bin; the re-export exists so external
// readers see a complete API. `TracingHandle::empty_for_test` is
// unit-test-only. As of T5 these are the only two items needing the
// dead-code allow.
pub use config::LogConfig;
#[allow(unused_imports)]
pub use config::LogWriter;
pub use init::{init_tracing, TracingHandle};
pub use tui_guard::enter_tui_log_guard;
#[allow(unused_imports)]
pub use tui_guard::TuiLogGuard;

/// Return `path.display().to_string()` if `cfg.full_paths` is true,
/// otherwise [`agentprof_core::observability::pii::hash_path`].
///
/// Path hashing is the default for tracing emissions in
/// `agentprof-cli`; users opt out via `AGENTPROF_LOG_FULL_PATHS=1`
/// (reflected in [`LogConfig::full_paths`]). See spec D-5.
///
/// Adapter / analyzer / aggregator crates (`agentprof-core`,
/// `agentprof-adapters`) cannot reach `cfg` and so hash
/// unconditionally; this opt-out is intentionally cli-only.
///
/// # Examples
///
/// ```text
/// // bin-crate: see tests/cli_tracing.rs for executable coverage.
/// use agentprof_cli::observability::{LogConfig, maybe_hash_path};
/// let cfg = LogConfig::default();
/// let s = maybe_hash_path(&cfg, std::path::Path::new("/tmp/x"));
/// assert_eq!(s.len(), 8); // hashed by default
/// ```
#[must_use]
pub fn maybe_hash_path(cfg: &LogConfig, path: &std::path::Path) -> String {
    if cfg.full_paths {
        path.display().to_string()
    } else {
        agentprof_core::observability::pii::hash_path(path)
    }
}
