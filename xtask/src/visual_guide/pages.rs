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
    LessonEntry {
        filename: "01-what-is-agentprof.html",
        title: "agentprof 是什么",
        description:
            "用 Rust 写的开源 token profiler — 给 AI agent 用的 perf flamegraph + ROI 报告器。",
        section: Section::Usage,
    },
    LessonEntry {
        filename: "02-install.html",
        title: "5 分钟上手",
        description: "从 cargo install 到第一次跑 analyze — 5 分钟跑出第一张火焰图。",
        section: Section::Usage,
    },
    LessonEntry {
        filename: "03-analyze.html",
        title: "analyze：看懂一次 session",
        description:
            "5 种 export 格式怎么挑 — md / tui / html / json / speedscope；如何读 Turn Summary、Tool Rank、Cache 段。",
        section: Section::Usage,
    },
    LessonEntry {
        filename: "04-list-aggregate.html",
        title: "list / aggregate：跨 session 视角",
        description:
            "不止看一次 session — list 最近 sessions、aggregate 跨 model / tool / day 聚合，看 7-30 天 token 趋势。",
        section: Section::Usage,
    },
    LessonEntry {
        filename: "05-serve.html",
        title: "serve：浏览器实时看板",
        description:
            "从 grep + cron 升级到 Grafana — agentprof serve 拉起 localhost HTTP 看板，5 个视图 5 秒轮询自动刷新。",
        section: Section::Usage,
    },
    LessonEntry {
        filename: "06-db-otlp.html",
        title: "db + ingest-otlp：存数据库 + 接入 OTLP",
        description:
            "从 grep 单文件到 SQLite 持久化 + OTLP push gateway — 接入 Claude Code / Codex 实时数据。",
        section: Section::Usage,
    },
    // T12-T18 will add more
    LessonEntry {
        filename: "01-architecture.html",
        title: "架构全景",
        description:
            "5 crate 依赖图 + L1/L2/L3 文档体系 + 24 份 ADR 索引 — 整个项目的全景图。",
        section: Section::Wiki,
    },
    LessonEntry {
        filename: "02-data-model.html",
        title: "数据模型",
        description:
            "Event → Episode → AnalysisReport 三层关系 + 关键字段表 — agentprof 数据流的核心类型。",
        section: Section::Wiki,
    },
    LessonEntry {
        filename: "03-adapter.html",
        title: "Adapter trait + 怎么写新 adapter",
        description:
            "Adapter trait 接口 + AgentKind 枚举 + CopilotAdapter 案例 + 写 Claude / Codex adapter 的 6 步指南。",
        section: Section::Wiki,
    },
    LessonEntry {
        filename: "04-analyzer.html",
        title: "分析层 rollups",
        description:
            "analyze() 流水线 + turn_summary / tool_rank / hook_rank / CacheMetrics 公式 + MCP waste 计算 — agentprof 的核心 ROI 算法。",
        section: Section::Wiki,
    },
    LessonEntry {
        filename: "05-storage.html",
        title: "存储层 hybrid mode",
        description:
            "ADR-0019 hybrid cache/store + SQLite schema (sessions / tools_loaded / turn_buckets) + dual-path 选择 — agentprof 怎么持久化 session 数据。",
        section: Section::Wiki,
    },
    LessonEntry {
        filename: "06-otlp-receiver.html",
        title: "OTLP receiver",
        description:
            "ADR-0021 架构 (receiver → router → buffer → flush sink → upsert → SQLite) + ADR-0022 4 层防御 (subtle 常时间 bearer / per-signal size caps / LRU cap=1024 / 256-byte session.id 上限)。",
        section: Section::Wiki,
    },
    LessonEntry {
        filename: "07-web-dashboard.html",
        title: "Web dashboard 架构",
        description:
            "ADR-0024 全 7 决策 (D-1..D-7) + chunk-endpoint pattern + axum + askama + vanilla JS poller — agentprof serve 的实现全景。",
        section: Section::Wiki,
    },
    LessonEntry {
        filename: "08-contributing.html",
        title: "贡献指南",
        description:
            "Conventional Commits + 9 阶段 pipeline (brainstorming → spec → ADR → plan → TDD → CI → review) + 怎么开 PR / 加 ADR / 通过 CI。",
        section: Section::Wiki,
    },
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
