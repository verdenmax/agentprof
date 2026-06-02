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

// `LogWriter` and `TuiLogGuard` are part of the documented public surface
// of this module but are referenced through full paths inside the crate;
// the re-exports exist so external readers of `agentprof_cli::observability`
// see a complete API. `TracingHandle::empty_for_test` is unit-test-only.
#![allow(dead_code, unused_imports)]

pub mod config;
pub mod init;
pub mod tui_guard;

pub use config::{LogConfig, LogWriter};
pub use init::{init_tracing, TracingHandle};
pub use tui_guard::{enter_tui_log_guard, TuiLogGuard};
