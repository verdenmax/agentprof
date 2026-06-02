//! CLI subcommand handlers.
//!
//! Each subcommand lives in its own submodule with an `*Cmd` args struct
//! (clap-derive) plus a `run(cmd: *Cmd, cfg: &LogConfig) -> Result<()>`
//! function. The `cfg` argument carries the resolved tracing config
//! (M1.6.4 T2) — it's consumed by TUI subcommands (T3) and by future
//! PII-aware tracing emission (T4).

pub mod aggregate;
pub mod analyze;
pub mod format;
pub mod list;
pub mod watch;

pub use crate::observability::LogConfig;
