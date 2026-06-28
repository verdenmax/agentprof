# Config Subcommand (`agentprof config`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `agentprof config <path|show|edit|init>` to manage the user's `config.toml`, and unify the duplicated config-path resolution.

**Architecture:** Bin-only (`agentprof-cli`, anyhow). A new `cmd/config.rs` holds the four actions; the `$AGENTPROF_CONFIG`→XDG path lookup is lifted into `agentprof_cli::config::resolve_config_path()` (single source of truth) and reused by `ingest-otlp` + `serve`. `config show` prints effective values (built-in defaults merged with file overrides) with `(default)`/`(from file)` source annotation. Scoped to the wired `storage`/`otlp`/`serve` blocks.

**Tech Stack:** clap derive, `toml` + serde (`PartialConfig`), `directories` (XDG), `assert_cmd` + `predicates` + `tempfile` (integration tests). Spec: `docs/superpowers/specs/2026-06-28-config-subcommand-design.md`.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/agentprof-cli/src/config.rs` (modify) | Add `resolve_config_path()`; existing `parse_toml`/`PartialConfig`/`resolve_storage_config` reused by `show`. |
| `crates/agentprof-cli/src/cmd/config.rs` (create) | `ConfigCmd` + `ConfigAction` enum + `run` dispatch + the four action impls (`run_path`/`run_show`/`run_init`/`run_edit`) + inline default template + effective-config rendering. |
| `crates/agentprof-cli/src/cmd/mod.rs` (modify) | `pub mod config;`. |
| `crates/agentprof-cli/src/main.rs` (modify) | `SubCmd::Config(cmd::config::ConfigCmd)` variant + dispatch arm. |
| `crates/agentprof-cli/src/cmd/ingest_otlp.rs` (modify) | Drop local `resolve_config_file_path`; call shared `resolve_config_path`. |
| `crates/agentprof-cli/src/cmd/serve/mod.rs` (modify) | Same dedup. |
| `crates/agentprof-cli/tests/cli_config.rs` (create) | `assert_cmd` integration tests for all four actions (env-isolated via `.env("AGENTPROF_CONFIG", …)` + `tempfile`). |
| Docs (Task 5) | architecture §8/§10, cli README, root README, ADR-0027, ROADMAP, CHANGELOG. |

**Tasks:** 1 = `path` + path-resolver dedup · 2 = `show` (effective + source) · 3 = `init` + template · 4 = `edit` · 5 = docs + ADR.

---

## Task 1: `config path` skeleton + unified `resolve_config_path()`

**Files:**
- Modify: `crates/agentprof-cli/src/config.rs` (add `resolve_config_path`)
- Create: `crates/agentprof-cli/src/cmd/config.rs`
- Modify: `crates/agentprof-cli/src/cmd/mod.rs`
- Modify: `crates/agentprof-cli/src/main.rs`
- Modify: `crates/agentprof-cli/src/cmd/ingest_otlp.rs:353-380`
- Modify: `crates/agentprof-cli/src/cmd/serve/mod.rs:287-318`
- Test: `crates/agentprof-cli/tests/cli_config.rs` (create)

- [ ] **Step 1: Write the failing integration test**

Create `crates/agentprof-cli/tests/cli_config.rs`:

```rust
//! Integration tests for `agentprof config`. Env-isolated via `.env`
//! (`AGENTPROF_CONFIG` points each child process at a tempdir file).

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("agentprof").expect("binary builds")
}

#[test]
fn config_path_reports_existing_file() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\nmode = \"cache\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains(cfg.to_str().unwrap()))
        .stdout(predicate::str::contains("[exists]"));
}

#[test]
fn config_path_reports_missing_file() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("absent.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[not found]"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p agentprof-cli --test cli_config`
Expected: FAIL — `error: unrecognized subcommand 'config'` (clap rejects it; both tests fail on non-success exit).

- [ ] **Step 3: Add `resolve_config_path()` to `config.rs`**

In `crates/agentprof-cli/src/config.rs`, ensure `use std::path::PathBuf;` is present (it is — `resolve_storage_config` uses it), then add:

```rust
/// Resolve the effective `config.toml` path: `$AGENTPROF_CONFIG` (if set)
/// wins, otherwise the platform XDG config dir
/// (`config_dir()/agentprof/config.toml`).
///
/// Returns `None` only when no override is set **and** no platform base
/// directory can be determined (rare — e.g. no `$HOME`). The file not
/// existing is **not** `None`: the path is still returned so callers can
/// report "not found".
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// std::env::set_var("AGENTPROF_CONFIG", "/tmp/agentprof-x.toml");
/// assert_eq!(
///     agentprof_cli::config::resolve_config_path(),
///     Some(PathBuf::from("/tmp/agentprof-x.toml")),
/// );
/// std::env::remove_var("AGENTPROF_CONFIG");
/// ```
#[must_use]
pub fn resolve_config_path() -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("AGENTPROF_CONFIG") {
        return Some(PathBuf::from(custom));
    }
    let dirs = directories::BaseDirs::new()?;
    Some(dirs.config_dir().join("agentprof").join("config.toml"))
}
```

- [ ] **Step 4: Create `cmd/config.rs` with the `Path` action only**

Create `crates/agentprof-cli/src/cmd/config.rs`:

```rust
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

/// The four `config` actions. `#[non_exhaustive]` keeps `set` addable later.
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
pub fn run(cmd: ConfigCmd) -> anyhow::Result<()> {
    match cmd.action {
        ConfigAction::Path => run_path(),
    }
}

/// Print the resolved config path + `[exists]` / `[not found]` marker.
fn run_path() -> anyhow::Result<()> {
    let path = crate::config::resolve_config_path().ok_or_else(|| {
        ExitKind::OutputError.into_anyhow(
            "cannot determine config directory: $AGENTPROF_CONFIG is unset \
             and no platform config directory is available"
                .to_string(),
        )
    })?;
    let marker = if path.exists() { "[exists]" } else { "[not found]" };
    println!("{} {marker}", path.display());
    Ok(())
}
```

- [ ] **Step 5: Wire the module + main enum**

In `crates/agentprof-cli/src/cmd/mod.rs` add (alphabetical with the other `pub mod`s):

```rust
pub mod config;
```

In `crates/agentprof-cli/src/main.rs`, add a variant to `enum SubCmd` (after `McpWaste`, before `Db`):

```rust
    /// Inspect and manage the user config file (`config path|show|edit|init`).
    Config(cmd::config::ConfigCmd),
```

and a dispatch arm in `fn run`'s `match cli.cmd` (mirror placement):

```rust
        SubCmd::Config(c) => cmd::config::run(c),
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p agentprof-cli --test cli_config`
Expected: PASS (2 tests).

- [ ] **Step 7: Dedup — point `ingest-otlp` + `serve` at the shared resolver**

In `crates/agentprof-cli/src/cmd/ingest_otlp.rs`: delete the local `fn resolve_config_file_path() -> Option<PathBuf>` (lines ~374-380) and change `load_partial_otlp_from_disk`'s first line from `let path = resolve_config_file_path()?;` to:

```rust
    let path = agentprof_cli::config::resolve_config_path()?;
```

In `crates/agentprof-cli/src/cmd/serve/mod.rs`: delete its local `fn resolve_config_file_path() -> Option<PathBuf>` (lines ~308-318) and change `load_partial_serve_from_disk`'s first line to:

```rust
    let path = agentprof_cli::config::resolve_config_path()?;
```

Remove any now-unused imports (`directories`, `std::path::Path`) flagged by the compiler in those two files.

- [ ] **Step 8: Verify nothing regressed**

Run: `cargo test -p agentprof-cli --all-features 2>&1 | tail -20`
Expected: all green (the `e2e_idle_sweeper_flushes_inactive_session` OTLP test is a known env flake — re-run isolated if it's the only failure).
Run: `cargo clippy -p agentprof-cli --all-targets --all-features -- -D warnings`
Expected: clean (no unused-import / dead-code warnings from the dedup).

- [ ] **Step 9: Commit**

```bash
git add crates/agentprof-cli/src/config.rs crates/agentprof-cli/src/cmd/config.rs \
        crates/agentprof-cli/src/cmd/mod.rs crates/agentprof-cli/src/main.rs \
        crates/agentprof-cli/src/cmd/ingest_otlp.rs crates/agentprof-cli/src/cmd/serve/mod.rs \
        crates/agentprof-cli/tests/cli_config.rs
git commit -m "feat(cli): config path action + unified resolve_config_path (L-4 T1)"
```

---

## Task 2: `config show` — effective config with source annotation

**Files:**
- Modify: `crates/agentprof-cli/src/cmd/config.rs` (add `Show` variant + `run_show` + render helpers)
- Modify: `crates/agentprof-cli/tests/cli_config.rs` (add show tests)

**Design:** `show` reuses real resolvers so displayed defaults never drift:
`storage` via `resolve_storage_config(partial, None)`, `otlp` via
`OtlpServerConfig::from_partial(partial)` (both pub, partial-only). Only
`serve` has no pub partial-only resolver (its resolver needs `&ServeCmd` +
returns a private type), so its 3 defaults are inlined (`127.0.0.1:4329` /
`5` / `true`, mirroring `serve/mod.rs:240/258/269`). Source = the matching
`PartialConfig` field's `is_some()`, captured **before** the partial is
moved into the resolver.

- [ ] **Step 1: Write the failing tests**

Append to `crates/agentprof-cli/tests/cli_config.rs`:

```rust
#[test]
fn show_marks_file_values_and_defaults() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    // Only `mode` is set in the file; auto_prune_days stays default.
    std::fs::write(&cfg, "[storage]\nmode = \"store\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mode = \"store\"  (from file)"))
        .stdout(predicate::str::contains("auto_prune_days = 30  (default)"));
}

#[test]
fn show_without_file_is_all_defaults() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("absent.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[not found]"))
        .stdout(predicate::str::contains("mode = \"cache\"  (default)"));
}

#[test]
fn show_rejects_malformed_config() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\nmode = = broken\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .code(2) // ExitKind::DataError
        .stderr(predicate::str::contains("failed to parse"));
}
```

> Note: `auto_prune_days` default is `30` per `architecture.md` §10 /
> `StorageConfig::default`. `StorageMode` IS `#[non_exhaustive]`, so
> `render_storage` formats it via `Debug`->`to_lowercase()` (yielding
> `cache`/`store`, matching the TOML `rename_all`) instead of a `match`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p agentprof-cli --test cli_config show_`
Expected: FAIL — `unrecognized subcommand`-style failures (no `show` yet).

- [ ] **Step 3: Add the `Show` variant + dispatch + shared render helpers**

In `crates/agentprof-cli/src/cmd/config.rs`, add to `enum ConfigAction` (after `Path`):

```rust
    /// Show the effective configuration (built-in defaults merged with
    /// file overrides), annotating each value's source.
    Show,
```

Add the dispatch arm in `run`:

```rust
        ConfigAction::Show => run_show(),
```

Add these helpers + `run_show` + `render_storage` to the module:

```rust
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

    let path = crate::config::resolve_config_path();
    let (partial, marker) = match path.as_ref() {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(src) => {
                let cfg = agentprof_cli::config::parse_toml(&src).map_err(|e| {
                    ExitKind::DataError.into_anyhow(format!(
                        "failed to parse config file {}: {e}",
                        p.display()
                    ))
                })?;
                (cfg, "[exists]")
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (PartialConfig::default(), "[not found]")
            }
            Err(e) => {
                return Err(ExitKind::OutputError.into_anyhow(format!(
                    "failed to read config file {}: {e}",
                    p.display()
                )));
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
fn render_storage(
    s: agentprof_storage::config::PartialStorageConfig,
) -> anyhow::Result<()> {
    // Capture source flags before the partial is moved into the resolver.
    let (mode_f, path_f, prune_f) =
        (s.mode.is_some(), s.path.is_some(), s.auto_prune_days.is_some());
    let r = agentprof_cli::config::resolve_storage_config(s, None).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("invalid [storage] config: {e}"))
    })?;
    // StorageMode is #[non_exhaustive]; Debug->lowercase matches the TOML
    // `rename_all = "lowercase"` representation without a brittle match.
    let mode = format!("{:?}", r.mode).to_lowercase();
    println!("[storage]");
    show_line("mode", &format!("\"{mode}\""), mode_f);
    show_line("path", &format!("\"{}\"", r.path.display()), path_f);
    show_line("auto_prune_days", &r.auto_prune_days.to_string(), prune_f);
    Ok(())
}
```

- [ ] **Step 4: Run storage-only tests to verify they pass**

Run: `cargo test -p agentprof-cli --test cli_config show_`
Expected: PASS (the three show tests; otlp/serve blocks already render via
the `#[cfg]` branches above — with `--all-features` they call the renderers
added next, so build them now).

- [ ] **Step 5: Add the feature-gated `render_otlp` + `render_serve`**

In `crates/agentprof-cli/src/cmd/config.rs`, add:

```rust
#[cfg(feature = "otlp")]
fn opt_addr(a: Option<std::net::SocketAddr>) -> String {
    a.map_or_else(|| "\"\" (disabled)".to_string(), |s| format!("\"{s}\""))
}

#[cfg(feature = "otlp")]
fn opt_path(p: Option<&std::path::Path>) -> String {
    p.map_or_else(|| "(unset)".to_string(), |p| format!("\"{}\"", p.display()))
}

/// Render the `[otlp]` block via `OtlpServerConfig::from_partial`
/// (reused → no default drift). `None` partial ⇒ all built-in defaults.
#[cfg(feature = "otlp")]
fn render_otlp(
    partial: Option<agentprof_storage::otlp::config::PartialOtlpServerConfig>,
) -> anyhow::Result<()> {
    use agentprof_storage::otlp::config::OtlpServerConfig;
    let p = partial.unwrap_or_default();
    // Capture source flags before `from_partial` consumes `p`.
    let (f_grpc, f_http, f_token) =
        (p.listen_grpc.is_some(), p.listen_http.is_some(), p.listen_token.is_some());
    let (f_cert, f_key, f_ca) =
        (p.tls_cert.is_some(), p.tls_key.is_some(), p.tls_client_ca.is_some());
    let (f_idle, f_grace) =
        (p.session_idle_timeout.is_some(), p.shutdown_grace.is_some());
    let (f_logs, f_metrics, f_traces, f_sessions) = (
        p.max_logs_request_bytes.is_some(),
        p.max_metrics_request_bytes.is_some(),
        p.max_traces_request_bytes.is_some(),
        p.max_open_sessions.is_some(),
    );
    let c = OtlpServerConfig::from_partial(p).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("invalid [otlp] config: {e}"))
    })?;
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
    show_line("max_logs_request_bytes", &c.max_logs_request_bytes.to_string(), f_logs);
    show_line(
        "max_metrics_request_bytes",
        &c.max_metrics_request_bytes.to_string(),
        f_metrics,
    );
    show_line("max_traces_request_bytes", &c.max_traces_request_bytes.to_string(), f_traces);
    show_line("max_open_sessions", &c.max_open_sessions.to_string(), f_sessions);
    Ok(())
}

/// Render the `[serve]` block. No pub partial-only resolver exists, so the
/// 3 defaults are inlined (mirror `serve/mod.rs:240/258/269`).
#[cfg(feature = "web")]
fn render_serve(partial: Option<agentprof_cli::config::PartialServeConfig>) {
    let p = partial.unwrap_or_default();
    println!("\n[serve]");
    let bind = p.bind.clone().unwrap_or_else(|| "127.0.0.1:4329".to_string());
    show_line("bind", &format!("\"{bind}\""), p.bind.is_some());
    show_line(
        "interval_default",
        &p.interval_default.unwrap_or(5).to_string(),
        p.interval_default.is_some(),
    );
    show_line("auto_open", &p.auto_open.unwrap_or(true).to_string(), p.auto_open.is_some());
}
```

> Requires `PartialOtlpServerConfig: Default` and `PartialServeConfig:
> Default` — both derive `Default` (config.rs:117 for serve; otlp partial
> in `agentprof-storage/src/otlp/config.rs:219`). Confirm and adjust if a
> field lacks a default.

- [ ] **Step 6: Run the full gate**

Run: `cargo test -p agentprof-cli --all-features --test cli_config`
Expected: PASS (all show + path tests).
Run: `cargo clippy -p agentprof-cli --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/agentprof-cli/src/cmd/config.rs crates/agentprof-cli/tests/cli_config.rs
git commit -m "feat(cli): config show (effective config + source annotation) (L-4 T2)"
```

---

## Task 3: `config init` — write a commented default template

**Files:**
- Modify: `crates/agentprof-cli/src/cmd/config.rs` (add `Init` variant + `run_init` + `CONFIG_TEMPLATE`)
- Modify: `crates/agentprof-cli/tests/cli_config.rs` (add init tests)

**Design:** The template leaves only `[storage]` un-commented (it is the
always-available block); `[otlp]`/`[serve]` are commented out so the file
parses in any build (un-commenting an `[otlp]` block in a non-`otlp` build
would hit `deny_unknown_fields`). The freshly written file must round-trip
through `config show` (a test asserts this).

- [ ] **Step 1: Write the failing tests**

Append to `crates/agentprof-cli/tests/cli_config.rs`:

```rust
#[test]
fn init_writes_template_and_creates_parent() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("nested").join("config.toml"); // parent absent
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "init"])
        .assert()
        .success();
    assert!(cfg.exists());
    // The template must itself parse cleanly via `show`.
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success();
}

#[test]
fn init_refuses_existing_without_force() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "init"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn init_force_overwrites() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    std::fs::write(&cfg, "[storage]\nmode = \"store\"\n").unwrap();
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .args(["config", "init", "--force"])
        .assert()
        .success();
    let written = std::fs::read_to_string(&cfg).unwrap();
    assert!(written.contains("agentprof configuration"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p agentprof-cli --test cli_config init_`
Expected: FAIL (no `init` subcommand).

- [ ] **Step 3: Add the `Init` variant + template + `run_init`**

In `enum ConfigAction` (after `Show`):

```rust
    /// Write a commented default config template to the resolved path.
    Init {
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
```

Dispatch arm in `run`:

```rust
        ConfigAction::Init { force } => run_init(force),
```

Add the template + impl:

```rust
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

/// Write [`CONFIG_TEMPLATE`] to the resolved path, creating parent dirs.
///
/// # Errors
///
/// [`ExitKind::UserError`] when the file exists and `force` is false;
/// [`ExitKind::OutputError`] on a missing config dir or write failure.
fn run_init(force: bool) -> anyhow::Result<()> {
    let path = crate::config::resolve_config_path().ok_or_else(|| {
        ExitKind::OutputError
            .into_anyhow("cannot determine config directory".to_string())
    })?;
    if path.exists() && !force {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "config file already exists at {}; pass --force to overwrite",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!(
                "failed to create config directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(&path, CONFIG_TEMPLATE).map_err(|e| {
        ExitKind::OutputError.into_anyhow(format!(
            "failed to write config file {}: {e}",
            path.display()
        ))
    })?;
    println!("wrote default config to {}", path.display());
    Ok(())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p agentprof-cli --all-features --test cli_config init_`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/agentprof-cli/src/cmd/config.rs crates/agentprof-cli/tests/cli_config.rs
git commit -m "feat(cli): config init (commented default template) (L-4 T3)"
```

---

## Task 4: `config edit` — open in `$VISUAL`/`$EDITOR`

**Files:**
- Modify: `crates/agentprof-cli/src/cmd/config.rs` (add `Edit` variant + `run_edit`)
- Modify: `crates/agentprof-cli/tests/cli_config.rs` (add edit tests)

- [ ] **Step 1: Write the failing tests**

Append to `crates/agentprof-cli/tests/cli_config.rs`:

```rust
#[test]
fn edit_creates_template_when_absent() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env("EDITOR", "true") // unix no-op editor, exits 0
        .env_remove("VISUAL")
        .args(["config", "edit"])
        .assert()
        .success();
    assert!(cfg.exists()); // template written before launching the editor
}

#[test]
fn edit_without_editor_env_errors() {
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.toml");
    bin()
        .env("AGENTPROF_CONFIG", &cfg)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(["config", "edit"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("set $VISUAL or $EDITOR"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p agentprof-cli --test cli_config edit_`
Expected: FAIL (no `edit` subcommand).

- [ ] **Step 3: Add the `Edit` variant + `run_edit`**

In `enum ConfigAction` (after `Init`):

```rust
    /// Open the config file in `$VISUAL`/`$EDITOR` (creating it first if absent).
    Edit,
```

Dispatch arm in `run`:

```rust
        ConfigAction::Edit => run_edit(),
```

Add the impl:

```rust
/// Open the config file in `$VISUAL` (preferred) or `$EDITOR`. Creates the
/// file from [`CONFIG_TEMPLATE`] first when absent (D-4).
///
/// # Errors
///
/// [`ExitKind::UserError`] when neither editor env var is set or the editor
/// exits non-zero; [`ExitKind::OutputError`] on a missing config dir, a
/// template-write failure, or an editor-spawn failure.
fn run_edit() -> anyhow::Result<()> {
    let editor = std::env::var_os("VISUAL")
        .or_else(|| std::env::var_os("EDITOR"))
        .ok_or_else(|| {
            ExitKind::UserError.into_anyhow(
                "no editor configured: set $VISUAL or $EDITOR".to_string(),
            )
        })?;
    let path = crate::config::resolve_config_path().ok_or_else(|| {
        ExitKind::OutputError
            .into_anyhow("cannot determine config directory".to_string())
    })?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ExitKind::OutputError.into_anyhow(format!(
                    "failed to create config directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(&path, CONFIG_TEMPLATE).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!(
                "failed to write config file {}: {e}",
                path.display()
            ))
        })?;
    }
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| {
            ExitKind::OutputError
                .into_anyhow(format!("failed to launch editor {editor:?}: {e}"))
        })?;
    if !status.success() {
        return Err(ExitKind::UserError
            .into_anyhow(format!("editor exited with status {status}")));
    }
    Ok(())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p agentprof-cli --all-features --test cli_config edit_`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/agentprof-cli/src/cmd/config.rs crates/agentprof-cli/tests/cli_config.rs
git commit -m "feat(cli): config edit (\$VISUAL/\$EDITOR) (L-4 T4)"
```

---

## Task 5: Documentation sync + ADR-0027

**Files (all docs — no `#[test]`, but the doc gate must pass):**
- Create: `docs/internals/adr-0027-config-subcommand.md`
- Modify: `docs/architecture.md` (§8 CLI table + §8 command block + §10 schema)
- Modify: `crates/agentprof-cli/README.md` (subcommand list + `config` section)
- Modify: `README.md` (quickstart mention)
- Modify: `tasks/ROADMAP.md` (L-4 row)
- Modify: `CHANGELOG.md` (`[Unreleased]` feat)
- Modify: `docs/internals/README.md` (ADR index row 0027)

- [ ] **Step 1: Write ADR-0027**

Create `docs/internals/adr-0027-config-subcommand.md` following the format
of `docs/internals/adr-0026-report-redaction.md` (Status / Context /
Decision / Consequences). Record these decisions (from the spec §3):

- **D-1** `config show` prints *effective* values (defaults merged with
  file overrides) with `(default)`/`(from file)` source annotation —
  chosen over raw `cat` and normalized re-emit. Reuses the real resolvers
  (`resolve_storage_config`, `OtlpServerConfig::from_partial`) so displayed
  defaults cannot drift; only `serve`'s 3 defaults are inlined (no pub
  partial-only resolver).
- **D-2** Unified `resolve_config_path()` — removes the duplicated
  `$AGENTPROF_CONFIG`→XDG lookup in `ingest-otlp` + `serve`.
- **D-3** Scoped to the wired `storage`/`otlp`/`serve` blocks; the same
  change fixes the architecture §10 schema-vs-`deny_unknown_fields`
  contradiction (paper `[paths]`/`[tokenizer]`/`[pricing]` would parse-fail).
- **D-4** `edit` self-heals (writes the template before launching the editor).
- **D-5** Feature-gated blocks degrade to `(feature not enabled in this build)`.
- **D-6** No `config set` (YAGNI; `edit` covers mutation; `#[non_exhaustive]`
  keeps it addable).

Set its Status to `Accepted`; if any prior ADR mentions a future config
command, leave it (none supersede).

- [ ] **Step 2: Fix architecture.md §8 + §10**

- **§8 CLI table** (`docs/architecture.md` ~L95 / ~L388): change the
  `config` mention from "🚧 规划中（Phase 2）" to shipped, listing
  `config path|show|edit|init`.
- **§8 command block** (~L503-504): replace
  ```
  config  [show | edit | path]                   # 🚧 规划中 — Phase 2
  ```
  with the four actions (`path` / `show` / `edit` / `init [--force]`),
  noting `show` prints effective values with source annotation and that
  `$AGENTPROF_CONFIG` overrides the XDG path.
- **§10 schema** (~L737-783): keep only the **wired** blocks
  (`[storage]` / `[otlp]` / `[serve]`) in the canonical TOML example. Move
  `[paths]`, `[tokenizer]`, `[pricing]` into an explicit note:
  > 🚧 **Planned, not yet wired.** `PartialConfig` uses
  > `deny_unknown_fields`, so these blocks currently cause a parse error —
  > they are reserved for a future milestone (adapter-path / pricing /
  > tokenizer config wiring), not consumed today.

  Add a one-line pointer: "Manage this file with `agentprof config`."

- [ ] **Step 3: Update cli README + root README**

- `crates/agentprof-cli/README.md`: add `config` to the subcommand list and
  a short section documenting `path` / `show` (effective + source) / `edit`
  / `init [--force]`, the `$AGENTPROF_CONFIG` override, and the exit codes
  (§5 of the spec).
- `README.md` (root): in quickstart, add a line such as
  `agentprof config init && agentprof config show` with one sentence.

- [ ] **Step 4: ROADMAP + CHANGELOG + ADR index**

- `tasks/ROADMAP.md` L-4 row: change `config` from "未实现" to shipped
  (config `path|show|edit|init`), and update the trailing "Phase 2
  (`config` 仅剩此项, 待定)" status to done.
- `CHANGELOG.md` under `[Unreleased]` Added:
  ```
  - `config` subcommand (`path` / `show` / `edit` / `init`) to manage the
    user config file; `show` prints the effective configuration with
    `(default)`/`(from file)` source annotation. Unifies config-path
    resolution across `ingest-otlp` / `serve`. (L-4, ADR-0027)
  ```
- `docs/internals/README.md`: add an ADR-index row for **0027**
  (config subcommand) after 0026.

- [ ] **Step 5: Full verification gate**

Run each; all must pass:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p agentprof-cli --all-features
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace
```
Expected: fmt clean; clippy clean; cli tests green (config: ~11 cases; the
OTLP `e2e_idle_sweeper` flake is unrelated — re-run isolated if it's the
only red); doc clean (ADR links + new rustdoc resolve).

- [ ] **Step 6: Commit**

```bash
git add docs/ crates/agentprof-cli/README.md README.md tasks/ROADMAP.md CHANGELOG.md
git commit -m "docs: config subcommand L1/L2/L3 sync + ADR-0027 (L-4 T5)"
```

---

## Plan Self-Review

**1. Spec coverage** (each spec §2 in-scope item → task):
- `cmd/config.rs` four actions → T1 (`path`) / T2 (`show`) / T3 (`init`) / T4 (`edit`). ✅
- `resolve_config_path()` + dedup → T1 Steps 3,7. ✅
- Starter template → T3 (`CONFIG_TEMPLATE`). ✅
- clap wiring on main enum → T1 Step 5. ✅
- Exit-code mapping (§5) → exercised by tests in T1–T4 (`.code(1)`/`.code(2)`)
  and `ExitKind` usage throughout. ✅
- ADR-0027 → T5 Step 1. ✅
- Doc sync (architecture §8/§10, cli/root README, ROADMAP, CHANGELOG) →
  T5 Steps 2–4. ✅
- Tests (unit + integration, env-isolated) → `cli_config.rs` grown across
  T1–T4. ✅
- Out-of-scope (`set`, `[paths]` wiring, CLI-flag merge in show, secret
  masking) → none implemented. ✅

**2. Placeholder scan:** No "TBD"/"implement later"/"add error handling"
without code. Every code step shows full code; doc steps name exact files +
exact text changes. ✅

**3. Type consistency:**
- `ConfigCmd` / `ConfigAction` (`Path`/`Show`/`Init{force}`/`Edit`) — same
  names in T1–T4. ✅
- `run` dispatch arms match the variants added per task. ✅
- `resolve_config_path` / `parse_toml` / `resolve_storage_config` /
  `OtlpServerConfig::from_partial` / `PartialServeConfig` — signatures match
  the inspected source. ✅
- `ExitKind::{UserError=1, DataError=2, OutputError=3}` used consistently
  with the test `.code(..)` assertions. ✅
- `CONFIG_TEMPLATE` defined in T3, reused in T4 (`run_edit`). ✅

**Implementation notes (verified against source):**
`PartialOtlpServerConfig` (otlp/config.rs:216) and `PartialServeConfig`
(config.rs:117) both derive `Default` ✅ (for `unwrap_or_default()`).
`StorageMode` IS `#[non_exhaustive]` — handled via `Debug`->`to_lowercase()`
in `render_storage` (no `match`). `assert_cmd`/`predicates`/`tempfile` are
in cli dev-deps ✅.
