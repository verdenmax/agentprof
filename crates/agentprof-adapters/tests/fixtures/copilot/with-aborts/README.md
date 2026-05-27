# Fixture: with-aborts

## Purpose
Abort attribution: 3 `abort` events fire at strategic points so each maps to a different open episode (or session-level when nothing is open).

## Scenarios covered
- Abort #1 during an open tool execution (`tool.execution_start` with no matching `tool.execution_complete` afterwards)
- Abort #2 during an open hook (`hook.start` followed by abort then `hook.end` with `success: false`)
- Abort #3 between turns, with no episode open — attaches to session level

## FRs exercised
- FR-1.6 (Abort variant)
- FR-2.5 (abort attribution to nearest open episode in M1.3 — fixture data only here)

## Expected
15 events, 0 parse warnings, exactly 3 `Abort` events present.
