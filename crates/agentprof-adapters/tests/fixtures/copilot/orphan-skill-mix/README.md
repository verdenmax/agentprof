# Fixture: orphan-skill-mix

## Purpose
B-6 (M1.6.4 follow-up M-3): closes a turn cleanly, then emits BOTH an
orphan tool completion AND a skill invocation after the turn has ended.
Exercises the orphan-section ordering and Speedscope's clamping logic
for events that fall outside any open turn frame.

## Scenarios covered
- Turn 0: normal `bash` tool call (matched start + complete)
- After `assistant.turn_end`:
  - `tool.execution_complete` for `tc-c3-orphan-tool` with NO matching
    start (orphan tool)
  - `skill.invoked` for `orphan-skill` with parentId pointing at the
    already-closed turn_end (orphan skill load)
- `session.shutdown` closes cleanly

## FRs exercised
- FR-2.5 (orphan synthesis path in `derive_episodes`)
- FR-2.6 (skill invocation outside a turn)

## Expected
- 10 events, 0 parse warnings
- 1 `SkillInvoked` event (orphan)
- 1 `tool.execution_complete` with no matching start (orphan tool)
- In the analyzer output, the orphan tool surfaces in the orphan section
  rather than within turn 0
