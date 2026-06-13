# Visual Guide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Chinese-language visual HTML guide for agentprof (14 lessons, 2 sections: Usage + Wiki) under `docs/visual-guide/`, built by a new `cargo xtask visual-guide` subcommand, deployed to GitHub Pages.

**Architecture:** Single static site, two chapters (`usage/` + `wiki/`), shared shell (CSS + nav + footer + favicon). Built by Rust xtask using askama 0.16 (already in workspace via `agentprof-cli`). Generated HTML is gitignored; only source + assets enter git. CI workflow rebuilds on main push and deploys to `gh-pages` via `actions/deploy-pages@v4`.

**Tech Stack:** Rust 2021, askama 0.16, clap (xtask CLI), chrono (footer timestamp/SHA), serde_json (for asset metadata if any), zero JS framework (inline static HTML + CSS).

**Spec:** `docs/superpowers/specs/2026-06-13-visual-guide-design.md` (commit `9c49949`).

**Branch:** `feat/visual-guide` (already created off `main` HEAD `5b86c15`).

---

## Task Map (21 tasks)

| # | Task | Files touched | Tests |
|---|---|---|---|
| T0 | Branch + SQL todos + dir scaffold | — | — |
| T1 | xtask Cargo.toml: add askama + clap derive | `xtask/Cargo.toml` | smoke |
| T2 | xtask visual-guide subcommand stub | `xtask/src/main.rs`, `xtask/src/visual_guide/mod.rs` | 1 surface test |
| T3 | shell.rs: head_meta + favicon + nav + footer | `xtask/src/visual_guide/shell.rs` + askama templates | 1 unit test |
| T4 | css.rs: design tokens + dark mode + responsive | `xtask/src/visual_guide/css.rs` | 1 smoke test |
| T5 | components.rs: accordion + comparison_table + source_ref + prev/next | `xtask/src/visual_guide/components.rs` | 2 unit tests |
| T6 | highlight.rs: Rust/bash/toml/sql lexer | `xtask/src/visual_guide/highlight.rs` | 4 fixture tests |
| T7 | pages.rs: PAGES const + index page renderer | `xtask/src/visual_guide/pages.rs` + `templates/index.html` | 1 unit test |
| T8 | Usage lesson 1: agentprof 是什么 | `xtask/src/visual_guide/usage_01.rs` | 1 render test |
| T9 | Usage lesson 2: 5 分钟上手 | `usage_02.rs` | 1 render test |
| T10 | Usage lesson 3: analyze 看懂一次 session | `usage_03.rs` + 1 asset (report-html-sample.png) | 1 render test |
| T11 | Usage lesson 4: list / aggregate | `usage_04.rs` | 1 render test |
| T12 | Usage lesson 5: serve 浏览器看板 | `usage_05.rs` + 5 assets (dashboard screenshots) | 1 render test |
| T13 | Usage lesson 6: db + ingest-otlp | `usage_06.rs` + 1 SVG asset | 1 render test |
| T14 | Wiki lesson 1: 架构全景 | `wiki_01.rs` + 1 SVG asset (5-crate deps) | 1 render test |
| T15 | Wiki lesson 2: 数据模型 (Event→Episode→AnalysisReport) | `wiki_02.rs` + 1 SVG | 1 render test |
| T16 | Wiki lesson 3: Adapter trait + how-to-write-new | `wiki_03.rs` | 1 render test |
| T17 | Wiki lesson 4: 分析层 rollups | `wiki_04.rs` | 1 render test |
| T18 | Wiki lesson 5-8 (storage / otlp / web-dashboard / contributing) | `wiki_05.rs` ... `wiki_08.rs` | 4 render tests |
| T19 | xtask tests: 5 integration tests + flamegraph asset | `xtask/tests/visual_guide.rs` + `docs/visual-guide/assets/flamegraph-sample.svg` | 5 integration tests |
| T20 | GH Pages CI workflow + README badge + L1/L2 doc sync | `.github/workflows/visual-guide.yml`, `README.md`, `docs/architecture.md` §15.1, `docs/visual-guide/README.md` | CI green |
| T21 | ADR-0025 + CHANGELOG + release decision (v0.3.4 tag or main-only) | `docs/internals/adr-0025-visual-guide.md`, `CHANGELOG.md`, optionally version bump | — |

**Total:** ~21 commits, ~27 tests added, no breaking changes, no new top-level workspace deps.

---

## Phase gates (controller checkpoints)

- After T7: index page renders end-to-end with 0 lessons → first `cargo xtask visual-guide` smoke succeeds
- After T13: all 6 usage lessons + index live → first full chapter complete
- After T18: all 14 lessons live → preview mode functional
- After T19: tests green, ready for CI
- Before T21: full workspace gate (`cargo test --workspace --all-features` + `cargo xtask visual-guide --check`)

---

## Tasks

<!-- Tasks T0..T21 inserted below incrementally via separate edit calls -->

### Task 0: Branch + SQL todos + directory scaffold

**Files:**
- Create (empty placeholders, just to anchor structure): `docs/visual-guide/.gitkeep`, `docs/visual-guide/assets/.gitkeep`

**Already done by controller before subagent dispatch:**
- Branch `feat/visual-guide` created (HEAD `9c49949` after spec commit)
- Spec committed: `docs/superpowers/specs/2026-06-13-visual-guide-design.md`
- Plan skeleton committed: `docs/superpowers/plans/2026-06-13-visual-guide.md`

- [ ] **Step 1: Insert 22 todos + dependencies into session SQL**

Controller runs (NOT a subagent task):

```sql
INSERT INTO todos (id, title, description, status) VALUES
  ('vg-t0',  'Visual guide T0 scaffold',           'branch + sql + dir',  'in_progress'),
  ('vg-t1',  'Visual guide T1 xtask Cargo.toml',   'add askama + clap',   'pending'),
  ('vg-t2',  'Visual guide T2 xtask subcommand',   'visual-guide stub',   'pending'),
  ('vg-t3',  'Visual guide T3 shell.rs',           'head_meta + nav',     'pending'),
  ('vg-t4',  'Visual guide T4 css.rs',             'design tokens',       'pending'),
  ('vg-t5',  'Visual guide T5 components.rs',      'accordion + tables',  'pending'),
  ('vg-t6',  'Visual guide T6 highlight.rs',       '4-lang lexer',        'pending'),
  ('vg-t7',  'Visual guide T7 pages.rs + index',   'PAGES + index page',  'pending'),
  ('vg-t8',  'Visual guide T8 usage_01',           'what is agentprof',   'pending'),
  ('vg-t9',  'Visual guide T9 usage_02',           '5-min quickstart',    'pending'),
  ('vg-t10', 'Visual guide T10 usage_03',          'analyze',             'pending'),
  ('vg-t11', 'Visual guide T11 usage_04',          'list / aggregate',    'pending'),
  ('vg-t12', 'Visual guide T12 usage_05',          'serve dashboard',     'pending'),
  ('vg-t13', 'Visual guide T13 usage_06',          'db + ingest-otlp',    'pending'),
  ('vg-t14', 'Visual guide T14 wiki_01',           'architecture',        'pending'),
  ('vg-t15', 'Visual guide T15 wiki_02',           'data model',          'pending'),
  ('vg-t16', 'Visual guide T16 wiki_03',           'adapter trait',       'pending'),
  ('vg-t17', 'Visual guide T17 wiki_04',           'analyzer rollups',    'pending'),
  ('vg-t18', 'Visual guide T18 wiki_05-08',        'storage/otlp/web/contrib', 'pending'),
  ('vg-t19', 'Visual guide T19 xtask tests',       '5 integration tests', 'pending'),
  ('vg-t20', 'Visual guide T20 CI + doc sync',     'GH Pages + README',   'pending'),
  ('vg-t21', 'Visual guide T21 ADR + release',     'ADR-0025 + CHANGELOG', 'pending');

INSERT INTO todo_deps VALUES
  ('vg-t2',  'vg-t1'),  -- subcommand needs Cargo.toml
  ('vg-t3',  'vg-t2'),  -- shell needs xtask scaffold
  ('vg-t4',  'vg-t2'),  -- css ditto
  ('vg-t5',  'vg-t3'),  ('vg-t5',  'vg-t4'),  -- components need shell + css
  ('vg-t6',  'vg-t2'),  -- highlight is independent of UI
  ('vg-t7',  'vg-t5'),  ('vg-t7',  'vg-t6'),  -- index page consumes components + highlight
  ('vg-t8',  'vg-t7'),  ('vg-t9',  'vg-t7'),  ('vg-t10', 'vg-t7'),  ('vg-t11', 'vg-t7'),
  ('vg-t12', 'vg-t7'),  ('vg-t13', 'vg-t7'),  -- usage lessons need PAGES wired
  ('vg-t14', 'vg-t7'),  ('vg-t15', 'vg-t7'),  ('vg-t16', 'vg-t7'),  ('vg-t17', 'vg-t7'),
  ('vg-t18', 'vg-t7'),  -- wiki lessons ditto
  ('vg-t19', 'vg-t18'), -- tests after all content
  ('vg-t20', 'vg-t19'), -- CI after tests pass
  ('vg-t21', 'vg-t20'); -- release last
```

- [ ] **Step 2: Create directory anchors**

```bash
mkdir -p docs/visual-guide/assets
touch docs/visual-guide/.gitkeep
touch docs/visual-guide/assets/.gitkeep
```

- [ ] **Step 3: Add .gitignore entries for generated HTML**

Edit `.gitignore` (project root), append:

```
# Visual guide — generated HTML (source-of-truth is xtask + assets/)
docs/visual-guide/*.html
docs/visual-guide/usage/
docs/visual-guide/wiki/
!docs/visual-guide/.gitkeep
!docs/visual-guide/README.md
```

(`docs/visual-guide/README.md` is hand-maintained; it gets created in T20.)

- [ ] **Step 4: Commit scaffold**

```bash
git add docs/visual-guide/.gitkeep docs/visual-guide/assets/.gitkeep .gitignore
git commit -m "chore(visual-guide): scaffold docs/visual-guide/ + gitignore (T0)

Anchors the output directory so docs/visual-guide/assets/ exists for
later asset commits. Generated HTML (index.html, usage/, wiki/) is
gitignored — only sources (xtask/src/visual_guide/) and assets/ enter
git per ADR-0025 D-2 (HTML-not-in-git).

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 0

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

**Done when:** branch `feat/visual-guide` has 3 commits ahead of main (spec + plan skeleton + T0 scaffold).

---

### Task 1: xtask Cargo.toml — add askama + clap derive


**Files:**
- Modify: `xtask/Cargo.toml`

**Reconnaissance first:**

```bash
cat xtask/Cargo.toml
grep -nE "^askama|^clap|^chrono" Cargo.toml | head -5
```

Verify: askama, clap (with `derive`), chrono are already in `[workspace.dependencies]` (confirmed during brainstorming — cli uses all three).

- [ ] **Step 1: Add deps to xtask/Cargo.toml**

Existing `[dependencies]` section gets these added (place adjacent to existing entries; do NOT remove existing entries):

```toml
askama = { workspace = true }
clap   = { workspace = true, features = ["derive"] }
chrono = { workspace = true }
```

If `clap` is already present without `derive`, change its line to include `features = ["derive"]`.

- [ ] **Step 2: Verify build**

```bash
cargo check -p xtask 2>&1 | tail -3
# expect: Finished `dev` profile ... (no errors)
```

- [ ] **Step 3: Commit**

```bash
git add xtask/Cargo.toml Cargo.lock
git commit -m "build(xtask): add askama + clap derive + chrono deps (T1)

Prepares the xtask crate for the visual-guide subcommand. All three
deps are already in workspace.dependencies (used by agentprof-cli
since v0.3.x); xtask adoption adds no new top-level workspace deps
and does not affect the main build graph.

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 1

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

### Task 2: xtask `visual-guide` subcommand stub

**Files:**
- Modify: `xtask/src/main.rs` (or wherever the existing xtask CLI dispatcher lives — grep first)
- Create: `xtask/src/visual_guide/mod.rs`

**Reconnaissance first:**

```bash
find xtask -name "*.rs" | xargs grep -lE "fn main|clap::Parser|Subcommand" 2>/dev/null | head
cat xtask/src/main.rs 2>/dev/null || cat xtask/src/lib.rs 2>/dev/null
```

Understand the existing xtask CLI shape. The existing `anonymize` + `release` (or similar) subcommands tell you the dispatcher style (clap derive vs match on argv).

- [ ] **Step 1: Add `visual_guide` mod declaration**

Add to `xtask/src/main.rs` (or `lib.rs` — wherever module tree starts):

```rust
mod visual_guide;
```

- [ ] **Step 2: Create `xtask/src/visual_guide/mod.rs`**

```rust
//! `cargo xtask visual-guide` — generate the agentprof visual guide
//! HTML site under `docs/visual-guide/`.
//!
//! Output: 1 `index.html` + 6 `usage/*.html` + 8 `wiki/*.html` = 15 files.
//!
//! See `docs/superpowers/specs/2026-06-13-visual-guide-design.md` for
//! the full design; ADR-0025 codifies the 7 decisions.

use clap::Args;

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
```

- [ ] **Step 3: Wire into top-level dispatcher**

The exact shape depends on existing xtask CLI structure. If `xtask` uses `clap::Subcommand` enum, add:

```rust
#[derive(Debug, Subcommand)]
enum XtaskCmd {
    // ... existing variants ...
    /// Generate the agentprof visual guide HTML site under docs/visual-guide/.
    VisualGuide(visual_guide::VisualGuideCmd),
}
```

And in the dispatcher `match` arm:

```rust
XtaskCmd::VisualGuide(c) => visual_guide::run(c),
```

If `xtask` uses raw `std::env::args` matching, add a `"visual-guide"` arm dispatching to `visual_guide::run` with `VisualGuideCmd::parse_from(args)`.

- [ ] **Step 4: Write surface test**

Create `xtask/tests/visual_guide_surface.rs`:

```rust
//! Verify `cargo xtask visual-guide --help` lists the subcommand.

use std::process::Command;

#[test]
fn visual_guide_help_lists_subcommand() {
    let out = Command::new(env!("CARGO"))
        .args(["xtask", "visual-guide", "--help"])
        .output()
        .expect("spawn cargo xtask");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{}{}", stdout, String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "expected success, got {:?}\n{}", out.status, combined);
    assert!(combined.contains("--clean"),  "missing --clean: {combined}");
    assert!(combined.contains("--check"),  "missing --check: {combined}");
}
```

- [ ] **Step 5: Verify build + test**

```bash
cargo check -p xtask 2>&1 | tail -3
cargo test -p xtask --test visual_guide_surface 2>&1 | tail -5
# expect: 1 passed
cargo xtask visual-guide --help 2>&1 | head -10
# expect: shows --clean, --check
```

- [ ] **Step 6: Commit**

```bash
git add xtask/src/main.rs xtask/src/visual_guide/mod.rs xtask/tests/visual_guide_surface.rs
git commit -m "feat(xtask): visual-guide subcommand stub (T2)

Adds \`cargo xtask visual-guide [--clean] [--check]\` skeleton. Both
flags are parsed but no-op at this stage — T3-T7 wire in the actual
shell + components + page generation.

1 surface test verifies --help lists both flags.

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 2

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

---

## End of plan skeleton.

Plan body fills in below as work progresses.

### Task 3: shell.rs — HTML shell (head_meta + favicon + nav + footer)

**Files:**
- Create: `xtask/src/visual_guide/shell.rs`
- Create: `xtask/src/visual_guide/templates/page.html`

**Reconnaissance first:**

Open the reference for visual conventions:
```bash
sed -n '50,150p' ~/course/langchain-visual-guide/src/shell.py
```

Note the structure:
- Inline base64 SVG favicon
- `head_meta(title, description)` produces `<meta>` tags + favicon `<link>`
- `page(filename, body_html, standalone, home_href)` wraps body in DOCTYPE + nav + footer
- `index_page(...)` is a sibling that wraps the index TOC

We mirror this shape in Rust + askama.

- [ ] **Step 1: Write failing unit test**

Append to `xtask/src/visual_guide/mod.rs` (or split into `mod.rs` `#[cfg(test)]` mod):

```rust
#[cfg(test)]
mod shell_smoke {
    use super::shell;

    #[test]
    fn page_includes_required_chrome() {
        let body = "<p>Hello agentprof.</p>";
        let html = shell::render_page(shell::PageMeta {
            title: "Test Lesson",
            description: "Test desc",
            section_label: "用法",
            home_href: "../index.html",
            prev: None,
            next: None,
        }, body).expect("render");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>agentprof 可视化指南 — Test Lesson</title>"));
        assert!(html.contains("data:image/svg+xml;base64,"));         // favicon
        assert!(html.contains("<nav"));
        assert!(html.contains("<footer"));
        assert!(html.contains(body));
    }
}
```

- [ ] **Step 2: Run test — fails (module doesn't exist)**

```bash
cargo test -p xtask shell_smoke 2>&1 | tail -5
# expect: FAIL with "unresolved module `shell`"
```

- [ ] **Step 3: Write minimal shell.rs**

Create `xtask/src/visual_guide/shell.rs`:

```rust
//! Shared HTML shell — DOCTYPE + head + nav + footer + favicon.
//!
//! Mirrors `langchain-visual-guide/src/shell.py` patterns: every lesson
//! page goes through `render_page`; the index page uses `render_index`
//! (T7). Both produce self-contained HTML so the site works from
//! `file://` and any static HTTP server.

use askama::Template;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

/// Inline SVG favicon — base64-encoded into a `data:` URL so pages
/// stay self-contained. Matches dashboard.css accent (#1a1a2e).
fn favicon_data_url() -> String {
    let svg = r#"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect width='32' height='32' rx='7' fill='#1a1a2e'/><text x='16' y='23' font-family='system-ui,sans-serif' font-size='20' font-weight='700' fill='#eee' text-anchor='middle'>a</text></svg>"#;
    format!("data:image/svg+xml;base64,{}", B64.encode(svg))
}

/// Per-page metadata.
pub struct PageMeta<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub section_label: &'a str,         // 例如 "用法" 或 "Wiki"
    pub home_href: &'a str,              // 例如 "../index.html"
    pub prev: Option<NavLink<'a>>,
    pub next: Option<NavLink<'a>>,
}

pub struct NavLink<'a> {
    pub href: &'a str,
    pub title: &'a str,
}

#[derive(Template)]
#[template(path = "page.html")]
struct PageTemplate<'a> {
    meta: &'a PageMeta<'a>,
    body_html: &'a str,
    favicon: &'a str,
    css: &'static str,
    pkg_version: &'static str,
    generated_at_utc: String,
    git_sha_short: &'a str,
}

/// Render a single lesson HTML page.
///
/// # Errors
///
/// Returns `askama::Error` if template rendering fails (should not
/// happen unless the template file under `templates/page.html` is
/// missing or malformed).
pub fn render_page(meta: PageMeta<'_>, body_html: &str) -> askama::Result<String> {
    let favicon = favicon_data_url();
    let tmpl = PageTemplate {
        meta: &meta,
        body_html,
        favicon: &favicon,
        css: super::css::ALL_CSS,
        pkg_version: env!("CARGO_PKG_VERSION"),
        generated_at_utc: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        git_sha_short: &super::git_sha_short_or_unknown(),
    };
    tmpl.render()
}
```

Also add to `xtask/src/visual_guide/mod.rs`:

```rust
pub mod shell;
pub mod css;     // T4 will fill this in; create empty file now: `pub const ALL_CSS: &str = "";`

/// Best-effort git short SHA; "unknown" on failure (e.g. CI without
/// .git). Footer-only; not security-sensitive.
fn git_sha_short_or_unknown() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}
```

- [ ] **Step 4: Create the askama template `xtask/src/visual_guide/templates/page.html`**

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>agentprof 可视化指南 — {{ meta.title }}</title>
<meta name="description" content="{{ meta.description }}">
<meta name="theme-color" content="#1a1a2e">
<link rel="icon" type="image/svg+xml" href="{{ favicon }}">
<meta property="og:type" content="article">
<meta property="og:site_name" content="agentprof 可视化指南">
<meta property="og:title" content="{{ meta.title }}">
<meta property="og:description" content="{{ meta.description }}">
<style>{{ css|safe }}</style>
</head>
<body>
<nav class="vg-top">
  <a class="brand" href="{{ meta.home_href }}">agentprof</a>
  <span class="section">· {{ meta.section_label }} ·</span>
  <span class="title">{{ meta.title }}</span>
  <span class="spacer"></span>
  {% match meta.prev %}{% when Some with (p) %}<a class="navlink" href="{{ p.href }}">← {{ p.title }}</a>{% when None %}{% endmatch %}
  <a class="navlink" href="{{ meta.home_href }}">目录</a>
  {% match meta.next %}{% when Some with (n) %}<a class="navlink" href="{{ n.href }}">{{ n.title }} →</a>{% when None %}{% endmatch %}
</nav>
<div class="vg-progress"><div id="vg-progress-bar"></div></div>
<main class="vg-main">
{{ body_html|safe }}
</main>
<footer class="vg-footer">
  agentprof 可视化指南 · v{{ pkg_version }} · 生成于 {{ generated_at_utc }} · git {{ git_sha_short }}<br>
  <a href="https://github.com/verdenmax/agentprof">GitHub</a> · MIT/Apache-2.0
</footer>
<script>
(function(){
  var bar = document.getElementById('vg-progress-bar');
  if(!bar) return;
  function update(){
    var h = document.documentElement;
    var max = (h.scrollHeight - h.clientHeight) || 1;
    bar.style.width = Math.round(100 * (h.scrollTop || document.body.scrollTop) / max) + '%';
  }
  window.addEventListener('scroll', update, { passive: true });
  update();
})();
</script>
</body>
</html>
```

- [ ] **Step 5: Add `base64` dep**

Verify whether `base64` is already in workspace:

```bash
grep -E "^base64" Cargo.toml
```

If absent, add to `xtask/Cargo.toml` dev-section (NOT workspace, since xtask is the only consumer):

```toml
base64 = "0.22"
```

- [ ] **Step 6: Run test — passes**

```bash
cargo test -p xtask shell_smoke 2>&1 | tail -5
# expect: 1 passed
```

If askama complains about template path resolution, verify `xtask/src/visual_guide/templates/` exists and askama 0.16 default lookup includes it. The askama 0.16 default `template_root` is `templates/` relative to the crate root, NOT relative to the source file — so the template MUST live at `xtask/templates/page.html`, not `xtask/src/visual_guide/templates/page.html`. **Fix path before retesting** if this happens.

If you hit this, move the template file:

```bash
mkdir -p xtask/templates/visual_guide
mv xtask/src/visual_guide/templates/page.html xtask/templates/visual_guide/page.html
```

And update the `#[template(path = "page.html")]` to `#[template(path = "visual_guide/page.html")]`.

- [ ] **Step 7: Commit**

```bash
git add xtask/Cargo.toml xtask/src/visual_guide/ xtask/templates/ Cargo.lock
git commit -m "feat(xtask): visual-guide shell + page template (T3)

Adds HTML shell (DOCTYPE + nav + footer + favicon + scroll progress
bar) for visual-guide lesson pages. Mirrors langchain-visual-guide
patterns: inline base64 SVG favicon (no extra HTTP), self-contained
templates work from file:// and any static server.

Includes 1 smoke test confirming the rendered page contains DOCTYPE,
title, favicon, nav, footer, and the supplied body verbatim.

T4 fills in css.rs with the actual design tokens; this commit
references an empty ALL_CSS const.

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 3

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

### Task 4: css.rs — design tokens + dark mode + responsive

**Files:**
- Modify (was empty stub from T3): `xtask/src/visual_guide/css.rs`

- [ ] **Step 1: Write failing smoke test**

Append to `xtask/src/visual_guide/mod.rs`:

```rust
#[cfg(test)]
mod css_smoke {
    use super::css;

    #[test]
    fn all_css_contains_required_tokens() {
        let css = css::ALL_CSS;
        // Light-mode root vars
        assert!(css.contains("--bg:"));
        assert!(css.contains("--ink:"));
        assert!(css.contains("--accent:"));
        // Dark-mode override
        assert!(css.contains("prefers-color-scheme: dark"));
        // Nav class used by shell.rs template
        assert!(css.contains(".vg-top"));
        assert!(css.contains(".vg-footer"));
        assert!(css.contains(".vg-main"));
        // Progress bar
        assert!(css.contains("#vg-progress-bar"));
        // Code block class used by T6 highlight.rs
        assert!(css.contains(".code"));
    }
}
```

- [ ] **Step 2: Run test — fails (ALL_CSS is empty string)**

```bash
cargo test -p xtask css_smoke 2>&1 | tail -3
```

- [ ] **Step 3: Fill in css.rs**

Replace the stub with:

```rust
//! CSS design tokens for the visual guide. Adapted from
//! `langchain-visual-guide/src/shell.py` palette but rebranded to
//! agentprof's dashboard accent (#1a1a2e) so both surfaces feel
//! like one product.
//!
//! Single concatenated `ALL_CSS` const inlined into every page by
//! `shell.rs::PageTemplate`. Keeping it as a single string means
//! self-contained HTML — no external stylesheet, works `file://`.

/// Concatenated CSS for the entire visual-guide site.
pub const ALL_CSS: &str = include_str!("guide.css");
```

Then create `xtask/src/visual_guide/guide.css`:

```css
* { box-sizing: border-box; margin: 0; padding: 0; }

:root {
  --bg: #f6f7f9; --panel: #ffffff; --panel-2: #f0f2f5;
  --ink: #1d2129; --muted: #5b6470; --faint: #8a939f; --line: #e1e5ea;
  --accent: #1a1a2e; --accent-soft: #e7e8f0; --accent-ink: #0e0e1a;
  --blue: #2563eb; --blue-soft: #e7efff;
  --amber: #b4690e; --amber-soft: #fdf1dd;
  --purple: #7c3aed; --purple-soft: #f0e9ff;
  --red: #d23f3f; --red-soft: #fbe6e6;
  --code-bg: #0f172a; --code-ink: #e2e8f0; --code-line: #1e293b;
  --shadow: 0 1px 2px rgba(16,24,40,.06), 0 8px 24px rgba(16,24,40,.06);
  --radius: 12px;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: #0e1116; --panel: #161b22; --panel-2: #1c232c;
    --ink: #e6edf3; --muted: #9aa6b2; --faint: #6e7a86; --line: #2a323c;
    --accent: #6e7eff; --accent-soft: #14152a; --accent-ink: #b8c1ff;
    --blue: #6ea8fe; --blue-soft: #16243f;
    --amber: #e0a44a; --amber-soft: #33270f;
    --purple: #b794f6; --purple-soft: #271a40;
    --red: #f08080; --red-soft: #3a1a1a;
    --code-bg: #0a0f1a; --code-ink: #d8e2f0; --code-line: #14202f;
    --shadow: 0 1px 2px rgba(0,0,0,.4), 0 10px 30px rgba(0,0,0,.35);
  }
}

html { scroll-behavior: smooth; overflow-x: hidden; }

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans SC",
               "PingFang SC", "Microsoft YaHei", system-ui, sans-serif;
  background: var(--bg); color: var(--ink); line-height: 1.75;
  -webkit-font-smoothing: antialiased;
}

a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }

code, .mono {
  font-family: "SF Mono", "JetBrains Mono", "Fira Code", ui-monospace, Menlo, Consolas, monospace;
  overflow-wrap: break-word;
}

/* ---- Top nav ---- */
.vg-top {
  background: var(--accent); color: #eee;
  padding: .7em 1.2em; display: flex; align-items: center;
  gap: 1em; flex-wrap: wrap; font-size: .92rem;
  position: sticky; top: 0; z-index: 10;
}
.vg-top .brand { color: #eee; font-weight: 700; font-size: 1.05rem; }
.vg-top .section, .vg-top .title { color: #b8c1ff; }
.vg-top .title { font-weight: 600; color: #eee; }
.vg-top .spacer { flex: 1; }
.vg-top .navlink { color: #eee; padding: .25em .55em; border-radius: 4px; }
.vg-top .navlink:hover { background: rgba(255,255,255,.12); text-decoration: none; }

/* ---- Scroll progress bar ---- */
.vg-progress {
  position: sticky; top: 0; height: 3px; background: transparent;
  pointer-events: none; z-index: 20;
}
#vg-progress-bar { height: 100%; width: 0; background: var(--accent); transition: width .1s linear; }

/* ---- Main content ---- */
.vg-main { padding: 1.5em 1.2em; max-width: 880px; margin: 0 auto; }
.vg-main h1 { margin: .3em 0 .7em; font-size: 2rem; color: var(--accent-ink); }
.vg-main h2 { margin: 1.8em 0 .5em; font-size: 1.4rem; border-bottom: 2px solid var(--accent-soft); padding-bottom: .25em; }
.vg-main h3 { margin: 1.2em 0 .4em; font-size: 1.1rem; }
.vg-main p { margin: 0 0 1em; }
.vg-main p.lead { font-size: 1.06rem; color: var(--muted); margin-top: -.4rem; }
.vg-main ul, .vg-main ol { margin: 0 0 1em 1.6em; }
.vg-main li { margin-bottom: .35em; }

/* ---- Tables ---- */
.vg-main table { border-collapse: collapse; width: 100%; margin: 1em 0; }
.vg-main th, .vg-main td { padding: .5em .7em; border-bottom: 1px solid var(--line); text-align: left; }
.vg-main th { background: var(--panel-2); font-weight: 600; }

/* ---- Cards / callouts ---- */
.card {
  background: var(--panel); border: 1px solid var(--line);
  border-radius: var(--radius); padding: 1em 1.2em;
  margin: 1em 0; box-shadow: var(--shadow);
}
.card .tag {
  display: inline-block; font-size: .75rem; padding: .15em .55em;
  border-radius: 4px; background: var(--accent-soft); color: var(--accent-ink);
  margin-bottom: .4em; font-weight: 600;
}
.card.analogy { background: var(--amber-soft); border-color: var(--amber); }
.card.analogy .tag { background: var(--amber); color: #fff; }
.card.warn { background: var(--red-soft); border-color: var(--red); }
.card.note { background: var(--blue-soft); border-color: var(--blue); }

/* ---- Accordion (折叠卡片) ---- */
details.accordion {
  background: var(--panel); border: 1px solid var(--line);
  border-radius: var(--radius); margin: 1em 0; overflow: hidden;
}
details.accordion summary {
  cursor: pointer; padding: 1em 1.2em; font-weight: 600;
  background: var(--panel-2); display: flex; align-items: center; gap: .7em;
}
details.accordion summary::-webkit-details-marker { display: none; }
details.accordion summary::after {
  content: "▸"; margin-left: auto; transition: transform .2s;
}
details.accordion[open] summary::after { transform: rotate(90deg); }
.badge-num {
  display: inline-flex; align-items: center; justify-content: center;
  width: 24px; height: 24px; border-radius: 50%;
  background: var(--accent); color: #fff; font-size: .85rem; font-weight: 700;
}
.hint { color: var(--faint); font-weight: 400; font-size: .85rem; margin-left: auto; }
.acc-body { padding: 1em 1.2em; border-top: 1px solid var(--line); }
.qa .q { font-weight: 600; color: var(--accent-ink); margin-top: .5em; }
.qa .a { margin-top: .25em; margin-bottom: .8em; }

/* ---- Code blocks ---- */
.code {
  display: block; background: var(--code-bg); color: var(--code-ink);
  padding: 1em 1.2em; border-radius: 8px; overflow-x: auto;
  font-family: "SF Mono", "JetBrains Mono", ui-monospace, monospace;
  font-size: .85rem; line-height: 1.55; white-space: pre;
  border: 1px solid var(--code-line); margin: .5em 0 1em;
}
.code .kw  { color: #c084fc; }   /* keyword */
.code .st  { color: #86efac; }   /* string */
.code .cm  { color: #94a3b8; font-style: italic; } /* comment */
.code .nm  { color: #fdba74; }   /* number */
.code .fn  { color: #93c5fd; }   /* function name */
.code .ty  { color: #67e8f9; }   /* type / struct */
.code .op  { color: #f0abfc; }   /* operator */

/* ---- Footer ---- */
.vg-footer {
  padding: 1em 1.2em; background: var(--panel-2); color: var(--muted);
  font-size: .82rem; text-align: center; border-top: 1px solid var(--line);
}
.vg-footer a { color: var(--accent); }

/* ---- Responsive ---- */
@media (max-width: 720px) {
  .vg-main { padding: 1em .8em; }
  .vg-top { font-size: .85rem; padding: .6em .9em; }
  .vg-top .title { display: none; }
}

/* ---- SVG diagrams ---- */
svg.diagram { max-width: 100%; height: auto; display: block; margin: 1em auto; }
img.shot {
  max-width: 100%; height: auto; display: block; margin: 1em auto;
  border: 1px solid var(--line); border-radius: 8px; box-shadow: var(--shadow);
}
img.shot + .caption { text-align: center; color: var(--muted); font-size: .85rem; margin-top: -.5em; }
```

- [ ] **Step 4: Run test — passes**

```bash
cargo test -p xtask css_smoke shell_smoke 2>&1 | tail -5
# expect: 2 passed
```

- [ ] **Step 5: Commit**

```bash
git add xtask/src/visual_guide/css.rs xtask/src/visual_guide/guide.css
git commit -m "feat(xtask): visual-guide CSS design tokens + dark mode (T4)

Adapted from langchain-visual-guide palette, rebranded to agentprof's
dashboard accent (#1a1a2e dark navy → matches the M2.3 web dashboard
chrome). Includes:
  - Light + dark theme via prefers-color-scheme
  - Sticky top nav with scroll progress bar
  - Cards (default + analogy + warn + note variants)
  - Accordion (details/summary) with rotated chevron
  - Code block with .kw/.st/.cm/.nm/.fn/.ty/.op syntax classes
    (consumed by T6 highlight.rs)
  - Responsive @media (max-width: 720px)
  - Image+SVG class .shot for screenshots / .diagram for flow charts

1 smoke test verifies all required CSS tokens + class names are
present so T3 shell + T5 components + T6 highlight all keep working
if anyone refactors guide.css.

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 4

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---
