//! Filesystem discovery for Copilot CLI session logs.
//!
//! Copilot CLI stores each session as a UUID-named directory under
//! `$HOME/.copilot/session-state/`, with the canonical event log at
//! `events.jsonl`. A live session additionally writes a sentinel file matching
//! `inuse.<pid>.lock` into that same directory.
//!
//! # Examples
//!
//! ```no_run
//! use agentprof_adapters::copilot::paths;
//!
//! if let Some(root) = paths::default_session_root() {
//!     let sessions = paths::discover_sessions(&root).unwrap();
//!     println!("discovered {} sessions", sessions.len());
//! }
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use agentprof_core::adapter::{AdapterError, AgentKind, SessionRef};

/// Default on-disk location where Copilot CLI persists sessions.
///
/// Resolves to `$HOME/.copilot/session-state`. Returns [`None`] when the
/// `HOME` environment variable is unset (uncommon — e.g. minimal CI
/// containers).
///
/// # Examples
///
/// ```
/// use agentprof_adapters::copilot::paths::default_session_root;
///
/// // On a typical Unix host this resolves to `$HOME/.copilot/session-state`.
/// let _maybe_root = default_session_root();
/// ```
#[must_use]
pub fn default_session_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".copilot").join("session-state"))
}

/// Enumerate every Copilot session under `root`.
///
/// For each direct subdirectory of `root` that contains an `events.jsonl`
/// file, a [`SessionRef`] is produced. Results are sorted by
/// [`SessionRef::modified_at`] in descending order (newest first).
///
/// Subdirectories without `events.jsonl`, and individual entries that fail
/// to stat, are silently skipped — discovery should never collapse on a
/// single bad session.
///
/// `is_live` is set to `true` when the session directory also contains a
/// file matching the `inuse.*.lock` sentinel pattern Copilot CLI writes
/// while a session is being recorded.
///
/// # Errors
///
/// - [`AdapterError::RootNotFound`] when `root` does not exist or is not
///   a directory.
/// - [`AdapterError::Io`] when `root` cannot be read.
///
/// # Examples
///
/// ```no_run
/// use agentprof_adapters::copilot::paths::discover_sessions;
/// use std::path::Path;
///
/// let sessions = discover_sessions(Path::new("/home/me/.copilot/session-state")).unwrap();
/// for s in sessions {
///     println!("{} ({} bytes, live={})", s.id, s.size_bytes, s.is_live);
/// }
/// ```
#[tracing::instrument(
    name = "adapter.discover",
    skip_all,
    fields(root = %agentprof_core::observability::pii::hash_path(root))
)]
pub fn discover_sessions(root: &Path) -> Result<Vec<SessionRef>, AdapterError> {
    if !root.is_dir() {
        return Err(AdapterError::RootNotFound {
            path: root.to_path_buf(),
        });
    }

    let entries = fs::read_dir(root).map_err(|source| AdapterError::Io {
        path: root.to_path_buf(),
        source,
    })?;

    let mut sessions = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let dir_path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let events_path = dir_path.join("events.jsonl");
        let Ok(meta) = fs::metadata(&events_path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let modified_at = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let id = dir_path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or_else(|| dir_path.to_string_lossy().into_owned(), str::to_owned);

        sessions.push(SessionRef::new(
            id,
            AgentKind::Copilot,
            events_path,
            modified_at,
            meta.len(),
            has_inuse_lock(&dir_path),
        ));
    }

    sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    tracing::debug!(found = sessions.len(), "discovered sessions");
    Ok(sessions)
}

/// Return `true` if `session_dir` contains any file matching `inuse.*.lock`.
///
/// Read errors are swallowed and treated as "no lock present" — discovery
/// must not abort on a transient permission failure.
fn has_inuse_lock(session_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(session_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let name_os = entry.file_name();
        if let Some(name) = name_os.to_str() {
            if name.starts_with("inuse.")
                && Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"))
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_session_root_returns_path_under_home_when_set() {
        // Don't mutate process env — just verify that when HOME is set
        // (the normal case in test runners), the path ends with the
        // expected suffix.
        if std::env::var_os("HOME").is_some() {
            let root = default_session_root().unwrap();
            assert!(
                root.ends_with("session-state"),
                "expected suffix `.copilot/session-state`, got {root:?}"
            );
            assert!(root.components().any(|c| c.as_os_str() == ".copilot"));
        }
    }

    #[test]
    fn has_inuse_lock_detects_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!has_inuse_lock(tmp.path()));
        fs::write(tmp.path().join("inuse.42.lock"), "").unwrap();
        assert!(has_inuse_lock(tmp.path()));
    }
}
