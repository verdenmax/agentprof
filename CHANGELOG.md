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

- `agentprof-tui` crate: first interactive ratatui TUI shipped as M1.5 (`analyze --export tui`).
  - **FlamegraphView**: per-turn horizontal gantt; segments are tool calls; whitespace = LLM thinking time.
  - **RoiView**: interactive tool rank with sort cycling (`1`/`2`/`3`/`4` = total / calls / success% / p50); recent-calls detail strip; user-blocking tools (`ask_user`) split into separate sub-table per M1.4 post-output-audit.
  - **AggregateView**: single-session By-Mode + By-Hook tables.
  - **Panic-safe terminal lifecycle**: `install_panic_hook` (Once-guarded) → `enter` → `run` → best-effort `leave`. See [ADR-0006](docs/internals/adr-0006-panic-safe-tui.md).
  - **TTY required**: piping yields `OutputError` (exit 3) with a helpful message; use `--export md` or `--export json` for headless.
  - References: spec [`2026-05-30-m1.5-tui-design.md`](docs/superpowers/specs/2026-05-30-m1.5-tui-design.md), plan [`2026-05-30-m1.5-tui.md`](docs/superpowers/plans/2026-05-30-m1.5-tui.md).

### Docs

#### Roadmap / progress sync (`docs(sync)` — 2026-05-30)

After 4 merged M1.4 followups (audit / turn-metadata / mode-vocab /
post-output-audit), several entry-point docs were misleading new readers —
`tasks/ROADMAP.md` still said "M1.2–M1.7 ❌ 未开始" and
`tasks/001-mvp-agent-token-profiler.md` had ❌ status lines for milestones
that had already shipped. This commit synchronises the docs to reality.

**docs touched** (no code change):

- `tasks/ROADMAP.md` — header (current commit / phase status), §2.2 当前位置,
  §2.3 仪表盘 (4/7 = 57%), §3.1 task table, §4.1 + §4.2 dependency graphs
  (M1.2–M1.4 now ✅, Copilot adapter no longer in Phase 3).
- `tasks/001-mvp-agent-token-profiler.md` — header status, §4 FR completion
  table, **M1.2 / M1.3 / M1.4 状态行** rewritten with merge-commit citations
  and pivot notes, §11 M3.2 CopilotAdapter entry removed (it was already
  delivered in M1.2; Phase 3 now lists only Claude / Codex / Gemini).
- `docs/plan.md` §6 + §8 — pivot note added explaining events-first
  divergence from original Phase 0/1 plan; §8 next-step now points to
  M1.5 (TUI) instead of "write Phase 0 prototype".
- `docs/architecture.md` — `AnalysisReport` struct definition updated to
  M1.4 shape (`parse_warnings`, `is_user_blocking`-bearing rollup rows),
  `analyze()` signature corrected (`&[ParseWarning]` third arg),
  `Mode` vocabulary updated (`Interactive / Plan / Autopilot / Unknown`),
  `DeriveWarning` count updated (4 → 5), `USER_BLOCKING_TOOLS` const +
  user-blocking split + post-output-audit referenced.
- `crates/agentprof-core/README.md` — `Event` trait now 8 methods (4 required + 4 default payload-*; was 4 required-only),
  `analyze()` signature corrected, `ORPHAN_TOOL_SENTINEL` /
  `USER_BLOCKING_TOOLS` / `is_user_blocking` / `parse_warnings` /
  `parent_tool_call_id` / Mode vocabulary documented; quick-start sample
  updated to demonstrate parse-warning + user-blocking inspection.
- `crates/agentprof-adapters/README.md` — `CopilotEvent` notes 4
  payload-* trait overrides; new section "Copilot CLI 1.0.x schema notes"
  documents the three `Option<String>` parser-compat fields and the
  fixture that locks them in. Phase classification corrected.
- `crates/agentprof-cli/README.md` — M1.4 status section rewritten as a
  5-row merge table; markdown output structure documented end-to-end
  (Session block with Parse warnings line, User-blocking tools split,
  Warnings two-stage breakdown); `askama` removed from dependency list
  (renderer is hand-rolled string-building since M1.4 audit followups).
- `README.md` (root) — sample markdown output updated: `Mode = auto` →
  `interactive`; `- Parse warnings: N` line added to Session block;
  `## User-blocking tools` section added with realistic `ask_user` row;
  `PreToolUse` hook example renamed to `postToolUse` (real Copilot CLI
  vocabulary).

No CHANGELOG entry was created for ADR-0005 §6 itself — that was already
shipped in the previous commit's CHANGELOG section under "Post-output
audit fixes".

### Fixed

#### Post-output audit fixes (`fix/post-output-audit`)

Closes the actionable findings from the 2026-05-29 audit of `agentprof analyze`
output against a real live Copilot CLI 1.0.54 session (11 806 lines). Three
classes of fix; one branch; documented in
[ADR-0005 §6](docs/internals/adr-0005-analyzer-and-payload-name.md#update-6-post-output-audit-fixes-parse-warning-visibility-schema-mismatches-user-blocking-split).

**adapters — schema-mismatch parser drops (~17 % event loss):**
- Real Copilot CLI 1.0.x emits multiple wire shapes for some events. Three
  payload structs required string fields that aren't actually universally
  present, causing serde to silently drop matching events with
  `"missing field X"` warnings.
  - `HookInput.source: String → Option<String>` — `postToolUse` hooks
    carry no `source` (100 % of postToolUse hooks were dropping; symptom
    was `synthesized = 100 %, total = 0ms` for the entire hook in Tool
    Rank).
  - `UserMessageData.source: String → Option<String>` — many CLI-typed
    prompts omit `source` (46 % of user.message events were dropping).
  - `AssistantMessageData.turn_id: String → Option<String>` — subagent-
    spawned messages (via `subagent.started`) carry `parentToolCallId`
    instead of `turnId` (71 % of assistant.message events were dropping,
    losing all subagent token usage).
- `AssistantMessageData` also gains a new `parent_tool_call_id:
  Option<String>` field for subagent visibility.
- New fixture `crates/agentprof-adapters/tests/fixtures/copilot/with-post-tool-use-hooks/`
  (10 events) locks all three schema variants in episode + analyzer
  snapshots. Real-session drop rate verified to go from 17 % → 0 %.

**core — parse warnings now user-visible:**
- `AnalysisReport` gains `parse_warnings: Vec<ParseWarning>` field
  (additive, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  keeps old empty reports byte-identical and old JSON deserializable).
- `analyze()` signature widens to `analyze(&Episodes, &SessionMeta,
  &[ParseWarning]) -> AnalysisReport`. Callers pass `raw.parse_warnings`;
  pure unit tests pass `&[]`. **BREAKING for any external code calling
  `analyze()` directly** (no such callers exist outside the workspace yet).
- `ParseWarning` gains `PartialEq + Eq` (needed for round-trip + test
  assertions; was an earlier oversight).
- Locked by new `analyzer::tests::analyze_carries_parse_warnings_through`
  and extended `analysis_report_json_round_trip_is_lossless`.

**cli — markdown renderer surfaces parse warnings + splits user-blocking tools:**
- Session header gains `- Parse warnings: N` line beside `- Derive warnings: N`.
- Warnings section adds a parse-stage breakdown (Json / Io / OutOfOrder
  counts) before the existing derive-stage breakdown.
- `ToolRankRow` gains `is_user_blocking: bool` (additive,
  `#[serde(default)]`). New `pub const USER_BLOCKING_TOOLS: &[&str] =
  &["ask_user"]` in `agentprof_core::analyzer::tool_rank` is the single
  source of truth.
- Markdown renderer's `write_tool_rank` now partitions rows: work tools
  render in `## Tool Rank (by total duration)` as before; user-blocking
  tools render in a new `## User-blocking tools (wall-clock includes
  user think time)` section. JSON contract is additive (still a flat
  `tool_rank` vec; each row carries the new bool).
- Real-session effect: `task` (4.85h, 136 calls) and `bash` (57m,
  1641 calls) now headline the work-tool ranking; `ask_user` (63h,
  61 calls, mostly user think time) gets its own visually-distinct
  section instead of dominating the chart.

**docs — privacy considerations (documentation-only):**
- New `docs/features/privacy.md` (L2 cross-crate feature doc) documents the PII /
  SII fields in `AnalysisReport` (Unix `cwd`, `branch`, model internal
  names, ~800 turn UUIDs per session) with a tier table + manual
  `sed`/`jq` redaction cheat sheets for both markdown and JSON outputs.
  Planned `--redact` / `--anonymize` CLI flags are scoped for M1.5+; no
  code change in this branch.

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

#### Turn metadata extraction (`feat/turn-metadata-extraction`)

Discovered while validating the M1.4 audit fixes by running `agentprof analyze` against the `minimal` fixture and a real local Copilot session. The Markdown report's **Model / Mode / Out-Tokens** columns were all `—` for every turn, despite the wire data carrying these fields (`AssistantMessageData.model`, `AssistantMessageData.output_tokens`, `ModeChangeData.new_mode`). Root cause: `derive_episodes` never read these payload fields — the existing `Turn` struct fields were initialized to `None` by `Turn::new()` and never written to. Spec FR-2.2 required only "fields exist and correctly typed", which the M1.4 audit verified as compliant — the audit had no obligation to check "fields populated with real data". This was a real audit / spec blind spot that surfaced immediately on first user inspection.

**`agentprof-core`:**
- `Event` trait extended with 3 new methods, all with default `None` (mirroring ADR-0005 D-1): `payload_model() -> Option<&str>`, `payload_output_tokens() -> Option<u32>`, `payload_mode() -> Option<&str>`.
- `DeriveState` gains a `current_mode: Option<Mode>` field tracking the active session mode across the event stream.
- New `on_assistant_message` handler populates `Turn.model` (last-wins across messages in a turn) and `Turn.output_tokens` (saturating sum). M1.5 ROI computations consume both.
- `on_mode_event` now reads `ev.payload_mode()` instead of pushing a hard-coded `Mode::Unknown("changed")` segment — the M1.3 PLACEHOLDER for "Task 10b will read actual mode value" is now resolved.
- `on_turn_start` captures `current_mode.clone_from(&...)` into `turn.mode`. Mid-turn mode changes don't retroactively update the current turn (matches user intuition: "this turn was started in X mode").
- Dispatch table gains `EventKind::AssistantMessage => state.on_assistant_message(ev)`.

**`agentprof-adapters`:**
- `CopilotEvent` overrides the 3 new trait methods for `AssistantMessage` and `ModeChanged` variants. `ModelChange` deliberately returns `None` for both `payload_model` and `payload_mode` (it announces a model switch, not a per-message model or a mode change).

**Snapshots:**
- 14 snapshots re-accepted (7 `episode_derive__*.snap` + 7 `analyzer_on_fixtures__*.snap`). `minimal` fixture now shows `model: "gpt-5-mini"`, `output_tokens: 10` (was both null). `with-mode-transitions` fixture shows populated `mode` values (`{"Unknown": "plan"}`, `{"Unknown": "autopilot"}`, etc. — wire vocabulary differs from `Mode::{Ask,Auto,Expert}` known set, so they correctly fall to the forward-compat `Unknown` variant). Fixtures without `assistant.message` events (cross-turn-tool, orphan-events) keep `model`/`output_tokens` as null — confirms we only populate fields with source data.

**Tests:**
- 3 unit tests for trait default `None` (adapter.rs)
- 5 unit tests for CopilotEvent overrides + ModelChange-vs-ModeChange disambiguation (event.rs)
- 4 unit tests for `derive.rs` aggregation semantics: single-message attribution, sum + last-wins, mode-mid-turn semantics, defensive no-message
- 1 CLI integration test asserting `minimal` fixture's `turn_summary[0].output_tokens == 10` end-to-end

**Out of scope (M1.5 deliverables):**
- Cost / ROI computation logic (price tables, per-model tokenizers, `--with-cost` flag)
- `agentprof aggregate` cross-session rollups
- This commit only provides the **inputs** M1.5 will consume.

**Test count delta:** 214 → 230 (+16: 3 + 5 + 4 + 1 unit/integration + 3 doctests, plus snapshot diffs which don't change count).

#### Mode vocabulary alignment (`fix/mode-vocabulary-alignment`)

Discovered immediately after the turn-metadata-extraction merge by running `agentprof analyze --section turn-summary` against the live local Copilot session and noticing every turn still showed `Mode: —`. Investigation via `find ~/.copilot/session-state -name events.jsonl | xargs grep '"type":"session.mode_changed"'` revealed the real Copilot CLI 1.0.54 wire vocabulary is `interactive` / `plan` / `autopilot` (73 events across 190 sessions; 0 of `ask` / `auto` / `expert`). The previous `Mode::{Ask, Auto, Expert}` enum variants were a fabricated vocabulary — likely from an early documentation guess — that never matched any real wire data.

**`agentprof-core/episode/mode_segment.rs`:**
- `Mode` enum variants renamed to match real wire vocabulary: `{Ask, Auto, Expert}` → `{Interactive, Plan, Autopilot}`. Each variant now has a doc comment with frequency from the 73-event sample (Plan 60, Interactive 52, Autopilot 34) and semantic context.
- `Mode::from_wire` rewired: `"interactive" → Interactive`, `"plan" → Plan`, `"autopilot" → Autopilot`, anything else → `Unknown(s)` for forward-compat.
- Updated unit tests assert the new vocabulary; one test explicitly verifies the OLD `ask`/`auto`/`default` strings round-trip through `Unknown` (defense against accidental reintroduction).

**`agentprof-core/episode/derive.rs`:**
- `DeriveState::new` now seeds the initial `ModeSegment` with `Mode::Interactive` (replacing the M1.3 placeholder `Mode::Unknown("default")`) AND initializes `current_mode: Some(Mode::Interactive)` (was `None`). Rationale: data analysis showed every `previousMode → newMode` transition opens with `previousMode = 'interactive'`, confirming Interactive is Copilot CLI's implicit default; sessions without explicit `mode_changed` events run entirely in Interactive.
- Updated `mode_change_attributes_to_next_turn_not_current` test to use real `interactive` / `autopilot` strings and assert against `Mode::Interactive` / `Mode::Autopilot`.

**`agentprof-cli/cmd/format/md.rs`:**
- `fmt_mode` now returns real strings: `interactive` / `plan` / `autopilot` (was `ask` / `auto` / `expert`).
- Updated `fmt_mode_handles_each_variant` test.

**User-visible impact**: every turn in every real Copilot session now shows the actual mode (typically `interactive` for the common case) instead of `—`. Sessions with mode transitions correctly show `plan` and `autopilot` at the right turn boundaries. This restores meaningful Mode column data.

**Snapshots:** 21 re-accepted (10 episode_derive + 10 analyzer_on_fixtures + 1 CLI insta md snapshot). Mode values in turn rows changed from `{"Unknown": "plan"}` → `"Plan"` (and similar), AND from `null` → `"Interactive"` for fixtures without explicit mode events. Initial `mode_segments[0]` value changed from `{"Unknown": "default"}` → `"Interactive"` across all snapshots.

**Test count delta:** 230 → 230 (renames + test rewires balance to net zero new tests, but +1 stronger assertion in `mode_from_wire_unknown_preserved` covering 3 invalid strings).

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
