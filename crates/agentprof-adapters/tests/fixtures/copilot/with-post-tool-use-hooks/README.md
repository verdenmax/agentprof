# with-post-tool-use-hooks fixture

**Purpose:** lock in the **schema-mismatch parser fix** for three real-world Copilot CLI 1.0.x event shapes that previously caused silent event drops (`ParseWarning::Json`):

| # | Required field that was wrong | Real wire reality | Failure rate observed |
|---|---|---|---|
| 1 | `HookInput.source = String` | `postToolUse` hooks have **no `source`** (they carry `toolName`/`toolArgs`/`toolResult` instead); only `sessionStart` hooks carry `source` | ~778 / 779 hooks (≈100 % of post-tool-use) |
| 2 | `UserMessageData.source = String` | Many CLI-typed prompts omit the `source` key entirely | ~17 / 37 user messages (≈46 %) |
| 3 | `AssistantMessageData.turn_id = String` | Messages emitted by **subagents** (spawned via `subagent.started`) carry `parentToolCallId` instead of `turnId`, and have **no `turnId` field** | ~1 995 / 2 789 assistant messages (≈71 %) |

In one inspected real local session of **11 806 lines**, this trio of bugs silently dropped **2 012 events (~17 %)** before the fix. Pre-fix, `agentprof analyze` therefore showed `postToolUse synthesized=100 %, p50=0ms` and missed all subagent token usage.

The fix is conservative: all three fields became `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Downstream analyzers never read these particular fields, so attribution is unaffected.

## Fixture event-by-event

| Line | Event | What it tests |
|---|---|---|
| 1 | session.start | meta extraction baseline |
| 2 | `user.message` (no `source`) | ★ schema fix #2 |
| 3 | assistant.turn_start | open Turn |
| 4 | tool.execution_start | bash tool starts |
| 5 | tool.execution_complete | bash tool finishes |
| 6 | `hook.start` (postToolUse, no `source`) | ★ schema fix #1 |
| 7 | hook.end (postToolUse) | matches the start (NOT synthesized) |
| 8 | `assistant.message` (subagent: `parentToolCallId`, no `turnId`) | ★ schema fix #3 |
| 9 | assistant.message (main turn: `turnId` present) | regression: in-turn shape still parses |
| 10 | assistant.turn_end | close turn |

## Expected snapshot assertions

After the fix:

- **No `ParseWarning::Json`** entries in `raw.parse_warnings` — all 10 lines parse cleanly.
- `Episodes.tools["bash"].calls.len() == 1` (status `Success`).
- `Episodes.hooks["postToolUse"].calls.len() == 1`, `synthesized_start == false`, `total_duration ≈ 200ms`.
  Pre-fix this was `synthesized_start = true, total = 0ms` (the hook.start was dropped and only orphan hook.end survived).
- `Episodes.turns[0].output_tokens == Some(19)` — both assistant messages contribute:
  the in-turn message's `7` plus the subagent message's `12`. See "Subagent attribution"
  below for why.
- `Episodes.warnings.len() == 0` — no derive warnings.

## Subagent attribution (intentional, with caveat)

`derive_episodes::on_assistant_message` writes `output_tokens` to whichever
`Turn` is currently open (`open_turn_idx`), without checking the payload's
`turn_id` / `parent_tool_call_id`. The reason: subagents that are spawned by
a `task` tool call execute *while the parent turn is still open*, so their
token usage is correctly attributable to the parent turn from a ROI
perspective ("this turn caused those tokens"). Hence the `Some(19)`
expectation in this fixture rather than `Some(7)`.

**Caveat — side-effect of this fix.** Before the post-output-audit T1 fix,
subagent assistant messages were silently dropping at parse (missing
`turnId` field), so they never reached `on_assistant_message` and never
inflated any turn's token count. After the fix they parse cleanly and
*do* inflate the parent turn's `output_tokens` by the subagent's
contribution. In a real session with ~2 K subagent messages, per-main-turn
`output_tokens` therefore increase. This is **closer to reality**, but
users comparing pre-fix and post-fix reports will see counts go up.

A future enhancement (not in this branch; tracked for M1.5+) may add a
`Turn.subagent_output_tokens: Option<u32>` field so the two sources can
be shown separately in the renderer.

## Conventions

- Path uses `/tmp/agentprof-fixture/with-post-tool-use-hooks` per ADR-0003 §3.
- Session UUID uses `00000000-0000-0000-0000-000000002000` series per ADR-0003 §3.
- Wire `id` / `parentId` fields use readable strings (`turn-A`, `tc-bash`, `tc-task`, etc.) — same documented deviation from ADR-0003 §3 as `cross-turn-tool` (justified for human-readable snapshot teaching value).
- The `toolArgs` / `toolResult` fields on the hook.start payload (line 6) are intentionally minimal — they're silently dropped by serde (we don't model them), so we just need any well-formed JSON.

## See also

- `docs/internals/adr-0005-analyzer-and-payload-name.md` §6 (parser-compat schema decisions)
- `crates/agentprof-adapters/src/copilot/event.rs` — `UserMessageData`, `AssistantMessageData`, `HookInput` rustdoc explains the schema reality per field.

