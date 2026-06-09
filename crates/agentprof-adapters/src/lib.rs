//! # agentprof-adapters
//!
//! Per-agent session log adapters.
//!
//! ## Supported agents
//!
//! - [`copilot`] — GitHub Copilot CLI (`~/.copilot/session-state/<uuid>/events.jsonl`)
//!
//! Future:
//! - `claude` — Anthropic Claude Code (Phase 2)
//! - `codex`  — `OpenAI` Codex CLI (Phase 3)
//!
//! See `docs/adapters.md` for the contribution guide.

pub mod copilot;
pub mod datasource;
pub mod registry;

pub use datasource::AdapterDataSource;
