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

---

## End of plan skeleton.

Plan body fills in below as work progresses.
