//! CLI subcommand handlers.
//!
//! Each subcommand lives in its own submodule with an
//! `*Cmd` args struct (clap-derive) plus a
//! `run(cmd: *Cmd, cfg: &LogConfig, tracing_handle: &TracingHandle) -> Result<()>`
//! function. The `cfg` argument carries the resolved tracing config
//! (M1.6.4 T2); `tracing_handle` is the reload handle returned from
//! `init_tracing` and is required so TUI subcommands (T3) can swap
//! their tracing writer to a file via `enter_tui_log_guard` before
//! the alt-screen takes over the terminal. Non-TUI subcommands
//! accept it for signature uniformity but do not consume it.

pub mod aggregate;
pub mod analyze;
pub mod format;
pub mod list;
pub mod watch;

pub use crate::observability::{LogConfig, TracingHandle};
