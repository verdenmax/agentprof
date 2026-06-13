//! `cargo xtask visual-guide` — generate the agentprof visual guide
//! HTML site under `docs/visual-guide/`.
//!
//! Output: 1 `index.html` + 6 `usage/*.html` + 8 `wiki/*.html` = 15 files.
//! Usage chapter is complete as of T13 (6/6). Wiki chapter complete as
//! of T18 (8/8) — full 14-lesson set delivered.
//!
//! See `docs/superpowers/specs/2026-06-13-visual-guide-design.md` for
//! the full design; ADR-0025 (T21) codifies the 7 decisions.

use std::fs;
use std::path::PathBuf;

use clap::Args;

pub mod components;
pub mod css;
pub mod highlight;
pub mod pages;
pub mod shell;
pub mod usage_01;
pub mod usage_02;
pub mod usage_03;
pub mod usage_04;
pub mod usage_05;
pub mod usage_06;
pub mod wiki_01;
pub mod wiki_02;
pub mod wiki_03;
pub mod wiki_04;
pub mod wiki_05;
pub mod wiki_06;
pub mod wiki_07;
pub mod wiki_08;

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
/// ```text
/// // Invoked via the xtask CLI, not directly:
/// // $ cargo run -p xtask -- visual-guide --check
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn run(cmd: VisualGuideCmd) -> anyhow::Result<()> {
    let out_root = workspace_root()?.join("docs").join("visual-guide");

    if cmd.clean {
        let _ = fs::remove_file(out_root.join("index.html"));
        for chapter in ["usage", "wiki"] {
            let _ = fs::remove_dir_all(out_root.join(chapter));
        }
    }

    let mut written: Vec<PathBuf> = Vec::new();

    let index_html = pages::render_index()?;
    if cmd.check {
        anyhow::ensure!(
            index_html.contains("<!DOCTYPE"),
            "index template missing DOCTYPE"
        );
    } else {
        fs::create_dir_all(&out_root)?;
        let idx_path = out_root.join("index.html");
        fs::write(&idx_path, index_html)?;
        written.push(idx_path);
    }

    let total = pages::PAGES.len();
    for (idx0, entry) in pages::PAGES.iter().enumerate() {
        let body_html = render_lesson_body(entry)?;
        let nav = compute_nav(entry);
        let html = shell::render_page(
            shell::PageMeta {
                title: entry.title,
                description: entry.description,
                section_label: entry.section.label(),
                home_href: "../index.html",
                prev: nav.prev.as_ref().map(|n| shell::NavLink {
                    href: &n.0,
                    title: n.1,
                }),
                next: nav.next.as_ref().map(|n| shell::NavLink {
                    href: &n.0,
                    title: n.1,
                }),
                lesson_index: idx0 + 1,
                total_lessons: total,
            },
            &body_html,
        )?;

        if !cmd.check {
            let dir = out_root.join(entry.section.dir());
            fs::create_dir_all(&dir)?;
            let path = dir.join(entry.filename);
            fs::write(&path, html)?;
            written.push(path);
        }
    }

    println!(
        "visual-guide: {} {} files",
        if cmd.check { "verified" } else { "wrote" },
        if cmd.check {
            pages::PAGES.len() + 1
        } else {
            written.len()
        }
    );
    for p in &written {
        println!("  - {}", p.display());
    }
    Ok(())
}

/// Owning nav-link pair: (href, title). Owned by `Nav` so the `&str`
/// returned to `shell::NavLink` references it for the duration of one
/// page render.
struct Nav {
    prev: Option<(String, &'static str)>,
    next: Option<(String, &'static str)>,
}

fn compute_nav(entry: &pages::LessonEntry) -> Nav {
    let idx = pages::PAGES.iter().position(|p| std::ptr::eq(p, entry));
    let prev = idx
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| pages::PAGES.get(i))
        .map(|p| {
            let href = if p.section == entry.section {
                p.filename.to_owned()
            } else {
                format!("../{}/{}", p.section.dir(), p.filename)
            };
            (href, p.title)
        });
    let next = idx.and_then(|i| pages::PAGES.get(i + 1)).map(|p| {
        let href = if p.section == entry.section {
            p.filename.to_owned()
        } else {
            format!("../{}/{}", p.section.dir(), p.filename)
        };
        (href, p.title)
    });
    Nav { prev, next }
}

/// Look up the per-lesson body renderer based on `entry.filename`.
/// T8+ each register a function pointer in this match arm.
fn render_lesson_body(entry: &pages::LessonEntry) -> anyhow::Result<String> {
    match entry.filename {
        "01-what-is-agentprof.html" => Ok(usage_01::render()),
        "02-install.html" => Ok(usage_02::render()),
        "03-analyze.html" => Ok(usage_03::render()),
        "04-list-aggregate.html" => Ok(usage_04::render()),
        "05-serve.html" => Ok(usage_05::render()),
        "06-db-otlp.html" => Ok(usage_06::render()),
        "01-architecture.html" => Ok(wiki_01::render()),
        "02-data-model.html" => Ok(wiki_02::render()),
        "03-adapter.html" => Ok(wiki_03::render()),
        "04-analyzer.html" => Ok(wiki_04::render()),
        "05-storage.html" => Ok(wiki_05::render()),
        "06-otlp-receiver.html" => Ok(wiki_06::render()),
        "07-web-dashboard.html" => Ok(wiki_07::render()),
        "08-contributing.html" => Ok(wiki_08::render()),
        _ => anyhow::bail!(
            "no renderer wired for {}; please update visual_guide::mod::render_lesson_body",
            entry.filename
        ),
    }
}

/// Find the workspace root (parent of `Cargo.lock`).
fn workspace_root() -> anyhow::Result<PathBuf> {
    let mut dir: PathBuf = std::env::current_dir()?;
    loop {
        if dir.join("Cargo.lock").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            anyhow::bail!("workspace root not found (no Cargo.lock in any ancestor)");
        }
    }
}

#[cfg(test)]
mod pages_tests {
    use super::pages;

    #[test]
    fn pages_array_is_non_empty_or_empty_but_well_formed() {
        for entry in pages::PAGES {
            assert!(std::path::Path::new(entry.filename)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("html")));
            assert!(!entry.title.is_empty());
            assert!(matches!(
                entry.section,
                pages::Section::Usage | pages::Section::Wiki
            ));
        }
        // PAGES can be empty at T7; T8+ adds entries.
    }

    #[test]
    fn render_index_includes_doctype_and_section_cards() {
        let html = pages::render_index().expect("render");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("用法"));
        assert!(html.contains("Wiki"));
    }
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
        // Step A redesign: new chrome class names.
        assert!(css.contains(".topbar"));
        assert!(css.contains(".vg-hero"));
        assert!(css.contains(".vg-main"));
        assert!(css.contains(".vg-footer"));
        assert!(css.contains(".footnav"));
        assert!(css.contains("#vg-progress-bar"));
        assert!(css.contains(".code"));
        assert!(css.contains(".src-ref"));
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
                lesson_index: 3,
                total_lessons: 14,
            },
            body,
        )
        .expect("render");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>3 · Test Lesson — agentprof 可视化指南</title>"));
        assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(html.contains("<nav"));
        assert!(html.contains("<footer"));
        assert!(html.contains(body));
        // New chrome assertions (Step A redesign):
        assert!(html.contains("vg-hero"), "hero block missing");
        assert!(html.contains("footnav"), "footer nav missing");
        assert!(html.contains("3 / 14"), "lesson progress pill missing");
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

#[cfg(test)]
mod usage_01_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::usage_01::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
    }
}

#[cfg(test)]
mod usage_02_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::usage_02::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
    }
}

#[cfg(test)]
mod usage_03_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::usage_03::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        assert!(html.contains("--export"));
        assert!(html.contains("../assets/report-html-sample.svg"));
    }
}

#[cfg(test)]
mod usage_04_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::usage_04::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        assert!(html.contains("--by"));
        assert!(html.contains("--since"));
        assert!(html.contains("aggregate"));
    }
}

#[cfg(test)]
mod usage_06_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::usage_06::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        assert!(html.contains("ingest-otlp"));
        assert!(html.contains("SQLite"));
        assert!(html.contains("127.0.0.1:4317"));
        assert!(html.contains("XDG_CACHE_HOME"));
        assert!(html.contains("XDG_DATA_HOME"));
        assert!(html.contains("<svg class=\"diagram\""));
        assert!(html.contains("agentprof db"));
    }
}

#[cfg(test)]
mod usage_05_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::usage_05::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        assert!(html.contains("serve"));
        assert!(html.contains("127.0.0.1:4329"));
        assert!(html.contains("/sessions"));
        assert!(html.contains("/aggregate"));
        assert!(html.contains("/mcp-waste"));
        assert!(html.contains("localStorage"));
        assert!(html.contains("../assets/dashboard-overview.svg"));
    }
}

#[cfg(test)]
mod wiki_02_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_02::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        // 三层数据模型必须都出现
        assert!(html.contains("Event"));
        assert!(html.contains("Episode"));
        assert!(html.contains("AnalysisReport"));
        // 真实类型名（recon 后修正）
        assert!(html.contains("EventKind"));
        assert!(html.contains("derive_episodes"));
        assert!(html.contains("ToolEpisode"));
        assert!(html.contains("HookEpisode"));
        assert!(html.contains("Span"));
        assert!(html.contains("DeriveWarning"));
        assert!(html.contains("NonMonotonicTimestamp"));
        assert!(html.contains("analyze"));
        assert!(html.contains("SessionMeta"));
        assert!(html.contains("ModelUsage"));
        assert!(html.contains("cache_metrics"));
        assert!(html.contains("ADR-0004"));
        // SVG 流程图
        assert!(html.contains("<svg class=\"diagram\""));
        // 4 节点 pipeline 标签
        assert!(html.contains("events.jsonl"));
    }
}

#[cfg(test)]
mod wiki_01_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_01::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        assert!(html.contains("agentprof-core"));
        assert!(html.contains("agentprof-cli"));
        assert!(html.contains("agentprof-adapters"));
        assert!(html.contains("agentprof-storage"));
        assert!(html.contains("agentprof-tui"));
        assert!(html.contains("../assets/architecture-deps.svg"));
        assert!(html.contains("ADR-0019"));
        assert!(html.contains("ADR-0024"));
        assert!(html.contains("ADR-0025"));
        assert!(html.contains("L1"));
        assert!(html.contains("L2"));
        assert!(html.contains("L3"));
    }
}

#[cfg(test)]
mod wiki_03_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_03::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        // 真实 trait 接口名（recon 后确认）
        assert!(html.contains("Adapter"));
        assert!(html.contains("agent_kind"));
        assert!(html.contains("default_session_root"));
        assert!(html.contains("discover_sessions"));
        assert!(html.contains("load_session"));
        assert!(html.contains("RawSession"));
        // AgentKind 3 variants + non_exhaustive
        assert!(html.contains("AgentKind"));
        assert!(html.contains("Copilot"));
        assert!(html.contains("Claude"));
        assert!(html.contains("Codex"));
        assert!(html.contains("#[non_exhaustive]"));
        // CopilotAdapter 案例 + 真实路径
        assert!(html.contains("CopilotAdapter"));
        assert!(html.contains("CopilotEvent"));
        assert!(html.contains("events.jsonl"));
        assert!(html.contains("~/.copilot/session-state"));
        // registry 真实 API
        assert!(html.contains("adapter_for"));
        assert!(html.contains("supported_agents"));
        // 6 步清单关键词
        assert!(html.contains("CHANGELOG"));
        assert!(html.contains("assert_cmd"));
        assert!(html.contains("fixture"));
        assert!(html.contains("docs/adapters.md"));
        // parse_agent 坑提示（recon 发现 storage 侧需要更新）
        assert!(html.contains("parse_agent"));
        // ADR-0004 cross-ref
        assert!(html.contains("ADR-0004"));
        // M3.1 / M3.2 roadmap
        assert!(html.contains("M3.1"));
        assert!(html.contains("M3.2"));
        // 3 source_ref
        assert!(html.contains("crates/agentprof-core/src/adapter.rs"));
        assert!(html.contains("crates/agentprof-adapters/src/registry.rs"));
        assert!(html.contains("crates/agentprof-adapters/src/copilot/adapter.rs"));
    }
}

#[cfg(test)]
mod wiki_04_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_04::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        // analyze pipeline 真实 fn 名（recon: NO cache_metrics 在 analyze 内）
        assert!(html.contains("analyze"));
        assert!(html.contains("turn_summary"));
        assert!(html.contains("tool_rank"));
        assert!(html.contains("hook_rank"));
        assert!(html.contains("model_metrics"));
        assert!(html.contains("loaded_mcp_tools"));
        // AnalysisReport 三组输入
        assert!(html.contains("AnalysisReport"));
        assert!(html.contains("Episodes"));
        assert!(html.contains("SessionMeta"));
        assert!(html.contains("ParseWarning"));
        // SVG flow diagram
        assert!(html.contains("<svg"));
        // 公式 / 常数（recon 真实值）
        assert!(html.contains("CacheMetrics"));
        assert!(html.contains("hit_rate_honest_pct"));
        assert!(html.contains("hit_rate_naive_pct"));
        assert!(html.contains("CACHE_READ_DISCOUNT"));
        assert!(html.contains("CACHE_WRITE_PREMIUM"));
        assert!(html.contains("0.9"));
        assert!(html.contains("0.25"));
        assert!(html.contains("saved_net"));
        assert!(html.contains("saved_gross"));
        // ADR-0023 cross-ref
        assert!(html.contains("ADR-0023"));
        // tool_rank percentile 真实 fn
        assert!(html.contains("percentile"));
        assert!(html.contains("p50"));
        assert!(html.contains("p95"));
        assert!(html.contains("ToolSource"));
        // MCP waste 真实 API
        assert!(html.contains("compute_waste"));
        assert!(html.contains("aggregate_waste"));
        assert!(html.contains("WasteComputeContext"));
        assert!(html.contains("with_tokenizer"));
        assert!(html.contains("with_config"));
        assert!(html.contains("with_sidecar"));
        assert!(html.contains("TokenizerKind"));
        assert!(html.contains("build_bpe"));
        assert!(html.contains("infer_tokenizer"));
        assert!(html.contains("tiktoken-rs"));
        // ETL transform 类比
        assert!(html.contains("ETL"));
        assert!(html.contains("GROUP BY"));
        // 3 source_ref
        assert!(html.contains("crates/agentprof-core/src/analyzer/mod.rs"));
        assert!(html.contains("crates/agentprof-core/src/analyzer/cache.rs"));
        assert!(html.contains("crates/agentprof-core/src/analyzer/waste.rs"));
    }
}

#[cfg(test)]
mod wiki_05_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_05::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        // hybrid mode 关键术语
        assert!(html.contains("cache"));
        assert!(html.contains("store"));
        assert!(html.contains("StorageMode"));
        assert!(html.contains("XDG_CACHE_HOME"));
        assert!(html.contains("XDG_DATA_HOME"));
        // schema 真实表名
        assert!(html.contains("sessions"));
        assert!(html.contains("tools_loaded"));
        assert!(html.contains("turn_buckets"));
        assert!(html.contains("episodes_json"));
        // ADR-0019
        assert!(html.contains("ADR-0019"));
        // dual-path
        assert!(html.contains("dual-path"));
        // 2 source_ref
        assert!(html.contains("crates/agentprof-storage/src/db.rs"));
        assert!(html.contains("crates/agentprof-storage/src/config.rs"));
    }
}

#[cfg(test)]
mod wiki_06_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_06::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        // OTLP 协议 + 端口
        assert!(html.contains("OTLP"));
        assert!(html.contains("4317"));
        assert!(html.contains("4318"));
        assert!(html.contains("gRPC"));
        // 真实 fn 名
        assert!(html.contains("serve_grpc"));
        assert!(html.contains("serve_http"));
        // ADRs
        assert!(html.contains("ADR-0021"));
        assert!(html.contains("ADR-0022"));
        // 4 层防御
        assert!(html.contains("subtle"));
        assert!(html.contains("ConstantTimeEq"));
        assert!(html.contains("LRU"));
        assert!(html.contains("1024"));
        assert!(html.contains("256"));
        assert!(html.contains("session.id") || html.contains("session_id"));
        // flow diagram
        assert!(html.contains("<svg"));
        // 2 source_ref
        assert!(html.contains("crates/agentprof-storage/src/otlp/server_grpc.rs"));
        assert!(html.contains("crates/agentprof-storage/src/otlp/server_http.rs"));
    }
}

#[cfg(test)]
mod wiki_07_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_07::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        // 关键术语
        assert!(html.contains("askama"));
        assert!(html.contains("axum"));
        assert!(html.contains("serve"));
        assert!(html.contains("chunk"));
        // ADR-0024
        assert!(html.contains("ADR-0024"));
        // 7 决策
        assert!(html.contains("D-1"));
        assert!(html.contains("D-7"));
        // 真实 fn 名
        assert!(html.contains("build_router"));
        assert!(html.contains("render_body_only"));
        // 5 视图 handler
        assert!(html.contains("sessions"));
        assert!(html.contains("aggregate"));
        assert!(html.contains("mcp_waste") || html.contains("mcp-waste"));
        // 2 source_ref
        assert!(html.contains("crates/agentprof-cli/src/cmd/serve/router.rs"));
        assert!(html.contains("crates/agentprof-cli/src/cmd/serve/handlers.rs"));
    }
}

#[cfg(test)]
mod wiki_08_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::wiki_08::render();
        assert!(
            html.len() > 1500,
            "expect substantial content, got {} chars",
            html.len()
        );
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
        // Conventional Commits
        assert!(html.contains("Conventional Commits"));
        assert!(html.contains("feat"));
        assert!(html.contains("fix"));
        assert!(html.contains("docs"));
        // 9 阶段 pipeline
        assert!(html.contains("pipeline"));
        assert!(html.contains("brainstorming"));
        assert!(html.contains("ADR"));
        assert!(html.contains("TDD"));
        // CHANGELOG
        assert!(html.contains("CHANGELOG"));
        // Wiki 8 link: 手写 a 链接到 .github/* 或 CONTRIBUTING.md
        assert!(html.contains("CONTRIBUTING.md") || html.contains("copilot-instructions.md"));
        assert!(html.contains("github.com/verdenmax/agentprof"));
    }
}
