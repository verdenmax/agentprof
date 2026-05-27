# Adapter Contribution Guide

> L2 documentation. Authoritative architecture: `docs/architecture.md`.
> Wire-format references for each agent: `docs/internals/adr-NNNN-<agent>-event-schema.md`.

## Why adapters

Each AI agent stores its session telemetry in its own format. Adapters
translate those formats into the unified `RawSession<E>` shape so
`agentprof-core::episode` (M1.3+) can analyze them agent-agnostically.

## Adapter contract

Implement `agentprof_core::adapter::Adapter` (see its rustdoc for full
documentation):

```rust
pub trait Adapter: Send + Sync {
    type Event: Event + DeserializeOwned + Serialize + Debug;
    fn agent_kind(&self) -> AgentKind;
    fn default_session_root(&self) -> Option<PathBuf>;
    fn discover_sessions(&self, root: &Path) -> Result<Vec<SessionRef>, AdapterError>;
    fn load_session(&self, sref: &SessionRef) -> Result<RawSession<Self::Event>, AdapterError>;
}
```

`Self::Event` must implement `agentprof_core::adapter::Event` (`id()` /
`kind()` / `timestamp()` / `parent_id()`).

## Adding a new adapter (e.g. for Claude)

1. **Clean-room observation only.** Read your own local sessions to learn the
   wire format. Never translate vendor SDK type definitions (see
   `adr-0003-synthetic-fixture-strategy.md` for the legal rationale).
2. Create `crates/agentprof-adapters/src/<agent>/` with `mod.rs`, `event.rs`
   (Event enum + payloads + `impl Event`), `parser.rs`, `paths.rs`,
   `adapter.rs` (`impl Adapter for <Agent>Adapter`).
3. Add a new ADR `docs/internals/adr-NNNN-<agent>-event-schema.md` with the
   variant table.
4. Author 5-9 synthetic fixtures under
   `crates/agentprof-adapters/tests/fixtures/<agent>/` (see
   `adr-0003-synthetic-fixture-strategy.md` for rules: 100% synthetic, no
   anonymizer, well-documented per-fixture READMEs).
5. Add per-variant round-trip tests + per-fixture snapshot tests.
6. Add an entry in `registry::adapter_for` and `registry::supported_agents`.
7. Update this file's "Supported agents" matrix.
8. Update `crates/agentprof-adapters/README.md`'s matrix.
9. CHANGELOG entry: `feat(adapters): add <agent> adapter`.

## Supported agents (matrix duplicate; canonical in README.md)

| Agent | Module | Status |
|---|---|---|
| GitHub Copilot CLI | `copilot` | ✅ MVP (M1.2) |
| Anthropic Claude Code | (planned) | ⏳ Phase 2 |
| OpenAI Codex CLI | (planned) | ⏳ Phase 3 |

## Fixture rules

See `crates/agentprof-adapters/tests/fixtures/<agent>/README.md` for the
authoring catalog. Universal rules:

- 100% synthetic content (no real-data anonymization)
- Stable UUIDs `00000000-0000-0000-0000-NNNNNNNNNNNN`
- Placeholder paths: `/tmp/agentprof-fixture/<scenario>`
- Round-trip test required (`serde_json::from_str::<<Agent>Event>` must
  succeed on every committed line except in intentionally-corrupt fixtures)
- `expected.json` generated via `cargo insta accept` after the parser stabilizes

## Local smoke tests

For developers wanting to check against their own real sessions without
committing data:

```bash
export AGENTPROF_LOCAL_FIXTURES_DIR=~/.<agent>/<path>
cargo test -p agentprof-adapters --test <agent>_smoke -- --include-ignored
```
