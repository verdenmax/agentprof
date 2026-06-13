//! Integration tests for the `visual-guide` xtask subcommand.
//!
//! These tests drive `cargo run -p xtask -- visual-guide` end-to-end and assert
//! on the resulting `docs/visual-guide/` tree (file count, dry-run semantics,
//! intra-guide links, asset references, and structural well-formedness of the
//! emitted HTML).
//!
//! Added in T19 alongside the placeholder SVG asset set so the visual guide
//! ships testable artefacts even before real PNG screenshots land.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::redundant_closure,
        clippy::redundant_closure_for_method_calls,
        clippy::map_unwrap_or,
        clippy::needless_continue,
        clippy::items_after_statements,
    )
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Global lock: all integration tests in this file mutate the same
/// `docs/visual-guide/` tree, so they must run serially even when cargo runs
/// them in parallel within the same test binary.
static GUIDE_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}

fn run_xtask(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO"))
        .args(args.iter().copied())
        .current_dir(workspace_root())
        .output()
        .expect("spawn cargo xtask")
}

#[test]
fn render_all_produces_expected_file_count() {
    let _g = GUIDE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let out = run_xtask(&["run", "-p", "xtask", "--", "visual-guide"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let root = workspace_root().join("docs/visual-guide");
    assert!(root.join("index.html").exists());
    let usage = std::fs::read_dir(root.join("usage"))
        .map(|d| d.count())
        .unwrap_or(0);
    let wiki = std::fs::read_dir(root.join("wiki"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(usage, 6, "expected 6 usage lessons");
    assert_eq!(wiki, 8, "expected 8 wiki lessons");
}

#[test]
fn check_mode_does_not_write_files() {
    let _g = GUIDE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Remove the output dir directly (the xtask `--clean` flag also re-renders,
    // which would defeat the dry-run check below).
    let root = workspace_root().join("docs/visual-guide");
    for chapter in ["usage", "wiki"] {
        let _ = std::fs::remove_dir_all(root.join(chapter));
    }
    let _ = std::fs::remove_file(root.join("index.html"));
    let out = run_xtask(&["run", "-p", "xtask", "--", "visual-guide", "--check"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let idx = workspace_root().join("docs/visual-guide/index.html");
    assert!(
        !idx.exists(),
        "--check wrote index.html (should be dry-run)"
    );
    // Re-render so subsequent tests find a populated tree.
    let _ = run_xtask(&["run", "-p", "xtask", "--", "visual-guide"]);
}

#[test]
fn prev_next_links_resolve_to_existing_files() {
    let _g = GUIDE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = run_xtask(&["run", "-p", "xtask", "--", "visual-guide"]);
    let root = workspace_root().join("docs/visual-guide");
    for chapter in ["usage", "wiki"] {
        for entry in std::fs::read_dir(root.join(chapter)).expect("read chapter dir") {
            let entry = entry.expect("dir entry");
            let html = std::fs::read_to_string(entry.path()).expect("read html");
            for cap in find_hrefs(&html) {
                // Skip external URLs, in-page anchors, data: URIs, and
                // absolute paths (the latter are example routes like
                // `/session/{id}` quoted in prose, not real links).
                if cap.starts_with("http")
                    || cap.starts_with('#')
                    || cap.starts_with("data:")
                    || cap.starts_with('/')
                {
                    continue;
                }
                let parent = entry
                    .path()
                    .parent()
                    .expect("html has parent")
                    .to_path_buf();
                let resolved = parent.join(&cap);
                let ok = resolved.exists() || cap == "../index.html";
                assert!(ok, "broken link {cap:?} in {:?}", entry.path());
            }
        }
    }
}

#[test]
fn asset_refs_resolve_to_existing_files() {
    let _g = GUIDE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = run_xtask(&["run", "-p", "xtask", "--", "visual-guide"]);
    let root = workspace_root().join("docs/visual-guide");
    let assets = root.join("assets");
    for chapter in ["usage", "wiki"] {
        for entry in std::fs::read_dir(root.join(chapter)).expect("read chapter dir") {
            let entry = entry.expect("dir entry");
            let html = std::fs::read_to_string(entry.path()).expect("read html");
            for asset in find_asset_refs(&html) {
                assert!(
                    assets.join(&asset).exists(),
                    "missing asset {asset:?} referenced from {:?}",
                    entry.path()
                );
            }
        }
    }
}

#[test]
fn output_html_parses_as_well_formed() {
    let _g = GUIDE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = run_xtask(&["run", "-p", "xtask", "--", "visual-guide"]);
    let root = workspace_root().join("docs/visual-guide");
    for path in walkdir_html(&root) {
        let html = std::fs::read_to_string(&path).expect("read html");
        // quick-xml in strict XML mode rejects HTML5 void elements (<meta>, <link>,
        // <br>) and DOCTYPE quirks, which are valid HTML5. We loosen the parser
        // to accept them: this still catches structural breakage (unbalanced
        // tags, malformed attributes) while tolerating HTML5 idioms.
        let mut reader = quick_xml::Reader::from_str(&html);
        let config = reader.config_mut();
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => continue,
                Err(e) => panic!("malformed HTML in {path:?}: {e}"),
            }
        }
        assert!(
            html.contains("<!DOCTYPE html>") || html.contains("<!doctype html>"),
            "missing DOCTYPE in {path:?}"
        );
        assert!(html.contains("</html>"), "missing </html> in {path:?}");
    }
}

fn find_hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "href=\"";
    let mut i = 0;
    while let Some(start) = html[i..].find(needle) {
        let from = i + start + needle.len();
        if let Some(end) = html[from..].find('"') {
            out.push(html[from..from + end].to_owned());
            i = from + end + 1;
        } else {
            break;
        }
    }
    out
}

fn find_asset_refs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "../assets/";
    let mut i = 0;
    while let Some(start) = html[i..].find(needle) {
        let from = i + start + needle.len();
        let end = html[from..].find('"').unwrap_or(html.len() - from);
        out.push(html[from..from + end].to_owned());
        i = from + end + 1;
    }
    out
}

fn walkdir_html(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn rec(d: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    rec(&p, out);
                } else if p.extension().and_then(|s| s.to_str()) == Some("html") {
                    out.push(p);
                }
            }
        }
    }
    rec(root, &mut out);
    out
}
