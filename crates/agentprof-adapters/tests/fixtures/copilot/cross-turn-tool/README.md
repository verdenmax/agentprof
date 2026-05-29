# cross-turn-tool fixture

**Purpose:** lock in the commit-call-turn-divergence fix (M1.4 Phase A.3).

Tool `bash` (tool_call_id `tc-X`) **starts in turn-A** and **completes
after turn-B has started**. The ToolExecComplete event has wire
`parentId = turn-B` (current open turn at write time) but its payload
`turnId = turn-A` (the turn at start time, copied from the matching
ToolExecStart per Copilot CLI wire convention).

**Scenarios covered:**

| Line | Event | What it tests |
|---|---|---|
| 1 | session.start | meta extraction |
| 2 | turn-A start | open Turn |
| 3 | tool.execution_start (in turn-A) | OpenToolCall pushed with turn_id="turn-A" |
| 4 | turn-A end | close turn-A (status=Completed) |
| 5 | turn-B start | open new Turn |
| 6 | tool.execution_complete (during turn-B) | pop OpenToolCall, commit; ADR-0005 D-2 requires back-ref to land in turn-A.tool_calls (NOT turn-B) |
| 7 | turn-B end | close turn-B |

**Expected episode_derive snapshot:**

- `Episodes.turns.len() == 2` (both Completed)
- `turns[0].id == "turn-A"`, `turns[0].tool_calls.len() == 1` ★ key assertion
- `turns[1].id == "turn-B"`, `turns[1].tool_calls.len() == 0` ★ key assertion
- `Episodes.tools["bash"].calls.len() == 1` with `turn_id = Some("turn-A")` and `status = Success`
- `Episodes.warnings.len() == 0` (clean fixture, no orphans/anomalies)

**Per ADR-0003:** synthetic UUIDs `00000000-0000-0000-0000-000000001000`
series; path `/tmp/agentprof-fixture/cross-turn-tool`.
