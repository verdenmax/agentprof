# Fixture: with-span-overlap

## Purpose
Lock Speedscope export's overlap-adjustment behavior (D-15). The derive layer accepts overlapping tool spans within a single turn without emitting a `DeriveWarning`; the export layer must detect the overlap and shift the later span by +1 ms so the resulting Speedscope profile satisfies its strict-nesting requirement.

## Scenarios covered
- One assistant turn (`turnId="0"`) wrapping two `bash` tool calls.
- Tool `call-A`: `tool.execution_start` at 13:00:01Z, `tool.execution_complete` at 13:00:03Z (2 s span).
- Tool `call-B`: `tool.execution_start` at 13:00:02Z, `tool.execution_complete` at 13:00:04Z (2 s span; **overlaps `call-A` by 1 s**).

## Expected
- 10 events, 0 parse warnings.
- `derive_episodes` produces exactly 1 turn with 2 `ToolCall`s on the `bash` tool, no `DeriveWarning` (the derive layer does not enforce strict nesting).
- `agentprof_core::export::speedscope::to_speedscope` returns a strictly-nested `SpeedscopeProfile` and exactly one `ExportWarning::SpanAdjustedForSpeedscope { tool_name: "bash", .. }` (D-15).
