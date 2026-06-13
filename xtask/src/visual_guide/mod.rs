//! `cargo xtask visual-guide` — generate the agentprof visual guide
//! HTML site under `docs/visual-guide/`.
//!
//! Output: 1 `index.html` + 6 `usage/*.html` + 8 `wiki/*.html` = 15 files.
//!
//! See `docs/superpowers/specs/2026-06-13-visual-guide-design.md` for
//! the full design; ADR-0025 (T21) codifies the 7 decisions.

// Stub: T7+ will consume `cmd` by value (fs writes) and return real `Result`s.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use clap::Args;

/// CLI arguments for `cargo xtask visual-guide`.
#[derive(Debug, Args)]
pub struct VisualGuideCmd {
    /// Delete existing generated `*.html` files under `docs/visual-guide/`
    /// before regenerating. Does NOT touch `assets/` or `README.md`.
    #[arg(long)]
    pub clean: bool,

    /// Validate only — render to in-memory strings, verify askama compiles
    /// and all components produce HTML, but DO NOT write any files.
    /// Used by CI on pull requests.
    #[arg(long)]
    pub check: bool,
}

/// Entry point for the `visual-guide` subcommand.
///
/// # Errors
///
/// Returns `anyhow::Error` if askama rendering fails, if any required
/// asset is missing, or if filesystem operations fail.
///
/// # Examples
///
/// ```no_run
/// // Invoked via the xtask CLI, not directly:
/// // $ cargo run -p xtask -- visual-guide --check
/// ```
pub fn run(cmd: VisualGuideCmd) -> anyhow::Result<()> {
    if cmd.check {
        println!("visual-guide: --check mode (not yet implemented, T7+)");
        return Ok(());
    }
    if cmd.clean {
        println!("visual-guide: --clean (not yet implemented, T7+)");
    }
    println!("visual-guide: render (not yet implemented, T7+)");
    Ok(())
}
