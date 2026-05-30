//! CLI subcommand handlers.
//!
//! Each subcommand lives in its own submodule with an `*Cmd` args struct
//! (clap-derive) plus a `run(cmd: *Cmd) -> Result<()>` function.

pub mod analyze;
pub mod format;
pub mod list;
