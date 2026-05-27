# Fixture: with-hooks-heavy

## Purpose
Stress test for hook.start/hook.end pairing. Verifies that M1.3's `HookEpisode` aggregation does not choke on volume and that fail-rate aggregation handles partial failures correctly.

## Scenarios covered
- 30 paired `hook.start` / `hook.end` events (60 hook events total) distributed evenly across 3 turns (10 pairs per turn)
- Alternating hook types: `PreToolUse` / `PostToolUse`
- 2 hook.end events with `success: false` (global hook indexes 7 and 22)
- 3 `assistant.turn_start` / `assistant.turn_end` envelopes
- One `assistant.message` per turn (no toolRequests; hooks fire post-message)

## FRs exercised
- FR-1.6 (HookStart, HookEnd variants)
- FR-1.7 (Option fields on HookOutput remain absent / default)
- FR-2.4 (large event count tolerated by parser)

## Expected
72 events, 0 parse warnings. 30 `HookStart` + 30 `HookEnd` + 6 turn envelopes + 3 assistant messages + 1 session.start + 1 user.message + 1 session.shutdown.
