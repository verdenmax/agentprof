//! Verify `cargo run -p xtask -- visual-guide --help` lists the subcommand.
//!
//! There is no `cargo xtask` alias configured in this workspace
//! (no `.cargo/config.toml`), so we invoke via `cargo run -p xtask --`
//! to match the pattern used by `schema_audit.rs`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::process::Command;

#[test]
fn visual_guide_help_lists_flags() {
    let out = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "-p",
            "xtask",
            "--",
            "visual-guide",
            "--help",
        ])
        .output()
        .expect("spawn cargo run -p xtask");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        out.status.success(),
        "expected success, got {:?}\n{combined}",
        out.status
    );
    assert!(combined.contains("--clean"), "missing --clean: {combined}");
    assert!(combined.contains("--check"), "missing --check: {combined}");
}
