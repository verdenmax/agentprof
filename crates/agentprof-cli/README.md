# agentprof-cli

> The `agentprof` binary — the only assembly layer in the workspace. Owns the CLI subcommands, config loading, HTML templating, and the `main()` entry point.

## Position in the agentprof architecture

This is the **assembly** crate: it depends on every other workspace crate, but **no lib crate is allowed to depend on it**. See [`docs/architecture.md`](../../docs/architecture.md) §3 (dependency rule) and §8 (CLI protocol).

## Status

`agentprof analyze` is **shipped** (M1.4 + 4 follow-up iterations merged on top). For a per-merge changelog of those followups, see [`CHANGELOG.md`](../../CHANGELOG.md) `[Unreleased]` section. The L2 doc below describes only the **current crate surface**, not the merge history.

Subcommand wiring (current):

- `--agent` (default `copilot`; `claude` / `codex` reserved with friendly errors)
- `--session` (`latest` / `previous` / `<uuid>` / `<path>`)
- `--root <DIR>` (override default `~/.copilot/session-state/`)
- `--export md|json|tui|speedscope|html` (default `md`)
- `--output <FILE>` (default stdout)
- `--section turn-summary,tool-rank,hook-rank` (md only; Session header + Warnings always included)

Markdown structure (after all M1.4 iterations):

```
# agentprof analyze — <session-id>
## Session
- Agent / Started / CWD / Branch / Live / Turns / Tools tracked / Hooks tracked
- Derive warnings: N
- Parse warnings: N          ← post-output-audit
## Turn Summary
| # | Turn ID | Status | Duration | Model | Mode | Tools | Hooks | Skills | Out-Tokens |
## Tool Rank (by total duration)
| Tool | Source | Calls | OK | Fail | Orphan | User-req | Total | p50 | p95 | Max |
## User-blocking tools (wall-clock includes user think time)   ← post-output-audit
| Tool | Source | Calls | OK | Fail | Orphan | User-req | Total | p50 | p95 | Max |
| ask_user | ... |
## Hook Rank (by total duration)
| Hook | Calls | OK | Fail | Synth | Total | p50 | p95 |
## Warnings
Parse-stage warnings: N
- Json (line failed to parse): n
- Io (line read error): n
- OutOfOrder (timestamps non-monotonic): n
Derive-stage warnings: M
- SynthesizedStart / OpenAtEndOfSession / AbortWithoutOpenElement /
  NonMonotonicTimestamp / PayloadNameMissing
```

All other subcommands (`watch` / `ingest-otlp` / `export` / `config`) remain **planned** for M1.6.3+.

## Quick start

```sh
# Build from source
cargo install --path crates/agentprof-cli

# Analyze your most recent Copilot CLI session, markdown to stdout
agentprof analyze

# Specific session by absolute path
agentprof analyze --session ~/.copilot/session-state/<uuid>

# Specific session by UUID (auto-discovered under the adapter root)
agentprof analyze --session 01234567-89ab-cdef-0123-456789abcdef

# Write JSON to a file
agentprof analyze --export json --output report.json

# Only Turn Summary + Tool Rank (skip Hook Rank); md export only
agentprof analyze --section turn-summary,tool-rank
```

Set `AGENTPROF_LOG=debug` to enable `tracing` output on stderr.

### `agentprof analyze --export speedscope`

Emit a [Speedscope evented JSON profile](https://github.com/jlfwong/speedscope/blob/main/file-format.md) suitable for upload to <https://speedscope.app>.

```sh
agentprof analyze --export speedscope > session.speedscope.json
agentprof analyze --export speedscope --output session.speedscope.json
```

**Frame naming** (per [ADR-0007](../../docs/internals/adr-0007-speedscope-export.md)):

| Source | Frame name |
|---|---|
| Builtin | `<tool>` |
| MCP | `mcp:<server>::<leaf>` |
| Hook | `hook:<name>` |
| Skill (invocation) | `skill:<skill>` |
| Tool whose ToolSource is Skill | `skill:<skill>:<leaf>` |
| Synthetic | `session`, `turn-<N>`, `turn-<N> (open)`, `turn-orphan` |

**Notes:**
- `--section` is ignored (speedscope is a single surface; a warning is printed).
- Span overlap within a turn is auto-adjusted (1 ms gap) with an `ExportWarning` on stderr.
- Timestamp anchor is the session's first event (`at = 0`), so output is reproducible.

### `agentprof analyze --export html`

Emit a self-contained static HTML report (no JS, no external assets) with embedded SVG flamegraph and full tables.

```sh
agentprof analyze --export html --output report.html
agentprof analyze --export html > report.html              # warns; prefer --output
agentprof analyze --export html --output report.html --section turn-summary,tool-rank
```

**Content:** Header (session ID + agent + model + duration + counts) → SVG flamegraph (responsive, colored by ToolSource) → Turn Summary → Tool Rank → Hook Rank → Warnings.

**Notes:**
- `--section` filter respected (same as `--export md`).
- `--output` recommended — HTML on terminal is ugly; a warning prints when stdout is used.
- Print-friendly CSS included (`@media print` query).

## `agentprof list`

Discover recent agent sessions in a compact 7-column plain-text table.

```sh
agentprof list                              # default: --since 7d --limit 20 --agent copilot
agentprof list --since 30d --limit 50
agentprof list --since all --root /custom/session-state-dir
agentprof list --since 24h --limit 5
```

**Columns:** `ID` / `Started (UTC)` / `Model` / `Turns` / `Out-tokens` / `Duration` / `Size`

**Flags:**

| Flag | Default | Description |
|---|---|---|
| `--agent` | `copilot` | Agent whose sessions to list (M1.6.1 supports `copilot` only) |
| `--root` | adapter default | Override default session-state root |
| `--since` | `7d` | Filter by mtime; accepts `<N>d/h/m/s` or `all` |
| `--limit` | `20` | Max sessions shown; `0` = unlimited |

**Error handling:** per-session parse failures degrade gracefully — successful rows still printed; failures summarized to stderr at end. All-failure case exits `DataError` (2).

## `agentprof aggregate` (M1.6.2)

Cross-session aggregation reports across the four canonical keys.

```sh
agentprof aggregate --by tool --since 30d                          # md table to stdout
agentprof aggregate --by mcp-server --since 7d --export csv
agentprof aggregate --by day --since 30d --low-utilization-threshold 25
agentprof aggregate --by model --since 90d --export html --output models.html
agentprof aggregate --by tool --since all --export json | jq '.data.buckets | length'
```

**Flags:**

| Flag | Default | Description |
|---|---|---|
| `--agent` | `copilot` | Agent whose sessions to aggregate (M1.6.2 supports `copilot` only) |
| `--root` | adapter default | Override default session-state root |
| `--by` | (required) | Group-by key: `tool`, `mcp-server`, `day`, or `model` |
| `--since` | `30d` | Filter by mtime; `<N>d/h/m/s` or `all` (renders as "all") |
| `--limit` | `0` | Max bucket rows; `0` = unlimited |
| `--export` | `md` | `md` / `json` / `csv` / `html`. TUI deferred to M1.6.3 |
| `--output` | stdout | Write to file instead of stdout |
| `--low-utilization-threshold` | `20.0` | Day bucket warn threshold; rows below are flagged |

**Per-key output**:

| `--by` | Columns |
|---|---|
| `tool` | Tool, Source, Calls, Success, Fail, Total, p50, p95, Sessions |
| `mcp-server` | Server, Tools, Calls, Failures, Total, Sessions |
| `day` | Date (UTC), Sessions, Wall, Tool time, Out tokens, Utilization% (⚠ on low rows) |
| `model` | Model, Sessions, Turns, Out tokens, Total wall |

**Notes**:
- **Sequential parse** of N sessions in the `--since` window (rayon parallelization deferred to a future perf milestone).
- **Fail-soft**: per-session parse failures degrade gracefully — successful rows still rendered; failures summarized to stderr. All-failure case exits `DataError` (2).
- **Empty window**: exits 0 with `no sessions matching --since=...` on stderr.
- **Day bucket UTC**: dates use UTC midnight boundaries (matches event timestamps). Future `--timezone` flag is a non-MVP extension.
- **TUI export**: deferred to M1.6.3 (combined with `watch` mode); `--export tui` is not a valid value today.
- **Percentile recomputation**: aggregate p50/p95 is re-computed from the pooled per-call durations across all sessions (NOT averaged from per-session p50s, which would be statistically wrong).
- **failure_count caveat**: as of M1.6.2 the Copilot adapter doesn't propagate the `success: false` bit, so the Failures column may always be 0. Tracked upstream as a deferred fix.

See [ADR-0008](../../docs/internals/adr-0008-aggregate-report-and-utilization.md) for the data model + utilization metric design.

## Public interface

This crate produces a binary, not a library. The user-facing protocol is the CLI itself:

```text
agentprof analyze    [--agent copilot] [--session ...] [--root ...]
                     [--export md|json|tui|speedscope|html] [--output ...] [--section ...]    # ✓ shipped (M1.4 + M1.5 tui + M1.6.4 speedscope|html)
agentprof list       [--agent copilot] [--root ...]
                     [--since <N>d|h|m|s|all] [--limit N]                 # ✓ shipped (M1.6.1)
agentprof aggregate  [--agent copilot] [--root ...] [--by tool|mcp-server|day|model]
                     [--since <N>d|h|m|s|all] [--limit N]
                     [--export md|json|csv|html] [--output ...]
                     [--low-utilization-threshold 20]                       # ✓ shipped (M1.6.2)
agentprof watch      [--agent ...]                                         # planned (M1.6.3)
agentprof ingest-otlp [--listen 0.0.0.0:4317]   # feature: otlp            # planned (Phase 2)
agentprof config     [show | edit | path]                                  # planned (Phase 2)
```

See [`docs/architecture.md`](../../docs/architecture.md) §8 for the canonical specification and exit codes.

## Modules

| Module | Purpose | Status |
|---|---|---|
| `cmd::analyze` | The `analyze` subcommand: session discovery, load+derive+analyze, render dispatch | ✓ shipped (M1.4) |
| `cmd::format::md` | Markdown renderer for `AnalysisReport` | ✓ shipped (M1.4) |
| `cmd::format::json` | JSON renderer for `AnalysisReport` | ✓ shipped (M1.4) |
| `cmd::format::speedscope` | Speedscope evented JSON exporter (thin wrapper over `agentprof_core::export::speedscope`) | ✓ shipped (M1.6.4) |
| `cmd::format::html` | Self-contained static HTML report (askama 0.16 template + embedded SVG flamegraph) | ✓ shipped (M1.6.4) |
| `cmd::list` | The `list` subcommand: cheap session discovery + 7-column table | ✓ shipped (M1.6.1) |
| `cmd::aggregate` | The `aggregate` subcommand: cross-session group-by (4 keys × 4 export formats) | ✓ shipped (M1.6.2) |
| `cmd::format::aggregate_md` / `aggregate_csv` / `aggregate_html` | Per-format renderers for `AnyAggregateReport` | ✓ shipped (M1.6.2) |
| `exit` | `ExitKind` enum + `classify_error` downcast | ✓ shipped (M1.4) |
| `cmd::{watch, ingest_otlp, export, config}` | One module per planned subcommand | planned (M1.6.3+) |
| `config` | TOML loader / writer for `~/.config/agentprof/config.toml` | planned (M1.5+) |
| `report_html` | `askama` templates for the HTML report | planned (M1.5+) |
| `main` | clap dispatch + tracing init + panic hook | ✓ shipped (M1.4) |

## Features

| Feature | Default | Effect |
|---|---|---|
| `full` | on | Enables both `anthropic-api` and `otlp`. |
| `anthropic-api` | via `full` | Forwards to `agentprof-core/anthropic-api`. |
| `otlp` | via `full` | Forwards to `agentprof-storage/otlp`; required for the `ingest-otlp` subcommand. |

To build a minimal binary: `cargo build -p agentprof-cli --no-default-features`.

## Dependencies

- Workspace internal: every other `agentprof-*` crate
- External: `clap`, `anyhow`, `tracing`, `tracing-subscriber`, `serde`, `serde_json`, `chrono`, `directories`, `askama 0.16` (re-activated in M1.6.4 for the HTML report template; md renderer remains hand-rolled string-building), `csv` (added in M1.6.2 for `aggregate --export csv`)

## Local commands

```sh
cargo run -p agentprof-cli -- --help
cargo test -p agentprof-cli --all-features
cargo doc  -p agentprof-cli --no-deps --open
```

Integration tests live under `tests/cli.rs` and use `assert_cmd` + `predicates`.

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) — entries prefixed `cli:`.
