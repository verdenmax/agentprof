# ADR-0025: Visual Guide Architecture (`docs/visual-guide/` + `cargo xtask visual-guide`)

**Status:** Accepted (2026-06-13)
**Context:** v0.3.3 ship complete; project lacks a Chinese illustrated tutorial for newcomers + a source-code navigation map for developers.
**Implements:** [`docs/superpowers/specs/2026-06-13-visual-guide-design.md`](../superpowers/specs/2026-06-13-visual-guide-design.md)
**Supersedes:** None
**Superseded by:** None
**Related:**
- [ADR-0024](adr-0024-web-dashboard-architecture.md) — web dashboard; borrows the same askama 0.16 template engine and template-resolution conventions, but ships HTML to GH Pages instead of an axum server.
- [ADR-0014](adr-0014-v0.1.0-release-strategy.md) — cargo-dist release strategy; the visual guide deliberately lives **outside** the cargo-dist release pipeline (docs-only, GH Pages, no binary artifact).

---

## Context

AgentProf already publishes documentation through **four channels**:

1. `README.md` — install + quickstart for users.
2. `docs/architecture.md` — L1 architecture for contributors.
3. `docs/internals/adr-*.md` — L3 decision records.
4. rustdoc — public API reference.

Each channel has a specific audience and an explicit gap:

- The README assumes the reader already knows what an "AI agent ROI report" is.
- `architecture.md` is text-dense and reads as reference, not as a walkthrough.
- ADRs document *why* decisions were made, not *how to use* the resulting feature.
- rustdoc is per-symbol and presupposes that the reader has navigated to the right
  module.

What is missing is a **single-entry, illustrated, story-shaped tutorial** that:

- guides newcomers from `cargo install agentprof` to "I understand what this report
  means" in one read-through;
- helps prospective contributors form a map of the 5-crate workspace before
  diving into source code;
- is friendly enough to share with non-technical stakeholders to communicate the
  product;
- is reachable from one URL (`https://verdenmax.github.io/agentprof/`).

The reference implementation is [`langchain-visual-guide`](https://github.com/verdenmax/langchain-visual-guide):
14 lessons / two chapters / reverse-analogy style / accordion-cards layout /
self-contained static HTML deployed to GH Pages. This ADR records the seven
decisions taken when adapting that recipe to a Rust project, plus seven
implementation-time discoveries that fell out of T1–T20.

---

## Decisions

### D-1: Single site + two chapters (shared shell)

The visual guide ships as **one** GH Pages site with two top-level chapters —
"用法" (6 newcomer lessons) and "Wiki" (8 developer lessons) — sharing the same
HTML chrome (header, nav, footer, CSS). This mirrors the `langchain-visual-guide`
shape because that layout is already well understood by the project owner and is
proven to be readable.

Alternatives rejected:

- **Two separate sites** (one for users, one for developers): double the
  maintenance, two domains to remember, no cross-linking between newcomer
  examples and the source code they reference.
- **Single site with JS-driven tab switching**: forces a SPA shell, breaks
  `file://` preview, breaks per-lesson SEO and bookmarking, fails when JS is
  disabled.
- **`mdBook`**: opinionated theme that cannot reach the reference design
  (accordion cards, reverse-analogy comparison tables, hand-drawn SVG dep
  diagram); customising it past those defaults is more work than a hand-rolled
  template.
- **Hand-written HTML per page**: 14 lessons × duplicated chrome means every
  CSS / header / nav tweak is a 14-file edit.

The shipped design — one shell template + 14 lesson modules feeding into it —
is the minimum infrastructure that supports both audiences without compromise.

### D-2: HTML output not in git (generated; only source + assets committed)

The committed surface is `xtask/src/visual_guide/`, `xtask/templates/visual_guide/`,
and `docs/visual-guide/assets/`. The rendered `docs/visual-guide/*.html` files
are **not committed**; CI regenerates them on every push to `main` and deploys
to GH Pages.

This is an **explicit reversal** of `langchain-visual-guide`'s convention,
where rendered HTML is committed. Rationale:

- Diff noise: every content tweak shows both the source-of-truth Rust edit and
  the regenerated HTML, doubling review surface area.
- Double-review risk: a contributor could edit only the HTML, drift it from the
  Rust source, and have CI pass anyway. With "HTML not in git" the source is
  the single source of truth.
- GH Pages already provides the served artifact; storing it in `main` adds
  nothing.

The trade-off — contributors must run `cargo run -p xtask -- visual-guide`
locally to preview — is acceptable because the build is <3 seconds and the
README documents the command. CI runs `--check` on PRs to guarantee the source
still renders before merge.

### D-3: Rust xtask + askama (not Python build.py)

The generator lives in the existing `xtask` crate as a new
`xtask::visual_guide` module and uses askama 0.16 (already a workspace
dependency, shared with `agentprof-cli`'s dashboard templates).

Alternatives rejected:

- **Python `build.py`** (as in `langchain-visual-guide`): drags Python into the
  CI matrix, adds a second toolchain for contributors, and breaks the project's
  "everything is Rust" promise.
- **JS / Node** (e.g. eleventy / Vite): same toolchain-bloat objection, plus
  npm dependency churn in a Rust project would be a maintenance burden out of
  proportion to the feature.

Staying in Rust + askama means: zero new top-level workspace dependencies
(base64 0.22 and quick-xml 0.36 are xtask-only); cargo-deny allowlist stays
small; CI keeps the same `cargo` invocation; the same `clap` derive style used
elsewhere in xtask powers the new subcommand.

### D-4: MVP scope — 14 lessons (6 usage + 8 wiki)

The shipped MVP covers exactly the features delivered through v0.3.3:
analyze / list / aggregate / mcp-waste / watch / serve on the usage side, and
core / adapters / storage / analyzer / OTLP / dashboard / cache analytics /
project conventions on the developer side.

Out of scope for MVP:

- Phase 3 multi-agent specifics (M3.1 ClaudeAdapter, M3.2 CodexAdapter) — those
  ship in v0.4.0 and will get their own Wiki lessons then.
- Per-feature deep-dive tutorials (e.g. "writing a custom adapter end-to-end") —
  belong in `docs/adapters.md`, not in the visual guide.
- Interactive quizzes (a `langchain-visual-guide` feature listed as F3
  followup).

Scoping to 14 lessons keeps the initial PR reviewable and matches the
reference site's information density, which is known to be readable in one
sitting.

### D-5: Style — Chinese + reverse-analogy + accordion cards

Every lesson follows the same template: lead paragraph → analogy card
(mapping an agentprof concept to a familiar real-world object) → comparison
table → 2–4 accordion cards (each with an "open by default" first card) →
"源码索引" section linking to the relevant Rust files. Body language is
Chinese, code samples and identifiers stay in English.

Rationale:

- Consistency across 14 lessons trains the reader's pattern recognition: once
  they understand the layout of lesson 1, all 14 are predictable.
- Reverse-analogy ("if you already know X, this is X-but-for-AI-agents") works
  well for technical readers and is proven on the reference site.
- Accordion cards keep the long lessons scannable; users can collapse the
  details they already know.
- Chinese body language matches the project owner's primary documentation
  language and the existing audience for the related ADRs and `architecture.md`
  sections that already mix Chinese commentary with English code.

### D-6: GitHub Pages CI integration

A new `.github/workflows/visual-guide.yml` runs on every PR (with `--check`,
under 30 seconds) and on every push to `main` (full render + `actions/deploy-pages@v4`).
Concurrency group is `pages`; cancel-in-progress is true so only the latest
main push deploys.

This was straightforward to choose because:

- agentprof has no existing GH Pages site, so there is no path conflict.
- `actions/deploy-pages@v4` is the official, supported path; no third-party
  action needed.
- The PR `--check` mode (which compares the rendered byte-count to a previous
  baseline and fails if either rendering panics or hash drifts) catches both
  hard failures (template parse error) and silent drift (someone edited the
  template without updating the test baseline).

Alternatives considered and rejected:

- `gh-pages` branch + force-push: works but is less observable in the GitHub UI
  than the modern Pages deployment.
- Per-commit deploy without concurrency control: causes races and wasted CI
  minutes when multiple commits land within minutes.

### D-7: No SemVer bump (commit-only release path)

The brainstorm raised two options for shipping:

- **Option A: cut v0.3.4** with the visual guide as the headline change.
- **Option B: commit-only on main**, leave the entry under `[Unreleased]` until
  the next real feature release rolls it up.

**Decided: Option B.** The visual guide is a pure documentation increment with
zero impact on public Rust API, CLI surface, SQLite schema, or runtime
behaviour. SemVer-bumping a Rust workspace just to document existing features
would push a "release with no real code changes" tag into the public release
stream, which devalues the tag history and obliges cargo-dist to build binaries
that are byte-identical to v0.3.3.

The next real feature release (v0.3.4 or v0.4.0, whichever comes first) will
roll up `[Unreleased]` entries — including this one — naturally. Users who want
the guide before then read it at `https://verdenmax.github.io/agentprof/`,
which is updated independently of binary releases.

---

## Implementation Notes

### Note 1: askama 0.16 template path resolution

askama 0.16 resolves `#[template(path = "…")]` relative to
`<crate-root>/templates/`, **not** relative to the source file that contains
the `#[derive(Template)]`. T3's initial layout placed `page.html` under
`xtask/src/visual_guide/templates/` and failed to compile with an opaque
"template not found" error. Working location is
`xtask/templates/visual_guide/page.html`. The constraint is documented inline
in `xtask/src/visual_guide/shell.rs` rustdoc so future template additions don't
re-discover it.

### Note 2: Owning `Nav` struct instead of `Box::leak`

T7 (lesson navigation) initially planned to use `Box::leak(format!(…).into_boxed_str())`
to convert dynamically built lesson URLs into the `&'static str` that
`shell::NavLink` originally required. The shipped implementation instead
introduces an owning `Nav { prev: Option<(String, String)>, next: Option<(String, String)>, … }`
struct whose lifetime spans exactly one page render. This avoids the leak with
no measurable complexity overhead and stays well-aligned with the project's
"no unwrap, no leak" coding guidelines.

### Note 3: Hand-written lexer over syntect / tree-sitter

T6 (syntax highlighting) ships ~200 LOC of hand-written Rust / bash / TOML /
SQL lexers instead of taking on syntect (~3 MB binary footprint, brings the
`onig` regex engine) or tree-sitter (~500 KB plus per-language WASM grammars).
Misses fall through as plain HTML-escaped text — the worst-case is missing
colour, never malformed markup. T6 unit tests confirm the 4 canonical patterns
that lesson modules use (Rust `fn` definitions, bash `for` loops, TOML
`[section]` headers, SQL `SELECT` statements) all highlight correctly.

### Note 4: UTF-8-safe byte iteration in the lexer

The T6 review caught that `bytes[i] as char` only sees the first byte of a
multi-byte UTF-8 code point, which would garble any Chinese character that
landed inside a code-block source span. The fix is
`src[i..].chars().next().map_or(1, char::len_utf8)` to advance by the correct
byte count. Important because lesson content modules use Chinese characters
extensively, and code samples occasionally contain Chinese comments.

### Note 5: Inline base64 SVG favicon

T3 embeds the favicon as a `data:image/svg+xml;base64,…` URL in the shell
template's `<link rel="icon">`. This keeps each rendered page self-contained
(no extra HTTP request, works under `file://` preview), at the cost of ~120
bytes per page. The encoding step uses the workspace `base64` 0.22 dependency,
which xtask already needs for asset bundling.

### Note 6: Placeholder SVG assets (T19 Phase A)

T19 ships 6 placeholder SVGs plus one real hand-drawn "5-crate dependency"
diagram. Real PNG screenshots from `agentprof analyze --export html` and
`agentprof serve` will replace the placeholders in a future commit (no code
change required — lesson modules reference `assets/<n>.svg` paths, so swapping
the file is sufficient). The implementing subagent could not browser-capture
inside the sandbox, hence the deferral. Tracked as followup F1.

### Note 7: Test serialization via `Mutex`

T19 added 5 integration tests under `xtask/tests/visual_guide.rs`. All five
mutate the same `docs/visual-guide/` output directory, and `cargo test`'s
per-binary parallel runner caused races (e.g. one test's `--clean` removed
files mid-render in a sibling test, producing flaky failures). Fix: a
`static GUIDE_LOCK: Mutex<()>` in the test file, acquired at the top of each
test body. Idiomatic for shared-state integration tests; documented in the
test file header so future test additions follow the same pattern.

---

## Consequences

**Positive:**

- Single entry point for both newcomers (用法 6 课) and developers (Wiki 8 课);
  one URL to remember, one site to share.
- Source-of-truth lives in Rust (`xtask::visual_guide`), so docs evolve with
  code — adding a new CLI feature naturally invites a new lesson written in the
  same toolchain.
- GH Pages auto-deploys on main push — zero manual release step, no "did
  someone remember to rebuild the docs?" failure mode.
- Zero new top-level workspace dependencies (askama, clap, chrono already
  shipped; base64 and quick-xml are xtask-only).
- 27 new tests (22 unit + 5 integration) keep the rendering pipeline
  regression-safe across future template / lesson edits.
- Establishes a reusable shell + style for future expansion (Phase 3 wiki
  lessons, quizzes, English translation).

**Negative:**

- HTML not in git (D-2) means contributors must run `cargo run -p xtask -- visual-guide`
  locally to preview before PR review. Mitigated by the `--check` mode
  documented in `docs/visual-guide/README.md`.
- Screenshot assets are placeholders (T19 Phase A deferred to F1); the
  rendered site has 6 generic SVGs where real PNG screenshots would
  communicate better.
- The hand-written lexer (Note 3) is best-effort; pathological code samples
  outside the four canonical patterns may render as un-highlighted plain text.
- xtask binary grows by ~200 KB of template / lesson content baked in via
  `include_str!`. Negligible because xtask is a build-time tool, never
  shipped to users.

---

## Followups (not blocking)

- **F1:** Replace 6 placeholder dashboard / report SVGs with real PNG
  screenshots from a v0.3.x agentprof run (file replace only, no code change).
- **F2:** Add `xtask visual-guide --refresh-screenshots` automating the
  capture via headless chromium.
- **F3:** Per-lesson quizzes (`quizzes.rs` per `langchain-visual-guide`).
- **F4:** PDF export of the full guide.
- **F5:** i18n / English translation, sharing the same lesson scaffolding.
- **F6:** Expand Wiki §3 (ClaudeAdapter / CodexAdapter sections) once M3.1
  and M3.2 ship.
