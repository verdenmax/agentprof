//! Storage configuration: hybrid `cache` vs `store` mode + path resolution.
//!
//! agentprof persists session-analysis state in `SQLite`. There are two
//! orthogonal use cases:
//!
//! - **Cache** (default): ephemeral, regenerable acceleration data; lives
//!   under `$XDG_CACHE_HOME` so distro cache-cleaners may safely wipe it.
//! - **Store** (opt-in): user-owned long-lived data (annotated runs, custom
//!   tags); lives under `$XDG_DATA_HOME`.
//!
//! See `docs/internals/adr-0018-storage-hybrid.md` for the design rationale.
//!
//! # Examples
//!
//! ```
//! use agentprof_storage::config::{StorageConfig, StorageMode};
//!
//! let cfg = StorageConfig::default();
//! assert_eq!(cfg.mode, StorageMode::Cache);
//! assert_eq!(cfg.auto_prune_days, 30);
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::SqliteError;

/// Which on-disk role the `SQLite` database plays.
///
/// `#[non_exhaustive]` so future modes (e.g. `Memory` for tests) can be added
/// without breaking exhaustive `match`es in downstream crates.
///
/// # Examples
///
/// ```
/// use agentprof_storage::config::StorageMode;
/// assert_eq!(StorageMode::default(), StorageMode::Cache);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum StorageMode {
    /// Ephemeral cache under `$XDG_CACHE_HOME/agentprof/cache.sqlite`.
    #[default]
    Cache,
    /// Durable user store under `$XDG_DATA_HOME/agentprof/store.sqlite`.
    Store,
}

impl StorageMode {
    /// XDG env var that takes precedence for this mode.
    const fn xdg_env(self) -> &'static str {
        match self {
            Self::Cache => "XDG_CACHE_HOME",
            Self::Store => "XDG_DATA_HOME",
        }
    }

    /// Fallback subdirectory under `$HOME` when the XDG var is unset.
    const fn home_fallback_subdir(self) -> &'static str {
        match self {
            Self::Cache => ".cache",
            Self::Store => ".local/share",
        }
    }

    /// Final filename within `<resolved-root>/agentprof/`.
    const fn filename(self) -> &'static str {
        match self {
            Self::Cache => "cache.sqlite",
            Self::Store => "store.sqlite",
        }
    }
}

/// Fully-resolved storage configuration.
///
/// Construct with [`StorageConfig::default`] for a sane out-of-the-box config,
/// or with [`StorageConfig::from_partial`] when merging a TOML config file.
///
/// `#[non_exhaustive]`: add fields without breaking downstream initializers.
///
/// # Examples
///
/// ```
/// use agentprof_storage::config::StorageConfig;
/// let cfg = StorageConfig::default();
/// assert!(cfg.path.ends_with("cache.sqlite"));
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StorageConfig {
    /// Cache vs store role.
    pub mode: StorageMode,
    /// Absolute path to the `SQLite` file.
    pub path: PathBuf,
    /// Rows older than this many days are eligible for auto-pruning.
    ///
    /// `0` disables auto-pruning (T2.7+ behavior).
    pub auto_prune_days: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let mode = StorageMode::default();
        Self {
            mode,
            path: Self::default_path_for(mode),
            auto_prune_days: 30,
        }
    }
}

impl StorageConfig {
    /// Compute the default on-disk path for `mode`, honouring XDG env vars.
    ///
    /// Resolution order:
    ///
    /// 1. `$XDG_CACHE_HOME` / `$XDG_DATA_HOME`, when set and non-empty.
    /// 2. `dirs::home_dir() + ".cache"` / `".local/share"`.
    /// 3. Bare `agentprof.sqlite` in the current directory (last-resort
    ///    fallback; callers should treat this as a degraded mode).
    ///
    /// The final segments are always `agentprof/<cache|store>.sqlite`.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::config::{StorageConfig, StorageMode};
    /// let p = StorageConfig::default_path_for(StorageMode::Cache);
    /// assert!(p.to_string_lossy().contains("agentprof"));
    /// ```
    #[must_use]
    pub fn default_path_for(mode: StorageMode) -> PathBuf {
        if let Some(root) = std::env::var_os(mode.xdg_env()) {
            if !root.is_empty() {
                return Path::new(&root).join("agentprof").join(mode.filename());
            }
        }
        if let Some(home) = dirs::home_dir() {
            return home
                .join(mode.home_fallback_subdir())
                .join("agentprof")
                .join(mode.filename());
        }
        PathBuf::from("agentprof.sqlite")
    }

    /// Build a fully-resolved [`StorageConfig`] from a [`PartialStorageConfig`].
    ///
    /// Unspecified fields fall back to the same defaults [`Self::default`]
    /// uses. The `path` field, when omitted, is computed from the (possibly
    /// defaulted) `mode`.
    ///
    /// # Errors
    ///
    /// Currently never fails, but the signature returns [`SqliteError`] so
    /// that future validation (e.g. rejecting non-UTF-8 paths on Windows)
    /// can be added without a breaking change.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::config::{PartialStorageConfig, StorageConfig, StorageMode};
    /// let merged = StorageConfig::from_partial(PartialStorageConfig::default()).unwrap();
    /// assert_eq!(merged.mode, StorageMode::Cache);
    /// ```
    pub fn from_partial(p: PartialStorageConfig) -> Result<Self, SqliteError> {
        let mode = p.mode.unwrap_or_default();
        let path = p.path.unwrap_or_else(|| Self::default_path_for(mode));
        let auto_prune_days = p.auto_prune_days.unwrap_or(30);
        Ok(Self {
            mode,
            path,
            auto_prune_days,
        })
    }
}

/// Partial / wire-format counterpart of [`StorageConfig`] for TOML config
/// deserialization.
///
/// Every field is `Option<_>` so unspecified keys round-trip cleanly into
/// "use the default". Merge into a [`StorageConfig`] with
/// [`StorageConfig::from_partial`].
///
/// # Examples
///
/// ```
/// use agentprof_storage::config::PartialStorageConfig;
/// let p: PartialStorageConfig = serde_json::from_str("{}").unwrap();
/// assert!(p.mode.is_none() && p.path.is_none() && p.auto_prune_days.is_none());
/// ```
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct PartialStorageConfig {
    /// Optional override for [`StorageConfig::mode`].
    pub mode: Option<StorageMode>,
    /// Optional override for [`StorageConfig::path`].
    pub path: Option<PathBuf>,
    /// Optional override for [`StorageConfig::auto_prune_days`].
    pub auto_prune_days: Option<u32>,
}
