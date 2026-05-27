# Fixture: live-truncated

## Purpose
Live session: the parser must (a) tolerate the incomplete tail line silently when `is_live=true`, (b) build meta from `session.start`, (c) emit no `Unclosed` warnings (live session suppresses them — implementation in M1.3).

## Scenarios covered
- 3 complete events (`session.start`, `user.message`, `assistant.turn_start`)
- A 4th line that is the prefix of an `assistant.message` with no closing braces and no trailing newline (simulates a flush mid-write)
- No `session.shutdown` event
- `session.start.data.alreadyInUse: true` mirrors the on-disk lock file
- Companion `inuse.778482.lock` empty file alongside `events.jsonl` (used by Task 12's `paths.rs` to flag the session as live)

## FRs exercised
- FR-1.5 (`is_live` parameter behaviour)
- FR-1.7 (partial-tail tolerance in live mode)
- FR-1.8 (warning emission for partial-tail in closed mode)

## Expected
- `parse_events_jsonl(..., is_live=true)`: 3 events, 0 parse warnings, `meta.is_live = true`.
- `parse_events_jsonl(..., is_live=false)`: 3 events, 1 parse warning (Json) for the truncated tail.
