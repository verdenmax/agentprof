//! Shared HTML shell — DOCTYPE + head + nav + footer + favicon.
//!
//! Mirrors `langchain-visual-guide/src/shell.py` patterns: every lesson
//! page goes through [`render_page`]; the index page uses `render_index`
//! (T7). Both produce self-contained HTML so the site works from
//! `file://` and any static HTTP server.

// T3 lands the shell ahead of its callers (T5+ lessons.rs / T7 generator).
// Until then xtask is a bin-only crate so the pub API looks "unused".
#![allow(dead_code)]

use askama::Template;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

/// Inline SVG favicon — base64-encoded into a `data:` URL so pages
/// stay self-contained. Matches dashboard.css accent (#1a1a2e).
///
/// # Examples
///
/// ```text
/// let url = favicon_data_url();
/// assert!(url.starts_with("data:image/svg+xml;base64,"));
/// ```
#[must_use]
pub fn favicon_data_url() -> String {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect width='32' height='32' rx='7' fill='#1a1a2e'/><text x='16' y='23' font-family='system-ui,sans-serif' font-size='20' font-weight='700' fill='#eee' text-anchor='middle'>a</text></svg>";
    format!("data:image/svg+xml;base64,{}", B64.encode(svg))
}

/// Per-page metadata supplied by the caller.
pub struct PageMeta<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub section_label: &'a str,
    pub home_href: &'a str,
    pub prev: Option<NavLink<'a>>,
    pub next: Option<NavLink<'a>>,
    /// 1-based lesson position within the entire `PAGES` slice (used in
    /// topbar progress pill "N / total" and the top progress bar fill).
    pub lesson_index: usize,
    /// Total lesson count across all chapters. Used together with
    /// `lesson_index` for the topbar pill.
    pub total_lessons: usize,
}

/// Navigation link (prev / next) in the top bar.
pub struct NavLink<'a> {
    pub href: &'a str,
    pub title: &'a str,
}

#[derive(Template)]
#[template(path = "visual_guide/page.html")]
struct PageTemplate<'a> {
    meta: &'a PageMeta<'a>,
    body_html: &'a str,
    favicon: &'a str,
    css: &'static str,
    pkg_version: &'static str,
    generated_at_utc: String,
    git_sha_short: String,
}

/// Render a single lesson HTML page.
///
/// # Errors
///
/// Returns [`askama::Error`] if template rendering fails (should not
/// happen unless `xtask/templates/visual_guide/page.html` is missing or
/// malformed at compile time).
///
/// # Examples
///
/// ```text
/// let html = render_page(
///     PageMeta {
///         title: "Demo",
///         description: "d",
///         section_label: "用法",
///         home_href: "../index.html",
///         prev: None,
///         next: None,
///     },
///     "<p>hi</p>",
/// ).unwrap();
/// assert!(html.contains("<!DOCTYPE html>"));
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn render_page(meta: PageMeta<'_>, body_html: &str) -> askama::Result<String> {
    let favicon = favicon_data_url();
    let tmpl = PageTemplate {
        meta: &meta,
        body_html,
        favicon: &favicon,
        css: super::css::ALL_CSS,
        pkg_version: env!("CARGO_PKG_VERSION"),
        generated_at_utc: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        git_sha_short: super::git_sha_short_or_unknown(),
    };
    tmpl.render()
}
