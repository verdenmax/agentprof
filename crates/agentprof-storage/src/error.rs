//! Error type for the `agentprof-storage` crate.
//!
//! All public, fallible APIs in this crate return [`SqliteError`]. The variant
//! set is `#[non_exhaustive]` so additional kinds (e.g. `Otlp`) can be added
//! without breaking downstream matchers.
//!
//! Per the workspace error-model rule (`docs/architecture.md` §16 / iron rule
//! #1), this crate is a library and therefore uses [`thiserror`] exclusively —
//! `anyhow` is forbidden here. Binary crates (`agentprof-cli`) are free to
//! convert these into `anyhow::Error` at their boundary.
//!
//! # Examples
//!
//! ```
//! use agentprof_storage::SqliteError;
//!
//! fn describe(err: &SqliteError) -> String {
//!     err.to_string()
//! }
//! # let e = SqliteError::ConfigPath { kind: "cache", message: "unset".into() };
//! # assert!(describe(&e).contains("cache"));
//! ```

use std::path::PathBuf;

/// Error returned by `agentprof-storage` APIs.
///
/// This enum is `#[non_exhaustive]`: callers must use a wildcard arm in
/// `match` to remain forward compatible.
///
/// # Examples
///
/// ```
/// use agentprof_storage::SqliteError;
///
/// let err = SqliteError::ConfigPath {
///     kind: "cache",
///     message: "XDG_CACHE_HOME points at a non-UTF-8 path".into(),
/// };
/// assert!(err.to_string().contains("cache"));
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SqliteError {
    /// A low-level `rusqlite` call failed.
    ///
    /// `context` carries a short human-readable description of *what the
    /// caller was trying to do* (e.g. `"opening cache database"`), and
    /// `source` is the original [`rusqlite::Error`].
    #[error("sqlite error ({context}): {source}")]
    Rusqlite {
        /// Short description of the operation that failed.
        context: String,
        /// Underlying `rusqlite` error.
        #[source]
        source: rusqlite::Error,
    },

    /// A schema migration failed.
    ///
    /// Wraps [`rusqlite_migration::Error`] via `#[from]` so call sites can use
    /// `?` directly on migration calls.
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    /// An I/O operation related to a database file or its parent directory
    /// failed (e.g. could not create `~/.cache/agentprof`).
    #[error("io error at {path}: {source}")]
    Io {
        /// Path the I/O call was operating on.
        path: PathBuf,
        /// Underlying `std::io::Error`.
        #[source]
        source: std::io::Error,
    },

    /// A configured or computed database path is invalid.
    ///
    /// Typical causes: `XDG_CACHE_HOME` / `XDG_DATA_HOME` is unset and
    /// `$HOME` is also missing, or the resolved path is not usable.
    #[error("invalid {kind} path: {message}")]
    ConfigPath {
        /// Kind of path that was being resolved (`"cache"` / `"store"` / …).
        kind: &'static str,
        /// Human-readable explanation.
        message: String,
    },

    /// Deserialization of a stored or configured value failed.
    #[error("serde error ({context}): {source}")]
    Serde {
        /// Short description of what was being (de)serialized.
        context: String,
        /// Underlying `serde_json` error.
        #[source]
        source: serde_json::Error,
    },
}
