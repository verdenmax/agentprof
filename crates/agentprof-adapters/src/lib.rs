//! # agentprof-adapters
//!
//! Per-agent session log adapters. Each supported agent has its own module
//! that implements the `Adapter` trait defined in
//! [`agentprof-core`](../agentprof_core/index.html).
//!
//! Adapters convert agent-specific log formats (e.g. Claude's JSONL files,
//! Codex CLI sessions, Copilot CLI state) into the unified `RawSession`
//! representation.
//!
//! See [`docs/adapters.md`](https://github.com/agentprof/agentprof/blob/main/docs/adapters.md)
//! for the contribution guide on adding a new adapter.
//!
//! ## Modules (planned)
//!
//! - `claude`   — Anthropic Claude Code JSONL parser
//! - `codex`    — OpenAI Codex CLI session parser
//! - `copilot`  — GitHub Copilot CLI session parser
//! - `registry` — `AgentKind` → boxed `Adapter` mapping (`--agent auto` support)
//! - `discovery` — shared filesystem helpers
