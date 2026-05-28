//! # schema-audit
//!
//! Developer tool for detecting `CopilotEvent` schema drift against real
//! Copilot CLI session data. See [`SchemaAuditCmd`] for CLI surface and
//! `docs/superpowers/specs/2026-05-27-m1.3-episode-and-schema-fix-design.md`
//! §FR-1 for requirements.

mod classifier;
mod report;
mod scanner;

use std::path::PathBuf;

use anyhow::{Context, Result};
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
///
/// Discovers Copilot sessions under `--root` (or `$HOME/.copilot/session-state`),
/// dual-parses every event line, classifies findings, and writes the markdown
/// report to stdout (default) or to `--output <file>`.
///
/// # Errors
///
/// Returns an error if:
/// - `$HOME` is unset and no `--root` was provided (no default available);
/// - the session root cannot be enumerated by [`agentprof_adapters::copilot::CopilotAdapter`];
/// - any individual session file cannot be read; or
/// - `--output` is provided and writing to it fails.
pub fn run(cmd: SchemaAuditCmd) -> Result<()> {
    let SchemaAuditCmd {
        root,
        sample_limit,
        output,
        sessions,
    } = cmd;
    let root = root
        .or_else(default_root)
        .context("could not determine session root (set $HOME or pass --root)")?;
    let audits = scanner::scan(&root, sample_limit, &sessions).context("scanning sessions")?;
    let classification = classifier::classify(&audits);
    let md = report::render(&classification, &root);

    if let Some(out) = output {
        std::fs::write(&out, &md)
            .with_context(|| format!("writing report to {}", out.display()))?;
        eprintln!("report written to {}", out.display());
    } else {
        print!("{md}");
    }
    Ok(())
}

fn default_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".copilot/session-state"))
}
