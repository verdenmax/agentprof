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
//!
//! Path redaction for span fields is handled by
//! [`agentprof_core::observability::pii::hash_path`], which itself
//! honours `AGENTPROF_LOG_FULL_PATHS=1` at every emission layer (no
//! cli-side wrapper needed — see the
//! `m1.6.4-final-followup-full-paths-l2-l3-gap` fix in CHANGELOG).

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
