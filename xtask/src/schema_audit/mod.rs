//! # schema-audit
//!
//! Developer tool for detecting `CopilotEvent` schema drift against real
//! Copilot CLI session data. See [`SchemaAuditCmd`] for CLI surface and
//! `docs/superpowers/specs/2026-05-27-m1.3-episode-and-schema-fix-design.md`
//! §FR-1 for requirements.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

/// CLI arguments for `cargo xtask schema-audit`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct SchemaAuditCmd {
    /// Root directory containing `<uuid>/events.jsonl` subdirectories.
    /// Defaults to `$HOME/.copilot/session-state`.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Cap the number of sessions scanned (most recent by mtime first).
    /// Default: scan all.
    #[arg(long)]
    pub sample_limit: Option<usize>,

    /// Write the markdown report to a file instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Only audit the given session UUIDs (repeat or comma-separate).
    #[arg(long, value_delimiter = ',')]
    pub sessions: Vec<String>,
}

/// Entry point invoked from `main.rs` when the `schema-audit` subcommand is chosen.
//
// Task 1 ships a no-op skeleton; Task 2 will consume `cmd` and return real
// errors. Allow the two transient lints until then.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn run(cmd: SchemaAuditCmd) -> Result<()> {
    let _ = cmd;
    eprintln!("schema-audit: Task 2 will wire the real implementation");
    Ok(())
}
