//! Shared redaction sidecar writer for `--privacy anonymize`.
//!
//! `anonymize` builds a [`RedactionMap`] mapping each redaction marker
//! (e.g. `<uuid-0>`) back to its original value. To keep the rendered
//! report a clean shareable artifact, that reverse map is written to a
//! sibling `agentprof-redaction-map.json` sidecar instead of being
//! embedded inline. See `docs/features/privacy.md`.

use std::path::{Path, PathBuf};

use agentprof_core::analyzer::redact::RedactionMap;

/// Sidecar path: sibling of `--output`, else CWD.
///
/// # Examples
///
/// ```ignore
/// // agentprof-cli is a bin-only crate, so this doctest is not executed
/// // (no library target to import from). The shape below mirrors how
/// // `cmd::analyze::run` derives the sidecar path.
/// use std::path::Path;
/// use agentprof_cli::cmd::privacy::sidecar_path;
/// let p = sidecar_path(Some(Path::new("/tmp/r.json")));
/// assert!(p.ends_with("agentprof-redaction-map.json"));
/// ```
#[must_use]
pub fn sidecar_path(output: Option<&Path>) -> PathBuf {
    output.map_or_else(
        || PathBuf::from("agentprof-redaction-map.json"),
        |p| {
            p.parent()
                .unwrap_or_else(|| Path::new("."))
                .join("agentprof-redaction-map.json")
        },
    )
}

/// Write the sidecar. Returns the path, or `(path, err)` so the caller can
/// warn + set exit 3 AFTER the report is already emitted.
///
/// # Errors
///
/// Returns the io error paired with the attempted path on write failure.
///
/// # Examples
///
/// ```ignore
/// // bin-only crate: shape only, not executed (see `sidecar_path`).
/// use agentprof_cli::cmd::privacy::write_sidecar;
/// use agentprof_core::analyzer::redact::RedactionMap;
/// let map = RedactionMap::default();
/// let _ = write_sidecar(&map, None);
/// ```
pub fn write_sidecar(
    map: &RedactionMap,
    output: Option<&Path>,
) -> Result<PathBuf, (PathBuf, std::io::Error)> {
    let path = sidecar_path(output);
    let json = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_string());
    match std::fs::write(&path, json) {
        Ok(()) => Ok(path),
        Err(e) => Err((path, e)),
    }
}
