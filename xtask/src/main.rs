//! # xtask
//!
//! Build / maintenance / release driver for the agentprof workspace.
//! Follows the [cargo-xtask](https://github.com/matklad/cargo-xtask)
//! convention: run via `cargo run -p xtask -- <task>`.

fn main() {
    // Concrete tasks (anonymize / dist-check / release-notes) will be wired
    // up alongside the Phase 0 prototype.
    eprintln!("xtask skeleton: no tasks wired yet.");
    std::process::exit(0);
}
