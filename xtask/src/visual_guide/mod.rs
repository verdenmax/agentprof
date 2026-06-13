//! `cargo xtask visual-guide` — generate the agentprof visual guide
//! HTML site under `docs/visual-guide/`.
//!
//! Output: 1 `index.html` + 6 `usage/*.html` + 8 `wiki/*.html` = 15 files.
//!
//! See `docs/superpowers/specs/2026-06-13-visual-guide-design.md` for
//! the full design; ADR-0025 (T21) codifies the 7 decisions.

// Stub: T7+ will consume `cmd` by value (fs writes) and return real `Result`s.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use clap::Args;

pub mod components;
pub mod css;
pub mod highlight;
pub mod shell;

/// Best-effort git short SHA (12 chars); `"unknown"` on failure (e.g.
/// CI checkout without `.git`, or git not on PATH). Footer-only;
/// not security-sensitive.
///
/// # Examples
///
/// ```text
/// let sha = git_sha_short_or_unknown();
/// assert!(!sha.is_empty());
/// ```
#[must_use]
pub fn git_sha_short_or_unknown() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned())
}

/// CLI arguments for `cargo xtask visual-guide`.
#[derive(Debug, Args)]
pub struct VisualGuideCmd {
    /// Delete existing generated `*.html` files under `docs/visual-guide/`
    /// before regenerating. Does NOT touch `assets/` or `README.md`.
    #[arg(long)]
    pub clean: bool,

    /// Validate only — render to in-memory strings, verify askama compiles
    /// and all components produce HTML, but DO NOT write any files.
    /// Used by CI on pull requests.
    #[arg(long)]
    pub check: bool,
}

/// Entry point for the `visual-guide` subcommand.
///
/// # Errors
///
/// Returns `anyhow::Error` if askama rendering fails, if any required
/// asset is missing, or if filesystem operations fail.
///
/// # Examples
///
/// ```no_run
/// // Invoked via the xtask CLI, not directly:
/// // $ cargo run -p xtask -- visual-guide --check
/// ```
pub fn run(cmd: VisualGuideCmd) -> anyhow::Result<()> {
    if cmd.check {
        println!("visual-guide: --check mode (not yet implemented, T7+)");
        return Ok(());
    }
    if cmd.clean {
        println!("visual-guide: --clean (not yet implemented, T7+)");
    }
    println!("visual-guide: render (not yet implemented, T7+)");
    Ok(())
}

#[cfg(test)]
mod css_smoke {
    use super::css;

    #[test]
    fn all_css_contains_required_tokens() {
        let css = css::ALL_CSS;
        assert!(css.contains("--bg:"));
        assert!(css.contains("--ink:"));
        assert!(css.contains("--accent:"));
        assert!(css.contains("prefers-color-scheme: dark"));
        assert!(css.contains(".vg-top"));
        assert!(css.contains(".vg-footer"));
        assert!(css.contains(".vg-main"));
        assert!(css.contains("#vg-progress-bar"));
        assert!(css.contains(".code"));
    }
}

#[cfg(test)]
mod shell_smoke {
    use super::shell;

    #[test]
    fn page_includes_required_chrome() {
        let body = "<p>Hello agentprof.</p>";
        let html = shell::render_page(
            shell::PageMeta {
                title: "Test Lesson",
                description: "Test desc",
                section_label: "用法",
                home_href: "../index.html",
                prev: None,
                next: None,
            },
            body,
        )
        .expect("render");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>agentprof 可视化指南 — Test Lesson</title>"));
        assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(html.contains("<nav"));
        assert!(html.contains("<footer"));
        assert!(html.contains(body));
    }
}

#[cfg(test)]
mod components_tests {
    use super::components::*;

    #[test]
    fn accordion_includes_summary_and_body() {
        let html = accordion(1, "厂商锁定", "<p>示例内容</p>");
        assert!(html.contains("<details"));
        assert!(html.contains("<summary"));
        assert!(html.contains("badge-num"));
        assert!(html.contains("厂商锁定"));
        assert!(html.contains("<p>示例内容</p>"));
    }

    #[test]
    fn comparison_table_renders_three_columns() {
        let rows = [
            ("黑盒", "看不到 token 去向", "agentprof 给出火焰图"),
            ("无 ROI", "猜哪个 tool 浪费", "agentprof 算 ROI 表"),
        ];
        let html = comparison_table(&["痛点", "没工具", "agentprof 的做法"], &rows);
        assert!(html.contains("<table"));
        assert!(html.contains("<th>痛点</th>"));
        assert!(html.contains("<td>看不到 token 去向</td>"));
        assert!(html.contains("<td>agentprof 算 ROI 表</td>"));
    }

    #[test]
    fn source_ref_produces_github_blob_url_without_line_number() {
        let html = source_ref("agentprof-core", "analyzer/cache.rs", "CacheMetrics");
        assert!(html.contains(
            "github.com/verdenmax/agentprof/blob/main/crates/agentprof-core/src/analyzer/cache.rs"
        ));
        assert!(html.contains("CacheMetrics"));
        assert!(!html.contains("#L"));
    }
}

#[cfg(test)]
mod highlight_tests {
    use super::highlight::{highlight, Lang};

    #[test]
    fn rust_marks_keywords_strings_comments() {
        let src = "// hello\nfn greet(name: &str) {\n    let msg = \"hi\";\n}";
        let html = highlight(Lang::Rust, src);
        assert!(html.contains(r#"<span class="cm">// hello</span>"#));
        assert!(html.contains(r#"<span class="kw">fn</span>"#));
        assert!(html.contains(r#"<span class="kw">let</span>"#));
        assert!(html.contains(r#"<span class="st">"hi"</span>"#));
    }

    #[test]
    fn bash_marks_comments_and_variables() {
        let src = "# comment\nfor f in *.rs; do\n  echo \"$f\"\ndone";
        let html = highlight(Lang::Bash, src);
        assert!(html.contains(r#"<span class="cm"># comment</span>"#));
        assert!(html.contains(r#"<span class="kw">for</span>"#));
        assert!(html.contains(r#"<span class="kw">do</span>"#));
        assert!(html.contains(r#"<span class="kw">done</span>"#));
        assert!(html.contains(r#"<span class="st">"$f"</span>"#));
    }

    #[test]
    fn toml_marks_section_headers_and_keys() {
        let src = "[serve]\nbind = \"127.0.0.1:4329\"\n# comment\ninterval = 5";
        let html = highlight(Lang::Toml, src);
        assert!(html.contains(r#"<span class="kw">[serve]</span>"#));
        assert!(html.contains(r#"<span class="st">"127.0.0.1:4329"</span>"#));
        assert!(html.contains(r#"<span class="cm"># comment</span>"#));
        assert!(html.contains(r#"<span class="nm">5</span>"#));
    }

    #[test]
    fn sql_marks_uppercase_keywords_and_dash_comments() {
        let src = "-- list sessions\nSELECT id, started_at FROM sessions WHERE started_at > 0;";
        let html = highlight(Lang::Sql, src);
        assert!(html.contains(r#"<span class="cm">-- list sessions</span>"#));
        assert!(html.contains(r#"<span class="kw">SELECT</span>"#));
        assert!(html.contains(r#"<span class="kw">FROM</span>"#));
        assert!(html.contains(r#"<span class="kw">WHERE</span>"#));
    }
}
