//! Workspace-shared observability helpers.
//!
//! Tracing emission policy and orchestrator-side config live in
//! `agentprof-cli::observability` — this module exposes only the
//! lib-leaf-safe helpers other workspace crates (adapters, tui) may
//! call directly. See ADR-0010 for the full design.

pub mod pii;
