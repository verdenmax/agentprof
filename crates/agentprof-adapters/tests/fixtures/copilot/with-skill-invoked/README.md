# Fixture: with-skill-invoked

## Purpose
Skill invocation followed by a tool call within the same turn. M1.3's `derive_episodes` will attribute the tool call to the skill.

## Scenarios covered
- `skill.invoked` event between user.message and turn_start
- Plugin-sourced skill (`source: plugin`, `pluginName: synthetic`, `pluginVersion: 0.0.0`)
- Trigger `agent-invoked`
- Subsequent tool call within the same interaction

## FRs exercised
- FR-1.6 (SkillInvoked variant)
- FR-1.7 (Option fields on SkillData: pluginName, pluginVersion)

## Expected
9 events, 0 parse warnings, at least one `SkillInvoked` event present.
