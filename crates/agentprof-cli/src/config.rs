//! CLI configuration file parsing.
//!
//! `agentprof` reads an optional TOML config file (resolution lives in a
//! later task) describing user defaults: log level, storage path, etc.
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
}
