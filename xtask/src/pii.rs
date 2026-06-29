//! # audit-pii
//!
//! Developer tool that scans a path for high-confidence real home directory
//! paths (`/home/<user>/`, `/Users/<user>/`, `C:\Users\<user>`) that should
//! never be committed. See `docs/superpowers/specs/2026-06-29-pii-audit-design.md`
//! §L-11 for requirements. Audit-only: it reports, never redacts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use walkdir::WalkDir;

/// Allowlisted segments: anonymized placeholders that are safe to commit.
const ALLOWLIST: [&str; 3] = ["USER", "<user>", "<username>"];

/// CLI arguments for `cargo xtask audit-pii`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct AuditPiiCmd {
    /// File or directory to scan recursively for real home paths.
    pub path: PathBuf,
}

/// Entry point invoked from `main.rs` when the `audit-pii` subcommand is chosen.
///
/// Prints one `path:line: text` per hit to stdout, then exits with code `2` if
/// any PII was found, or returns `Ok(())` (exit `0`) when clean.
///
/// # Examples
///
/// ```text
/// run(AuditPiiCmd { path: ".".into() })  // prints hits; exit 2 if any
/// ```
///
/// # Errors
///
/// Returns an error if the path cannot be walked.
pub fn run(cmd: AuditPiiCmd) -> Result<()> {
    let AuditPiiCmd { path } = cmd;
    let hits = scan(&path)?;
    for h in &hits {
        println!("{h}");
    }
    if hits.is_empty() {
        Ok(())
    } else {
        std::process::exit(2);
    }
}

/// Recursively scan `root`, returning a `path:line: text` string per hit.
///
/// Skips `target/` and `.git/` directories and binary (non-UTF-8) files.
///
/// # Examples
///
/// ```text
/// scan(Path::new("Cargo.toml"))  // → [] (clean)
/// ```
///
/// # Errors
///
/// Returns an error if a directory entry cannot be traversed.
pub fn scan(root: &Path) -> Result<Vec<String>> {
    let mut hits = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some("target" | ".git")))
    {
        let entry = entry.context("walking directory")?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if is_pii(line) {
                hits.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
            }
        }
    }
    Ok(hits)
}

/// Return `true` if `line` contains a real (non-allowlisted) home path.
///
/// # Examples
///
/// ```text
/// is_pii("see /home/alice/notes")  // true
/// is_pii("see /home/USER/notes")   // false (allowlisted)
/// ```
#[must_use]
pub fn is_pii(line: &str) -> bool {
    [("/home/", '/'), ("/Users/", '/'), (r"C:\Users\", '\\')]
        .iter()
        .any(|&(pat, sep)| segments(line, pat, sep).any(|seg| !ALLOWLIST.contains(&seg)))
}

/// Yield every path segment that follows an occurrence of `pat`, so a
/// placeholder earlier on the line can't mask a real path later.
fn segments<'a>(line: &'a str, pat: &'a str, sep: char) -> impl Iterator<Item = &'a str> {
    line.match_indices(pat).filter_map(move |(i, _)| {
        let rest = &line[i + pat.len()..];
        let seg = rest.split([sep, ' ', '"', '\'']).next()?;
        (!seg.is_empty()).then_some(seg)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn alice_hits_user_skipped() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x /home/alice/p").unwrap();
        std::fs::write(dir.path().join("b.txt"), "ok /home/USER/p").unwrap();
        let hits = scan(dir.path()).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("a.txt:1:"));
        assert!(hits[0].contains("/home/alice/p"));
    }

    #[test]
    fn empty_dir_clean() {
        let dir = TempDir::new().unwrap();
        assert_eq!(scan(dir.path()).unwrap().len(), 0);
    }

    #[test]
    fn macos_and_windows_paths_hit() {
        assert!(is_pii("/Users/bob/dev"));
        assert!(is_pii(r"C:\Users\bob\dev"));
        assert!(!is_pii("/Users/USER/dev"));
        assert!(!is_pii(r"C:\Users\USER\dev"));
    }

    #[test]
    fn placeholder_first_does_not_mask_real_path() {
        // Earlier allowlisted segment must not hide a later real path.
        assert!(is_pii("/home/USER/a and /home/alice/b"));
    }

    #[test]
    fn binary_file_skipped() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(scan(dir.path()).unwrap().len(), 0);
    }

    #[test]
    fn clean_text_zero() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("c.txt"), "nothing here").unwrap();
        assert_eq!(scan(dir.path()).unwrap().len(), 0);
    }
}
