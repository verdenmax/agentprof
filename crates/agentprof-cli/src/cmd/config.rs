//! `agentprof config` — inspect and manage the user config file
//! (`~/.config/agentprof/config.toml` or `$AGENTPROF_CONFIG`).
//!
//! Actions: `path` (print resolved path), `show` (effective config),
//! `edit` (open in `$VISUAL`/`$EDITOR`), `init` (write a template).
//! Scoped to the wired `[storage]` / `[otlp]` / `[serve]` blocks; see
//! `docs/superpowers/specs/2026-06-28-config-subcommand-design.md`.

use clap::{Args, Subcommand};

use crate::cmd::exit::ExitKind;

/// `agentprof config` command (subcommand dispatcher).
#[derive(Args, Debug)]
pub struct ConfigCmd {
    #[command(subcommand)]
    action: ConfigAction,
}

/// The `config` actions (only `path` is wired in T1; `show` / `init` /
/// `edit` land in later tasks). `#[non_exhaustive]` documents intent for
/// external readers but is inert while the enum is private, so `run`'s
/// `match` must still be updated when variants are added.
#[derive(Subcommand, Debug)]
#[non_exhaustive]
enum ConfigAction {
    /// Print the effective config-file path and whether it exists.
    Path,
}

/// Dispatch a `config` invocation. Bin-only; errors carry an
/// [`ExitKind`] so `main` maps them to the right process exit code.
///
/// # Errors
///
/// Returns an [`ExitKind`]-tagged error when the config directory cannot
/// be determined (`OutputError`).
// Future actions (`show`/`init`/`edit`) consume owned data out of
// `ConfigAction`; the dispatcher takes `ConfigCmd` by value to match.
#[allow(clippy::needless_pass_by_value)]
pub fn run(cmd: ConfigCmd) -> anyhow::Result<()> {
    match cmd.action {
        ConfigAction::Path => run_path(),
    }
}

/// Print the resolved config path + `[exists]` / `[not found]` marker.
fn run_path() -> anyhow::Result<()> {
    let path = agentprof_cli::config::resolve_config_path().ok_or_else(|| {
        ExitKind::OutputError.into_anyhow(
            "cannot determine config directory: $AGENTPROF_CONFIG is unset \
             and no platform config directory is available"
                .to_string(),
        )
    })?;
    let marker = if path.exists() {
        "[exists]"
    } else {
        "[not found]"
    };
    // `resolve_config_path` discards provenance, so re-check the env var
    // here to flag when the path came from an explicit override (spec §4).
    let source = if std::env::var_os("AGENTPROF_CONFIG").is_some() {
        " (from $AGENTPROF_CONFIG)"
    } else {
        ""
    };
    println!("{} {marker}{source}", path.display());
    Ok(())
}
