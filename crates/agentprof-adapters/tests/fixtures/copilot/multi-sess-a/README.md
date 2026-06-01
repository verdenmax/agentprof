# Fixture: multi-sess-a

## Purpose
Multi-session aggregation companion fixture (M1.6.2). One of three
sibling fixtures (`multi-sess-a/b/c`) that together exercise
cross-session aggregation across the 4 group-by keys.

## Profile
- Date (UTC): 2026-05-29
- Model: gpt-5
- Turns: 2
- Tools: bash (2 calls, all 1s, all success),
  mcp__github__list_pulls (1 call, 2s, success)
- Hooks: none
- Skills: none

## Aggregation coverage
- `--by tool`: contributes 2 bash + 1 mcp__github__list_pulls
- `--by mcp-server`: contributes 1 call to server `github`
- `--by day`: one of 3 distinct UTC dates
- `--by model`: one of 2 distinct models (shared with multi-sess-b)
