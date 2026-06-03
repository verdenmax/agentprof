# Fixture: tool-and-skill-same-turn

## Purpose
B-6 (M1.6.4 follow-up M-3): a single turn contains BOTH a builtin tool
call (`bash`) AND a skill invocation (`code-reviewer`). Exercises the
speedscope / HTML / markdown renderers' ability to display heterogeneous
child events under the same turn frame.

## Scenarios covered
- Turn 0 is a smoke warm-up (no tools)
- Turn 1 has a `skill.invoked` event between `user.message` and
  `turn_start`, and the assistant message inside the turn requests TWO
  tool calls: `bash` (builtin) and `skill__code-reviewer__run`
  (classifies as `ToolSource::Skill { name: "code-reviewer" }`)
- Both tool calls have matching start + complete pairs

## FRs exercised
- FR-2.3 (tool classification across all ToolSource variants in one turn)
- FR-2.6 (skill invocation in turn)

## Expected
- 15 events, 0 parse warnings
- 1 `SkillInvoked` event
- 2 distinct `ToolSource` rows in `tool_rank`: `Builtin` and
  `Skill { name: "code-reviewer" }`
