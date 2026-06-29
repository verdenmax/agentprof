//! CLI configuration file parsing.
//!
//! `agentprof` reads an optional TOML config file (path resolved by
//! [`resolve_config_path`]) describing user defaults: log level, storage
//! path, etc.
//! This module defines the top-level [`PartialConfig`] wire-format struct
//! plus the helper [`resolve_storage_config`] that merges a parsed
//! `[storage]` section with command-line overrides into a fully-resolved
//! [`StorageConfig`].
//!
//! See `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`
//! §3 for how the CLI layer composes adapter + storage. The `[storage]`
//! section deserializes into [`PartialStorageConfig`] verbatim — schema
//! and defaults live in `agentprof-storage` so the CLI never owns
//! storage policy.
//!
//! ## `[otlp]` block (feature `otlp`)
//!
//! When the `otlp` feature is enabled an additional `[otlp]` section is
//! recognized and deserialized into
//! [`agentprof_storage::otlp::config::PartialOtlpServerConfig`] verbatim.
//! Merge order (highest priority first), applied by
//! `agentprof ingest-otlp`:
//!
//! 1. CLI flags (`--grpc`, `--http`, `--bearer-token`, …).
//! 2. The `AGENTPROF_OTLP_TOKEN` environment variable (folded into
//!    `--bearer-token` by `clap`).
//! 3. The `[otlp]` config-file block.
//! 4. The built-in defaults documented on [`OtlpServerConfig::default`].
//!
//! See `docs/superpowers/specs/2026-06-10-m2.2-otlp-receiver-design.md`
//! §§8–9 for the canonical specification.
//!
//! [`OtlpServerConfig::default`]:
//!     agentprof_storage::otlp::config::OtlpServerConfig::default
//!
//! # Examples
//!
//! ```
//! use agentprof_cli::config::{parse_toml, resolve_storage_config};
//!
//! let cfg = parse_toml("[storage]\nmode = \"cache\"\n").unwrap();
//! let resolved = resolve_storage_config(cfg.storage, None).unwrap();
//! assert!(resolved.path.ends_with("cache.sqlite"));
//! ```

use std::path::PathBuf;

use agentprof_storage::config::{PartialStorageConfig, StorageConfig};
use agentprof_storage::error::SqliteError;
use serde::Deserialize;
use thiserror::Error;

/// Top-level TOML config wire format.
///
/// Every section is optional; omitted sections fall back to the section's
/// own `Default` impl. New sections (e.g. `[tracing]`, `[ui]`) will land
/// here in future milestones — `#[non_exhaustive]` keeps that additive.
///
/// # Examples
///
/// ```
/// use agentprof_cli::config::PartialConfig;
/// let cfg: PartialConfig = toml::from_str("").unwrap();
/// assert!(cfg.storage.path.is_none());
/// ```
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct PartialConfig {
    /// `[storage]` section — see [`PartialStorageConfig`].
    pub storage: PartialStorageConfig,

    /// `[otlp]` section — see
    /// [`agentprof_storage::otlp::config::PartialOtlpServerConfig`].
    ///
    /// `None` means the section was absent from the TOML file; the
    /// `ingest-otlp` builder then falls back to built-in defaults
    /// (possibly overridden by CLI flags).
    #[cfg(feature = "otlp")]
    pub otlp: Option<agentprof_storage::otlp::config::PartialOtlpServerConfig>,

    /// `[serve]` section — see [`PartialServeConfig`] (M2.3, `web`
    /// feature).
    ///
    /// `None` means the section was absent from the TOML file; the
    /// `serve` command's resolver then falls back to built-in defaults
    /// (`bind = "127.0.0.1:4329"`, `interval_default = 5`,
    /// `auto_open = true`), possibly overridden by CLI flags.
    #[cfg(feature = "web")]
    pub serve: Option<PartialServeConfig>,
}

/// `[serve]` config-file block (M2.3) — all fields optional.
///
/// Resolved into a complete runtime config by
/// `crate::cmd::serve::resolve_serve_config` with priority
/// **CLI flag > `[serve]` file block > built-in default**.
///
/// Mirrors the [`agentprof_storage::otlp::config::PartialOtlpServerConfig`]
/// pattern shipped in M2.2 T8.2 so the user-facing TOML stays uniform
/// across subcommands.
///
/// # Examples
///
/// ```
/// use agentprof_cli::config::{parse_toml, PartialServeConfig};
/// let cfg = parse_toml(
///     "[serve]\nbind = \"0.0.0.0:9000\"\ninterval_default = 10\n",
/// )
/// .expect("valid toml");
/// let serve: PartialServeConfig = cfg.serve.expect("serve section present");
/// assert_eq!(serve.bind.as_deref(), Some("0.0.0.0:9000"));
/// assert_eq!(serve.interval_default, Some(10));
/// assert_eq!(serve.auto_open, None);
/// ```
#[cfg(feature = "web")]
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct PartialServeConfig {
    /// Bind address as a string (e.g. `"127.0.0.1:4329"`).
    ///
    /// Parsed into a [`std::net::SocketAddr`] by the resolver; a
    /// malformed value surfaces as a `UserError` at command start, not
    /// at config-load time, so the rest of the file can still be read.
    pub bind: Option<String>,

    /// Browser-side default poll interval in seconds. Allowed range
    /// `1..=60`; out-of-range values are rejected by the resolver.
    pub interval_default: Option<u8>,

    /// Whether to open the user's browser on start. When omitted, the
    /// resolver defaults to `true`. The CLI `--no-open` flag forces
    /// `false` regardless of this setting.
    pub auto_open: Option<bool>,
}

/// Errors raised while loading / merging the CLI config.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The TOML text failed to parse.
    #[error("failed to parse agentprof config: {0}")]
    Toml(#[from] toml::de::Error),
    /// The `[storage]` section was syntactically valid but semantically
    /// rejected by `agentprof-storage`.
    #[error("invalid [storage] config: {0}")]
    Storage(#[from] SqliteError),
}

/// Parse a TOML config blob into a [`PartialConfig`].
///
/// Thin convenience wrapper around `toml::from_str` that returns the
/// crate-local [`ConfigError`] so callers can mix parse + resolve errors
/// in a single `?` chain.
///
/// # Errors
///
/// Returns [`ConfigError::Toml`] when the input is not valid TOML or
/// contains unknown keys (the underlying structs all use
/// `deny_unknown_fields`).
///
/// # Examples
///
/// ```
/// use agentprof_cli::config::parse_toml;
/// let cfg = parse_toml("[storage]\nauto_prune_days = 7\n").unwrap();
/// assert_eq!(cfg.storage.auto_prune_days, Some(7));
/// ```
pub fn parse_toml(src: &str) -> Result<PartialConfig, ConfigError> {
    Ok(toml::from_str(src)?)
}

/// Merge a parsed `[storage]` section with a CLI `--storage-path`
/// override into a fully-resolved [`StorageConfig`].
///
/// Resolution rules:
///
/// 1. Start from [`StorageConfig::from_partial`] applied to `partial`
///    (which already encodes "unspecified ⇒ default" semantics).
/// 2. If `cli_override_path` is `Some`, replace `.path` — flags always
///    beat config file values, per `docs/architecture.md` §10.
///
/// # Errors
///
/// Forwards any [`SqliteError`] surfaced by
/// [`StorageConfig::from_partial`] (currently unreachable, retained for
/// API stability).
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use agentprof_cli::config::resolve_storage_config;
/// use agentprof_storage::config::PartialStorageConfig;
///
/// let resolved = resolve_storage_config(
///     PartialStorageConfig::default(),
///     Some(PathBuf::from("/tmp/override.sqlite")),
/// ).unwrap();
/// assert_eq!(resolved.path, PathBuf::from("/tmp/override.sqlite"));
/// ```
pub fn resolve_storage_config(
    partial: PartialStorageConfig,
    cli_override_path: Option<PathBuf>,
) -> Result<StorageConfig, ConfigError> {
    let mut cfg = StorageConfig::from_partial(partial)?;
    if let Some(p) = cli_override_path {
        cfg.path = p;
    }
    Ok(cfg)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use agentprof_storage::config::StorageMode;

    #[test]
    fn empty_toml_yields_default_storage() {
        let cfg = parse_toml("").unwrap();
        let resolved = resolve_storage_config(cfg.storage, None).unwrap();
        assert_eq!(resolved.mode, StorageMode::Cache);
        assert_eq!(resolved.auto_prune_days, 30);
    }

    #[test]
    fn storage_section_parsed_and_path_override_wins() {
        let cfg = parse_toml(
            r#"
                [storage]
                mode = "store"
                path = "/from/config.sqlite"
                auto_prune_days = 7
            "#,
        )
        .unwrap();
        assert_eq!(cfg.storage.mode, Some(StorageMode::Store));
        assert_eq!(cfg.storage.auto_prune_days, Some(7));

        let resolved =
            resolve_storage_config(cfg.storage, Some(PathBuf::from("/from/cli.sqlite"))).unwrap();
        assert_eq!(resolved.path, PathBuf::from("/from/cli.sqlite"));
        assert_eq!(resolved.mode, StorageMode::Store);
        assert_eq!(resolved.auto_prune_days, 7);
    }

    #[test]
    fn unknown_top_level_section_rejected() {
        let err = parse_toml("[nope]\nfoo = 1\n").unwrap_err();
        assert!(matches!(err, ConfigError::Toml(_)));
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn otlp_section_round_trips_into_partial() {
        let cfg = parse_toml(
            r#"
                [otlp]
                listen_grpc = "0.0.0.0:9317"
                listen_token = "shared-secret"
                session_idle_timeout = "10m"
            "#,
        )
        .unwrap();
        let otlp = cfg.otlp.expect("otlp section present");
        assert_eq!(otlp.listen_grpc.as_deref(), Some("0.0.0.0:9317"));
        assert_eq!(otlp.listen_token.as_deref(), Some("shared-secret"));
        assert_eq!(otlp.session_idle_timeout.as_deref(), Some("10m"));
    }

    #[cfg(feature = "otlp")]
    #[test]
    fn missing_otlp_section_yields_none() {
        let cfg = parse_toml("[storage]\nauto_prune_days = 7\n").unwrap();
        assert!(cfg.otlp.is_none());
    }
}
