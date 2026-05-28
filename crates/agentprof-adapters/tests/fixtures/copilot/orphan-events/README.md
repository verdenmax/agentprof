# orphan-events fixture

**Purpose:** exercise `derive_episodes` orphan-synthesis paths and the
`AbortWithoutOpenElement` path. Driven by ADR-0004 IMP-006.

## Scenarios covered

| Line | Event | What it tests |
|---|---|---|
| 3 | `hook.end` (orphan) | `DeriveWarning::SynthesizedStart` for `HookStart` |
| 4 | `tool.execution_complete` (orphan) | `DeriveWarning::SynthesizedStart` for `ToolExecStart` |
| 6 | `abort` (no open element) | `DeriveWarning::AbortWithoutOpenElement` + entry in `Episodes.aborts` |

## Expected `derive_episodes` output

- `Episodes.tools.len() == 1` with one `ToolCallStatus::OrphanSynthesizedStart` call
- `Episodes.hooks.len() == 1` with one `HookCall { synthesized_start: true, .. }`
- `Episodes.turns.len() == 1` with `TurnStatus::Completed`
- `Episodes.aborts.len() == 1`
- `Episodes.warnings.len() == 3`

## Notes

- UUIDs use the `00000000-0000-0000-0000-000000000900` series.
- Paths use `/tmp/agentprof-fixture/orphan-events`.
- Per ADR-0003 (100% synthetic).
- No `session.shutdown` — the session ends with an abort at rest.
