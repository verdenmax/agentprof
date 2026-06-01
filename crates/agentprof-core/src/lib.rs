//! # agentprof-core
//!
//! Core domain model and analysis types for **agentprof**.
//!
//! This crate is the **leaf** of the workspace dependency graph; it does
//! **not** depend on any other workspace crate.
//!
//! ## Public modules
//!
//! - [`adapter`] — the [`adapter::Adapter`] trait, [`adapter::Event`] trait,
//!   and supporting types.
//! - [`analyzer`] — rollup functions ([`analyzer::analyze`],
//!   [`analyzer::AnalysisReport`]) consuming `Episodes`.
//! - [`model`] — domain types ([`model::session::RawSession`],
//!   [`model::meta::SessionMeta`], [`model::tool_source::ToolSource`]).
//! - [`error`] — workspace-level errors ([`error::CoreError`],
//!   [`error::ParseWarning`]).
//!
//! See `docs/architecture.md` for the L1 architecture documentation and
//! `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md`
//! for the M1.2 specification.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod adapter;
pub mod analyzer;
pub mod episode;
pub mod error;
pub mod export;
pub mod model;
