# Fixture: multi-sess-c

## Purpose
Multi-session aggregation companion fixture (M1.6.2). **Different model**
from a/b (`claude-sonnet-4.6`), includes a tool failure and a skill
invocation.

## Profile
- Date (UTC): 2026-05-31
- Model: claude-sonnet-4.6
- Turns: 3
- Tools:
  - mcp__github__create_pr (1 call, 2s, success)
  - bash (2 calls; turn 1 = 1s failure, turn 2 = 1s success)
- Hooks: none
- Skills: 1 invocation of `synthetic-example` (plugin source)

## Aggregation coverage
- `--by tool`: contributes bash failure to bash's `failure_count`
- `--by mcp-server`: contributes 1 call to server `github` (different
  tool name than multi-sess-a → bumps `github.tool_count` to 2)
- `--by day`: one of 3 distinct UTC dates
- `--by model`: introduces a 2nd distinct model `claude-sonnet-4.6`
