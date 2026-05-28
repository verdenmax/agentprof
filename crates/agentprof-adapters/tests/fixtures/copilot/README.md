# Copilot fixtures

These fixtures are **100% synthetic** — they are NEVER derived from real
sessions. Per `docs/internals/adr-0003-synthetic-fixture-strategy.md`.

## Authoring rules
- Paths use `/tmp/agentprof-fixture/<scenario-slug>`
- UUIDs follow `00000000-0000-0000-0000-NNNNNNNNNNNN` pattern for stability
- User prompts are `[fixture-prompt-N]` placeholder strings
- Assistant content is `[fixture-response-N]` placeholder
- Tool args use minimal synthetic values
- All `events.jsonl` lines must round-trip through `serde_json::from_str::<CopilotEvent>` (except the `corrupt/` fixture's intentionally-bad line)
- Each fixture has its own README explaining purpose and assertions
- `expected.json` is generated and locked via `cargo insta review`

## Scenario catalog
| Fixture | Purpose | FRs |
|---|---|---|
| `minimal/` | Smallest valid session | FR-1.5, FR-1.6, FR-1.7 |
| `corrupt/` | One broken line; parser accumulates warning | FR-1.8 |
| `builtin-tools-only/` | bash + str_replace_editor only | FR-2.3 |
| `with-mcp-calls/` | mcp__github__*, mcp__filesystem__* | FR-2.3, FR-2.4 |
| `with-skill-invoked/` | skill.invoked + subsequent tool calls | FR-2.3, FR-2.6 |
| `with-hooks-heavy/` | 30+ hook.start/end pairs | FR-2.5 |
| `with-aborts/` | abort during tool, hook, between turns | FR-2.8 |
| `with-mode-transitions/` | interactive → plan → autopilot | FR-2.7 |
| `live-truncated/` | No shutdown, inuse.lock, last line partial | FR-1.4, FR-1.9 |
| `orphan-events/` | Test `derive_episodes` orphan synthesis + abort-without-open paths (M1.3 Phase C) | FR-2.5, FR-2.8 |

## Local smoke tests (developers only)

```bash
export AGENTPROF_LOCAL_FIXTURES_DIR=~/.copilot/session-state
cargo test -p agentprof-adapters --test copilot_smoke -- --include-ignored
```

These do NOT commit anything; they assert the parser handles real local
data without errors and emits zero `Unknown` events (schema-drift check).
