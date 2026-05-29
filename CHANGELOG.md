# Changelog

All notable changes to **agentprof** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries are prefixed with the affected crate when relevant:
`core:` / `adapters:` / `storage:` / `tui:` / `cli:` / `xtask:`.

Breaking changes are marked `BREAKING:` (matching the Conventional Commits
prefix used in commit messages).

## [Unreleased]

### Fixed

#### M1.4 audit followups (`fix/m1.4-audit-followups`)

Closes the actionable findings from the 4-part M1.4 audit. All 10 fixes
land in 10 commits on a single branch.

**core — data correctness:**
- `derive_episodes` no longer emits per-event UUIDs as `ToolEpisode`
  keys for orphan `tool.execution_complete` events (audit-a2-orphan-
  tool-uuid-key). All orphan completes now aggregate under the new
  `ORPHAN_TOOL_SENTINEL = "<orphan>"` constant (exported from
  `agentprof_core::episode`). Per-call accountability preserved via
  existing `DeriveWarning::SynthesizedStart` warnings carrying the
  original event id. Before this fix, `tool_rank` output was polluted
  with one fake "tool" per orphan event, each labeled with an opaque
  UUID and call_count=1. Snapshot updates: `orphan-events` fixture
  re-accepted in both episode + analyzer layers.

**core — defensive instrumentation:**
- New `DeriveWarning::PayloadNameMissing { kind, event_id }` variant
  emitted whenever `Event::payload_name()` returns `None` for an
  event whose kind indicates it SHOULD have a name (audit-a4-payload-
  name-silent-failure / design D1). Closes the silent-failure risk
  for upcoming Claude (Phase 2) and Codex (Phase 3) adapter authors:
  if they forget to override `payload_name` for `ToolExecStart` /
  `HookStart` / `HookEnd` / `SkillInvoked`, downstream consumers see
  a warning instead of silently degrading to one episode per event.
  Markdown renderer's `## Warnings` section gains a `PayloadNameMissing:
  N` counter. `CopilotEvent` correctly overrides all 5 name-bearing
  variants, so existing snapshots are unaffected.

**core — round-trip contract:**
- `SessionMeta` and `AnalysisReport` now derive `PartialEq`
  (audit-a4-analysisreport-round-trip-test). New unit test
  `analysis_report_json_round_trip_is_lossless` locks
  `serde_json::to_string{,_pretty}` → `from_str` equality. Closes
  spec FR-2.12 from "partial" to "fully covered".

**cli — UX polish:**
- `resolve_session_by_path` no longer double-appends `events.jsonl`
  when given a non-existent `.jsonl`-named file. Error reads
  `events.jsonl not found at /x/events.jsonl` (was
  `events.jsonl not found at /x/events.jsonl/events.jsonl (and ...)`).
  Closes `t10-path-error-msg`.
- `looks_like_uuid` now validates ASCII hex digits + dash positions
  (8/13/18/23), not just length + dash count (audit-a3-uuid-typo-
  dumps-sessions). Previously a typo like `00000000-...-0g` passed
  the heuristic, fell through to `discover_sessions`, and the error
  dumped real session UUIDs to stderr — mild info-leak risk on
  shared terminals / CI logs. +5 unit tests cover canonical
  accept (lowercase/uppercase), wrong-length reject, dash-position
  reject, non-hex reject, and integration via `SessionSelector::
  from_str`.
- `--agent claude` / `--agent codex` now returns
  `Claude adapter not yet implemented (M1.4 ships copilot only;
  claude and codex are on the M1.5+ roadmap — see docs/plan.md)`
  instead of the cryptic `no adapter wired for agent Claude`
  (audit-a3-claude-codex-unfriendly-error).
- `--export json` output gains a trailing newline so shell prompts
  don't stick to the closing `}` and file output is POSIX-compliant
  (audit-a4-json-no-trailing-newline).

**cli — markdown table safety:**
- `md::render` now escapes `|` (→ `\|`) and newlines (→ `<br>`) in
  all user-controlled cell content via new `md_cell_escape(s: &str)
  -> Cow<str>` helper (audit-a3-md-pipe-escape). Affected cells:
  `turn_id`, `model`, `cwd`, `branch`, tool/hook `name`, `source`
  (via Debug-then-escape), `fmt_status(Aborted(reason))`, and
  `fmt_mode(Mode::Unknown(s))`. Returns `Cow::Borrowed` for
  safe inputs (no allocation in the common case). +6 unit tests
  cover the escape behavior + boundary cases (mixed pipes &
  newlines, `Aborted(user|cancel)`, `Mode::Unknown("pipe|in|mode")`).

**cli — coverage gap closures:**
- New integration test
  `analyze_unparseable_session_exits_with_data_error` synthesizes
  an inline tempfile events.jsonl with no `session.start`, asserts
  exit 2 + `data error` in stderr. Closes spec FR-3.11 (audit-a4-
  corrupt-exit-2-test-missing).
- New integration test
  `analyze_output_to_unwritable_path_exits_with_output_error` —
  first E2E exit-3 test.
- New integration test
  `analyze_unsupported_agent_exits_with_friendly_message` — regression
  guard for the `--agent claude` UX fix; will fail (intentionally)
  when Claude adapter ships in Phase 2 as a "review me" signal.
- New unit test `exit_kind_downcast_survives_extra_context_layers`
  defends against the M1.4 audit design observation D5 (future
  refactors adding `.context(...)` layers must not hide `ExitKind`
  from `main::classify_error`).

**docs:**
- ADR-0005 D-1 table fixed: `ToolExecComplete` split onto its own
  row with `None (stack pop preserves name)` rationale (was
  incorrectly listed alongside `ToolExecStart` as
  `payload.tool_name`). Also corrected `HookStart`/`HookEnd` →
  `hook_type` (was `hook_name`) and `SkillInvoked` → `name` (was
  `skill_name`) to match what `CopilotEvent::payload_name` actually
  reads. Closes audit-a1-adr-0005-d1-table-stale.
- ADR-0005 gains "Update §1: Orphan tool aggregation via sentinel"
  and "Update §2: PayloadNameMissing warning addition" sections
  documenting the M1.4 audit decisions (kept ADR Status as Accepted
  since these are additive refinements, not reversals).
- `analyzer/tool_rank::percentile` rustdoc clarifies the nearest-rank
  algorithm + even-sample upper-midpoint behavior; `ToolRankRow.
  p50_duration` / `HookRankRow.p50_duration` field docs changed from
  "Median per-call duration" to "Approximate median (nearest-rank
  percentile)" (audit-a2-percentile-doc-says-median).

**chore:**
- Removed unused `askama` dependency from `agentprof-cli` and the
  workspace `[workspace.dependencies]` table (audit-a4-askama-
  unused-dep). The markdown renderer is hand-written; carrying
  the dep was supply-chain noise + a phantom signal that templates
  were in use.

**Tests:** 196 → 215 (+19 across all the new unit + integration
tests). All gates clean (fmt / clippy `-D warnings` / full workspace
tests / `cargo doc -Dwarnings`).

**Out-of-scope (still tracked in m14_followups SQL table):**
- `classifier-zip-fix` (xtask audit tool; P2-optional)
- `negative-duration-span` (non-monotonic-timestamp edge; P2-optional)
- `tooltelemetry-restricted-props-skip-if` (small serde polish;
  P2-optional)
- `skill-call-count-fixture` (fixture reshape, not a bug; P3-defer)

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
- `CallRef { name: String, index: usize }` (added pre-merge): self-describing
  replacement for bare `Vec<usize>` indices in `Turn.{tool,hook,skill}_calls`
  and `SkillInvocation.triggered_tools`, so back-references can be
  dereferenced as `episodes.tools[r.name].calls[r.index]` without external
  context. Same commit also fixes the previous `triggered_tools`
  miscalculation where `tool_idx` was the cumulative `calls.len()` sum
  across all tool episodes; attribution now happens in `commit_tool_call`
  where the tool's real name and per-name index are in scope. ADR-0004
  updated with a CallRef section.

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

#### M1.4 — CLI + analyzer rollups (`feat/m1.4-cli-and-analyzer`)

Reference: spec `docs/superpowers/specs/2026-05-29-m1.4-cli-and-analyzer-design.md`, ADR-0005, plan `docs/superpowers/plans/2026-05-29-m1.4-cli-and-analyzer.md`.

**core — Event trait extension + P0 fix (Phase A):**
- `Event::payload_name() -> Option<&str>` (default `None`) added to the trait; `CopilotEvent` overrides for `tool.execution_start` / `tool.user_requested` (→ `data.toolName`), `hook.start` / `hook.end` (→ `data.hookType`), `skill.invoked` (→ `data.name`). Other variants (incl. `tool.execution_complete`) return `None`.
- `derive_episodes` now uses `payload_name()` (with `event.id()` safety-net fallback) so tools/hooks/skills group by their real wire names ('bash', 'PreToolUse', 'brainstorming') instead of opaque event UUIDs.
- `commit_tool_call` / `commit_hook_call` now attribute back-references to the **start-time** Turn (via `call.turn_id` + `Vec::rposition` lookup), not the end-time `open_turn_idx`. Fixes `commit-call-turn-divergence` (P0 follow-up from M1.3 final review): for tool spans crossing a Turn boundary, `Turn.tool_calls` now matches `ToolCall.turn_id` (single source of truth restored).
- `cross-turn-tool` synthetic fixture (7 events; 'bash' starts in turn-A, completes in turn-B) locks the fix in via hand-verified snapshot.
- 6 M1.3 episode snapshots re-accepted with real payload names.

**core — analyzer module (Phase B):**
- New `agentprof_core::analyzer` module with `AnalysisReport` container + `analyze(&Episodes, &SessionMeta) -> AnalysisReport` bundler.
- `turn_summary(&Episodes) -> Vec<TurnSummaryRow>` — per-turn rollup (turn_id, started_at, duration, status, model, mode, output_tokens, tool/hook/skill call counts).
- `tool_rank(&Episodes) -> Vec<ToolRankRow>` — per-tool rollup with call/success/failure/orphan/user-requested counts and p50/p95/max durations; sorted by total_duration desc.
- `hook_rank(&Episodes) -> Vec<HookRankRow>` — per-hook rollup with success/failure/synthesized_start counts and p50/p95 durations.
- `tool_rank::percentile(&[Duration], f64) -> Duration` shared helper (nearest-rank algorithm).
- `duration_ms` / `duration_ms_opt` serde helpers for stable integer-ms JSON serialization (per ADR-0004 IMP-007 convention).
- New `analyzer_on_fixtures.rs` integration tests with 10 insta snapshots locking the full `load → derive → analyze` pipeline.

**cli — first real binary (Phase C):**
- `agentprof analyze` subcommand wired end-to-end: `--agent` (default copilot), `--session` (latest/previous/uuid/path; default latest), `--root`, `--export md|json` (default md), `--output`, `--section turn-summary,tool-rank,hook-rank` (default all).
- Structured `ExitKind` enum (UserError=1, DataError=2, OutputError=3) carried via `anyhow::Error::msg().context()`; `main.rs::classify_error` downcasts to pick the process exit code.
- Helpful error diagnostics: `'session UUID X not found under Y; first 5 available: a, b, c, d, e'`.
- Markdown renderer (`cmd/format/md.rs`): Session header + Turn Summary table + Tool Rank table + Hook Rank table + Warnings; durations rendered in friendly units (`500ms` / `2.50s` / `2.0m` / `2.00h`); sections filterable via `--section`.
- JSON renderer (`cmd/format/json.rs`): `serde_json::to_string_pretty(&AnalysisReport)`; stable shape with integer-ms Duration fields.
- `tracing` initialization gated by `AGENTPROF_LOG` env var; writes to stderr.
- 6 `assert_cmd` integration tests + 1 insta md snapshot (`cli__analyze_md__cross_turn_tool`).
- ADR-0005 D-2 fix confirmed at FOUR independent layers: derive unit test → episode snapshot → analyzer snapshot → CLI md/JSON snapshot+assertion.

**core — Cargo features:**
- New optional `clap-derive` feature on `agentprof-core` enabling `#[derive(clap::ValueEnum)]` on `AgentKind` via `cfg_attr` (lets `agentprof-cli` use AgentKind directly in clap-derive structs without `agentprof-core` taking a hard `clap` dep).
- `agentprof-cli` enables the feature on its `agentprof-core` dependency and adds `thiserror` for `ExitKind`.

**docs:**
- ADR-0005 (Accepted): Analyzer foundations + `Event::payload_name()` trait extension + start-time turn attribution + `AnalysisReport` placement in core (not cli) rationale.
- `docs/architecture.md` §7.2 analyzer rollups subsection added; §8 `analyze` block amended for M1.4 reality; §14 ADR list adds row for ADR-0005.
- `crates/agentprof-cli/README.md` updated to mark `analyze` as shipped (with Quick start examples); other subcommands kept as planned.
- `crates/agentprof-core/README.md` Public interface table gains `analyzer` row; Reference ADRs adds ADR-0005.
- Root `README.md` Status notice updated to "M1.4 shipped"; new Quick start section with runnable examples and sample output structure.

**Carried forward (not in M1.4 scope):**
- M1.3 P2 follow-ups remain tracked: `classifier-zip-fix`, `negative-duration-span`, `tooltelemetry-restricted-props-skip-if`.
- `with-skill-invoked` fixture's skill fires before turn_start, so `turn_summary[*].skill_call_count == 0` across all rows — derive behavior is correct (skills outside open turn aren't turn-attributed); fixture reshape deferred to P3.
- `analyze --session` Path-vs-Uuid: fixture dirs are named by purpose (not UUID), so `--session <dirname>` is rejected by `looks_like_uuid` heuristic; users (and integration tests) should use `--session <full-path-to-dir>` instead. Real `~/.copilot/session-state/<uuid>` dirs work as expected.
- `corrupt → exit 2` integration test: corrupt fixture's bad line produces parse-time warnings (not fatal); would need a fully-unparseable fixture. Defer to M1.5 polish.

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
