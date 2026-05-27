# Fixture: builtin-tools-only

## Purpose
A session using only built-in tools (bash, str_replace_editor) with no MCP / Skill / Hook invocations.

## Scenarios covered
- AssistantMessage with 2 toolRequests
- Two paired tool.execution_start / tool.execution_complete cycles
- ToolSource::infer should classify both as Builtin

## FRs exercised
- FR-1.6 (ToolExecStart, ToolExecComplete variants)
- FR-2.3 (Builtin tool source inference)

## Expected
10 events, 0 parse warnings, 2 distinct tool names (both Builtin).
