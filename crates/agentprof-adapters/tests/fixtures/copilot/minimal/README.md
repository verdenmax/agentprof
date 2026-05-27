# Fixture: minimal

## Purpose
Smallest valid session that exercises the happy path: start → user prompt → assistant turn (no tools) → shutdown.

## Scenarios covered
- Session start with full SessionContext
- One user.message with transformedContent equal to content (no system-prompt expansion)
- One assistant turn opened, one assistant.message with empty toolRequests, one turn closed
- Clean shutdown with empty codeChanges and empty modelMetrics

## FRs exercised
- FR-1.5 (load_session basic happy path)
- FR-1.6 (CopilotEvent variants: SessionStart, UserMessage, TurnStart, AssistantMessage, TurnEnd, Shutdown)
- FR-1.7 (Option<T> defaults work for absent fields)

## Expected
6 events, 0 parse warnings, meta.is_live=false, meta.id ==
"00000000-0000-0000-0000-000000000001".
