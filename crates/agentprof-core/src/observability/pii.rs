//! PII redaction helpers for tracing emissions.
//!
//! See [`hash_path`] for the canonical "short hash a session path"
//! function; emit it via `tracing::info_span!("cmd.analyze", session = %hash_path(p))`
//! and consumers can group / correlate without seeing the raw filesystem path.

use sha2::{Digest, Sha256};
use std::path::Path;

/// Stable 8-character lowercase hex prefix of SHA-256 of the path's
/// byte representation (lossy-converted to UTF-8 to tolerate non-UTF-8
/// paths). Deterministic across runs and across operating systems.
///
/// # PII trade-off
///
/// 8 hex chars = 32 bits. Birthday-bound collision probability ≈ 50 %
/// at √(2³²) ≈ 65 536 distinct paths. For a developer tool that
/// typically processes < 1 000 sessions per run this is fine; if you
/// observe a collision in support traces, set
/// `AGENTPROF_LOG_FULL_PATHS=1` to emit raw paths and re-run.
///
/// **Non-UTF-8 paths note**: paths are normalized via
/// [`std::path::Path::to_string_lossy`] before hashing. Two distinct
/// OS-level paths that differ only in invalid byte sequences (e.g.
/// raw bytes that fail UTF-8 decoding) can therefore collide pre-hash
/// — the replacement character `U+FFFD` substitution flattens those
/// distinctions. Acceptable for PII-redaction purposes; flagged here
/// so consumers don't over-trust the determinism guarantee.
///
/// # Examples
///
/// ```
/// use agentprof_core::observability::pii::hash_path;
/// use std::path::PathBuf;
///
/// let h1 = hash_path(&PathBuf::from("/home/alice/.cache/x"));
/// let h2 = hash_path(&PathBuf::from("/home/alice/.cache/x"));
/// assert_eq!(h1, h2);
/// assert_eq!(h1.len(), 8);
/// ```
#[must_use]
pub fn hash_path(p: &Path) -> String {
    hash_short(&p.to_string_lossy())
}

/// Stable 8-character lowercase hex prefix of SHA-256(input bytes).
/// Same collision trade-off as [`hash_path`].
///
/// # Examples
///
/// ```
/// use agentprof_core::observability::pii::hash_short;
///
/// let h = hash_short("abc");
/// assert_eq!(h.len(), 8);
/// assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
/// ```
#[must_use]
pub fn hash_short(s: &str) -> String {
    use std::fmt::Write;
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    // 4 bytes = 8 hex chars.
    let mut out = String::with_capacity(8);
    for b in &digest[..4] {
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn hash_path_is_deterministic() {
        let a = hash_path(&PathBuf::from("/home/alice/.cache/copilot/abc"));
        let b = hash_path(&PathBuf::from("/home/alice/.cache/copilot/abc"));
        assert_eq!(a, b, "same input must hash to same output");
    }

    #[test]
    fn hash_path_distinguishes_inputs() {
        let a = hash_path(&PathBuf::from("/home/alice/.cache/copilot/abc"));
        let b = hash_path(&PathBuf::from("/home/alice/.cache/copilot/xyz"));
        assert_ne!(a, b, "different inputs must hash to different outputs");
    }

    #[test]
    fn hash_path_returns_8_hex_chars() {
        let h = hash_path(&PathBuf::from("/x"));
        assert_eq!(h.len(), 8);
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "must be ASCII hex"
        );
    }

    #[test]
    fn hash_path_handles_empty() {
        let _ = hash_path(&PathBuf::new());
    }

    #[test]
    fn hash_short_is_deterministic() {
        assert_eq!(hash_short("abc"), hash_short("abc"));
    }

    #[test]
    fn hash_short_distinguishes_inputs() {
        assert_ne!(hash_short("abc"), hash_short("xyz"));
    }

    #[test]
    fn hash_short_returns_8_hex_chars() {
        let h = hash_short("anything");
        assert_eq!(h.len(), 8);
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "must be ASCII hex"
        );
    }
}
