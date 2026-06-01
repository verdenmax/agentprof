# Fixture: multi-sess-b

## Purpose
Multi-session aggregation companion fixture (M1.6.2). Same model as
multi-sess-a, **different UTC date**, no MCP usage.

## Profile
- Date (UTC): 2026-05-30
- Model: gpt-5
- Turns: 1
- Tools: bash (1 call, 1s, success), str_replace_editor (1 call, 1s, success)
- Hooks: none
- Skills: none

## Aggregation coverage
- `--by tool`: contributes 1 bash + 1 str_replace_editor
- `--by mcp-server`: contributes 0 (no MCP)
- `--by day`: one of 3 distinct UTC dates
- `--by model`: shares model `gpt-5` with multi-sess-a → bumps that
  bucket's `session_count` to 2
