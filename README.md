# agentprof

> Perf flamegraph and ROI profiler for AI coding agents (Claude Code / Codex CLI / Copilot CLI).
> Tell which tools earn their `tools_schema` tokens — and which ones you can safely kill.

**Status: M1.4 — `analyze` subcommand shipped.** First user-facing release;
Copilot CLI session analysis with markdown + JSON output works end-to-end.
TUI, multi-session aggregation, Claude / Codex adapters land in M1.5+.
See [`docs/plan.md`](docs/plan.md) for the roadmap and
[`docs/architecture.md`](docs/architecture.md) for the architecture (L1).

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

## Quick start (M1.4)

`agentprof analyze` is now shipped — it can read a real Copilot CLI session
and produce a structured report:

```sh
# From source (release binaries forthcoming)
git clone <repo>
cd agentprof
cargo install --path crates/agentprof-cli

# Default: analyze your latest Copilot CLI session, markdown to stdout
agentprof analyze

# Choose export format and output destination
agentprof analyze --export json --output report.json

# Analyze a specific session path (handy for testing with the fixtures)
agentprof analyze --session ./crates/agentprof-adapters/tests/fixtures/copilot/cross-turn-tool
```

Sample output structure:

```markdown
# agentprof analyze — <session-uuid>

## Session
- Agent: Copilot (v1.0.99)
- Started: 2026-05-29 12:43:43 UTC
- CWD: /path/to/cwd
- Live: no
- Turns: N
...

## Turn Summary
| # | Turn ID | Status | Duration | Model | Mode | Tools | Hooks | Skills | Out-Tokens |
| 1 | turn-a  | Completed | 2.34s | claude-opus-4.7 | auto | 3 | 1 | 0 | 412 |
...

## Tool Rank (by total duration)
| Tool | Source | Calls | OK | Fail | Orphan | User-req | Total | p50 | p95 | Max |
| bash | Builtin | 12 | 11 | 1 | 0 | 0 | 18.45s | 220ms | 4.20s | 8.10s |
...

## Hook Rank (by total duration)
| Hook | Calls | OK | Fail | Synth | Total | p50 | p95 |
| PreToolUse | 25 | 25 | 0 | 0 | 1.82s | 60ms | 180ms |
...

## Warnings
(none)
```

See [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md) for
full CLI documentation.

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
