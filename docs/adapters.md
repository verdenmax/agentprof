# docs/adapters — adding a new agent

> L2 contributor guide for implementing a new adapter under `crates/agentprof-adapters`.

Status: **placeholder** — full content lands together with the first real
adapter implementation. The checklist below is normative even today.

## Checklist for a new adapter

Every new adapter (`gemini`, `qwen`, future Copilot variants, …) must deliver
all of the following **in a single PR**:

1. **Source**: `crates/agentprof-adapters/src/<name>.rs`
   - File-level `//!` rustdoc covering: data source path, wire-format notes,
     known quirks, and a link to this guide
   - `impl Adapter for <Name>Adapter`
2. **Registry**: register the new adapter in
   `crates/agentprof-adapters/src/registry.rs`
3. **Fixtures**: at least one anonymized session under
   `crates/agentprof-adapters/tests/fixtures/<name>/`
   - Use `cargo run -p xtask -- anonymize <real-log>` to scrub paths /
     emails / tokens before committing
   - Fixture must contain ≥1 unused tool, ≥1 frequently-called tool, ≥1
     failing tool result (covers the three ROI buckets)
4. **Unit tests**: `crates/agentprof-adapters/tests/<name>.rs` — assert
   correct parsing of the fixtures above
5. **Integration test**: at least one `assert_cmd` test in
   `crates/agentprof-cli/tests/cli.rs` running
   `analyze --agent <name> --path <fixture>`
6. **Documentation**:
   - Update L1 `docs/architecture.md` §6 (default path table) and §17
     (roadmap, if applicable)
   - Expand this file (`docs/adapters.md`) with the adapter's wire-format
     notes
   - Update L2 `crates/agentprof-adapters/README.md` "supported agents"
     section
   - rustdoc on the public adapter struct + every public method
7. **Changelog**: add a `feat: add <name> adapter` entry to `CHANGELOG.md`
8. **Conventional Commit** message: `feat(adapters): add <name> adapter`

## Wire-format notes (to be filled in)

Each adapter implementation should append a subsection here describing:

- File system layout
- JSON / JSONL schema highlights
- How tools are serialized into the prompt (this directly affects
  `schema_tokens` accuracy)
- Anything the implementor wishes the next person knew

### claude (Claude Code)

*To be filled in alongside the Phase 0 prototype.*

### codex (OpenAI Codex CLI)

*To be filled in during Phase 3.*

### copilot (GitHub Copilot CLI)

*To be filled in during Phase 3, after a real session log is captured.*
