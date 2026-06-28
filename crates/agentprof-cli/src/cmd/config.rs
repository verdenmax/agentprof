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

/// The `config` actions (all four wired: `path` / `show` / `edit` / `init`).
/// `#[non_exhaustive]` documents intent for external readers but is inert
/// while the enum is private, so `run`'s `match` must still be updated when
/// variants are added.
#[derive(Subcommand, Debug)]
#[non_exhaustive]
enum ConfigAction {
    /// Print the effective config-file path and whether it exists.
    Path,
    /// Show the effective configuration (built-in defaults merged with
    /// file overrides), annotating each value's source.
    Show,
    /// Open the config file in `$VISUAL`/`$EDITOR` (creating it first if absent).
    Edit,
    /// Write a commented default config template to the resolved path.
    Init {
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
}

/// Dispatch a `config` invocation. Bin-only; errors carry an
/// [`ExitKind`] so `main` maps them to the right process exit code.
///
/// # Errors
///
/// Returns an [`ExitKind`]-tagged error when the config directory cannot
/// be determined (`OutputError`).
// `match cmd.action` only borrows / copies out unit + `Copy` variant data,
// so clippy wants `&ConfigCmd`; we take it by value for dispatch uniformity
// with the sibling `cmd::*::run` fns.
#[allow(clippy::needless_pass_by_value)]
pub fn run(cmd: ConfigCmd) -> anyhow::Result<()> {
    match cmd.action {
        ConfigAction::Path => run_path(),
        ConfigAction::Show => run_show(),
        ConfigAction::Edit => run_edit(),
        ConfigAction::Init { force } => run_init(force),
    }
}

/// Build the standard "no config directory" error, shared by actions that
/// must resolve a path. Actionable: names `$AGENTPROF_CONFIG`.
fn config_dir_err() -> anyhow::Error {
    ExitKind::OutputError.into_anyhow(
        "cannot determine config directory: $AGENTPROF_CONFIG is unset \
         and no platform config directory is available"
            .to_string(),
    )
}

/// Print the resolved config path + `[exists]` / `[not found]` marker.
fn run_path() -> anyhow::Result<()> {
    let path = agentprof_cli::config::resolve_config_path().ok_or_else(config_dir_err)?;
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

/// Commented default config. Only `[storage]` is active; `[otlp]`/`[serve]`
/// are commented so the file parses in any feature build.
const CONFIG_TEMPLATE: &str = r#"# agentprof configuration
# Location: $AGENTPROF_CONFIG, else ~/.config/agentprof/config.toml (XDG).
# Only the blocks below are wired; unknown keys are rejected on load.

[storage]
# "cache" (default, XDG_CACHE_HOME) | "store" (XDG_DATA_HOME, opt-in)
mode = "cache"
# Override the XDG-derived DB path for the active mode.
# path = "~/.cache/agentprof/cache.sqlite"
# Cache mode only; 0 disables auto-pruning.
auto_prune_days = 30

# [otlp]  — requires a build with the `otlp` feature.
# listen_grpc = "127.0.0.1:4317"
# listen_http = "127.0.0.1:4318"
# listen_token = "shared-secret"
# session_idle_timeout = "5m"
# shutdown_grace = "10s"
# max_logs_request_bytes = 8388608
# max_open_sessions = 1024

# [serve]  — requires a build with the `web` feature.
# bind = "127.0.0.1:4329"
# interval_default = 5
# auto_open = true
"#;

/// Write [`CONFIG_TEMPLATE`] to `path`, creating any missing parent dirs.
/// Shared by `init` and `edit` (the latter self-heals an absent file, D-4).
///
/// # Errors
///
/// [`ExitKind::OutputError`] when the parent directory cannot be created or
/// the file cannot be written.
fn write_template(path: &std::path::Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!(
                "failed to create config directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(path, CONFIG_TEMPLATE).map_err(|e| {
        ExitKind::OutputError.into_anyhow(format!(
            "failed to write config file {}: {e}",
            path.display()
        ))
    })?;
    Ok(())
}

/// Write [`CONFIG_TEMPLATE`] to the resolved path, creating parent dirs.
///
/// # Errors
///
/// [`ExitKind::UserError`] when the file exists and `force` is false;
/// [`ExitKind::OutputError`] on a missing config dir or write failure.
fn run_init(force: bool) -> anyhow::Result<()> {
    let path = agentprof_cli::config::resolve_config_path().ok_or_else(config_dir_err)?;
    if path.exists() && !force {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "config file already exists at {}; pass --force to overwrite",
            path.display()
        )));
    }
    write_template(&path)?;
    println!("wrote default config to {}", path.display());
    Ok(())
}

/// Open the config file in `$VISUAL` (preferred) or `$EDITOR`. Creates the
/// file from [`CONFIG_TEMPLATE`] first when absent (D-4).
///
/// # Errors
///
/// [`ExitKind::UserError`] when neither editor env var is set or the editor
/// exits non-zero; [`ExitKind::OutputError`] on a missing config dir, a
/// template-write failure, or an editor-spawn failure.
fn run_edit() -> anyhow::Result<()> {
    // An empty var (`VISUAL=`) is treated as unset so it falls through to
    // $EDITOR rather than spawning `""` (which would mis-report as exit 3).
    let editor = std::env::var_os("VISUAL")
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os("EDITOR").filter(|v| !v.is_empty()))
        .ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow("no editor configured: set $VISUAL or $EDITOR".to_string())
        })?;
    let path = agentprof_cli::config::resolve_config_path().ok_or_else(config_dir_err)?;
    // Self-heal: write the template before spawning so the editor always
    // opens an existing, parseable file (D-4).
    if !path.exists() {
        write_template(&path)?;
    }
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!("failed to launch editor {editor:?}: {e}"))
        })?;
    if !status.success() {
        return Err(ExitKind::UserError.into_anyhow(status.code().map_or_else(
            || "editor terminated by signal".to_string(),
            |c| format!("editor exited with status {c}"),
        )));
    }
    Ok(())
}

/// `"(from file)"` when the value came from the config file, else
/// `"(default)"`.
const fn source_marker(from_file: bool) -> &'static str {
    if from_file {
        "(from file)"
    } else {
        "(default)"
    }
}

/// Print one `key = value  (source)` line.
fn show_line(key: &str, value: &str, from_file: bool) {
    println!("{key} = {value}  {}", source_marker(from_file));
}

/// Show the effective configuration. Reuses the real per-block resolvers
/// so displayed defaults cannot drift from runtime behavior.
///
/// # Errors
///
/// [`ExitKind::DataError`] when the file is present but fails to parse or
/// a block fails to resolve; [`ExitKind::OutputError`] on a read error
/// other than "not found".
fn run_show() -> anyhow::Result<()> {
    use agentprof_cli::config::PartialConfig;

    let path = agentprof_cli::config::resolve_config_path();
    let (partial, marker) = match path.as_ref() {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(src) => {
                let cfg = agentprof_cli::config::parse_toml(&src).map_err(|e| {
                    ExitKind::DataError
                        .into_anyhow(format!("failed to parse config file {}: {e}", p.display()))
                })?;
                (cfg, "[exists]")
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (PartialConfig::default(), "[not found]")
            }
            Err(e) => {
                return Err(ExitKind::OutputError
                    .into_anyhow(format!("failed to read config file {}: {e}", p.display())));
            }
        },
        None => (PartialConfig::default(), "[no config dir]"),
    };
    let path_str = path
        .as_ref()
        .map_or_else(|| "<none>".to_string(), |p| p.display().to_string());
    println!("# Effective configuration  (path: {path_str} {marker})");
    println!();
    render_storage(partial.storage)?;
    #[cfg(feature = "otlp")]
    render_otlp(partial.otlp)?;
    #[cfg(not(feature = "otlp"))]
    println!("\n[otlp]  (feature not enabled in this build)");
    #[cfg(feature = "web")]
    render_serve(partial.serve);
    #[cfg(not(feature = "web"))]
    println!("\n[serve]  (feature not enabled in this build)");
    Ok(())
}

/// Render the `[storage]` block. `resolve_storage_config` is reused so
/// the default path/mode shown is exactly what the runtime would use.
///
/// # Errors
///
/// [`ExitKind::DataError`] when the `[storage]` block fails to resolve.
fn render_storage(s: agentprof_storage::config::PartialStorageConfig) -> anyhow::Result<()> {
    // Capture source flags before the partial is moved into the resolver.
    let (mode_f, path_f, prune_f) = (
        s.mode.is_some(),
        s.path.is_some(),
        s.auto_prune_days.is_some(),
    );
    let r = agentprof_cli::config::resolve_storage_config(s, None)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("invalid [storage] config: {e}")))?;
    // StorageMode is #[non_exhaustive]; Debug->lowercase matches the TOML
    // `rename_all = "lowercase"` representation without a brittle match.
    let mode = format!("{:?}", r.mode).to_lowercase();
    println!("[storage]");
    show_line("mode", &format!("\"{mode}\""), mode_f);
    show_line("path", &format!("\"{}\"", r.path.display()), path_f);
    show_line("auto_prune_days", &r.auto_prune_days.to_string(), prune_f);
    Ok(())
}

/// Format an optional listener address: `Some` → quoted; `None` (empty
/// string in the file) → `"" (disabled)`.
#[cfg(feature = "otlp")]
fn opt_addr(a: Option<std::net::SocketAddr>) -> String {
    a.map_or_else(|| "\"\" (disabled)".to_string(), |s| format!("\"{s}\""))
}

/// Format an optional path: `Some` → quoted display; `None` → `(unset)`.
#[cfg(feature = "otlp")]
fn opt_path(p: Option<&std::path::Path>) -> String {
    p.map_or_else(|| "(unset)".to_string(), |p| format!("\"{}\"", p.display()))
}

/// Render the `[otlp]` block via `OtlpServerConfig::from_partial`
/// (reused → no default drift). `None` partial ⇒ all built-in defaults.
///
/// # Errors
///
/// [`ExitKind::DataError`] when the `[otlp]` block fails to resolve.
#[cfg(feature = "otlp")]
fn render_otlp(
    partial: Option<agentprof_storage::otlp::config::PartialOtlpServerConfig>,
) -> anyhow::Result<()> {
    use agentprof_storage::otlp::config::OtlpServerConfig;
    let p = partial.unwrap_or_default();
    // Capture source flags before `from_partial` consumes `p`.
    let (f_grpc, f_http, f_token) = (
        p.listen_grpc.is_some(),
        p.listen_http.is_some(),
        p.listen_token.is_some(),
    );
    let (f_cert, f_key, f_ca) = (
        p.tls_cert.is_some(),
        p.tls_key.is_some(),
        p.tls_client_ca.is_some(),
    );
    let (f_idle, f_grace) = (p.session_idle_timeout.is_some(), p.shutdown_grace.is_some());
    let (f_logs, f_metrics, f_traces, f_sessions) = (
        p.max_logs_request_bytes.is_some(),
        p.max_metrics_request_bytes.is_some(),
        p.max_traces_request_bytes.is_some(),
        p.max_open_sessions.is_some(),
    );
    // `from_partial` merges file values over built-in defaults. We do NOT
    // call `validate()`: `show` is a diagnostic, so an invalid effective
    // config should be displayed for the user to fix, not rejected here.
    let c = OtlpServerConfig::from_partial(p)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("invalid [otlp] config: {e}")))?;
    println!("\n[otlp]");
    show_line("listen_grpc", &opt_addr(c.listen_grpc), f_grpc);
    show_line("listen_http", &opt_addr(c.listen_http), f_http);
    show_line(
        "listen_token",
        &c.listen_token
            .as_deref()
            .map_or_else(|| "(unset)".to_string(), |t| format!("\"{t}\"")),
        f_token,
    );
    show_line("tls_cert", &opt_path(c.tls_cert.as_deref()), f_cert);
    show_line("tls_key", &opt_path(c.tls_key.as_deref()), f_key);
    show_line("tls_client_ca", &opt_path(c.tls_client_ca.as_deref()), f_ca);
    show_line(
        "session_idle_timeout",
        &format!("\"{}s\"", c.session_idle_timeout.as_secs()),
        f_idle,
    );
    show_line(
        "shutdown_grace",
        &format!("\"{}s\"", c.shutdown_grace.as_secs()),
        f_grace,
    );
    show_line(
        "max_logs_request_bytes",
        &c.max_logs_request_bytes.to_string(),
        f_logs,
    );
    show_line(
        "max_metrics_request_bytes",
        &c.max_metrics_request_bytes.to_string(),
        f_metrics,
    );
    show_line(
        "max_traces_request_bytes",
        &c.max_traces_request_bytes.to_string(),
        f_traces,
    );
    show_line(
        "max_open_sessions",
        &c.max_open_sessions.to_string(),
        f_sessions,
    );
    Ok(())
}

/// Render the `[serve]` block. No pub partial-only resolver exists, so the
/// 3 defaults are inlined (mirror `serve/mod.rs:240/258/269`).
#[cfg(feature = "web")]
fn render_serve(partial: Option<agentprof_cli::config::PartialServeConfig>) {
    let p = partial.unwrap_or_default();
    println!("\n[serve]");
    let bind = p
        .bind
        .clone()
        .unwrap_or_else(|| "127.0.0.1:4329".to_string());
    show_line("bind", &format!("\"{bind}\""), p.bind.is_some());
    show_line(
        "interval_default",
        &p.interval_default.unwrap_or(5).to_string(),
        p.interval_default.is_some(),
    );
    show_line(
        "auto_open",
        &p.auto_open.unwrap_or(true).to_string(),
        p.auto_open.is_some(),
    );
}
