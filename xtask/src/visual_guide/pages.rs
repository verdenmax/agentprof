//! `PAGES` registry — every lesson here gets generated to disk and
//! linked from the index. T8-T18 add `usage_*` / `wiki_*` modules
//! and append entries here.
//!
//! Ordering is significant: it drives prev/next nav.

// dead_code expected until T8+ lessons consume PAGES
#![allow(dead_code)]

use askama::Template;

/// Which chapter a lesson belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// 用法 chapter (面向新手, 6 lessons).
    Usage,
    /// Wiki chapter (面向中阶 + 开发者, 8 lessons).
    Wiki,
}

impl Section {
    /// Localized chapter label for nav and TOC.
    ///
    /// # Examples
    ///
    /// ```text
    /// assert_eq!(Section::Usage.label(), "用法");
    /// ```
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Usage => "用法",
            Self::Wiki => "Wiki",
        }
    }
    /// Output subdirectory name under `docs/visual-guide/`.
    ///
    /// # Examples
    ///
    /// ```text
    /// assert_eq!(Section::Wiki.dir(), "wiki");
    /// ```
    #[must_use]
    pub const fn dir(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Wiki => "wiki",
        }
    }
}

/// One lesson entry in the master `PAGES` registry.
pub struct LessonEntry {
    /// Filename within the section dir (e.g. `"01-what-is-agentprof.html"`).
    pub filename: &'static str,
    /// Display title in nav + index.
    pub title: &'static str,
    /// One-line description for `<meta description>` + index card.
    pub description: &'static str,
    /// Chapter.
    pub section: Section,
}

/// Master list of all lessons. T8-T18 each append one entry here.
pub const PAGES: &[LessonEntry] = &[
    // T8-T18 will populate these stubs.
];

#[derive(Template)]
#[template(path = "visual_guide/index.html")]
struct IndexTemplate {
    usage_lessons: Vec<IndexRow>,
    wiki_lessons: Vec<IndexRow>,
    pkg_version: &'static str,
    generated_at_utc: String,
    git_sha_short: String,
    favicon: String,
    css: &'static str,
}

struct IndexRow {
    href: String,
    title: &'static str,
    description: &'static str,
    number: usize,
}

/// Render the index page (the site root `index.html`).
///
/// Collects `PAGES` filtered by `Section`, then renders the
/// `visual_guide/index.html` askama template with hero + two
/// section grids (用法 / Wiki). Empty sections render a placeholder.
///
/// # Errors
///
/// Returns `askama::Error` if template rendering fails (typically a
/// programmer error in the template itself).
///
/// # Examples
///
/// ```text
/// let html = pages::render_index().expect("render");
/// assert!(html.contains("<!DOCTYPE html>"));
/// ```
pub fn render_index() -> askama::Result<String> {
    let usage_lessons: Vec<IndexRow> = PAGES
        .iter()
        .filter(|p| p.section == Section::Usage)
        .enumerate()
        .map(|(i, p)| IndexRow {
            href: format!("{}/{}", p.section.dir(), p.filename),
            title: p.title,
            description: p.description,
            number: i + 1,
        })
        .collect();
    let wiki_lessons: Vec<IndexRow> = PAGES
        .iter()
        .filter(|p| p.section == Section::Wiki)
        .enumerate()
        .map(|(i, p)| IndexRow {
            href: format!("{}/{}", p.section.dir(), p.filename),
            title: p.title,
            description: p.description,
            number: i + 1,
        })
        .collect();

    IndexTemplate {
        usage_lessons,
        wiki_lessons,
        pkg_version: env!("CARGO_PKG_VERSION"),
        generated_at_utc: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        git_sha_short: super::git_sha_short_or_unknown(),
        favicon: super::shell::favicon_data_url(),
        css: super::css::ALL_CSS,
    }
    .render()
}
