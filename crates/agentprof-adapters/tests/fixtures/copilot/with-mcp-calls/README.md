# Fixture: with-mcp-calls

## Purpose
Two MCP tool calls; one succeeds, one fails. Verifies `ToolSource::Mcp { server }` inference for `github` and `filesystem` servers.

## Scenarios covered
- AssistantMessage with 2 toolRequests using `mcp__<server>__<tool>` naming
- One tool.execution_complete with `success: true`, one with `success: false` + `error` payload
- ToolSource::infer should classify both as Mcp with distinct server names

## FRs exercised
- FR-1.6 (ToolExecStart, ToolExecComplete variants)
- FR-2.3 (Mcp tool source inference)
- FR-1.7 (optional `error` field on tool.execution_complete)

## Expected
10 events, 0 parse warnings, 2 tool names both prefixed with `mcp__`.
