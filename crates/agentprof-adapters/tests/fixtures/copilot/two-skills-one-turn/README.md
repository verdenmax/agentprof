# Fixture: two-skills-one-turn

## Purpose
B-6 (M1.6.4 follow-up M-3): one turn invokes TWO distinct skills,
`code-reviewer` and `git-flow`. Locks in that the analyzer aggregates
distinct `ToolSource::Skill { name }` rows separately rather than
collapsing them into a single "skill" bucket.

## Scenarios covered
- Two `skill.invoked` events emitted back-to-back before `turn_start`
- Single turn whose `assistant.message` requests two tool calls, one for
  each skill: `skill__code-reviewer__run` and `skill__git-flow__release`
- Both tool calls have matching start + complete pairs

## FRs exercised
- FR-2.3 (per-skill grouping in tool_rank)
- FR-2.6 (multiple skills in a single turn)

## Expected
- 12 events, 0 parse warnings
- 2 `SkillInvoked` events
- 2 distinct `ToolSource::Skill` rows in `tool_rank`:
  `Skill { name: "code-reviewer" }` and `Skill { name: "git-flow" }`
