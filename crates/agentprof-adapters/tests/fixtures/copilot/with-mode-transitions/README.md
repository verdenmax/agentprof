# Fixture: with-mode-transitions

## Purpose
Mode segments: validates `ModeSegment` construction in M1.3 (4 segments: `interactive` → `plan` → `autopilot` → `interactive`).

## Scenarios covered
- Session implicitly starts in `interactive`
- 3 `session.mode_changed` events: interactive→plan, plan→autopilot, autopilot→interactive
- One assistant turn occurs while in `plan` mode, one while in `autopilot`
- Final segment (`interactive`) closes at `session.shutdown`

## FRs exercised
- FR-1.6 (ModeChanged variant)
- FR-2.6 (mode-segment derivation in M1.3 — fixture data only here)

## Expected
12 events, 0 parse warnings, exactly 3 `ModeChanged` events yielding 4 mode segments.
