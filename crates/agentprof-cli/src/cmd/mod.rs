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
//!
//! Cross-cutting modules:
//! - [`exit`] — process exit-code taxonomy ([`exit::ExitKind`]) shared
//!   by all subcommands (per full-review CLI #10).
//! - [`since`] — `--since` value parser ([`since::parse_since`]) shared
//!   by `list`, `aggregate`, and `watch aggregate` (per full-review
//!   CLI #1).

pub mod aggregate;
pub mod analyze;
pub mod db;
pub mod exit;
pub mod format;
#[cfg(feature = "otlp")]
pub mod ingest_otlp;
pub mod list;
pub mod mcp_waste;
pub mod model_hint;
pub mod since;
pub mod watch;

pub use crate::observability::{LogConfig, TracingHandle};
