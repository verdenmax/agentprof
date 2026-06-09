//! Load optional MCP tool-description sidecar files for token-cost computation.
//!
//! Implements [`SidecarLookup`] + [`SidecarToolEntry`] traits from
//! `agentprof_core` so the core stays a leaf (file I/O lives here per
//! ADR-0015 architecture). Introduced in M1.6.6.
//!
//! Format auto-detect by path type (see [`load_sidecar`] doc).
//! See `docs/superpowers/specs/2026-06-08-m1.6.6-token-cost-design.md` §4.4.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use agentprof_core::analyzer::waste::{SidecarLookup, SidecarToolEntry};

/// Errors from [`load_sidecar`] — only fired for explicit-path failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SidecarError {
    /// Path doesn't exist or is unreadable.
    #[error("sidecar path {0} does not exist or is unreadable")]
    NotFound(std::path::PathBuf),

    /// File parse failed (invalid JSON or unrecognized shape).
    #[error("sidecar parse failed for {path}: {source}")]
    Parse {
        /// Path of the offending file.
        path: std::path::PathBuf,
        /// Underlying `serde_json` error.
        #[source]
        source: serde_json::Error,
    },

    /// I/O error while reading.
    #[error("io error reading sidecar {path}: {source}")]
    Io {
        /// Path of the offending file.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Loaded MCP tool-description sidecar. Key = `mcp__<server>__<tool>` full name.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Sidecar {
    by_full_name: BTreeMap<String, ToolEntry>,
}

impl Sidecar {
    /// Look up a [`ToolEntry`] by full name (`mcp__<server>__<tool>`).
    ///
    /// Returns the concrete entry type. Use [`SidecarLookup::lookup`] for the
    /// trait-object form that core analyzer code consumes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use agentprof_adapters::copilot::tool_sidecar::Sidecar;
    /// fn use_sidecar(s: &Sidecar) {
    ///     let _ = s.lookup_concrete("mcp__github__search");
    /// }
    /// ```
    #[must_use]
    pub fn lookup_concrete(&self, full_name: &str) -> Option<&ToolEntry> {
        self.by_full_name.get(full_name)
    }

    /// Number of entries loaded.
    ///
    /// # Examples
    ///
    /// ```
    /// # use agentprof_adapters::copilot::tool_sidecar::Sidecar;
    /// fn count(s: &Sidecar) -> usize { s.len() }
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_full_name.len()
    }

    /// `true` when no entries are loaded.
    ///
    /// # Examples
    ///
    /// ```
    /// # use agentprof_adapters::copilot::tool_sidecar::Sidecar;
    /// fn check(s: &Sidecar) -> bool { s.is_empty() }
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_full_name.is_empty()
    }
}

impl SidecarLookup for Sidecar {
    fn lookup(&self, full_name: &str) -> Option<&dyn SidecarToolEntry> {
        self.by_full_name
            .get(full_name)
            .map(|e| e as &dyn SidecarToolEntry)
    }
}

/// One entry in the sidecar — mirror of the MCP `tools/list` response shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolEntry {
    /// Tool name (without the `mcp__<server>__` prefix).
    pub name: String,

    /// Optional human-readable description (counts toward token cost).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional JSON schema for the tool's input arguments.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "inputSchema"
    )]
    pub input_schema: Option<serde_json::Value>,

    /// Optional MCP annotations object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
}

impl SidecarToolEntry for ToolEntry {
    fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Load a sidecar from `path`. File → global JSON; dir → per-server scan.
/// See ADR-0016 D-2 for the format details.
///
/// # Errors
///
/// Returns [`SidecarError`] when the path doesn't exist or any file fails
/// to parse. Per-file parse failures in DIR mode emit a `tracing::warn!`
/// but do NOT abort the load — they're skipped and the rest of the dir
/// loads successfully.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agentprof_adapters::copilot::tool_sidecar::load_sidecar;
/// let s = load_sidecar(Path::new("/tmp/sidecar.json"));
/// match s {
///     Ok(s) => println!("loaded {} tools", s.len()),
///     Err(e) => eprintln!("sidecar load failed: {e}"),
/// }
/// ```
#[tracing::instrument(name = "adapter.tool_sidecar", skip_all, fields(path = %path.display()))]
pub fn load_sidecar(path: &Path) -> Result<Sidecar, SidecarError> {
    // Audit B3: pre-fix this used `.map_err(|_| NotFound(...))` which
    // collapsed every I/O failure (permission denied, stale NFS handle,
    // I/O error on an unreadable mount) into a `NotFound` masquerade,
    // hiding the real cause from operators. Discriminate explicitly so
    // ENOENT keeps the user-friendly NotFound variant while anything
    // else preserves the underlying `io::Error` via `SidecarError::Io`.
    let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => SidecarError::NotFound(path.to_path_buf()),
        _ => SidecarError::Io {
            path: path.to_path_buf(),
            source: e,
        },
    })?;

    if metadata.is_file() {
        load_global_file(path)
    } else if metadata.is_dir() {
        load_per_server_dir(path)
    } else {
        Err(SidecarError::NotFound(path.to_path_buf()))
    }
}

fn load_global_file(path: &Path) -> Result<Sidecar, SidecarError> {
    let bytes = std::fs::read(path).map_err(|e| SidecarError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let map: BTreeMap<String, Vec<ToolEntry>> =
        serde_json::from_slice(&bytes).map_err(|e| SidecarError::Parse {
            path: path.to_path_buf(),
            source: e,
        })?;
    let mut by_full_name = BTreeMap::new();
    for (server, tools) in map {
        for entry in tools {
            let full = format!("mcp__{server}__{}", entry.name);
            by_full_name.insert(full, entry);
        }
    }
    Ok(Sidecar { by_full_name })
}

fn load_per_server_dir(dir: &Path) -> Result<Sidecar, SidecarError> {
    let mut by_full_name = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| SidecarError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let server = match p.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(path = %p.display(), error = %e, "skip unreadable sidecar file");
                continue;
            }
        };
        let tools = match parse_per_server_file(&bytes) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(path = %p.display(), error = %e, "skip malformed sidecar file");
                continue;
            }
        };
        for entry in tools {
            let full = format!("mcp__{server}__{}", entry.name);
            by_full_name.insert(full, entry);
        }
    }
    Ok(Sidecar { by_full_name })
}

fn parse_per_server_file(bytes: &[u8]) -> Result<Vec<ToolEntry>, serde_json::Error> {
    #[derive(Deserialize)]
    struct Wrapped {
        tools: Vec<ToolEntry>,
    }
    if let Ok(w) = serde_json::from_slice::<Wrapped>(bytes) {
        return Ok(w.tools);
    }
    serde_json::from_slice::<Vec<ToolEntry>>(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_sidecar_missing_path_returns_not_found_err() {
        let r = load_sidecar(Path::new("/this/dir/does/not/exist/sidecar.json"));
        assert!(matches!(r, Err(SidecarError::NotFound(_))));
    }

    // Audit B3 regression: a non-NotFound IO failure on metadata() must
    // preserve the underlying io::Error via SidecarError::Io and NOT
    // masquerade as SidecarError::NotFound. Exercised by chmod 000 on a
    // file inside a tempdir then probing it as a non-root caller. On
    // root or filesystems that ignore mode bits (e.g. some CI sandboxes)
    // the metadata() call succeeds — we skip the assertion in that
    // case so the test stays portable, but the Io branch is also
    // covered by manual review.
    //
    // Unix-only: Windows has no PermissionsExt::from_mode; the Io branch
    // is covered there by manual review (and on other platforms by this test).
    #[cfg(unix)]
    #[test]
    fn load_sidecar_permission_denied_returns_io_err_not_not_found() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        // Make a directory with no permissions, then attempt to stat a
        // file inside it. `metadata()` on the inner path traverses the
        // dir and fails with PermissionDenied on the parent lookup.
        let locked_dir = tmp.path().join("locked");
        std::fs::create_dir(&locked_dir).expect("mkdir");
        let inner = locked_dir.join("sidecar.json");
        std::fs::write(&inner, "[]").expect("write");
        std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o000))
            .expect("chmod 000");

        let r = load_sidecar(&inner);
        // Restore permissions so tempdir cleanup works.
        let _ = std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755));

        match r {
            Err(SidecarError::Io { source, .. }) => {
                assert_ne!(
                    source.kind(),
                    std::io::ErrorKind::NotFound,
                    "ENOENT should have produced SidecarError::NotFound, not Io"
                );
            }
            Err(SidecarError::NotFound(_))
                if std::env::var_os("USER").as_deref() == Some(std::ffi::OsStr::new("root")) =>
            {
                // Root bypasses mode bits; the chmod 000 dir is still
                // readable. Accept NotFound for the inner missing file.
            }
            Ok(_) => {
                // Some test harnesses (containerized CI, NFS) ignore
                // mode bits — accept that and rely on review for the
                // Io branch.
            }
            other => panic!("expected Io or skipped, got {other:?}"),
        }
    }

    #[test]
    fn load_sidecar_file_global_json_format() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let p = tmp.path().join("sidecar.json");
        let mut f = std::fs::File::create(&p).expect("create");
        writeln!(
            f,
            r#"{{
            "github": [
                {{"name": "search", "description": "Search issues",
                  "inputSchema": {{"type": "object"}}}}
            ]
        }}"#
        )
        .expect("write");
        let s = load_sidecar(&p).expect("load");
        assert_eq!(s.len(), 1);
        let e = s.lookup_concrete("mcp__github__search").expect("found");
        assert_eq!(e.name, "search");
        assert_eq!(e.description.as_deref(), Some("Search issues"));
    }

    #[test]
    fn load_sidecar_dir_per_server_format_tools_wrapper() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let github = tmp.path().join("github.json");
        std::fs::write(
            &github,
            r#"{
            "tools": [
                {"name": "create_issue", "description": "Create issue",
                 "inputSchema": {"type": "object"}}
            ]
        }"#,
        )
        .expect("write");
        let s = load_sidecar(tmp.path()).expect("load");
        assert_eq!(s.len(), 1);
        assert!(s.lookup_concrete("mcp__github__create_issue").is_some());
    }

    #[test]
    fn load_sidecar_dir_per_server_format_bare_array() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fs = tmp.path().join("filesystem.json");
        std::fs::write(
            &fs,
            r#"[
            {"name": "read_file", "description": "Read a file",
             "inputSchema": {}}
        ]"#,
        )
        .expect("write");
        let s = load_sidecar(tmp.path()).expect("load");
        assert_eq!(s.len(), 1);
        assert!(s.lookup_concrete("mcp__filesystem__read_file").is_some());
    }

    #[test]
    fn load_sidecar_dir_mixed_shapes_tolerated() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("a.json"), r#"{"tools": [{"name": "t1"}]}"#).expect("a");
        std::fs::write(tmp.path().join("b.json"), r#"[{"name": "t2"}]"#).expect("b");
        let s = load_sidecar(tmp.path()).expect("load");
        assert_eq!(s.len(), 2);
        assert!(s.lookup_concrete("mcp__a__t1").is_some());
        assert!(s.lookup_concrete("mcp__b__t2").is_some());
    }

    #[test]
    fn load_sidecar_dir_skip_non_json_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("github.json"),
            r#"{"tools": [{"name": "x"}]}"#,
        )
        .expect("write");
        std::fs::write(tmp.path().join("README.md"), "ignore me").expect("ignored");
        let s = load_sidecar(tmp.path()).expect("load");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn load_sidecar_file_malformed_json_returns_parse_err() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let p = tmp.path().join("bad.json");
        std::fs::write(&p, "{ not valid json").expect("write");
        let r = load_sidecar(&p);
        assert!(matches!(r, Err(SidecarError::Parse { .. })));
    }
}
