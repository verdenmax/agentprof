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
