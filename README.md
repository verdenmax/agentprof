# agentprof

> Perf flamegraph and ROI profiler for AI coding agents (Claude Code / Codex CLI / Copilot CLI).
> Tell which tools earn their `tools_schema` tokens — and which ones you can safely kill.

**Status: M1.6.4 — Speedscope + HTML (2026-05-31, ADR-0007) + tracing infrastructure (2026-06-02, ADR-0010) shipped + 2026-06-03 follow-up wave; M1.7 v0.1.0 release pending.** MVP feature
work complete (8/8 shippable surface ≈ 98%; M1.1–M1.6.4 ✅; 剩 M1.7 v0.1.0 release); `analyze` / `list` / `aggregate` / `watch`
all functional end-to-end against real Copilot CLI sessions, with five
export formats (`md` / `json` / `csv` / `html` / `tui`) plus `speedscope`
for single-session flamegraphs. Claude / Codex adapters remain Phase 3
post-MVP. Next milestone: **M1.7 v0.1.0 release** (`cargo-dist` binaries +
GitHub Release).
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

## Install (M1.7 — release pending)

```sh
cargo install agentprof          # multi-platform binaries via cargo-dist
# or
cargo install --git https://github.com/agentprof/agentprof agentprof-cli
```

The `cargo install agentprof` form is wired to `cargo-dist` but the v0.1.0
release has not been tagged yet (planned milestone M1.7). Until then, use
the `--git` form or `cargo install --path crates/agentprof-cli` from a
local checkout (see `## Quick start` below).

---

## Quick start

`agentprof analyze` / `list` / `aggregate` / `watch` all ship today. The
quickest path on a fresh checkout:

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

## CLI Subcommands

- `agentprof analyze` — analyze a single session (`--export md|json|tui|speedscope|html`). See [`docs/architecture.md`](docs/architecture.md) §8.
  - M1.6.4 adds `--export speedscope` (for upload to <https://speedscope.app>) and `--export html` (self-contained static report, no JS).
- `agentprof list` (M1.6.1) — discover recent sessions in a compact 7-column table. `--since 7d --limit 20` defaults keep the command snappy; per-session parse failures degrade gracefully. See [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md) `## agentprof list`.
- `agentprof aggregate` (M1.6.2 + M1.6.3 tui) — cross-session aggregation reports (`--by tool|mcp-server|day|model`, `--export md|json|csv|html|tui`). See [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md) `## agentprof aggregate`.
- `agentprof watch` (M1.6.3) — live-refresh single-session TUI (kernel-event-driven via `notify-debouncer-mini`; default 250 ms debounce). See [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md) `## agentprof watch` and [ADR-0009](docs/internals/adr-0009-watch-runner-and-notify.md).
- `agentprof watch aggregate --by KEY` (M1.6.3) — live-refresh cross-session aggregate TUI; accepts every `aggregate` flag (except `--export` / `--output`, which are rejected because the output is always TUI).

---

## Quick usage

```sh
agentprof analyze    --export tui                         # interactive flamegraph (shipped M1.5)
agentprof analyze    --export speedscope                  # open in speedscope.app (shipped M1.6.4)
agentprof analyze    --export html --output report.html   # self-contained static report (shipped M1.6.4)
agentprof list       --since 7d                           # discover recent sessions (shipped M1.6.1)
agentprof aggregate  --by mcp-server --since 30d          # ROI leaderboard (shipped M1.6.2)
agentprof aggregate  --by tool --export tui               # static cross-session TUI (shipped M1.6.3)
agentprof watch                                           # live single-session TUI (shipped M1.6.3)
agentprof watch aggregate --by tool                       # live cross-session TUI (shipped M1.6.3)

# Global flags (M1.6.4) — work on every subcommand (clap global = true):
agentprof --log-level debug list                          # raise tracing verbosity (default: warn)
agentprof --log-file /tmp/agentprof.log analyze           # send tracing events to a file instead of stderr
```

All commands default to `--agent copilot` (the only adapter shipped in MVP);
`--agent claude|codex` are reserved for Phase 3 post-MVP.

**Global flags (M1.6.4)**: `--log-level <LEVEL>` and `--log-file <PATH>` work
on every subcommand. TUI modes (`analyze --export tui`, `watch`,
`watch aggregate`) auto-redirect tracing to a rolling log under
`$XDG_STATE_HOME/agentprof/agentprof.log` to avoid alt-screen corruption.
See [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md)
`## Tracing & logging` for env vars (`AGENTPROF_LOG_LEVEL` /
`AGENTPROF_LOG_FILE` / `AGENTPROF_LOG_FULL_PATHS`) and
[ADR-0010](docs/internals/adr-0010-tracing-infrastructure.md) for design.

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

## TUI (M1.5 + F1)

`agentprof analyze --export tui` opens an interactive ratatui terminal UI on top of the same `AnalysisReport` the markdown / JSON exporters consume. Four views, all driven from one session:

**FlamegraphView (`1`):** Per-turn horizontal gantt; each row is one turn. Three cell types: `█` colored by [`ToolSource`](crates/agentprof-core/src/model/tool_source.rs) (Builtin / MCP / Skill) = tool execution, `░` = LLM thinking time (in-turn, no tool running), `·` = padding (turn ended; shorter than the longest non-user-blocking turn). Selected turn shows a footer line with its tool call breakdown (e.g. `T3 selected: bash(120ms) read_file(85ms) +2 more · Enter for detail`); press `Enter` to open a full-screen detail view listing every tool call in the turn. Args are populated for Copilot CLI sessions; other adapters show `(not captured)` until they implement `Event::payload_tool_requests`.

```
┌─ Flamegraph (1/3) ───────────────────────────────────────────────┐
│   T1   9.6s  ██████████████░░                                    │
│   T2   4.7s  ██████░░                                            │
│   T3  11.6s  ██████████████████████░ (1 FAIL)                    │
│ ...                                                              │
└──────────────────────────────────────────────────────────────────┘
```

**RoiView (`2`):** Interactive tool rank; press `t`/`c`/`s`/`p` to cycle sort key. User-blocking tools (e.g. `ask_user`) split into a separate sub-table so think time doesn't skew the headline rank.

```
┌─ RoiView (2/3) — Sort: [t]total  c=calls  s=success%  p=p50 ────┐
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

**ModelsView (`4`):** Session-level per-model token rollup (input / output / cache_read / cache_write), sorted by input desc with a bold totals footer row. Populated from `AnalysisReport.model_metrics` (sourced from `session.shutdown.modelMetrics` for Copilot CLI sessions; other adapters opt in by implementing `Event::payload_model_metrics`). Shows a centered "(no model usage data — session has not emitted shutdown event yet)" placeholder when the rollup is unavailable. `Esc` returns to Flamegraph.

Key bindings: `1`/`2`/`3`/`4` switch view, `Tab` cycles, `↑`/`↓` or `k`/`j` selects (viewport follows), `Enter` opens the detail view for the selected turn (`Esc` to return), `G` / `gg` jump to last / first row (vim-style), `t`/`c`/`s`/`p` cycles RoiView sort, `?` opens help, `q` / Ctrl-C quits.

**For deeper per-call timeline analysis** (zoom, search, frame stack, drag-and-drop into a browser), the TUI Flamegraph is intentionally lightweight — use `agentprof analyze --export speedscope <session>` and load the resulting JSON into <https://speedscope.app> for the full interactive profiler. TUI Flamegraph is for "quickly skim which turn is slow + what tool category dominates"; Speedscope is for "drill into per-call traces".

Requires a TTY on stdout; piping yields `OutputError` (exit 3) with a helpful message. See [`crates/agentprof-tui/README.md`](crates/agentprof-tui/README.md) and [ADR-0006](docs/internals/adr-0006-panic-safe-tui.md) for the panic-safe lifecycle.

---

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
