# agentprof

> Perf flamegraph and ROI profiler for AI coding agents (Claude Code / Codex CLI / Copilot CLI).
> Tell which tools earn their `tools_schema` tokens — and which ones you can safely kill.

**Status: pre-alpha skeleton.** Architecture is finalized in
[`docs/architecture.md`](docs/architecture.md). Phase 0 prototype implementation
has not started yet — see [`docs/plan.md`](docs/plan.md) for the roadmap.

---

## Why agentprof

CLI agents like Claude Code, Codex CLI, and Copilot CLI report token totals
(`/cost`, `ccusage`, `tokscale`, …) but never tell you what is *actually* being
spent on. With MCP servers proliferating, it is common to load 5+ servers and
20 000+ tokens worth of tool schemas — most of which are **never invoked** in a
given session.

`agentprof` answers:

- How does the context window break down per turn? (`system / tools_schema / history / user / tool_result / output`)
- Which loaded tools were never called? How much did that cost?
- Which MCP server has the worst ROI?
- Is `schema_utilization` trending down over time?

---

## Quick links

- **Roadmap & motivation**: [`docs/plan.md`](docs/plan.md)
- **Architecture (L1)**: [`docs/architecture.md`](docs/architecture.md)
- **Contributing & documentation rules**: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- **Change log**: [`CHANGELOG.md`](CHANGELOG.md)
- **AI-assistant instructions**: [`.github/copilot-instructions.md`](.github/copilot-instructions.md)

---

## Install (future)

```sh
cargo install agentprof          # multi-platform binaries via cargo-dist
# or
cargo install --git https://github.com/agentprof/agentprof agentprof-cli
```

The above are placeholders — release binaries will be wired up once the first
prototype lands.

---

## Quick usage (future CLI surface)

```sh
agentprof analyze    --agent claude --export tui          # interactive flamegraph
agentprof analyze    --agent claude --export speedscope   # open in speedscope.app
agentprof analyze    --agent claude --export html --out report.html
agentprof aggregate  --by mcp-server --since 30d          # ROI leaderboard
agentprof watch      --agent claude                       # live TUI
```

See [`docs/architecture.md`](docs/architecture.md) §8 for the canonical CLI
protocol and exit codes.

---

## Development

```sh
# format / lint
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings

# test (includes doctests + snapshots)
cargo test --workspace --all-features
cargo insta test --check

# docs (warnings are errors)
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace

# dependency audit
cargo deny check
```

For repository structure and crate boundaries, see
[`docs/architecture.md`](docs/architecture.md) §3 and §15.1.

---

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
