# Changelog

All notable changes to **agentprof** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries are prefixed with the affected crate when relevant:
`core:` / `adapters:` / `storage:` / `tui:` / `cli:` / `xtask:`.

Breaking changes are marked `BREAKING:` (matching the Conventional Commits
prefix used in commit messages).

## [Unreleased]

### Added

#### M1.2 — Copilot CLI adapter (`feat/m1.2-copilot-adapter`)

Reference: `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md`, ADRs 0001 / 0002 / 0003, plan `docs/superpowers/plans/2026-05-26-m1.2-copilot-adapter.md`.

**core — adapter contract layer** (`agentprof-core::adapter`, `::model`, `::error`):
- `Adapter` trait: `agent_kind` / `default_session_root` / `discover_sessions` / `load_session`, with associated `Event` type.
- `Event` trait with four methods (`id`, `kind`, `timestamp`, `parent_id`) so analyzer layers can treat per-adapter event enums uniformly.
- `EventKind` enum (19 variants: 18 named + `Unknown`) and `AgentKind` enum (`Copilot` / `Claude` / `Codex`, `#[non_exhaustive]`).
- `SessionRef` struct + `SessionRef::new` constructor (path, agent, id, modified-at, size, is-live).
- `RawSession<E>` generic + `SessionMeta` + `SessionMeta::new` + `RawSession::new` — the unified shape every adapter produces.
- `ToolSource` enum + `ToolSource::infer` classifier (`Builtin` / `Mcp { server }` / `Skill { plugin }` / `User` / `Unknown`).
- Error types: `AdapterError` (struct-variant `RootNotFound { path }`, `Io { path, source }`, `MissingSessionStart`, `UnsupportedVersion`, `Parse`), `CoreError`, and the `ParseWarning` taxonomy (`Json`, `OutOfOrder`, `MissingField`, `UnknownVariant`).

**adapters — Copilot CLI implementation** (`agentprof-adapters::copilot`):
- `CopilotEvent` enum: 18 named variants tagged by `type` field (covering `session.{start,info,mode_changed,model_change,plan_changed,shutdown}`, `user.message`, `assistant.{turn_start,message,turn_end}`, `system.message`, `tool.{execution_start,execution_complete,user_requested}`, `hook.{start,end}`, `skill.invoked`, `abort`) + `Unknown` (`#[serde(other)]`) for forward compatibility.
- `WithEnvelope<D>` generic envelope (`id`, `timestamp`, `parent_id`, `ephemeral`, `data`) plus ~25 `#[non_exhaustive]` payload structs (`SessionStartData`, `AssistantMessageData`, `ToolExecData`, `HookStartData`, `SkillData`, `AbortData`, …).
- `impl Event for CopilotEvent` — full dispatch including `EventKind::Unknown` for the catch-all variant.
- `copilot::parser::parse_events_jsonl(path, is_live)` — line-by-line streaming parser producing `RawSession<CopilotEvent>`. Per-line JSON failures accumulate as `ParseWarning::Json`; non-monotonic timestamps emit `ParseWarning::OutOfOrder`; the trailing line of a live session (`is_live=true`) is silently skipped when `looks_like_incomplete_json` detects a partial write; missing `session.start` returns `AdapterError::MissingSessionStart`.
- `copilot::parser::looks_like_incomplete_json` — brace-depth heuristic respecting string literals and escapes, used for live-session tail tolerance.
- `copilot::paths::default_session_root()` — XDG-aware resolver returning `$HOME/.copilot/session-state`.
- `copilot::paths::discover_sessions(root)` — walks `<root>/<uuid>/events.jsonl`, returns `Vec<SessionRef>` sorted by mtime descending, marks `is_live` when an `inuse.<pid>.lock` sibling file exists. Silently skips individual malformed subdirectories.
- `copilot::adapter::CopilotAdapter` — zero-sized struct implementing `Adapter`, delegating to the `parser` and `paths` modules.
- `registry::adapter_for(kind)` and `registry::supported_agents()` — agent-kind dispatch.

**adapters — test fixtures** (`agentprof-adapters/tests/fixtures/copilot/`):
- 9 synthetic JSONL fixtures per ADR-0003 (100% synthetic, stable UUIDs, `/tmp/agentprof-fixture/<slug>` paths):
  - `minimal/` (canonical 8-event happy path)
  - `corrupt/` (intentionally-broken JSON for `ParseWarning::Json` coverage)
  - `builtin-tools-only/` (5 builtin tool invocations)
  - `with-mcp-calls/` (`mcp__<server>__<tool>` flow)
  - `with-skill-invoked/` (`skill.invoked` lifecycle)
  - `with-hooks-heavy/` (72 events, 30 hook start/end pairs across phases)
  - `with-aborts/` (3 user-initiated aborts at distinct lifecycle points)
  - `with-mode-transitions/` (4 mode segments: `ask` → `auto` → `expert` → `ask`)
  - `live-truncated/` (3 valid events + truncated trailing line + `inuse.778482.lock`)
- Per-fixture `README.md` explaining the scenario.
- `copilot_event_parse` (23 round-trip tests, one per variant + Unknown), `copilot_fixture_load` (9 fixture-level tests with `insta` snapshots), `copilot_paths` (6 discovery tests).
- `copilot_smoke` integration test scaffold (`#[ignore]` by default; runs against `$AGENTPROF_LOCAL_FIXTURES_DIR` with `--include-ignored`; asserts zero `CopilotEvent::Unknown` against real local data, catching schema drift between Copilot CLI versions).

**docs:**
- ADR-0001 (events-first product pivot), ADR-0002 (Copilot event schema), ADR-0003 (synthetic-only fixture strategy).
- `crates/agentprof-adapters/README.md` rewritten per the L2 template (in-architecture context, public-interface index, modules table, supported-agent matrix, local-smoke instructions, ADR pointers).
- `docs/adapters.md` rewritten as the contribution guide (trait contract, new-adapter checklist, fixture rules, smoke-test pattern).

**chore:**
- `.gitignore` — `/local-fixtures/` and `/smoke-data/` excluded to prevent accidental commit of developer-local session data.

#### M1.3 Phase A+B — Copilot schema calibration (`feat/m1.3-episode-and-schema-fix`)

Driven by a forward-looking audit tool plus real-data analysis.

**xtask — `cargo xtask schema-audit`** (Phase A):
- New developer tool that scans `~/.copilot/session-state/` (or
  `--root`), classifies `CopilotEvent::Unknown` by wire `type` (with
  candidate Rust variant names), summarizes `ParseWarning` distribution,
  and reports `start`/`end` pair balance with severity thresholds.
- Submodules: `scanner.rs` (dual-load raw + typed), `classifier.rs`
  (group + redact + balance compute), `report.rs` (markdown).
- CLI: `--root`, `--sample-limit`, `--output`, `--sessions`.
- Documented in `xtask/README.md` with 5 invocation patterns.
- Integration test ensures all 4 report sections emit on fixture root.
- Re-runnable after every Copilot CLI upgrade.

**adapters — 10 new `CopilotEvent` variants** (Phase B, audit-driven):
- `Subagent{Started,Completed,Failed}`, `SystemNotification`,
  `Session{Warning,Resume,CompactionStart,CompactionComplete}`,
  `Permission{Requested,Completed}`.
- `WithEnvelope` gained `agent_id: Option<String>` (camelCase: `agentId`).

**adapters — `tool.execution_*` payload-shape expansion** (Phase B):
- `ToolResultData` extended with `interaction_id`, `model`,
  `result: Option<ToolResult>`, `tool_telemetry: Option<ToolTelemetry>`,
  all Optional for cross-version compatibility.
- New helper structs: `ToolResult { content, detailed_content }`,
  `ToolTelemetry { metrics, properties, restricted_properties }`.

**adapters — testing:**
- 15 new round-trip tests in `copilot_event_parse.rs` (23 → 38).

**docs:**
- ADR-0002 marked `Updated 2026-05-27`, with detailed Schema Updates section.
- 18 → 28 named variants documented.

**Audit impact** (on developer's 187-session / 117K-event data):
- `CopilotEvent::Unknown`: 3411 → 278 (−92%)
- `ParseWarning::Json`: 58339 → 38176 (−35%)

#### M1.3 Phase C — Episode aggregation (`feat/m1.3-episode-and-schema-fix`)

**core — new `agentprof_core::episode` module:**
- `Turn` + `TurnStatus` (`Open` / `Completed` / `Aborted(AbortInfo)`) +
  `Span` (with `instant()` for orphan synthesis) + `AbortInfo`.
- `ToolEpisode` + `ToolCall` + `ToolCallStatus` (`Success` / `Failure { message }` /
  `OrphanSynthesizedStart` / `OpenAtEndOfSession`).
- `HookEpisode` + `HookCall` (with `synthesized_start` flag).
- `SkillEpisode` + `SkillInvocation` (with `triggered_tools` window).
- `ModeSegment` + `Mode` (`Ask` / `Auto` / `Expert` / `Unknown(String)`).
- `Episodes` container (7 fields, snapshot-stable `BTreeMap` ordering).
- `DeriveWarning` 4-variant data-quality enum.
- `derive_episodes<E: Event>(events, meta) -> Episodes`: pure, total,
  single-pass aggregation function. Algorithm in ADR-0004.

**adapters — testing:**
- New synthetic fixture `tests/fixtures/copilot/orphan-events/`
  exercising orphan-end synthesis + abort-without-open paths.
- `tests/episode_derive.rs` integration tests with 9 insta snapshots
  (one per fixture) + 1 no-panic test. Placed under agentprof-adapters
  to avoid dev-dep cycle.
- `orphan-events` added to `every_fixture_line_parses_as_copilot_event`.

**docs:**
- `crates/agentprof-core/README.md` (new/rewritten): full L2 README.
- `docs/architecture.md` §5.1: Episode types section added; §14.4 ADR list
  updated with ADR-0004.
- `docs/internals/adr-0004-episode-derivation.md`: cross-checked against
  implementation; no semantic changes.

**Known limitation (Event trait):**
Tool/hook/skill names in `Episodes` use `event.id()` as placeholder
because the Event trait doesn't expose payload fields. M1.4 may extend
Event with `payload_name() -> Option<&str>`. Snapshots reflect this.

#### M1.1 — pre-existing entries

- **Project roadmap entry-point** — `tasks/ROADMAP.md` (378 lines): the master document new contributors and AI agents should read first. Sections cover (1) document map across L1/L2/L3 + AI guides, (2) project phases timeline with current commit position, (3) task file index with status/release mapping, (4) milestone dependency graph (within MVP and across phases), (5) release cadence and SemVer rules, (6) how-to-use guide for 6 personas (newcomer / developer / feature author / releaser / reviewer / maintainer), (7) long-term vision and explicit "won't do" boundaries, plus self-update discipline at the bottom.
- 001 task file now back-links to `tasks/ROADMAP.md` in its authoritative-documents preamble.
- **MVP task file** — `tasks/001-mvp-agent-token-profiler.md` (1009 lines): full PRD + implementation plan covering Phase 0 + Phase 1. Format mirrors the reference `proteinCopilot/tasks/001-mvp-proteomics-search-platform.md`:
  - PRD sections §1–§9: Introduction / Goals / User Stories (US-1…US-7) / Functional Requirements (FR-1…FR-7) / Non-Goals (NG-1…NG-10) / Design / Technical / Success Metrics (SM-1…SM-10) / Open Questions (OQ-1…OQ-8).
  - §10 Implementation Milestones: M1.1 (skeleton ✅) → M1.7 (release v0.1.0). 7 main milestones broken into 46 Tasks → 222 Sub-tasks.
  - §11 Phase 2/3 outline: SQLite persistence, OTLP receiver, Codex/Copilot adapters, pricing auto-sync, v1.0.0 release (6 additional milestones).
  - Each milestone explicitly tied to the 9-stage skill pipeline (`.github/copilot-instructions.md` §5): which skill produces which artifact at each step.

### Changed
- **Pipeline 衔接性增强（§5 重写为三层结构）**：
  - 流程图现明确分为**主线**（Stage 0→1→2→3→4→7→8）、**横切层**（Stage 5/6）、**Pipeline 外**（writing-skills），避免之前画成串行的误导。
  - 新增 §5.5 「Stage 2 触发门槛」表格 + 判断口诀（*"半年后回头看会问『为什么这么做？』就写 ADR"*），区分 8 类场景。
  - 新增 §5.6 「横切层规则」：Stage 5 不打断主线、Stage 6 修完返回触发它的 stage（不跳到 Stage 7）。
  - 3 个原本"孤儿"的 skill 找到明确归属：
    - `dispatching-parallel-agents` → Stage 1（并行调研多源） + Stage 4（并行多模块影响面）
    - `using-git-worktrees` → Stage 3→4 之间的可选 env prep
    - `writing-skills` → 标明为 "Pipeline 之外的元能力"
  - §5.7 commit 粒度新增 Stage 6 规则（`fix:` 前缀 + 关联失败测试）。

### Added
- **Skill pipeline integration (corrected layout)** — five curated skills from `github/awesome-copilot` placed at `<repo>/.github/skills/<name>/SKILL.md` (project-level path per [GitHub Copilot CLI skills docs](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-skills)), plus two `.instructions.md` files at `.github/instructions/`. All checked into git and propagated by `git clone` — no global install step required:
  - `cli-mastery` (Stage 4)
  - `copilot-cli-quickstart` (Stage 4)
  - `github-release` (Stage 8)
  - `create-github-action-workflow-specification` (Stage 5)
  - `create-architectural-decision-record` (Stage 2)
  - Plus `.github/skills/README.md` documenting provenance, upstream sync command, license, and verification (`/skills list`, `/skills reload`).
- **Unified 9-stage pipeline** — `.github/copilot-instructions.md` §5 rewritten as a Boot → Discovery → Decision → Planning → Implementation → CI/Infra → Debugging → Completion → Release flowchart; covers every obra + project skill with stage, trigger, output, and exit criterion.
- `.github/copilot-instructions.md` §6 extended: §6.1/§6.2 expanded with the five new skills and the `Pipeline 阶段` column; new §6.6 "Stage 0 常驻 instructions" and §6.7 "Skill 来源说明" (obra/superpowers global vs `.github/skills/` per-repo).
- `docs/architecture.md` §14.7 rewritten to map all 19 skills to pipeline stages and document outputs; new §14.8 acknowledging the two always-on instruction files.
- Skills usage matrix integrated into both AI and architecture docs (🔴 MUST / 🟡 recommended / 🟢 optional tiers + anti-patterns).
- Workspace skeleton with five crates (`agentprof-core`, `agentprof-adapters`, `agentprof-storage`, `agentprof-tui`, `agentprof-cli`) and an `xtask` helper.
- Architecture authority document (`docs/architecture.md`, L1).
- AI-assistant guide (`.github/copilot-instructions.md`).
- Adapter contributor guide placeholder (`docs/adapters.md`, L2).
- L1/L2/L3 documentation system definition (see `docs/architecture.md` §14).
- Repository configuration: `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.editorconfig`, `.gitignore`, dual `LICENSE-*` files.

[Unreleased]: https://github.com/agentprof/agentprof/compare/HEAD...HEAD
