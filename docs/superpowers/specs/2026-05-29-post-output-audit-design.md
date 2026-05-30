# Post-output audit fixes — design

> **Status.** Approved 2026-05-29 (verbal user approval per conversation
> branch on `fix/post-output-audit`).
>
> **Scope.** Close 3 of the 5 findings from the 2026-05-29 audit of
> `agentprof analyze` output against a real live Copilot CLI 1.0.54
> session. Pagination (B-2) deferred; privacy documented-only.

## 1. Audit summary (input)

A 4-part audit (data correctness, output reasonability, privacy, UX) was
run against the real session at
`~/.copilot/session-state/252068e5-ca16-4186-a181-719462643d83/events.jsonl`
(11 806 lines). Cross-checked turn 1 fields source-vs-output: all match.
Uncovered:

| # | Severity | Finding | This spec |
|---|---|---|---|
| 1 | 🔴 P0 | `HookInput.source` schema mismatch silently drops ~24 % of events | ✅ T1 |
| 2 | 🔴 P0 | `ParseWarning`s invisible — user can't see silent drops | ✅ T2 |
| 3 | 🟡 P1 | `ask_user` (user think time) dominates Tool Rank — misleading | ✅ T3 |
| 4 | 🟡 PII | 5 PII / SII fields leak in default output | 📝 T4 docs only |
| 5 | 🟢 UX | 745+ row turn_summary table has no pagination | ⏭️ deferred |

## 2. Approved design (3 tasks)

### T1 — HookInput.source → Option<String>

`HookInput.source: String` (required) was rejecting `postToolUse` hooks,
which carry tool-specific fields (`toolName`/`toolArgs`/`toolResult`)
instead of `source`. Only `sessionStart`-style hooks carry `source`.

**Implementation.** Single field change to `Option<String>` with
`#[serde(default, skip_serializing_if = "Option::is_none")]`. Tool-
specific fields (`toolName`/`toolArgs`/`toolResult`) silently ignored
by serde (no `deny_unknown_fields` on the struct). New fixture
`with-post-tool-use-hooks/` exercises the postToolUse shape.

**Out-of-scope discovery during implementation.** Two parallel schema
mismatches with the same root cause were found and fixed in the same
commit (per audit's spirit even if not pre-planned):
- `UserMessageData.source: String → Option<String>` (46 % drop rate).
- `AssistantMessageData.turn_id: String → Option<String>` +
  added `parent_tool_call_id: Option<String>` for subagent visibility
  (71 % drop rate; subagent-spawned messages have no `turnId`).

Fixture grew from 7 → 10 events covering all three variants.

### T2 — Parse warnings visible

`AnalysisReport` gains `parse_warnings: Vec<ParseWarning>`. `analyze()`
signature becomes `analyze(&Episodes, &SessionMeta, &[ParseWarning])`.
`ParseWarning` gains `PartialEq + Eq`. CLI's `cmd/analyze.rs::run()`
passes `raw.parse_warnings`.

Markdown renderer:
- Session block adds `- Parse warnings: N` line.
- Warnings section adds parse-stage breakdown (Json / Io / OutOfOrder)
  BEFORE the existing derive-stage breakdown.

JSON contract is additive: `#[serde(default, skip_serializing_if =
"Vec::is_empty")]` keeps old empty reports byte-identical.

### T3 — User-blocking tools split

`ToolRankRow` gains `is_user_blocking: bool`. New
`pub const USER_BLOCKING_TOOLS: &[&str] = &["ask_user"]` in
`agentprof_core::analyzer::tool_rank` is the single source of truth.
`tool_rank()` populates the flag at analyzer time via membership check.

Markdown renderer's `write_tool_rank` partitions rows: work tools in
`## Tool Rank`; user-blocking in `## User-blocking tools (wall-clock
includes user think time)`.

JSON kept flat: each row carries `is_user_blocking` (default `false`
for older JSON via `#[serde(default)]`).

### T4 — Privacy doc only

New `docs/internals/privacy-considerations.md` describes:
- What `agentprof analyze` does NOT carry (user prompts, tool args,
  tool results, assistant content, reasoning text — none read).
- 5 🔴 HIGH-tier PII fields in current output (`meta.cwd`, `branch`,
  `meta.model`, session UUID, ~800 turn UUIDs per session).
- 4 🟡 MEDIUM-tier fields (MCP tool names, agent_version, started_at,
  copilot_version).
- Manual `sed`/`jq` redaction cheat sheets for md + JSON.
- Planned `--redact` / `--anonymize` CLI flags for M1.5+.

No code change; CLI flags scoped for M1.5+.

## 3. Deferred (NOT in this branch)

- **B-2 pagination of 745+ row turn_summary.** User requested defer.
  Real impact only when humans paste the report into limited-height
  viewers; markdown renderers handle large tables fine.
- **`--redact` / `--anonymize` CLI flags.** Scoped for M1.5+ when
  `aggregate` subcommand lands (the natural place for these flags).
  Until then, manual redaction per the cheat sheet in
  `privacy-considerations.md` §3.

## 4. Acceptance criteria

- Real session drop rate measurably zero (verified: 2013 → 1 ParseWarning;
  the remaining 1 is `OutOfOrder`, a meta flag, not an event drop).
- `agentprof analyze --export md` shows "Parse warnings: N" line and
  per-error breakdown when N > 0.
- `agentprof analyze --export md` on a session containing `ask_user`
  emits a separate `## User-blocking tools` section.
- All ADR-0005 §6 documented invariants hold.
- All 8 affected `.snap` files re-accepted in CI-mode (`cargo insta test`).
- Full workspace gate (`cargo fmt && clippy -D warnings && test --all
  --all-features && doc -Dwarnings`) green.

## 5. References

- Audit report: conversation thread (post-output-audit)
- Implementation: `fix/post-output-audit` branch
- Architectural record: ADR-0005 §6 (Update)
- Privacy doc: `docs/internals/privacy-considerations.md`
