//! # agentprof-tui
//!
//! Ratatui-based terminal views for **agentprof**: per-session flamegraph,
//! tool ROI matrix, and per-session aggregate dashboards.
//!
//! Depends only on [`agentprof_core`]. No other crate is allowed to depend
//! on `ratatui` or `crossterm` directly.
//!
//! ## Panic safety
//!
//! This crate is **forbidden** from panicking at runtime
//! (see `docs/architecture.md` §12.3 / `docs/internals/adr-0006-panic-safe-tui.md`).
//! Call [`app::terminal::install_panic_hook`] before [`app::terminal::enter`]
//! to guarantee terminal state is restored even on panic.
//!
//! ## Public surface (M1.5)
//!
//! - [`theme`] — palette + style modifiers
//! - [`error::TuiError`] — crate-level errors
//! - [`app::terminal`] — terminal lifecycle (`install_panic_hook`, `enter`, `leave`)
//!
//! `AppRunner` ships in T6; per-view rendering ships in T3–T5.

pub mod app;
pub mod error;
pub mod theme;
