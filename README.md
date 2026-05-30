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
- Tools tracked: 5
- Hooks tracked: 2
- Derive warnings: 0
- Parse warnings: 0

## Turn Summary
| # | Turn ID | Status | Duration | Model | Mode | Tools | Hooks | Skills | Out-Tokens |
| 1 | turn-a  | Completed | 2.34s | claude-opus-4.7 | interactive | 3 | 1 | 0 | 412 |
...

## Tool Rank (by total duration)
| Tool | Source | Calls | OK | Fail | Orphan | User-req | Total | p50 | p95 | Max |
| bash | Builtin | 12 | 11 | 1 | 0 | 0 | 18.45s | 220ms | 4.20s | 8.10s |
...

## User-blocking tools (wall-clock includes user think time)
These tools block on the human, not on agent or machine work; their `Total` reflects how long the user took to respond, not engineering cost.
| Tool | Source | Calls | OK | Fail | Orphan | User-req | Total | p50 | p95 | Max |
| ask_user | Builtin | 6 | 6 | 0 | 0 | 0 | 14.2m | 1.4m | 5.1m | 5.1m |

## Hook Rank (by total duration)
| Hook | Calls | OK | Fail | Synth | Total | p50 | p95 |
| postToolUse | 25 | 25 | 0 | 0 | 1.82s | 60ms | 180ms |
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

## TUI (M1.5)

`agentprof analyze --export tui` opens an interactive ratatui terminal UI on top of the same `AnalysisReport` the markdown / JSON exporters consume. Three views, all driven from one session:

**FlamegraphView (`1`):** Per-turn horizontal gantt; each row is one turn; segments are tool calls; whitespace = LLM thinking time.

```
┌─ Flamegraph (1/3) ───────────────────────────────────────────────┐
│   T1   9.6s  ██████████████░░                                    │
│   T2   4.7s  ██████░░                                            │
│   T3  11.6s  ██████████████████████░ (1 FAIL)                    │
│ ...                                                              │
└──────────────────────────────────────────────────────────────────┘
```

**RoiView (`2`):** Interactive tool rank; press `1`/`2`/`3`/`4` to cycle sort key. User-blocking tools (e.g. `ask_user`) split into a separate sub-table so think time doesn't skew the headline rank.

```
┌─ RoiView (2/3) — Sort: [1]total  2=calls  3=success%  4=p50 ────┐
│ #  Tool       Source   Calls  OK  Fail   Total    p50           │
│ 1  bash       builtin   1641 1641   0    57.4m   12ms           │
│ 2  task       builtin    137  137   0    4.90h   3.2s           │
│ ─ User-blocking (user think time) ─────────────────────────────  │
│    ask_user   builtin     61   61   0    69.4h                  │
│ ─ Selected: bash ───────────────────────────────────────────── │
│   t-1 (609ms✓)  t-3 (1.2s✓)  t-7 (412ms✓)  t-12 (FAIL✗)         │
└─────────────────────────────────────────────────────────────────┘
```

**AggregateView (`3`):** Single-session breakdown — By Mode (interactive / plan / autopilot) + By Hook.

Key bindings: `1`/`2`/`3` switch view, `Tab` cycles, `↑`/`↓` selects, `?` opens help, `q` quits.

Requires a TTY on stdout; piping yields `OutputError` (exit 3) with a helpful message. See [`crates/agentprof-tui/README.md`](crates/agentprof-tui/README.md) and [ADR-0006](docs/internals/adr-0006-panic-safe-tui.md) for the panic-safe lifecycle.

---

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
