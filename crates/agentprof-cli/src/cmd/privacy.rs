//! Shared redaction sidecar writer for `--privacy anonymize`.
//!
//! `anonymize` builds a [`RedactionMap`] mapping each redaction marker
//! (e.g. `<uuid-0>`) back to its original value. To keep the rendered
//! report a clean shareable artifact, that reverse map is written to a
//! sibling `agentprof-redaction-map.json` sidecar instead of being
//! embedded inline. See `docs/features/privacy.md`.

use std::path::{Path, PathBuf};

use agentprof_core::analyzer::redact::{PrivacyLevel, RedactionMap};

use crate::cmd::exit::ExitKind;

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

/// Emit the `anonymize` redaction-map sidecar after the report is written.
///
/// Shared by `cmd::analyze` and `cmd::aggregate`. No-op unless
/// `privacy == PrivacyLevel::Anonymize` and the map is non-empty. The write is
/// non-fatal to the already-emitted report: a failure is warned to stderr and
/// surfaced as [`ExitKind::OutputError`] (exit 3) *after* stdout/file output,
/// so the user-facing report is never lost.
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is
/// [`ExitKind::OutputError`] when the sidecar write fails.
///
/// # Examples
///
/// ```ignore
/// // bin-only crate: shape only, not executed (see `sidecar_path`).
/// use agentprof_cli::cmd::privacy::emit_redaction_sidecar;
/// use agentprof_core::analyzer::redact::{PrivacyLevel, RedactionMap};
/// let map = RedactionMap::default();
/// emit_redaction_sidecar(&map, PrivacyLevel::Anonymize, None)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn emit_redaction_sidecar(
    map: &RedactionMap,
    privacy: PrivacyLevel,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    if privacy != PrivacyLevel::Anonymize || map.is_empty() {
        return Ok(());
    }
    match write_sidecar(map, output) {
        Ok(p) => {
            eprintln!("agentprof: redaction map → {}", p.display());
            Ok(())
        }
        Err((p, e)) => {
            eprintln!("agentprof: warn: failed to write {}: {e}", p.display());
            Err(ExitKind::OutputError.into_anyhow(format!("sidecar write failed: {}", p.display())))
        }
    }
}
