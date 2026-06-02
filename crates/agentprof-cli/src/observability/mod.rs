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

// `enter_tui_log_guard` / `TuiLogGuard` / `TracingHandle::swap_writer` are
// constructed in T2 but only consumed in T3 (TUI entry-site wiring). Allow
// dead_code + unused_imports here so T2 lands clean — T3 removes the
// allow by call-site usage.
#![allow(dead_code, unused_imports)]

pub mod config;
pub mod init;
pub mod tui_guard;

pub use config::{LogConfig, LogWriter};
pub use init::{init_tracing, TracingHandle};
pub use tui_guard::{enter_tui_log_guard, TuiLogGuard};
