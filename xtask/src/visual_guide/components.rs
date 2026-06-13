//! HTML fragment helpers used by lesson content modules
//! (`usage_*` / `wiki_*`).
//!
//! Each function returns a `String` of well-formed HTML; callers
//! concatenate fragments into a final lesson body that gets passed
//! to [`super::shell::render_page`].
//!
//! All public functions are pure: same inputs → same output, no
//! filesystem or network I/O.

// dead_code is expected until T8+ lesson modules start calling these;
// removed at T18 once all 14 lessons reference the helpers.
#![allow(dead_code)]

use std::fmt::Write as _;

/// Render an accordion (foldable card) block.
///
/// `num` is the badge number shown in the summary; `title` is the
/// summary text; `body_html` is the expanded content (already
/// HTML-formatted).
///
/// # Examples
///
/// ```text
/// let html = accordion(1, "厂商锁定", "<p>内容</p>");
/// assert!(html.contains("<details"));
/// ```
#[must_use]
pub fn accordion(num: u32, title: &str, body_html: &str) -> String {
    format!(
        "<details class=\"accordion\">\n  <summary><span class=\"badge-num\">{num}</span> {title} <span class=\"hint\">点击展开</span></summary>\n  <div class=\"acc-body\">{body_html}</div>\n</details>\n"
    )
}

/// Render a comparison table — typical three-column "痛点 / 没工具 / agentprof 的做法"
/// shape.
///
/// `headers` is a slice of column headers (any length, table uses all of
/// them in `<th>`). `rows` is a slice of 3-string-tuples — one row per
/// tuple. The table is wrapped in the project's standard styling.
///
/// # Examples
///
/// ```text
/// let html = comparison_table(
///     &["痛点", "没工具", "agentprof 的做法"],
///     &[("黑盒", "看不到 token 去向", "agentprof 给出火焰图")],
/// );
/// ```
#[must_use]
pub fn comparison_table(headers: &[&str], rows: &[(&str, &str, &str)]) -> String {
    let head = headers.iter().fold(String::new(), |mut acc, h| {
        acc.push_str("<th>");
        acc.push_str(h);
        acc.push_str("</th>");
        acc
    });
    let body = rows.iter().fold(String::new(), |mut acc, (a, b, c)| {
        acc.push_str("<tr><td>");
        acc.push_str(a);
        acc.push_str("</td><td>");
        acc.push_str(b);
        acc.push_str("</td><td>");
        acc.push_str(c);
        acc.push_str("</td></tr>");
        acc
    });
    format!("<table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table>\n")
}

/// Render a "相关源码" link to a GitHub blob URL.
///
/// The URL points to `main` branch and contains no line number — line
/// numbers drift on every refactor. Readers Ctrl+F for `symbol`.
///
/// `crate_name` is the full workspace member name (e.g.
/// `"agentprof-core"` → `crates/agentprof-core/src/...`).
/// `path_in_src` is the file path relative to `src/` (e.g.
/// `"analyzer/cache.rs"`). `symbol` is the rustdoc-visible identifier.
///
/// # Examples
///
/// ```text
/// let html = source_ref("agentprof-core", "analyzer/cache.rs", "CacheMetrics");
/// assert!(html.contains("blob/main/crates/agentprof-core/src/analyzer/cache.rs"));
/// ```
#[must_use]
pub fn source_ref(crate_name: &str, path_in_src: &str, symbol: &str) -> String {
    format!(
        "<p class=\"src-ref\">📂 相关源码：\n<a href=\"https://github.com/verdenmax/agentprof/blob/main/crates/{crate_name}/src/{path_in_src}\"><code>{crate_name}/{path_in_src}</code></a>\n&nbsp;<code class=\"mono\">{symbol}</code></p>\n"
    )
}

/// Inline SVG flow diagram — kept simple, intended for 2-5 node
/// pipeline arrows.
///
/// Returns an `<svg class="diagram">…</svg>` snippet. Nodes laid out
/// left-to-right with arrows auto-drawn between adjacent boxes.
/// Empty `nodes` → empty string.
///
/// # Examples
///
/// ```text
/// let svg = flow_diagram(&["events.jsonl", "Adapter", "compute_analysis"]);
/// assert!(svg.contains("<svg"));
/// ```
#[must_use]
pub fn flow_diagram(nodes: &[&str]) -> String {
    use std::fmt::Write as _;
    if nodes.is_empty() {
        return String::new();
    }
    let node_w: usize = 140;
    let node_h: usize = 50;
    let gap: usize = 40;
    let total_w = nodes.len() * node_w + (nodes.len() - 1) * gap;
    let mut svg = format!(
        "<svg class=\"diagram\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {total_w} 80\">\n  <defs>\n    <marker id=\"arr\" viewBox=\"0 0 10 10\" refX=\"8\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\">\n      <path d=\"M0,0 L10,5 L0,10 z\" fill=\"currentColor\"/>\n    </marker>\n  </defs>\n"
    );
    for (i, label) in nodes.iter().enumerate() {
        let x = i * (node_w + gap);
        let cx = x + node_w / 2;
        let _ = write!(
            svg,
            "  <rect x=\"{x}\" y=\"15\" width=\"{node_w}\" height=\"{node_h}\" rx=\"6\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\"/>\n  <text x=\"{cx}\" y=\"45\" font-size=\"13\" text-anchor=\"middle\" fill=\"currentColor\">{label}</text>\n"
        );
        if i < nodes.len() - 1 {
            let from = x + node_w + 2;
            let to = (i + 1) * (node_w + gap) - 2;
            let _ = writeln!(
                svg,
                "  <line x1=\"{from}\" y1=\"40\" x2=\"{to}\" y2=\"40\" stroke=\"currentColor\" stroke-width=\"1.5\" marker-end=\"url(#arr)\"/>"
            );
        }
    }
    svg.push_str("</svg>\n");
    svg
}

/// Render a `<nav class="prev-next">` block at the lesson bottom.
///
/// `prev` and `next` are `(href, title)` tuples; pass `None` for the
/// first or last lesson respectively.
///
/// # Examples
///
/// ```text
/// let nav = prev_next(Some(("01-foo.html", "上一课")), Some(("03-baz.html", "下一课")));
/// assert!(nav.contains("← 上一课"));
/// ```
#[must_use]
pub fn prev_next(prev: Option<(&str, &str)>, next: Option<(&str, &str)>) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("<nav class=\"prev-next\" style=\"display:flex;justify-content:space-between;margin-top:2em;font-size:.9rem\">");
    match prev {
        Some((href, title)) => {
            let _ = writeln!(s, "<a href=\"{href}\">← {title}</a>");
        }
        None => s.push_str("<span></span>"),
    }
    match next {
        Some((href, title)) => {
            let _ = write!(s, "<a href=\"{href}\">{title} →</a>");
        }
        None => s.push_str("<span></span>"),
    }
    s.push_str("</nav>\n");
    s
}

// ----------------------------------------------------------------------------
// Step B visual components — added to enrich lessons with at-a-glance figures.
// ----------------------------------------------------------------------------

/// Render a row of 3 to 5 visual comparison cards laid out as a CSS grid.
///
/// Each card is `(icon, title, line)`. Cards auto-wrap on narrow viewports.
/// Use when you want a visually impactful "三类痛点 / 三种模式" comparison
/// at a glance — bigger visual weight than a `<table>` but more compact
/// than 3 separate accordion cards.
///
/// # Examples
///
/// ```text
/// let html = visual_compare(&[
///     ("🕳️", "看不见 token 去向", "总数对，但归属不明"),
///     ("📉", "没有 ROI 信号", "MCP 装一堆没人用"),
///     ("🌫️", "Prompt cache 黑盒", "命中率？省了多少？"),
/// ]);
/// assert!(html.contains("vis-cmp"));
/// ```
#[must_use]
pub fn visual_compare(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from("<div class=\"vis-cmp\">\n");
    for (icon, title, line) in items {
        write!(s, "  <div class=\"vis-cmp-item\">\n    <div class=\"icon\">{icon}</div>\n    <div class=\"ttl\">{title}</div>\n    <div class=\"ln\">{line}</div>\n  </div>").ok();
    }
    s.push_str("</div>\n");
    s
}

/// Render a `(question, branches)` decision-tree style block.
///
/// `branches` is a slice of `(condition, recommendation)` — typically 2-4
/// branches. Renders as a vertical chain of bordered rows: question on top,
/// each branch as a small row with condition (left) → recommendation
/// (right) — no real SVG tree drawing, just typographic structure with
/// arrows / indentation for clarity.
///
/// # Examples
///
/// ```text
/// let html = decision_tree(
///     "已经装好 Rust toolchain？",
///     &[
///         ("✅ 是", "cargo install agentprof-cli — 30 秒搞定"),
///         ("❌ 否", "用 one-line installer（curl ... | sh）— 不要装 Rust"),
///     ],
/// );
/// assert!(html.contains("dec-tree"));
/// ```
#[must_use]
pub fn decision_tree(question: &str, branches: &[(&str, &str)]) -> String {
    let mut s = String::from("<div class=\"dec-tree\">\n");
    writeln!(s, "  <div class=\"dec-q\">❓ {question}</div>").ok();
    for (cond, recom) in branches {
        writeln!(s, "  <div class=\"dec-branch\"><div class=\"dec-cond\">{cond}</div><div class=\"dec-arr\">→</div><div class=\"dec-recom\">{recom}</div></div>").ok();
    }
    s.push_str("</div>\n");
    s
}

/// Render a struct / schema field table — 4 columns: field, type, required, description.
///
/// Use for documenting data-model field shapes (Wiki 2 / Wiki 5 etc.).
/// `rows` is a slice of `(field_name, type_text, required, description)`
/// tuples; `required` is a free text marker like "✓" / "✗" / "—".
///
/// # Examples
///
/// ```text
/// let html = schema_table(&[
///     ("id", "String", "✓", "Session 唯一 id"),
///     ("agent", "AgentKind", "✓", "Copilot / Claude / Codex"),
///     ("started_at", "DateTime<Utc>", "✓", "Session 开始时间"),
/// ]);
/// assert!(html.contains("schema-table"));
/// ```
#[must_use]
pub fn schema_table(rows: &[(&str, &str, &str, &str)]) -> String {
    let mut s = String::from(
        "<table class=\"schema-table\">\n<thead><tr><th>字段</th><th>类型</th><th>必填</th><th>说明</th></tr></thead>\n<tbody>\n",
    );
    for (field, ty, req, desc) in rows {
        writeln!(s, "<tr><td class=\"sf-field\"><code>{field}</code></td><td class=\"sf-type\"><code>{ty}</code></td><td class=\"sf-req\">{req}</td><td class=\"sf-desc\">{desc}</td></tr>").ok();
    }
    s.push_str("</tbody>\n</table>\n");
    s
}

/// Render a 2-to-6 cell KPI / metric grid.
///
/// Each cell is `(label, value, unit_or_subtitle)`. Renders as a CSS grid
/// of bordered "stat" cards — useful for highlighting key numbers (e.g.
/// "32 个测试 / 1332 workspace tests / 0 failures").
///
/// # Examples
///
/// ```text
/// let html = metric_grid(&[
///     ("crate 数", "5", "(+ xtask)"),
///     ("ADR 数", "25", "决策记录"),
///     ("测试", "1332", "workspace, 0 failed"),
/// ]);
/// assert!(html.contains("metric-grid"));
/// ```
#[must_use]
pub fn metric_grid(cells: &[(&str, &str, &str)]) -> String {
    let mut s = String::from("<div class=\"metric-grid\">\n");
    for (label, value, sub) in cells {
        writeln!(s, "  <div class=\"metric\"><div class=\"label\">{label}</div><div class=\"value\">{value}</div><div class=\"sub\">{sub}</div></div>").ok();
    }
    s.push_str("</div>\n");
    s
}

/// Render a vertical step list — numbered, each with title + detail.
///
/// Use for "6 步清单" / "9 阶段 pipeline" style content where ordering
/// matters and each step has structural prominence.
///
/// # Examples
///
/// ```text
/// let html = step_list(&[
///     ("实现 trait", "在 crates/agentprof-adapters/src/<name>.rs"),
///     ("注册", "registry.rs 加 variant + match arm"),
///     ("fixture", "tests/fixtures/<name>/ 至少 1 个 anonymized session"),
/// ]);
/// assert!(html.contains("step-list"));
/// ```
#[must_use]
pub fn step_list(steps: &[(&str, &str)]) -> String {
    let mut s = String::from("<ol class=\"step-list\">\n");
    for (title, detail) in steps {
        writeln!(
            s,
            "  <li><div class=\"st-ttl\">{title}</div><div class=\"st-dt\">{detail}</div></li>"
        )
        .ok();
    }
    s.push_str("</ol>\n");
    s
}
