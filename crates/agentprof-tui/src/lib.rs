//! # agentprof-tui
//!
//! Ratatui-based terminal views for **agentprof**: per-session flamegraph,
//! tool ROI matrix, per-session aggregate dashboards, plus M1.6.3
//! live-refresh `watch` + cross-session aggregate TUI.
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
//! ## Public surface
//!
//! - [`theme`] — palette + style modifiers
//! - [`error::TuiError`] — crate-level errors
//! - [`app::terminal`] — terminal lifecycle (`install_panic_hook`, `enter`, `leave`)
//! - [`AppRunner`] — M1.5 borrow-based event loop / view switcher
//! - [`WatchRunner`] — M1.6.3 owned-data runner; supports static + live-refresh modes

pub mod app;
pub mod error;
pub mod theme;
pub mod views;
pub mod watch;

pub use app::AppRunner;
pub use watch::WatchRunner;
