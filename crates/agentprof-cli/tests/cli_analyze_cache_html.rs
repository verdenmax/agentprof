//! End-to-end tests for the `<section id="cache">` block in
//! `analyze --export html`.
//!
//! Per ADR-0023 and M2.5 Task 7, the HTML renderer must:
//! - Emit a `<section id="cache">` with a 6-row metrics table when
//!   [`agentprof_core::analyzer::AnalysisReport::cache_metrics`] returns
//!   `Some`.
//! - Omit the section entirely when `cache_metrics()` returns `None`,
//!   matching the markdown renderer's behaviour (T5).

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
/// so the report's `cache_metrics()` should be `Some` and the HTML
/// renderer must emit a `<section id="cache">` block with all six rows.
#[test]
fn analyze_html_emits_cache_section_when_session_has_cache() {
    let path = fixtures_root().join("with-session-shutdown");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(&path)
        .args(["--export", "html"])
        .assert()
        .success()
        .stdout(contains("<section id=\"cache\">"))
        .stdout(contains("<h2>Cache</h2>"))
        .stdout(contains("Creation tokens"))
        .stdout(contains("Read tokens"))
        .stdout(contains("Hit% (honest)"))
        .stdout(contains("Hit% (naive)"))
        .stdout(contains("Net saved tokens"))
        .stdout(contains("Gross saved tokens"));
}

/// `cross-turn-tool` is a small fixture with no cache token activity;
/// `cache_metrics()` returns `None`, so the HTML renderer must skip the
/// cache section entirely (no `<section id="cache">`, no `<h2>Cache</h2>`).
#[test]
fn analyze_html_omits_cache_section_when_no_cache_activity() {
    let path = fixtures_root().join("cross-turn-tool");
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["analyze", "--session"])
        .arg(&path)
        .args(["--export", "html"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(
        !s.contains("<section id=\"cache\">"),
        "fixture has no cache activity; cache section must be omitted, got:\n{s}"
    );
    assert!(
        !s.contains("<h2>Cache</h2>"),
        "fixture has no cache activity; '<h2>Cache</h2>' must be omitted, got:\n{s}"
    );
}
