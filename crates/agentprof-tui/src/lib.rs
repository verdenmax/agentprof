//! # agentprof-tui
//!
//! Ratatui-based terminal views for **agentprof**: per-session flamegraph,
//! tool ROI matrix, and cross-session aggregate dashboards.
//!
//! Depends only on [`agentprof-core`](../agentprof_core/index.html). No other
//! crate is allowed to depend on `ratatui` or `crossterm` directly.
//!
//! ## Panic safety
//!
//! This crate is **forbidden** from panicking at runtime
//! (see `docs/architecture.md` §16, rule 11). `app::AppRunner` installs a
//! panic hook that restores the terminal raw mode before re-emitting the
//! panic, so a crash never leaves the user's shell in an unusable state.
//!
//! ## Modules (planned)
//!
//! - `app`               — event loop, view switching, terminal lifecycle
//! - `views::flamegraph` — per-turn token flamegraph
//! - `views::roi`        — Tool ROI matrix
//! - `views::aggregate`  — cross-session aggregates
//! - `theme`             — palette and styling primitives
