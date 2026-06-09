//! `agentprof-cli` library surface.
//!
//! The CLI is primarily a binary crate (`src/main.rs`), but a thin
//! library facade exists so that:
//!
//! - integration tests can exercise composer types
//!   ([`data_source::DualPathDataSource`]) directly without spawning
//!   the binary;
//! - future embedders (e.g. an in-process TUI launcher) can reuse the
//!   composition logic without re-implementing it.
//!
//! Per `docs/architecture.md` §3 the CLI must remain the **only**
//! assembly layer that wires lib crates together — so this `lib.rs`
//! must not grow into a general-purpose API.
//!
//! See `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`
//! §3.2 for the dual-path data-source design.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod config;
pub mod data_source;
