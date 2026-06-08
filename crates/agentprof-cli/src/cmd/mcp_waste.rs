//! `agentprof mcp-waste` — cross-session report of MCP tools loaded but never
//! called. See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md`
//! §7.3 for the user-facing contract.
//!
//! This module is the T4.1 scaffold: it exposes the clap-derive arg struct
//! ([`McpWasteArgs`]), the supported output formats ([`McpWasteExport`]),
//! and a shared [`resolve_mcp_config_path`] helper that the
//! `analyze --section mcp-waste` dispatch (T3.1) also consumes. The
//! [`run`] entry point intentionally returns a not-yet-implemented error;
//! T4.2 fills in the cross-session aggregation pipeline.

use std::path::PathBuf;

use clap::ValueEnum;

use crate::cmd::{LogConfig, TracingHandle};

/// Arguments for `agentprof mcp-waste`.
///
/// Mirrors the spec §7.3 CLI surface: time-window filter, optional adapter
/// root override, `mcp.json` path override, and an export format that
/// deliberately excludes TUI (see [`McpWasteExport`] for rationale).
#[derive(Debug, clap::Args)]
#[non_exhaustive]
pub struct McpWasteArgs {
    /// Adapter session-state root (default: per-adapter standard path).
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Time-window filter (e.g. `7d`, `30d`, `all`). Default: `7d`.
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Limit "Always unused" table to top N entries. Default 20.
    #[arg(long, default_value_t = 20)]
    pub top: usize,

    /// Override `mcp.json` path (default: `~/.copilot/mcp.json`).
    /// Relative paths resolve against CWD; `~`-prefixed paths expand
    /// via `directories::BaseDirs::home_dir()` to match `--root`
    /// convention.
    #[arg(long)]
    pub mcp_config: Option<PathBuf>,

    /// Output format. TUI is deferred to a future milestone — see
    /// spec §10 for the rationale.
    #[arg(long, value_enum, default_value = "md")]
    pub export: McpWasteExport,

    /// Output file (default: stdout).
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Output formats supported by `agentprof mcp-waste`.
///
/// TUI is intentionally **not** included; per spec §7.3 / §10 the live
/// dashboard for waste lives in `agentprof watch` and the dedicated TUI
/// view is deferred to a future milestone.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
#[non_exhaustive]
pub enum McpWasteExport {
    /// Markdown report (default; renders the same template family as
    /// `analyze --export md`).
    Md,
    /// JSON document suitable for machine consumption / CI gating.
    Json,
    /// Self-contained HTML report (askama template).
    Html,
}

/// Resolve `~/.copilot/mcp.json` (or the override) into an absolute
/// [`PathBuf`].
///
/// This helper is shared with `cmd::analyze`'s `--section mcp-waste`
/// dispatch (T3.1) so both code paths read the same configuration with
/// identical `~`-expansion semantics.
///
/// # Examples
///
/// ```ignore
/// # use std::path::Path;
/// # use agentprof_cli::cmd::mcp_waste::resolve_mcp_config_path;
/// // Override wins over the default home-based lookup.
/// let p = resolve_mcp_config_path(Some(Path::new("./mcp.json"))).unwrap();
/// assert!(p.ends_with("mcp.json"));
/// ```
///
/// # Errors
///
/// Returns an `anyhow::Error` when `override_path` is `None` and
/// `directories::BaseDirs::new()` fails to determine the user's home
/// directory (e.g. on a minimal CI container without `HOME` set and no
/// `/etc/passwd` entry for the current uid).
pub fn resolve_mcp_config_path(override_path: Option<&std::path::Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(expand_tilde(p));
    }
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("could not resolve user home directory"))?;
    Ok(base.home_dir().join(".copilot").join("mcp.json"))
}

/// Expand a leading `~/` against the current user's home directory.
///
/// Falls back to the original path when no home directory can be
/// determined (preserves the user's literal input for the caller's
/// downstream error message).
fn expand_tilde(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(base) = directories::BaseDirs::new() {
            return base.home_dir().join(rest);
        }
    }
    p.to_path_buf()
}

/// Subcommand entry point.
///
/// **T4.1 scaffold**: returns a not-yet-implemented error. The full
/// cross-session aggregation pipeline (adapter walk → waste analyzer
/// → renderer dispatch) lands in T4.2.
///
/// Accepts `_cfg` and `_tracing_handle` for signature uniformity with
/// the other `cmd::*::run` entry points; both are unused today.
///
/// # Errors
///
/// Always returns an `anyhow::Error` until T4.2 supplies the
/// implementation.
pub fn run(
    _args: McpWasteArgs,
    _cfg: &LogConfig,
    _tracing_handle: &TracingHandle,
) -> anyhow::Result<()> {
    anyhow::bail!("mcp-waste run() not yet implemented (T4.2)")
}
