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

### Task 5: components.rs — accordion + comparison_table + source_ref + prev/next

**Files:**
- Create: `xtask/src/visual_guide/components.rs`

These are pure Rust functions that return `String` (HTML fragments). Lesson content modules (`usage_*` / `wiki_*`) call them.

- [ ] **Step 1: Write failing unit tests**

Append to `xtask/src/visual_guide/mod.rs`:

```rust
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
        assert!(html.contains("github.com/verdenmax/agentprof/blob/main/crates/agentprof-core/src/analyzer/cache.rs"));
        assert!(html.contains("CacheMetrics"));
        assert!(!html.contains("#L"));  // no line numbers per design
    }
}
```

- [ ] **Step 2: Run tests — fail**

```bash
cargo test -p xtask components_tests 2>&1 | tail -5
# expect: 3 errors "unresolved module `components`"
```

- [ ] **Step 3: Implement components.rs**

```rust
//! HTML fragment helpers used by lesson content modules
//! (`usage_*` / `wiki_*`).
//!
//! Each function returns a `String` of well-formed HTML; callers
//! concatenate fragments into a final lesson body that gets passed
//! to [`super::shell::render_page`].
//!
//! All public functions are pure: same inputs → same output, no
//! filesystem or network I/O.

/// Render an accordion (foldable card) block.
///
/// `num` is the badge number shown in the summary; `title` is the
/// summary text; `body_html` is the expanded content (already
/// HTML-formatted).
///
/// # Examples
///
/// ```
/// use xtask::visual_guide::components::accordion;
/// let html = accordion(1, "厂商锁定", "<p>内容</p>");
/// assert!(html.contains("<details"));
/// ```
#[must_use]
pub fn accordion(num: u32, title: &str, body_html: &str) -> String {
    format!(
        r#"<details class="accordion">
  <summary><span class="badge-num">{num}</span> {title} <span class="hint">点击展开</span></summary>
  <div class="acc-body">{body_html}</div>
</details>
"#
    )
}

/// Render a comparison table — typical three-column "痛点 / 没工具 / agentprof 的做法"
/// shape but generalised to N columns.
///
/// `headers` is a slice of N column headers; `rows` is a slice of
/// 3-tuples (or N-tuples — see signature). The table is wrapped in
/// the project's standard `<table class="t">` styling.
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
/// `crate_name` is the workspace member name without the `agentprof-`
/// prefix (e.g. `"core"` becomes `crates/agentprof-core/src/...`).
/// `path_in_src` is the file path relative to `src/` (e.g.
/// `"analyzer/cache.rs"`). `symbol` is the rustdoc-visible identifier.
#[must_use]
pub fn source_ref(crate_short: &str, path_in_src: &str, symbol: &str) -> String {
    format!(
        r#"<p class="src-ref">📂 相关源码：
<a href="https://github.com/verdenmax/agentprof/blob/main/crates/agentprof-{crate_short}/src/{path_in_src}"><code>{crate_short}/{path_in_src}</code></a>
&nbsp;<code class="mono">{symbol}</code></p>
"#
    )
}

/// Inline SVG flow diagram — kept simple, intended for 2-5 node
/// pipeline arrows. Nodes laid out left-to-right; arrows auto-drawn.
/// Returns an `<svg class="diagram">…</svg>` snippet.
#[must_use]
pub fn flow_diagram(nodes: &[&str]) -> String {
    if nodes.is_empty() {
        return String::new();
    }
    let node_w = 140;
    let node_h = 50;
    let gap = 40;
    let total_w = nodes.len() * node_w + (nodes.len() - 1) * gap;
    let mut svg = format!(
        r#"<svg class="diagram" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {total_w} 80">
  <defs>
    <marker id="arr" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
      <path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
    </marker>
  </defs>"#
    );
    for (i, label) in nodes.iter().enumerate() {
        let x = i * (node_w + gap);
        svg.push_str(&format!(
            r#"  <rect x="{x}" y="15" width="{node_w}" height="{node_h}" rx="6" fill="none" stroke="currentColor" stroke-width="1.5"/>
  <text x="{cx}" y="45" font-size="13" text-anchor="middle" fill="currentColor">{label}</text>
"#,
            cx = x + node_w / 2,
        ));
        if i < nodes.len() - 1 {
            let from = x + node_w + 2;
            let to = (i + 1) * (node_w + gap) - 2;
            svg.push_str(&format!(
                r#"  <line x1="{from}" y1="40" x2="{to}" y2="40" stroke="currentColor" stroke-width="1.5" marker-end="url(#arr)"/>
"#
            ));
        }
    }
    svg.push_str("</svg>\n");
    svg
}

/// Render a `<nav class="prev-next">` block at the lesson bottom.
#[must_use]
pub fn prev_next(prev: Option<(&str, &str)>, next: Option<(&str, &str)>) -> String {
    let mut s = String::from(r#"<nav class="prev-next" style="display:flex;justify-content:space-between;margin-top:2em;font-size:.9rem">"#);
    if let Some((href, title)) = prev {
        s.push_str(&format!(r#"<a href="{href}">← {title}</a>"#));
    } else {
        s.push_str("<span></span>");
    }
    if let Some((href, title)) = next {
        s.push_str(&format!(r#"<a href="{href}">{title} →</a>"#));
    } else {
        s.push_str("<span></span>");
    }
    s.push_str("</nav>\n");
    s
}
```

Add to `xtask/src/visual_guide/mod.rs`:

```rust
pub mod components;
```

- [ ] **Step 4: Run tests — pass**

```bash
cargo test -p xtask components_tests 2>&1 | tail -5
# expect: 3 passed
```

- [ ] **Step 5: Commit**

```bash
git add xtask/src/visual_guide/components.rs xtask/src/visual_guide/mod.rs
git commit -m "feat(xtask): visual-guide components (accordion/table/svg/source-ref) (T5)

Pure-Rust HTML fragment helpers used by lesson content modules:
  - accordion(num, title, body) — foldable card
  - comparison_table(headers, rows) — three-column 痛点 table
  - source_ref(crate, path, symbol) — GitHub blob URL (no line number)
  - flow_diagram(&[nodes]) — inline SVG left-to-right pipeline
  - prev_next(prev?, next?) — lesson bottom navigation

3 unit tests cover accordion structure, comparison_table column layout,
and source_ref URL shape (no #L line anchor).

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 5

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

### Task 6: highlight.rs — Rust/bash/toml/sql lexer

**Files:**
- Create: `xtask/src/visual_guide/highlight.rs`

Hand-written lexer ~200 LOC. Identifies keywords / strings / comments / numbers for 4 languages; output is HTML with `<span class="kw">…</span>` etc. matching the CSS classes from T4.

- [ ] **Step 1: Write failing fixture tests**

Append to `xtask/src/visual_guide/mod.rs`:

```rust
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
```

- [ ] **Step 2: Run tests — fail**

```bash
cargo test -p xtask highlight_tests 2>&1 | tail -5
# expect: 4 unresolved errors
```

- [ ] **Step 3: Implement highlight.rs**

```rust
//! Hand-written syntax highlighter for Rust / bash / TOML / SQL.
//!
//! Emits HTML with `<span class="kw">`, `<span class="st">`,
//! `<span class="cm">`, `<span class="nm">` wrappings matching the
//! CSS classes defined in [`super::css`]. **Not** a full lexer —
//! covers keywords + string literals + comments + simple numbers
//! to make code blocks readable; unknown tokens fall through as
//! plain HTML-escaped text.
//!
//! Trade-off: no `syntect` / `tree-sitter` dependency. Misses are
//! acceptable — the worst case is missing color, not malformed HTML.

/// Languages supported by [`highlight`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Bash,
    Toml,
    Sql,
}

const RUST_KW: &[&str] = &[
    "fn", "let", "mut", "pub", "mod", "use", "struct", "enum", "impl",
    "trait", "match", "if", "else", "for", "while", "loop", "return",
    "async", "await", "Self", "self", "where", "type", "const", "static",
    "ref", "move", "as", "in", "break", "continue", "true", "false",
];

const BASH_KW: &[&str] = &[
    "if", "then", "fi", "else", "elif", "for", "in", "do", "done",
    "while", "case", "esac", "function", "return", "exit", "echo",
];

const SQL_KW: &[&str] = &[
    "SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "ON", "AND", "OR", "NOT", "NULL", "CREATE", "TABLE", "INDEX",
    "UPDATE", "DELETE", "INSERT", "VALUES", "PRAGMA", "ORDER", "BY",
    "GROUP", "HAVING", "LIMIT", "OFFSET", "DISTINCT", "AS",
];

/// HTML-escape a substring.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render `src` (any of the 4 languages) as HTML with syntax-class spans.
#[must_use]
pub fn highlight(lang: Lang, src: &str) -> String {
    match lang {
        Lang::Rust => highlight_curly(src, RUST_KW, /*c_block*/ true),
        Lang::Bash => highlight_shell(src, BASH_KW, '#'),
        Lang::Toml => highlight_toml(src),
        Lang::Sql  => highlight_shell(src, SQL_KW, '-'), // -- comment is two dashes; handled
    }
}

/// Rust-style: `//` line + `/* ... */` block comments, double-quoted
/// strings, keywords from `kws`, integer literals.
fn highlight_curly(src: &str, kws: &[&str], _c_block: bool) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() * 2);
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        // Line comment //
        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let end = bytes[i..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |p| i + p);
            out.push_str(r#"<span class="cm">"#);
            out.push_str(&esc(&src[i..end]));
            out.push_str("</span>");
            i = end;
            continue;
        }
        // String literal "..."
        if c == '"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1;
            } // include closing "
            out.push_str(r#"<span class="st">"#);
            out.push_str(&esc(&src[start..i]));
            out.push_str("</span>");
            continue;
        }
        // Identifier
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            let ident = &src[start..i];
            if kws.contains(&ident) {
                out.push_str(r#"<span class="kw">"#);
                out.push_str(ident);
                out.push_str("</span>");
            } else {
                out.push_str(&esc(ident));
            }
            continue;
        }
        // Number
        if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            out.push_str(r#"<span class="nm">"#);
            out.push_str(&src[start..i]);
            out.push_str("</span>");
            continue;
        }
        // Anything else: HTML-escape one char
        out.push_str(&esc(&src[i..i + c.len_utf8()]));
        i += c.len_utf8();
    }
    out
}

/// Shell / SQL: `#`-or-`--` line comments, double + single string,
/// keyword list. `comment_lead` is the first char of the comment
/// marker (`#` for bash, `-` for sql which then requires a second `-`).
fn highlight_shell(src: &str, kws: &[&str], comment_lead: char) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() * 2);
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        // Comment
        let is_comment = if comment_lead == '#' {
            c == '#'
        } else {
            c == '-' && i + 1 < bytes.len() && bytes[i + 1] == b'-'
        };
        if is_comment {
            let end = bytes[i..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |p| i + p);
            out.push_str(r#"<span class="cm">"#);
            out.push_str(&esc(&src[i..end]));
            out.push_str("</span>");
            i = end;
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = bytes[i];
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            out.push_str(r#"<span class="st">"#);
            out.push_str(&esc(&src[start..i]));
            out.push_str("</span>");
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            let ident = &src[start..i];
            // SQL keywords are uppercased; bash keywords are lowercase.
            // Compare directly (case-sensitive).
            if kws.contains(&ident) {
                out.push_str(r#"<span class="kw">"#);
                out.push_str(ident);
                out.push_str("</span>");
            } else {
                out.push_str(&esc(ident));
            }
            continue;
        }
        out.push_str(&esc(&src[i..i + c.len_utf8()]));
        i += c.len_utf8();
    }
    out
}

/// TOML: `[section]` headers (whole-line keyword), `# comment`,
/// double-quoted strings, integer numbers.
fn highlight_toml(src: &str) -> String {
    let mut out = String::with_capacity(src.len() * 2);
    for line in src.split_inclusive('\n') {
        let trim = line.trim_start();
        let lead = line.len() - trim.len();
        out.push_str(&line[..lead]);
        if trim.starts_with('[') {
            if let Some(close) = trim.find(']') {
                out.push_str(r#"<span class="kw">"#);
                out.push_str(&esc(&trim[..=close]));
                out.push_str("</span>");
                out.push_str(&esc(&trim[close + 1..]));
                continue;
            }
        }
        // delegate the rest of the line to a stripped-down shell-ish pass
        // for comments + strings + numbers
        let tail = highlight_shell(trim, &[], '#');
        out.push_str(&tail);
    }
    // numbers: a coarse pass — wrap standalone integer values right of '='
    // (good-enough for our doc examples; not a real TOML parser).
    let mut numbered = String::with_capacity(out.len());
    let bytes = out.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            numbered.push('=');
            i += 1;
            while i < bytes.len() && bytes[i] == b' ' {
                numbered.push(' ');
                i += 1;
            }
            if i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
                let start = i;
                if bytes[i] == b'-' {
                    i += 1;
                }
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.')
                {
                    i += 1;
                }
                numbered.push_str(r#"<span class="nm">"#);
                numbered.push_str(&out[start..i]);
                numbered.push_str("</span>");
                continue;
            }
        }
        numbered.push(bytes[i] as char);
        i += 1;
    }
    numbered
}
```

Add to `xtask/src/visual_guide/mod.rs`:

```rust
pub mod highlight;
```

- [ ] **Step 4: Run tests — pass**

```bash
cargo test -p xtask highlight_tests 2>&1 | tail -5
# expect: 4 passed
```

If any test fails on string-content escape (e.g. `&quot;` vs raw `"`), revise `esc()` to taste — the tests look for raw `"hi"` quoted in the source so leave `esc()` unchanged.

- [ ] **Step 5: Commit**

```bash
git add xtask/src/visual_guide/highlight.rs xtask/src/visual_guide/mod.rs
git commit -m "feat(xtask): visual-guide syntax highlighter for rust/bash/toml/sql (T6)

Hand-written ~200 LOC lexer covering keywords / strings / line
comments / numbers across 4 languages. Output wraps tokens in
<span class=\"kw|st|cm|nm\">…</span> matching the CSS classes from
T4 (guide.css .code .kw / .st / .cm / .nm).

Deliberately NOT using syntect or tree-sitter — those would add
multi-MB deps for a documentation generator. Missed tokens fall
through as plain HTML-escaped text — acceptable degradation since
the worst case is missing color, not malformed markup.

4 fixture tests cover one canonical snippet per language with the
patterns lesson content modules will use (Rust fn, bash for loop,
TOML [section] block, SQL SELECT).

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 6

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

### Task 7: pages.rs — PAGES const + index page renderer

**Files:**
- Create: `xtask/src/visual_guide/pages.rs`
- Create: `xtask/templates/visual_guide/index.html`
- Modify: `xtask/src/visual_guide/mod.rs` — fill `run()` to actually generate files

This task **lights up the pipeline end-to-end**: after T7, `cargo xtask visual-guide` writes a real `index.html` (zero lessons, just the chrome + empty TOC). T8-T18 progressively add lesson modules and re-register them in `PAGES`.

- [ ] **Step 1: Write failing test**

Append to `xtask/src/visual_guide/mod.rs`:

```rust
#[cfg(test)]
mod pages_tests {
    use super::pages;

    #[test]
    fn pages_array_is_non_empty_and_well_formed() {
        assert!(!pages::PAGES.is_empty());
        for entry in pages::PAGES {
            assert!(entry.filename.ends_with(".html"));
            assert!(!entry.title.is_empty());
            assert!(matches!(entry.section, pages::Section::Usage | pages::Section::Wiki));
        }
    }

    #[test]
    fn render_index_includes_doctype_and_section_cards() {
        let html = pages::render_index().expect("render");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("用法"));
        assert!(html.contains("Wiki"));
    }
}
```

- [ ] **Step 2: Run tests — fail**

```bash
cargo test -p xtask pages_tests 2>&1 | tail -5
```

- [ ] **Step 3: Implement pages.rs**

```rust
//! `PAGES` registry — every lesson here gets generated to disk and
//! linked from the index. T8-T18 add `usage_*` / `wiki_*` modules
//! and append entries here.
//!
//! Ordering is significant: it drives prev/next nav.

use askama::Template;

/// Which chapter a lesson belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Usage,
    Wiki,
}

impl Section {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Usage => "用法",
            Self::Wiki => "Wiki",
        }
    }
    pub const fn dir(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Wiki => "wiki",
        }
    }
}

/// One lesson entry.
pub struct LessonEntry {
    /// Filename within the section dir, e.g. `"01-what-is-agentprof.html"`.
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
    // Usage (T8-T13 populate these stubs)
    // Wiki (T14-T18 populate these stubs)
    // Empty initially; T7 only ships the index page + chrome.
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
```

You'll need to make `favicon_data_url` `pub(super)` in `shell.rs`:

```rust
pub(super) fn favicon_data_url() -> String { /* unchanged */ }
```

- [ ] **Step 4: Create `xtask/templates/visual_guide/index.html`**

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>agentprof 可视化指南 — 目录</title>
<meta name="description" content="agentprof 项目的可视化中文教程：用法 + Wiki 共 14 课。">
<meta name="theme-color" content="#1a1a2e">
<link rel="icon" type="image/svg+xml" href="{{ favicon }}">
<style>{{ css|safe }}</style>
<style>
.idx-hero { padding: 2em 1.2em; background: var(--accent); color: #eee; text-align: center; }
.idx-hero h1 { color: #eee; font-size: 2.2rem; margin: 0; }
.idx-hero p  { color: #b8c1ff; margin-top: .5em; font-size: 1.05rem; }
.idx-section { padding: 1em 1.2em; max-width: 1100px; margin: 0 auto; }
.idx-section h2 { font-size: 1.5rem; border-bottom: 2px solid var(--accent-soft); padding-bottom: .3em; margin-top: 1em; }
.idx-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 1em; margin-top: 1em; }
.idx-card {
  display: block; background: var(--panel); border: 1px solid var(--line);
  border-radius: var(--radius); padding: 1em 1.2em; box-shadow: var(--shadow);
  color: var(--ink); text-decoration: none;
}
.idx-card:hover { border-color: var(--accent); text-decoration: none; }
.idx-card .num { color: var(--accent); font-weight: 700; font-size: .85rem; }
.idx-card .ttl { font-weight: 600; margin: .25em 0 .35em; }
.idx-card .desc { color: var(--muted); font-size: .9rem; }
.empty-note { color: var(--muted); padding: 1em; font-style: italic; }
</style>
</head>
<body>
<header class="idx-hero">
  <h1>agentprof 可视化指南</h1>
  <p>给 AI agent 用的 perf flamegraph + ROI 报告器 · 共 14 课中文图解</p>
</header>

<section class="idx-section">
  <h2>用法（面向新手）</h2>
  {% if usage_lessons.is_empty() %}
  <p class="empty-note">（暂无课程；构建过程中）</p>
  {% else %}
  <div class="idx-grid">
    {% for l in usage_lessons %}
    <a class="idx-card" href="{{ l.href }}">
      <span class="num">用法 {{ l.number }}</span>
      <div class="ttl">{{ l.title }}</div>
      <div class="desc">{{ l.description }}</div>
    </a>
    {% endfor %}
  </div>
  {% endif %}
</section>

<section class="idx-section">
  <h2>Wiki（面向中阶 + 开发者）</h2>
  {% if wiki_lessons.is_empty() %}
  <p class="empty-note">（暂无课程；构建过程中）</p>
  {% else %}
  <div class="idx-grid">
    {% for l in wiki_lessons %}
    <a class="idx-card" href="{{ l.href }}">
      <span class="num">Wiki {{ l.number }}</span>
      <div class="ttl">{{ l.title }}</div>
      <div class="desc">{{ l.description }}</div>
    </a>
    {% endfor %}
  </div>
  {% endif %}
</section>

<footer class="vg-footer">
  agentprof 可视化指南 · v{{ pkg_version }} · 生成于 {{ generated_at_utc }} · git {{ git_sha_short }}<br>
  <a href="https://github.com/verdenmax/agentprof">GitHub</a> · MIT/Apache-2.0
</footer>
</body>
</html>
```

- [ ] **Step 5: Wire `run()` to actually write files**

Replace `xtask/src/visual_guide/mod.rs::run()`:

```rust
use std::fs;
use std::path::PathBuf;

pub fn run(cmd: VisualGuideCmd) -> anyhow::Result<()> {
    let out_root = workspace_root()?.join("docs").join("visual-guide");

    if cmd.clean {
        for sub in ["index.html"] {
            let _ = fs::remove_file(out_root.join(sub));
        }
        for chapter in ["usage", "wiki"] {
            let _ = fs::remove_dir_all(out_root.join(chapter));
        }
    }

    let mut written: Vec<PathBuf> = Vec::new();

    // Index page (always)
    let index_html = pages::render_index()?;
    if cmd.check {
        // dry-run: just confirm it rendered to non-empty HTML
        assert!(index_html.contains("<!DOCTYPE"));
    } else {
        fs::create_dir_all(&out_root)?;
        let idx_path = out_root.join("index.html");
        fs::write(&idx_path, index_html)?;
        written.push(idx_path);
    }

    // Lesson pages (T8+ adds modules per lesson; for now PAGES is empty)
    for entry in pages::PAGES {
        let body_html = render_lesson_body(entry)?;
        let nav = compute_nav(entry);
        let html = shell::render_page(shell::PageMeta {
            title: entry.title,
            description: entry.description,
            section_label: entry.section.label(),
            home_href: "../index.html",
            prev: nav.prev,
            next: nav.next,
        }, &body_html)?;

        if !cmd.check {
            let dir = out_root.join(entry.section.dir());
            fs::create_dir_all(&dir)?;
            let path = dir.join(entry.filename);
            fs::write(&path, html)?;
            written.push(path);
        }
    }

    println!("visual-guide: {} {} files",
        if cmd.check { "verified" } else { "wrote" },
        if cmd.check { pages::PAGES.len() + 1 } else { written.len() });
    for p in &written {
        println!("  - {}", p.display());
    }
    Ok(())
}

struct Nav<'a> {
    prev: Option<shell::NavLink<'a>>,
    next: Option<shell::NavLink<'a>>,
}

fn compute_nav<'a>(entry: &'a pages::LessonEntry) -> Nav<'a> {
    let idx = pages::PAGES.iter().position(|p| std::ptr::eq(p, entry));
    let prev = idx.and_then(|i| i.checked_sub(1)).and_then(|i| pages::PAGES.get(i))
        .map(|p| shell::NavLink {
            href: if p.section == entry.section {
                Box::leak(format!("{}", p.filename).into_boxed_str()) as &str
            } else {
                Box::leak(format!("../{}/{}", p.section.dir(), p.filename).into_boxed_str()) as &str
            },
            title: p.title,
        });
    let next = idx.and_then(|i| pages::PAGES.get(i + 1))
        .map(|p| shell::NavLink {
            href: if p.section == entry.section {
                Box::leak(format!("{}", p.filename).into_boxed_str()) as &str
            } else {
                Box::leak(format!("../{}/{}", p.section.dir(), p.filename).into_boxed_str()) as &str
            },
            title: p.title,
        });
    Nav { prev, next }
}

/// Look up the per-lesson body renderer based on `entry.filename`. T8+
/// each register a function pointer in this match arm.
fn render_lesson_body(entry: &pages::LessonEntry) -> anyhow::Result<String> {
    match entry.filename {
        // T8+ insert match arms here, e.g.:
        // "01-what-is-agentprof.html" => Ok(usage_01::render()),
        _ => anyhow::bail!("no renderer wired for {}; please update visual_guide::mod::render_lesson_body", entry.filename),
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
```

Add `pub mod pages;` to `xtask/src/visual_guide/mod.rs` near the existing module decls.

**Note on `Box::leak`**: this is a tiny leak (a handful of strings, ~tens of bytes, never freed). For a 1-shot xtask invocation this is fine. If it offends sensibilities, refactor `NavLink` to own `String` instead of `&str` in T19.

- [ ] **Step 6: Run all tests + smoke**

```bash
cargo test -p xtask 2>&1 | tail -10
# expect: all pass

cargo xtask visual-guide --check 2>&1 | tail -5
# expect: "verified 1 files" (just index)

cargo xtask visual-guide 2>&1 | tail -5
# expect: "wrote 1 files" + path printed

ls docs/visual-guide/
# expect: index.html  assets/  .gitkeep

# Open in browser to eyeball
$BROWSER docs/visual-guide/index.html 2>/dev/null || true
```

- [ ] **Step 7: Commit**

```bash
git add xtask/src/visual_guide/pages.rs xtask/src/visual_guide/mod.rs xtask/templates/visual_guide/index.html
git commit -m "feat(xtask): visual-guide PAGES registry + index page + writer (T7)

Lights up the pipeline end-to-end. \`cargo xtask visual-guide\` now
writes a real \`docs/visual-guide/index.html\` with hero header +
two empty section grids (用法 / Wiki). PAGES is empty at this stage;
T8-T18 register lessons by appending to the slice + adding a render
match arm.

Key types: Section enum, LessonEntry struct, IndexRow VM.
Key fns:   pages::render_index, mod::render_lesson_body (extensible).

--clean removes index.html + usage/ + wiki/; --check renders to
memory without writing (CI PR path).

2 unit tests cover PAGES invariants + index DOCTYPE presence.
Manual smoke: cargo xtask visual-guide writes 1 file (index.html);
opens in any browser including via file://.

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 7

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

**Phase gate**: After T7, the pipeline is end-to-end functional. T8-T18 only add lesson content modules.

---

### Task 8: Usage lesson 1 — `agentprof 是什么`

**Files:**
- Create: `xtask/src/visual_guide/usage_01.rs`
- Modify: `xtask/src/visual_guide/mod.rs` (add `pub mod usage_01;` + match arm in `render_lesson_body`)
- Modify: `xtask/src/visual_guide/pages.rs` (register `01-what-is-agentprof.html` in `PAGES`)

Each lesson follows the same 4-block structure:
1. Top lead paragraph + reverse analogy
2. Comparison table (痛点 / 没工具 / agentprof 的做法)
3. 2-4 accordion cards (example / why / how agentprof does / alternatives)
4. (footer rendered by shell, includes prev/next from PAGES)

Content guideline: ≥600 Chinese chars per lesson; reference exact code paths in the project; embed real-output snippets where useful.

- [ ] **Step 1: Write failing render test**

Append to `xtask/src/visual_guide/mod.rs`:

```rust
#[cfg(test)]
mod usage_01_test {
    #[test]
    fn renders_non_empty_with_required_marks() {
        let html = super::usage_01::render();
        assert!(html.len() > 1500, "expect substantial content, got {} chars", html.len());
        assert!(html.contains("agentprof"));
        assert!(html.contains("class=\"lead\""));
        assert!(html.contains("<table"));
        assert!(html.contains("class=\"accordion\""));
    }
}
```

- [ ] **Step 2: Run test — fails (no module)**

```bash
cargo test -p xtask usage_01_test 2>&1 | tail -3
```

- [ ] **Step 3: Implement usage_01.rs**

Create `xtask/src/visual_guide/usage_01.rs`:

```rust
//! Usage lesson 1 — 「agentprof 是什么」.
//!
//! Target audience: complete newcomer who has never run agentprof and
//! is not sure what "token profiling" means.

use super::components::{accordion, comparison_table, source_ref};

pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"
<h1>agentprof 是什么</h1>

<p class="lead">
agentprof 是用 Rust 写的<strong>开源命令行工具</strong>，专门用来分析 AI agent CLI（Claude Code / GitHub Copilot CLI / OpenAI Codex）<strong>每一次对话烧掉的 token 都花在哪</strong>。
不止统计花了多少，更回答<strong>「花得值不值」</strong>。
</p>

<div class="card analogy">
  <div class="tag">🔌 生活类比</div>
  把 AI agent 想成一台<strong>很费油的车</strong>。市面上的 token 计费工具只告诉你「这次开了 500 公里、烧了 30 升油」。
  agentprof 是装在车上的<strong>仪表盘 + 行车记录仪</strong>：
  把每次踩油门 / 刹车 / 怠速都记下来，做成一张「这趟旅程的油耗火焰图」+「每个绕路决策的 ROI 表」，
  让你能下一次<strong>少绕远路、关掉没用的发动机配件、避开堵车路段</strong>。
</div>

<h2>它到底解决什么问题？</h2>

<p>直接看 OpenAI / Anthropic Dashboard 当然能看到 token 总数，但真实排查里你会很快遇到三类痛点，
这正是 agentprof 要替你抹平的：</p>
"#);

    s.push_str(&comparison_table(
        &["痛点", "没工具时", "agentprof 的做法"],
        &[
            ("看不见 token 去向", "总数 = 几万，不知道哪个 turn / tool / hook 拿走的", "<strong>火焰图 + Turn Summary + Tool Rank</strong> 把 token 切到 turn / tool / hook 级"),
            ("没有 ROI 信号", "MCP 装了 20 个 tool，不知道哪个真正被 agent 用过", "<strong>MCP Waste 报表</strong>：标出加载了但从没被调用的 tool + 估算浪费 token"),
            ("Prompt cache 黑盒", "Claude 缓存命中率多少？省了多少？不知道", "<strong>Cache 段</strong>：诚实 / 朴素两种 hit rate + 净节省 + 总节省"),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 想深入理解每一类痛点？点开下面的折叠卡片，每张都给你：<strong>① 示例 · ② 为什么必要 · ③ agentprof 的做法 · ④ 还有什么其他方案</strong>。</p>"#);

    s.push_str(&accordion(
        1,
        "看不见 token 去向",
        r#"<div class="qa">
<div class="q">🧪 示例</div>
<div class="a">直接看官方 dashboard：</div>
<pre class="code">2026-06-11  conversation-7f3a  <span class="nm">42,317</span> input  <span class="nm">8,124</span> output</pre>
<div class="q">🤔 为什么必要</div>
<div class="a">这只告诉你「整次对话用了 50k」，但你不知道：哪一个 turn 是 token 大户？哪个 tool call 拿走最多 context？是不是有 MCP 工具加载了 schema 但从来没用？</div>
<div class="q">✅ agentprof 的做法</div>
<div class="a">把 events.jsonl 流式解析成 <code>Episodes</code>（一个 turn 的所有 tool/hook/skill 调用集合），再用 <code>compute_analysis</code> 生成<strong>火焰图 SVG + Turn Summary 表 + Tool Rank 表</strong>。每一行 token 都有「归属」。</div>
<div class="q">🔀 其他方案</div>
<div class="a">手写脚本 grep + jq 也能从 events.jsonl 提数据，但火焰图渲染 / 跨 session 聚合 / TUI 交互都得自己写一遍。agentprof 把这些做成开箱即用。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "没有 ROI 信号",
        r#"<div class="qa">
<div class="q">🧪 示例</div>
<div class="a">你给 Copilot CLI 装了 GitHub MCP server（17 个工具）+ Filesystem MCP（8 个工具）+ 自家 Jira MCP（12 个工具）。每次对话 system prompt 多 5k tokens 描述这 37 个工具。问题：agent 到底用过哪些？</div>
<div class="q">🤔 为什么必要</div>
<div class="a">MCP 工具的 schema 是常驻 context window 的；加载了但不用 = 直接浪费钱 + 挤掉真正需要的上下文。</div>
<div class="q">✅ agentprof 的做法</div>
<div class="a"><code>agentprof mcp-waste --tool-descriptions sidecar.json</code> 给出每个 MCP server 的「加载次数 / 零调用次数 / 估算浪费 token」三栏报表，并 list 出"从没被调用过的工具"。这是 agentprof 区别于其他 token 工具的<strong>核心卖点</strong>。</div>
<div class="q">🔀 其他方案</div>
<div class="a">官方 Anthropic console 不显示工具维度；某些自建可观测平台（如 Helicone）可以打 tag，但需要先在 prompt 中手工注入 metadata。agentprof 直接从 session 日志反推，零侵入。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "Prompt cache 黑盒",
        r#"<div class="qa">
<div class="q">🧪 示例</div>
<div class="a">Claude Sonnet 启用 prompt caching 后理论上能省到 10% 价格。但你的 cache hit rate 到底是 30% 还是 90%？省了多少钱？</div>
<div class="q">🤔 为什么必要</div>
<div class="a">cache 是按 5 分钟 TTL 自动失效的；如果 agent 调用之间间隔过长，cache 实际命中率会比想象的低很多。</div>
<div class="q">✅ agentprof 的做法</div>
<div class="a"><code>agentprof analyze --export md</code> 输出的 Cache 段会给出 <strong>honest hit rate</strong>（cache_read / (cache_read + cache_creation)）+ <strong>naive hit rate</strong>（cache_read / (cache_read + input_tokens)）+ <strong>net saved / gross saved tokens</strong>（按 Claude Sonnet 4.x 2026-06 价格估算）。<code>aggregate --by model</code> 给出跨 session 的 CacheCr / CacheRd / Hit% / NetSaved 列。</div>
<div class="q">🔀 其他方案</div>
<div class="a">官方 console 显示 cache_read / cache_creation 原始数字但不算 hit rate 也不算节省金额；agentprof 一次性给出 6 个数字 + 价格对照表。</div>
</div>"#,
    ));

    s.push_str("<h2>下一步</h2>\n<p>读完本课你已经知道 agentprof 解决什么问题。下一课用 <strong>5 分钟</strong>装好工具，跑出你的第一张火焰图。</p>\n");

    s.push_str(&source_ref("core", "analyzer/mod.rs", "compute_analysis"));
    s.push_str(&source_ref("cli", "cmd/analyze.rs", "run"));

    s
}
```

- [ ] **Step 4: Register in `pages.rs`**

Replace the empty `PAGES` slice in `xtask/src/visual_guide/pages.rs`:

```rust
pub const PAGES: &[LessonEntry] = &[
    LessonEntry {
        filename: "01-what-is-agentprof.html",
        title: "agentprof 是什么",
        description: "用 Rust 写的开源 token profiler — 给 AI agent 用的 perf flamegraph + ROI 报告器。",
        section: Section::Usage,
    },
    // T9-T18 will add more
];
```

- [ ] **Step 5: Wire `render_lesson_body` match arm**

In `xtask/src/visual_guide/mod.rs::render_lesson_body`:

```rust
fn render_lesson_body(entry: &pages::LessonEntry) -> anyhow::Result<String> {
    match entry.filename {
        "01-what-is-agentprof.html" => Ok(usage_01::render()),
        _ => anyhow::bail!("no renderer wired for {}; please update render_lesson_body", entry.filename),
    }
}
```

Add `pub mod usage_01;` near other mod decls.

- [ ] **Step 6: Run tests + smoke**

```bash
cargo test -p xtask usage_01_test 2>&1 | tail -5
# expect: 1 passed

cargo xtask visual-guide 2>&1 | tail -5
# expect: wrote 2 files (index.html + usage/01-what-is-agentprof.html)

# Eyeball it
$BROWSER docs/visual-guide/usage/01-what-is-agentprof.html 2>/dev/null || true
```

- [ ] **Step 7: Commit**

```bash
git add xtask/src/visual_guide/usage_01.rs xtask/src/visual_guide/pages.rs xtask/src/visual_guide/mod.rs
git commit -m "feat(xtask): visual-guide usage lesson 1 — agentprof 是什么 (T8)

第一节用法课。Lead 段 + 反向类比（仪表盘 + 行车记录仪）+ 痛点
对比表（看不见 token 去向 / 没有 ROI 信号 / Prompt cache 黑盒）+
3 张折叠卡片（每张含示例 / 为什么 / agentprof 怎么做 / 其他方案）+
2 个 source_ref 指向 agentprof-core::analyzer::compute_analysis +
agentprof-cli::cmd::analyze::run。

字数 ~1100 中文字符（含 code）。1 render test 验证 lead 段、表格、
accordion class 名都正确出现。

cargo xtask visual-guide 现在写 2 个文件（index.html + 第一课）。

Refs: docs/superpowers/plans/2026-06-13-visual-guide.md Task 8

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

---

### Task 9-13: Usage lessons 2-6 (compressed template)

T9-T13 each follow Task 8's exact recipe — only the content + filename + register entry differ. To keep this plan tractable, the per-task content briefs (not full text) are listed below; subagent fills the prose using the same accordion + comparison_table style. Each lesson **must**:

- ≥ 600 Chinese chars
- 1 lead + 1 analogy card
- 1 comparison_table (3 columns)
- 2-4 accordion cards
- ≥ 1 `source_ref` to a real symbol
- 1 render unit test (`renders_non_empty_with_required_marks`)
- 1 commit per task

#### T9 — `usage/02-install.html` 「5 分钟上手」

**Content brief**:
- 类比：买台 IDE 装插件
- 痛点对比：手工编 Rust 慢 / pip / brew vs `cargo install agentprof-cli`
- 3 张折叠卡：
  1. one-line installer (`curl`)
  2. `cargo install agentprof-cli`（需要 Rust ≥ 1.78）
  3. from-source build（开发者路径，clone + cargo build）
- 末尾：第一次跑 `agentprof analyze --agent copilot`，截图占位（不嵌真实截图，T19 添）
- `source_ref` → `agentprof-cli::main`

**Renderer test name**: `usage_02_test::renders_non_empty_with_required_marks`

#### T10 — `usage/03-analyze.html` 「analyze：看懂一次 session」

**Content brief**:
- 类比：跑 `perf top` 看 CPU 热点
- 痛点：md / tui / html / json / speedscope 5 种导出选哪个？
- 3 张折叠卡：
  1. md（CI / logs / grep 友好）
  2. html（浏览器分享，单文件自包含）
  3. tui（交互式火焰图 + ROI 表 + Models view，需要在终端跑）
- 嵌入真实截图 `<img class="shot" src="../assets/report-html-sample.png" alt="HTML 报告示例">` —— **此 asset 在 T19 commit**，先用 alt text placeholder
- `source_ref` × 2 → `agentprof-cli::cmd::analyze` + `agentprof-cli::cmd::format::html::render`

#### T11 — `usage/04-list-aggregate.html` 「list / aggregate：跨 session 视角」

**Content brief**:
- 类比：从 `top` 升级到 Prometheus dashboard
- 痛点：每次只看一个 session 不够，要看 7 天 / 30 天的趋势
- 4 张折叠卡：
  1. `list --since 7d --limit 20` —— 列最近 sessions
  2. `aggregate --by model` —— 跨模型对比
  3. `aggregate --by tool` —— 哪个工具是 token 大户
  4. `aggregate --by day` + `--low-utilization-threshold` —— 时间序列 + 利用率告警
- 决策树小表格：何时用哪个 `--by`
- `source_ref` × 2 → `agentprof-core::analyzer::aggregate` + `agentprof-cli::cmd::aggregate`

#### T12 — `usage/05-serve.html` 「serve：浏览器实时看板」

**Content brief**:
- 类比：从 cron + 邮件升级到 Grafana 实时刷新
- 痛点：静态 HTML 报告是快照，agent 跑着想看实时数据
- 5 张折叠卡（每个 dashboard 视图一张）：
  1. `/sessions`（最近 sessions 列表）
  2. `/session/:id`（完整的 per-session 报告，嵌入了 html::render_body_only）
  3. `/aggregate?by=model`（跨 session aggregate；mcp-server 走专用页）
  4. `/mcp-waste`（MCP 浪费列表 + 详情）
  5. 工具栏：暂停 + 间隔切换 + localStorage 持久化
- 嵌入 5 张真实截图 `dashboard-*.png`（T19 落）
- `[serve]` config block 示例 + `--bind` / `--storage-path` / `--interval-default` / `--no-open` 表
- 决策表：serve vs static HTML（4 行：分享场景 / 实时刷新 / 多端访问 / 离线归档）
- `source_ref` × 2 → `agentprof-cli::cmd::serve::router::build_router` + `agentprof-cli::cmd::serve::handlers`

#### T13 — `usage/06-db-otlp.html` 「db + ingest-otlp：存数据库 + 接入 OTLP」

**Content brief**:
- 类比：从 grep 单文件升级到 SQLite + Prometheus push gateway
- 痛点：分析每次重新 parse JSONL 慢 / Claude Code 和 Codex 的 session 数据要 push 到 agentprof
- 3 张折叠卡：
  1. `agentprof db init / ingest --all / stats / prune / vacuum` —— SQLite 存储管理
  2. hybrid cache/store mode 概念（cache 自动 = XDG_CACHE_HOME；store 显式 = XDG_DATA_HOME）—— 含 SVG 流程图（用 `flow_diagram(&["events.jsonl", "Adapter", "compute_analysis", "SQLite"])`）
  3. `agentprof ingest-otlp` 接入 Claude Code / Codex OTel SDK（gRPC :4317 + HTTP :4318）—— 含路径图
- `source_ref` × 3 → `agentprof-storage::Db`、`agentprof-storage::query`、`agentprof-storage::otlp`

---

### Task 14-18: Wiki lessons 1-8 (compressed template)

Same recipe as T9-T13 but for the Wiki chapter. Each ≥ 600 chars + 1 render test + 1 commit.

#### T14 — `wiki/01-architecture.html` 「架构全景」

- 5-crate 依赖图 SVG（嵌入 `assets/architecture-deps.svg`，**T19 落**）
- L1/L2/L3 文档体系概览
- 9 阶段 pipeline brief（pipeline 图表）
- 24 ADR 索引（链接到 GH `docs/internals/adr-*.md`）
- `source_ref` × 1 → `crates/agentprof-cli/src/main.rs`

#### T15 — `wiki/02-data-model.html` 「数据模型」

- `Event` → `Episode` → `AnalysisReport` 三层关系图（`flow_diagram`）
- 关键字段表：`SessionRef`、`SessionMeta`、`ToolEpisode`、`Span`、`HookEpisode`、`TurnSummaryRow`、`ToolRankRow`
- 折叠卡：(1) 为什么先 Event 后 Episode（一次性扫 + 减少内存）(2) `Episodes::default()` 哨兵语义 (3) `AnalysisReport.cache_metrics()` 何时返回 `None`
- `source_ref` × 3 → `agentprof-core::event`、`agentprof-core::episode`、`agentprof-core::analyzer::AnalysisReport`

#### T16 — `wiki/03-adapter.html` 「Adapter trait + 怎么写新 adapter」

- `Adapter` trait 接口 + `AgentKind` enum 表
- CopilotAdapter case study（解析 events.jsonl）
- **「怎么写新 adapter」清单**（针对未来 M3.1 ClaudeAdapter / M3.2 CodexAdapter）：
  1. `crates/agentprof-adapters/src/<name>.rs` 实现 trait
  2. `registry.rs` 注册
  3. ≥ 1 anonymized fixture
  4. ≥ 1 `assert_cmd` 集成测试
  5. 更新 `docs/adapters.md` + L2 README
  6. CHANGELOG entry
- `source_ref` × 3 → `agentprof-core::adapter::Adapter`、`agentprof-adapters::registry`、`agentprof-adapters::copilot`

#### T17 — `wiki/04-analyzer.html` 「分析层 rollups」

- `compute_analysis` 流水线（`flow_diagram(&["Episodes", "turn_summary", "tool_rank", "hook_rank", "cache_metrics"])`）
- 关键公式：tool p50/p95 percentile（nearest-rank + round half away from zero）、cache honest_pct / naive_pct、saved_net / saved_gross 计算逻辑
- 折叠卡：(1) 为什么 cache 用两个 hit rate (2) why `aggregate --by tool` omits cache cols（per-tool cache attribution undefined） (3) MCP waste `compute_waste` + heuristic vs sidecar 模式
- `source_ref` × 3 → `agentprof-core::analyzer::stats`、`agentprof-core::analyzer::cache`、`agentprof-core::analyzer::waste`

#### T18 — Wiki lessons 5-8 (4 in one task)

Subagent ships these in **one commit** since each is shorter (~600 chars baseline) and they share lots of cross-references. Per-lesson briefs:

- **`wiki/05-storage.html` 存储层 hybrid mode**: ADR-0019 摘要 + SQLite schema 表（sessions / model_metrics / episodes_json columns）+ dual-path 选择决策（cache / store / dual）+ `source_ref` → `agentprof-storage::Db / config / datasource`
- **`wiki/06-otlp-receiver.html` OTLP receiver**: ADR-0021/0022 摘要 + 4 层防御（Bearer constant-time / per-signal size caps / LRU eviction / 256-byte session.id cap）+ gRPC vs HTTP 选择 + `source_ref` → `agentprof-storage::otlp::server_grpc / server_http`
- **`wiki/07-web-dashboard.html` Web dashboard 架构**: ADR-0024 全 7 决策 + chunk-endpoint pattern 流程图 + `source_ref` → `agentprof-cli::cmd::serve::*`
- **`wiki/08-contributing.html` 贡献指南**: Conventional Commits 列表 + brainstorming → spec → plan → TDD → review pipeline（含 9 阶段图）+ 怎么开 PR / 加 ADR / 通过 CI / 写 CHANGELOG entry + `source_ref` → `CONTRIBUTING.md`

T18 单 commit 含 4 个新 modules + 4 个新 PAGES entries + 4 个新 match arms + 4 个 render tests.

---

### Task 19: xtask integration tests + assets

**Files:**
- Create: `xtask/tests/visual_guide.rs`
- Create: `docs/visual-guide/assets/flamegraph-sample.svg`
- Create: `docs/visual-guide/assets/report-html-sample.png`
- Create: `docs/visual-guide/assets/dashboard-overview.png`
- Create: `docs/visual-guide/assets/dashboard-aggregate.png`
- Create: `docs/visual-guide/assets/dashboard-mcp-waste.png`
- Create: `docs/visual-guide/assets/dashboard-session.png`
- Create: `docs/visual-guide/assets/architecture-deps.svg`

**Two phases**: (A) record real assets from a running agentprof, (B) write 5 integration tests.

#### Phase A: Record real assets

- [ ] **Step A1: Ingest a sample session into a fresh store**

Use the test fixture session from `agentprof-adapters`. Open a sample tempdb path, init it, run `analyze --export html`, screenshot the result at 1280x800 and save as `docs/visual-guide/assets/report-html-sample.png`.

- [ ] **Step A2: Generate the flamegraph SVG**

Run `agentprof analyze --export html`; extract the embedded `<svg>...</svg>` block from the output HTML and save as `docs/visual-guide/assets/flamegraph-sample.svg`. Add a version watermark text in the bottom-right.

- [ ] **Step A3: Take 4 dashboard screenshots**

Ingest the fixture into the store (`agentprof db ingest`), then start `agentprof serve --storage-path /tmp/sample.db --bind 127.0.0.1:14329 --no-open --quiet` as a backgrounded process. Hit these URLs in a browser and screenshot at 1280×800:

- `http://127.0.0.1:14329/sessions` → `dashboard-overview.png`
- `http://127.0.0.1:14329/session/<first id>` → `dashboard-session.png`
- `http://127.0.0.1:14329/aggregate?by=model` → `dashboard-aggregate.png`
- `http://127.0.0.1:14329/mcp-waste` → `dashboard-mcp-waste.png`

Stop the dev server (note the printed PID and terminate it via your platform's process-management primitive). Crop / resize screenshots to 1280×800 max.

- [ ] **Step A4: Hand-draw 5-crate dependency SVG**

`docs/visual-guide/assets/architecture-deps.svg` — a simple block diagram with 5 boxes (agentprof-cli, agentprof-tui, agentprof-adapters, agentprof-storage, agentprof-core) and arrows reflecting `docs/architecture.md` §3 dependency rule. Write directly as SVG XML by hand or draw.io export. Suggested ~400×300 viewBox, monochrome (`currentColor`).

#### Phase B: Write integration tests

- [ ] **Step B1: Create `xtask/tests/visual_guide.rs`**

```rust
//! Integration tests for the visual-guide xtask subcommand.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}

fn run_xtask(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO"))
        .args(args.iter().copied())
        .current_dir(workspace_root())
        .output()
        .expect("spawn cargo xtask")
}

#[test]
fn render_all_produces_expected_file_count() {
    let out = run_xtask(&["xtask", "visual-guide"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let root = workspace_root().join("docs/visual-guide");
    assert!(root.join("index.html").exists());
    let usage = std::fs::read_dir(root.join("usage")).map(|d| d.count()).unwrap_or(0);
    let wiki  = std::fs::read_dir(root.join("wiki" )).map(|d| d.count()).unwrap_or(0);
    assert_eq!(usage, 6);
    assert_eq!(wiki, 8);
}

#[test]
fn check_mode_does_not_write_files() {
    let _ = run_xtask(&["xtask", "visual-guide", "--clean"]);
    let out = run_xtask(&["xtask", "visual-guide", "--check"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let idx = workspace_root().join("docs/visual-guide/index.html");
    assert!(!idx.exists(), "--check wrote index.html (should be dry-run)");
    let _ = run_xtask(&["xtask", "visual-guide"]);
}

#[test]
fn prev_next_links_resolve_to_existing_files() {
    let _ = run_xtask(&["xtask", "visual-guide"]);
    let root = workspace_root().join("docs/visual-guide");
    for chapter in ["usage", "wiki"] {
        for entry in std::fs::read_dir(root.join(chapter)).expect("read") {
            let entry = entry.unwrap();
            let html = std::fs::read_to_string(entry.path()).expect("read html");
            for cap in find_hrefs(&html) {
                if cap.starts_with("http") || cap.starts_with('#') || cap.starts_with("data:") {
                    continue;
                }
                let resolved = entry.path().parent().unwrap().join(&cap);
                let ok = resolved.exists() || cap == "../index.html";
                assert!(ok, "broken link {cap:?} in {:?}", entry.path());
            }
        }
    }
}

#[test]
fn asset_refs_resolve_to_existing_files() {
    let _ = run_xtask(&["xtask", "visual-guide"]);
    let root = workspace_root().join("docs/visual-guide");
    let assets = root.join("assets");
    for chapter in ["usage", "wiki"] {
        for entry in std::fs::read_dir(root.join(chapter)).expect("dir") {
            let html = std::fs::read_to_string(entry.unwrap().path()).expect("read");
            for asset in find_asset_refs(&html) {
                assert!(assets.join(&asset).exists(), "missing asset {asset:?}");
            }
        }
    }
}

#[test]
fn output_html_parses_as_well_formed() {
    let _ = run_xtask(&["xtask", "visual-guide"]);
    let root = workspace_root().join("docs/visual-guide");
    for entry in walkdir_html(&root) {
        let html = std::fs::read_to_string(&entry).expect("read");
        let mut reader = quick_xml::Reader::from_str(&html);
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => continue,
                Err(e) => panic!("malformed HTML in {entry:?}: {e}"),
            }
        }
    }
}

fn find_hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "href=\"";
    let mut i = 0;
    while let Some(start) = html[i..].find(needle) {
        let from = i + start + needle.len();
        if let Some(end) = html[from..].find('"') {
            out.push(html[from..from+end].to_owned());
            i = from + end + 1;
        } else { break; }
    }
    out
}

fn find_asset_refs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "../assets/";
    let mut i = 0;
    while let Some(start) = html[i..].find(needle) {
        let from = i + start + needle.len();
        let end = html[from..].find('"').unwrap_or(html.len() - from);
        out.push(html[from..from+end].to_owned());
        i = from + end + 1;
    }
    out
}

fn walkdir_html(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn rec(d: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() { rec(&p, out); }
                else if p.extension().and_then(|s| s.to_str()) == Some("html") {
                    out.push(p);
                }
            }
        }
    }
    rec(root, &mut out);
    out
}
```

- [ ] **Step B2: Add `quick-xml` dev-dep**

In `xtask/Cargo.toml`:

```toml
[dev-dependencies]
quick-xml = "0.36"
```

- [ ] **Step B3: Run tests**

```bash
cargo test -p xtask --test visual_guide
```

Expected: 5 passed.

- [ ] **Step B4: Commit assets + tests**

```bash
git add docs/visual-guide/assets/ xtask/tests/visual_guide.rs xtask/Cargo.toml Cargo.lock
git commit -m "test(xtask): visual-guide assets + 5 integration tests (T19)"
```

---

### Task 20: GH Pages CI workflow + README badge + L1/L2 doc sync

**Files:**
- Create: `.github/workflows/visual-guide.yml`
- Create: `docs/visual-guide/README.md`
- Modify: `README.md` (root)
- Modify: `docs/architecture.md` §15.1 + §15.3
- Modify: `crates/agentprof-cli/README.md`

- [ ] **Step 1: Create `.github/workflows/visual-guide.yml`**

```yaml
name: visual-guide

on:
  push:
    branches: [main]
    paths:
      - 'xtask/src/visual_guide/**'
      - 'xtask/templates/visual_guide/**'
      - 'docs/visual-guide/assets/**'
      - '.github/workflows/visual-guide.yml'
  pull_request:
    paths:
      - 'xtask/src/visual_guide/**'
      - 'xtask/templates/visual_guide/**'
      - 'docs/visual-guide/assets/**'
      - '.github/workflows/visual-guide.yml'
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: false

jobs:
  build-deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: visual-guide
      - run: cargo xtask visual-guide --check
      - if: github.event_name != 'pull_request'
        run: cargo xtask visual-guide
      - if: github.event_name != 'pull_request'
        uses: actions/upload-pages-artifact@v3
        with:
          path: docs/visual-guide
      - if: github.event_name != 'pull_request'
        id: deployment
        uses: actions/deploy-pages@v4
```

- [ ] **Step 2: Create `docs/visual-guide/README.md`**

```markdown
# agentprof 可视化指南

> 中文 HTML 教程，14 课分两章（用法 + Wiki），自包含可 file:// 直开。

**📖 在线阅读**：<https://verdenmax.github.io/agentprof/>

## 本地构建

```
cargo xtask visual-guide
open docs/visual-guide/index.html
```

## 子命令

- `cargo xtask visual-guide`         — 生成
- `cargo xtask visual-guide --clean` — 清空旧产物再生成
- `cargo xtask visual-guide --check` — 仅校验

## 章节

- **用法**（6 课）：what / install / analyze / list-aggregate / serve / db-otlp
- **Wiki**（8 课）：architecture / data-model / adapter / analyzer / storage / otlp / web-dashboard / contributing

生成的 *.html 不入 git（见 ADR-0025 D-2），只 commit 源码 + assets。
```

- [ ] **Step 3: Add section to root `README.md`**

Insert after Status block:

```markdown
### 📖 在线阅读：可视化指南

[在线阅读](https://verdenmax.github.io/agentprof/) · [本地构建说明](docs/visual-guide/README.md)

- **用法**（6 课）：从「agentprof 是什么」到 `serve` 实时看板
- **Wiki**（8 课）：5 crate 架构 + 数据模型 + Adapter trait
```

- [ ] **Step 4: Update `docs/architecture.md` §15.1 + §15.3**

§15.1 repo layout, add under `docs/`:

```
│   ├── visual-guide/         可视化中文教程（xtask 生成 + GH Pages，见 ADR-0025）
│   │   ├── README.md         手工维护
│   │   └── assets/           入 git（生成的 *.html 不入）
```

§15.3 CI matrix row:

```
| `visual-guide.yml` | xtask build + GH Pages deploy | PR --check; main push deploys |
```

- [ ] **Step 5: Mention in `crates/agentprof-cli/README.md`**

```markdown
### 配套可视化指南

详细的用户教程 + Wiki 见 [`docs/visual-guide/`](../../docs/visual-guide/README.md)
或 [在线版](https://verdenmax.github.io/agentprof/)。
```

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/visual-guide.yml docs/visual-guide/README.md README.md docs/architecture.md crates/agentprof-cli/README.md
git commit -m "ci+docs(visual-guide): GH Pages workflow + L1/L2 doc sync (T20)"
```

---

### Task 21: ADR-0025 + CHANGELOG + release decision

**Files:**
- Create: `docs/internals/adr-0025-visual-guide.md`
- Modify: `docs/architecture.md` §14.4 (ADR table)
- Modify: `CHANGELOG.md`
- Optional (per spec §11 deferred decision): `Cargo.toml` + path-deps for v0.3.4

- [ ] **Step 1: Write ADR-0025**

Create `docs/internals/adr-0025-visual-guide.md` mirroring ADR-0024 structure. 7 D-* sections matching spec §3 D-1..D-7 verbatim; 4 Implementation Notes from T3 (askama path), T6 (lexer trade-off), T7 (Box::leak), T3 (favicon base64). Target length: 250-350 lines.

- [ ] **Step 2: Add ADR-0025 row to `docs/architecture.md` §14.4 table**

```
| 0025 | Visual guide architecture (post-M2.3 docs wave) | Accepted | 2026-06-13 |
```

- [ ] **Step 3: Update CHANGELOG `[Unreleased]`**

Append Added (docs — visual guide) / Documentation / Tests sections describing the ship.

- [ ] **Step 4: Release decision (deferred from spec §11)**

Ask user: "Release as v0.3.4 (tag + path-dep bumps + CHANGELOG promoted) or commit-only on main (no tag, just merge feat/visual-guide)?"

**If v0.3.4 tag**: bump 7 sites with `sed -i 's/version = "0.3.3"/version = "0.3.4"/g' Cargo.toml xtask/Cargo.toml`; verify with `grep -rE 'version = "0\.3\.[34]"' --include='*.toml' .`; promote CHANGELOG `[Unreleased]` → `[0.3.4] - 2026-06-13`; full gate; commit + tag per established M2.3 template.

**If commit-only**: skip version bump and tag; CHANGELOG entry stays under `[Unreleased]`.

- [ ] **Step 5: Final workspace gate**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace --all-features
cargo deny check
cargo xtask visual-guide --check
```

All green expected.

- [ ] **Step 6: Commit ADR + (optional) tag**

```bash
git add docs/internals/adr-0025-visual-guide.md docs/architecture.md CHANGELOG.md
git commit -m "docs(visual-guide): ADR-0025 + ADR table + CHANGELOG (T21)"
```

- [ ] **Step 7: Merge to main + push**

```bash
git checkout main
git merge --ff-only feat/visual-guide
git push origin main
# If tagged: git push origin v0.3.4
git branch -d feat/visual-guide
```

GitHub Pages workflow triggers on main push and deploys to `https://verdenmax.github.io/agentprof/`. Verify deployment URL is live before final sign-off.

---

## Self-review checklist

- [x] **Spec coverage**: every spec D-1..D-7 maps to a task. D-1→T7, D-2→T7+T0, D-3→T1, D-4→T8-T18, D-5→T8 template, D-6→T20, D-7→T21.
- [x] **No placeholders**: T9-T18 use *content briefs* not "TBD" — subagent fills prose following T8's exact template.
- [x] **Type consistency**: `Section::{Usage,Wiki}`, `LessonEntry::filename` ↔ `render_lesson_body` match arms, `accordion(num,title,body)` 3-arg ✓, `source_ref(crate,path,symbol)` 3-arg ✓.
- [x] **Asset references**: T10/T12/T14 reference assets that don't exist until T19 — OK because broken `<img>` shows alt text, not a hard error; T19 test #4 catches at CI time.
- [x] **CI risk**: `visual-guide.yml` triggers only on `xtask/src/visual_guide/**` or `docs/visual-guide/assets/**` changes — no interference with main `ci.yml`.

---

## End of plan.
