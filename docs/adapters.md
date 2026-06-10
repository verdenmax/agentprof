# Adapter Contribution Guide

> L2 documentation. Authoritative architecture: `docs/architecture.md`.
> Wire-format references for each agent: `docs/internals/adr-NNNN-<agent>-event-schema.md`.

## Why adapters

Each AI agent stores its session telemetry in its own format. Adapters
translate those formats into the unified `RawSession<E>` shape so
`agentprof-core::episode` (M1.3+) can analyze them agent-agnostically.

> **OTLP is *not* an adapter.** The OTLP receiver (M2.2, feature `otlp`;
> see [ADR-0021](internals/adr-0021-otlp-receiver-architecture.md))
> deliberately does **not** implement the `Adapter` trait. `Adapter` is a
> file-pull / per-session iteration model (`discover_sessions(&Path) →
> Vec<SessionRef>` then `load_session(&SessionRef) → RawSession`), which
> is structurally incompatible with OTLP's push-mode streaming and
> cross-cutting session grouping (events from many sessions are
> interleaved on the wire and must be routed into per-`session.id`
> in-memory buffers with idle / size / shutdown flush). From the user's
> perspective OTLP is still a *session source* (analyze / list /
> aggregate treat OTLP-ingested rows identically to file-ingested ones),
> but its wiring goes through `agentprof_storage::otlp::SessionRouter`
> + `StorageFlushSink → upsert_report` rather than the `Adapter` trait.
> See [ADR-0021 §Decision 3](internals/adr-0021-otlp-receiver-architecture.md)
> for the full rationale.

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
7. Wrap the `discover_sessions` / `load_session` hot paths in
   `#[tracing::instrument(name = "adapter.discover", ...)]` and
   `#[tracing::instrument(name = "adapter.parse", ...)]` spans, matching
   the copilot adapter (`crates/agentprof-adapters/src/copilot/{paths,parser}.rs`).
   Hash any session-state path attached as a span field via
   `agentprof_core::observability::pii::hash_path` — see
   [ADR-0010](../internals/adr-0010-tracing-infrastructure.md) Layer 2 +
   D-3 / D-13 (PII redaction) and `docs/features/privacy.md` §7.
8. Update this file's "Supported agents" matrix.
9. Update `crates/agentprof-adapters/README.md`'s matrix.
10. CHANGELOG entry: `feat(adapters): add <agent> adapter`.

## Supported agents (matrix duplicate; canonical in `crates/agentprof-adapters/README.md`)

| Agent | Module | Status |
|---|---|---|
| GitHub Copilot CLI | `copilot` | ✅ MVP (M1.2) + ongoing schema iterations through M1.6.x |
| Anthropic Claude Code | (planned) | ⏳ Phase 3 / M3.1 |
| OpenAI Codex CLI | (planned) | ⏳ Phase 3 / M3.2 |

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

## Optional `Event` overrides (F1)

These trait methods on `agentprof_core::adapter::Event` have safe
default impls (empty `Vec` / `None`) so existing adapters compile
unchanged. New adapters wishing to enable the TUI `TurnDetailView`'s
args preview SHOULD implement both as a pair.

### `Event::payload_tool_requests`

Adapters that want their users to benefit from the TUI
`TurnDetailView`'s args preview SHOULD implement this method. Return a
`Vec<(String, serde_json::Value)>` of `(tool_call_id, arguments)` pairs
declared by the event. Default impl returns empty `Vec`; adapters
without an override silently ship the `(not captured)` placeholder in
the TUI detail view.

Example (Copilot adapter): see `crates/agentprof-adapters/src/copilot/event.rs`
for the canonical impl across `AssistantMessage` (multi-pair) and
`ToolUserRequested` (single-pair) variants.

See ADR-0011 D-2 for rationale on the method shape (returns `Vec` not
`Option<...>` because some events carry multiple tool requests).

### `Event::tool_call_id`

Companion to `payload_tool_requests`. Adapters that emit args via the
above MUST also implement this so `derive_episodes` can look up args
at tool-close time. Returns `Option<&str>` — `Some` for variants whose
payload carries `tool_call_id` (typically `ToolExecStart`,
`ToolExecComplete`, `ToolUserRequested`), `None` otherwise. See
ADR-0011 D-3 + D-6-revised for rationale.

### Optional: `Event::payload_model_metrics` (F1.7)

Adapters that want their users to see per-model session-level token
totals (input / output / cache_read / cache_write) in the TUI
`Models` view (key `4`) SHOULD implement this method. Return
`Some(BTreeMap<String, ModelUsage>)` for the event variant that
carries the rollup; `None` otherwise. Default returns `None`;
adapters without an override silently ship the "no model usage
data" empty-state in the Models view.

Last-wins on multiple emitting events (matches `Turn::model`
semantics). For Copilot CLI, the data lives in
`session.shutdown.modelMetrics[model].usage` — see
`crates/agentprof-adapters/src/copilot/event.rs` for the
canonical free-form `serde_json::Value` walker that's robust
against wire schema drift.

See ADR-0012 D-4 + D-7 for rationale.
