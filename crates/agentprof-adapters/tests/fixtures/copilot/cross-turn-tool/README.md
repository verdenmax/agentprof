# cross-turn-tool fixture

**Purpose:** lock in the commit-call-turn-divergence fix (M1.4 Phase A.3).

Tool `bash` (tool_call_id `tc-X`) **starts in turn-A** and **completes
after turn-B has started**. The ToolExecComplete event has wire
`parentId = turn-B` (current open turn at write time) but its payload
`turnId = turn-A` (the turn at start time, copied from the matching
ToolExecStart per Copilot CLI wire convention). This `parentId !=
data.turnId` divergence on line 6 is the exact trigger ADR-0005 D-2
addresses — without the fix the back-reference would land in turn-B.

**Scenarios covered:**

| Line | Event | What it tests |
|---|---|---|
| 1 | session.start | meta extraction |
| 2 | turn-A start (`+1s`) | open Turn |
| 3 | tool.execution_start (in turn-A, `+1s`) | OpenToolCall pushed with turn_id="turn-A" |
| 4 | turn-A end (`+1s`) | close turn-A (status=Completed) |
| 5 | turn-B start (`+1s`) | open new Turn |
| 6 | tool.execution_complete (during turn-B, `+3s`) | pop OpenToolCall, commit; `parentId=turn-B` vs `data.turnId=turn-A` — ADR-0005 D-2 requires back-ref to land in turn-A.tool_calls (NOT turn-B). The 3s gap from line 5 is load-bearing: the resulting `bash.total_duration = [5, 0]` confirms the tool span crossed the turn boundary, not just instant. |
| 7 | turn-B end (`+1s`) | close turn-B |

**Expected episode_derive snapshot:**

- `Episodes.turns.len() == 2` (both Completed)
- `turns[0].id == "turn-A"`, `turns[0].tool_calls.len() == 1` ★ key assertion
- `turns[1].id == "turn-B"`, `turns[1].tool_calls.len() == 0` ★ key assertion
- `Episodes.tools["bash"].calls.len() == 1` with `turn_id = Some("turn-A")` and `status = Success`
- `Episodes.tools["bash"].total_duration` reflects the 5s start→complete span ★
- `Episodes.warnings.len() == 0` (clean fixture, no orphans/anomalies)

**Conventions:**

- Path uses `/tmp/agentprof-fixture/cross-turn-tool` per ADR-0003 §3.
- Session UUID uses the `00000000-0000-0000-0000-000000001000` series
  per ADR-0003 §3.
- Wire `id` / `parentId` fields **deliberately use readable strings**
  (`turn-A`, `tool-X-start`, etc.) rather than the UUID series. This is
  a documented deviation from ADR-0003 §3, justified because this
  fixture's primary purpose is teaching the cross-turn pattern through
  a human-readable snapshot. `derive_episodes` copies wire `id` →
  `Turn.id` → `ToolCall.turn_id`, so the snapshot's `turns[0].id ==
  "turn-A"` assertion would be opaque (a UUID) if we followed the
  convention literally. The inner `turnId`/`toolCallId`/`interactionId`
  payload fields also use readable strings for the same reason.
