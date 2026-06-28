//! # agentprof
//!
//! `agentprof` — the perf flamegraph and ROI profiler for AI coding agents.
//!
//! Entry point: parses CLI arguments, initializes `tracing`, dispatches
//! to the appropriate `cmd::*` subcommand. M1.4 shipped `analyze`; M1.6.1
//! adds `list`; `aggregate` lands in M1.6.2; `watch` in M1.6.3;
//! structured tracing infrastructure lands in M1.6.4.
//!
//! See `docs/architecture.md` §8 (CLI protocol) for the canonical
//! specification.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmd;
mod observability;

#[derive(Parser, Debug)]
#[command(
    name = "agentprof",
    version,
    about = "Perf flamegraph + ROI profiler for AI coding agents"
)]
struct Cli {
    /// Tracing level filter (`trace`|`debug`|`info`|`warn`|`error`) or
    /// full env-filter syntax (e.g. `warn,agentprof_core=trace`).
    /// Default: env `AGENTPROF_LOG_LEVEL` / `AGENTPROF_LOG`, then `warn`.
    #[arg(long, global = true)]
    log_level: Option<String>,

    /// Write trace events to this file instead of stderr. Use `-` to
    /// force stderr (overrides TUI auto-redirect). Default: stderr for
    /// non-TUI, auto under `$XDG_STATE_HOME/agentprof/agentprof.log`
    /// for `--export tui` and `watch`.
    #[arg(long, global = true)]
    log_file: Option<PathBuf>,

    /// Skip all storage I/O (degrades dual-path to single-path adapter).
    ///
    /// When set, `agentprof` behaves as if no `SQLite` cache existed:
    /// the adapter is the sole source of truth, and no rows are read
    /// from or written to the storage layer (M2.1 T4.3).
    #[arg(global = true, long)]
    no_cache: bool,

    /// Override the resolved storage DB path.
    ///
    /// Beats both the `[storage]` config-file value and the XDG-derived
    /// default. See `agentprof_storage::config::StorageConfig` for the
    /// resolution order (M2.1 T4.3).
    #[arg(global = true, long, value_name = "PATH")]
    storage_path: Option<PathBuf>,

    /// Suppress per-session divergence warning lines on stderr.
    ///
    /// Affects only the dual-path `adapter vs storage` divergence
    /// warnings introduced in M2.1 T4.1; structured `tracing` events
    /// remain unchanged (M2.1 T4.3).
    #[arg(global = true, long)]
    quiet: bool,

    #[command(subcommand)]
    cmd: SubCmd,
}

#[derive(Subcommand, Debug)]
enum SubCmd {
    /// Analyze a single agent session and produce a markdown or JSON report.
    Analyze(cmd::analyze::AnalyzeCmd),
    /// List recent agent sessions in a compact table.
    List(cmd::list::ListCmd),
    /// Aggregate metrics across many recent sessions.
    Aggregate(cmd::aggregate::AggregateCmd),
    /// Live-refresh TUI: monitor session file(s) and redraw on change (M1.6.3).
    Watch(cmd::watch::WatchCmd),
    /// Cross-session MCP server waste analysis (M1.6.5).
    McpWaste(cmd::mcp_waste::McpWasteArgs),
    /// Inspect and manage the user config file (`config path|show|edit|init`).
    Config(cmd::config::ConfigCmd),
    /// Database lifecycle and inspection: `init` / `stats` / `ingest`
    /// / `prune` / `vacuum` / `export` (M2.1 T6).
    Db(cmd::db::DbArgs),
    /// Run the embedded OTLP receiver and persist incoming sessions
    /// to the local store (M2.2 T8.1).
    #[cfg(feature = "otlp")]
    IngestOtlp(cmd::ingest_otlp::IngestOtlpCmd),
    /// Run the embedded HTTP dashboard (`agentprof serve`, M2.3).
    #[cfg(feature = "web")]
    Serve(cmd::serve::ServeCmd),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cfg = observability::LogConfig::resolve_from_env_and_flags(
        cli.log_level.clone(),
        cli.log_file.clone(),
    );
    let tracing_handle = observability::init_tracing(&cfg);

    match run(cli, &cfg, &tracing_handle) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // D-7 exception: the user-facing final error message MUST reach
            // stderr even when the tracing subscriber is pointed at a file.
            eprintln!("agentprof: {err:#}");
            ExitCode::from(classify_error(&err))
        }
    }
}

fn run(
    cli: Cli,
    cfg: &observability::LogConfig,
    tracing_handle: &observability::TracingHandle,
) -> Result<()> {
    let no_cache = cli.no_cache;
    let storage_path = cli.storage_path.clone();
    let quiet = cli.quiet;
    match cli.cmd {
        SubCmd::Analyze(c) => cmd::analyze::run(c, cfg, tracing_handle, no_cache, storage_path),
        SubCmd::List(c) => cmd::list::run(c, cfg, tracing_handle, no_cache, storage_path, quiet),
        SubCmd::Aggregate(c) => {
            cmd::aggregate::run(c, cfg, tracing_handle, no_cache, storage_path, quiet)
        }
        SubCmd::Watch(c) => cmd::watch::run(c, cfg, tracing_handle, no_cache, storage_path),
        SubCmd::McpWaste(c) => {
            cmd::mcp_waste::run(c, cfg, tracing_handle, no_cache, storage_path, quiet)
        }
        SubCmd::Config(c) => cmd::config::run(c),
        SubCmd::Db(c) => cmd::db::run(c, storage_path),
        #[cfg(feature = "otlp")]
        SubCmd::IngestOtlp(c) => cmd::ingest_otlp::run(c, storage_path),
        #[cfg(feature = "web")]
        SubCmd::Serve(c) => cmd::serve::run(c),
    }
}

fn classify_error(err: &anyhow::Error) -> u8 {
    err.downcast_ref::<cmd::exit::ExitKind>()
        .map_or(1, |k| *k as u8)
}
