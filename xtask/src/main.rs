//! # xtask
//!
//! Build / maintenance / release driver for the agentprof workspace.
//! Follows the [cargo-xtask](https://github.com/matklad/cargo-xtask)
//! convention: run via `cargo run -p xtask -- <task>` or `cargo xtask <task>`
//! (if the `[alias]` is configured in `.cargo/config.toml`).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use anyhow::Result;
use clap::Parser;

mod schema_audit;
mod visual_guide;

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "agentprof workspace tasks", version)]
enum Cli {
    /// Audit Copilot CLI session data against the current `CopilotEvent` schema.
    ///
    /// Scans `~/.copilot/session-state/` (or `--root`), classifies
    /// `CopilotEvent::Unknown` events by their wire `type`, summarizes
    /// `ParseWarning` distribution, and reports `start`/`end` pair balance.
    /// Use after Copilot CLI upgrades to detect schema drift.
    SchemaAudit(schema_audit::SchemaAuditCmd),

    /// Generate the agentprof visual guide HTML site under `docs/visual-guide/`.
    VisualGuide(visual_guide::VisualGuideCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::SchemaAudit(cmd) => schema_audit::run(cmd),
        Cli::VisualGuide(cmd) => visual_guide::run(cmd),
    }
}
