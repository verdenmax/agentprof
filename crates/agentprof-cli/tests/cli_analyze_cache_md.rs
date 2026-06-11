//! End-to-end tests for the `## Cache` section in `analyze --export md`.
//!
//! Per ADR-0023 and M2.5 Task 5, the markdown renderer must:
//! - Emit a `## Cache` section with 6 rows when
//!   [`agentprof_core::analyzer::AnalysisReport::cache_metrics`] returns `Some`.
//! - Omit the section entirely when `cache_metrics()` returns `None`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

/// `with-session-shutdown` has a session.shutdown event whose
/// `modelMetrics` carries non-zero `cacheReadTokens` / `cacheWriteTokens`,
/// so the report's `cache_metrics()` should be `Some` and the markdown
/// renderer must emit a `## Cache` section.
#[test]
fn analyze_md_emits_cache_section_when_session_has_cache() {
    let path = fixtures_root().join("with-session-shutdown");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(&path)
        .args(["--export", "md"])
        .assert()
        .success()
        .stdout(contains("## Cache"))
        .stdout(contains("Creation tokens"))
        .stdout(contains("Read tokens"))
        .stdout(contains("Hit% (honest)"))
        .stdout(contains("Hit% (naive)"))
        .stdout(contains("Net saved tokens"))
        .stdout(contains("Gross saved tokens"));
}

/// `cross-turn-tool` is a small fixture with no cache token activity;
/// `cache_metrics()` returns `None`, so the markdown renderer must skip
/// the `## Cache` section entirely (no header, no rows).
#[test]
fn analyze_md_omits_cache_section_when_no_cache_activity() {
    let path = fixtures_root().join("cross-turn-tool");
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(&path)
        .args(["--export", "md"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(
        !s.contains("## Cache"),
        "fixture has no cache activity; '## Cache' section must be omitted, got:\n{s}"
    );
}
