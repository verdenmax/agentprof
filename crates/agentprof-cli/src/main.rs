//! # agentprof
//!
//! `agentprof` — the perf flamegraph and ROI profiler for AI coding agents.
//!
//! Entry point: parses CLI arguments, initializes `tracing`, dispatches
//! to the appropriate `cmd::*` subcommand. M1.4 ships the single
//! `analyze` subcommand; `list`, `aggregate`, `watch` land in M1.5+.
//!
//! See `docs/architecture.md` §8 (CLI protocol) for the canonical
//! specification.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

mod cmd;

#[derive(Parser, Debug)]
#[command(
    name = "agentprof",
    version,
    about = "Perf flamegraph + ROI profiler for AI coding agents"
)]
enum Cli {
    /// Analyze a single agent session and produce a markdown or JSON report.
    Analyze(cmd::analyze::AnalyzeCmd),
}

fn main() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("agentprof: {err:#}");
            ExitCode::from(classify_error(&err))
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli {
        Cli::Analyze(cmd) => cmd::analyze::run(cmd),
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_env("AGENTPROF_LOG").unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn classify_error(err: &anyhow::Error) -> u8 {
    err.downcast_ref::<cmd::analyze::ExitKind>()
        .map_or(1, |k| *k as u8)
}
