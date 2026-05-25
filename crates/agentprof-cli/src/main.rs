//! # agentprof
//!
//! `agentprof` — the perf flamegraph and ROI profiler for AI coding agents.
//!
//! Entry point: parses CLI arguments, initializes `tracing`, installs a
//! panic hook (required by the TUI), and dispatches to the appropriate
//! subcommand module under `cmd::*`.
//!
//! See `docs/architecture.md` §8 (CLI protocol) for the canonical
//! specification.

fn main() {
    // Subcommand wiring lands in Phase 0 prototype. See
    // docs/superpowers/specs/ for the next implementation plan.
    eprintln!("agentprof skeleton: CLI subcommands not yet implemented.");
    eprintln!("See docs/plan.md and docs/architecture.md for the roadmap.");
    std::process::exit(0);
}
