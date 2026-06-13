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
            let _ = write!(s, "<a href=\"{href}\">← {title}</a>");
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
