# agentprof-cli

> The `agentprof` binary — the only assembly layer in the workspace. Owns the CLI subcommands, config loading, HTML templating, and the `main()` entry point.

### 配套可视化指南

详细的用户教程 + Wiki 见 [`docs/visual-guide/`](../../docs/visual-guide/README.md)
或 [在线版](https://verdenmax.github.io/agentprof/)。

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
- `--section turn-summary,tool-rank,hook-rank[,mcp-waste]` (md / json / html;
  Session header + Warnings always included. `mcp-waste` is **opt-in only**
  — never included in the default set so the baseline analyze output stays
  byte-identical. When requested, it adds a "MCP Server Waste" section
  (md), a top-level `mcp_waste` field (json), or a dedicated `<section
  id="mcp-waste">` (html) populated via
  `agentprof_core::analyzer::compute_waste`.)

> **M2.5**: `--export md` / `html` / `json` automatically include a
> Cache section (md: `## Cache` 6-row table / html: `<section id="cache">` /
> json: top-level `cache_metrics` field) **only when the session had any
> `cache_read` or `cache_creation` tokens**. Reports without cache
> activity are byte-identical to v0.3.0. See
> [ADR-0023](../../docs/internals/adr-0023-cache-metrics.md) D-2.

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
## Cache                                                        ← M2.5 (only when cache_metrics() is Some; ADR-0023 D-2)
| Metric | Value |
| Creation tokens / Read tokens / Hit% (honest) / Hit% (naive) / Net saved tokens / Gross saved tokens |
## Warnings
Parse-stage warnings: N
- Json (line failed to parse): n
- Io (line read error): n
- OutOfOrder (timestamps non-monotonic): n
Derive-stage warnings: M
- SynthesizedStart / OpenAtEndOfSession / AbortWithoutOpenElement /
  NonMonotonicTimestamp / PayloadNameMissing
```

All other subcommands (`config`) remain **planned** for Phase 2.
The `ingest-otlp` subcommand is **shipped** (M2.2 T8.1 + T8.2; gated on
the `otlp` cargo feature). See [§ `agentprof ingest-otlp`](#agentprof-ingest-otlp-m22) below.

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

# M1.6.6 — token-cost columns on the MCP Server Waste section (heuristic or sidecar-exact)
agentprof analyze --section mcp-waste                                         # heuristic only (default 200 tokens/tool)
agentprof analyze --section mcp-waste --tokens-per-tool 150                   # tune the heuristic
agentprof analyze --section mcp-waste --tool-descriptions ~/.copilot/tools/   # sidecar dir → tiktoken-exact counts where covered
```

The two M1.6.6 flags (`--tokens-per-tool <N>`, default `200`, and
`--tool-descriptions <path>`, file or dir, `~` expanded) are only
consulted when `--section mcp-waste` is rendered; on every other
`--section` they are accepted silently and ignored. Sidecar shape =
file = global `{"tools":[{name,description},…]}` JSON; dir = one
`<server>.json` per server in either `{"tools":[…]}` or bare-array
shape. Per-tool counts default to the heuristic and switch to
`TokenSource::SidecarExact` only when the sidecar covers that tool.

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

**Content:** Header (session ID + agent + model + duration + counts) → SVG flamegraph (responsive, colored by ToolSource) → Turn Summary → Tool Rank → Hook Rank → MCP Waste (opt-in) → Cache (only when `cache_metrics()` is `Some`; M2.5 / ADR-0023 D-2) → Warnings.

**Notes:**
- `--section` filter respected (same as `--export md`).
- `--output` recommended — HTML on terminal is ugly; a warning prints when stdout is used.
- Print-friendly CSS included (`@media print` query).

## `agentprof list`

Discover recent agent sessions in a compact 8-column plain-text table.

```sh
agentprof list                              # default: --since 7d --limit 20 --agent copilot
agentprof list --since 30d --limit 50
agentprof list --since all --root /custom/session-state-dir
agentprof list --since 24h --limit 5
```

**Columns:** `ID` / `Started (UTC)` / `Model` / `Turns` / `Out-tokens` / `Duration` / `Size` / **`Cache%`** (M2.5 — honest hit-rate `read / (read + creation)`; empty cell when the session had no cache activity; see [ADR-0023](../../docs/internals/adr-0023-cache-metrics.md) D-2)

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

> **M2.1.1 update — cache-accelerated**: `aggregate` is now SQLite-cache-
> accelerated when the backing store is populated (via `agentprof db
> ingest --all` or by running `analyze` per session). Cache hit eliminates
> re-parsing the jsonl + re-deriving `Episodes`, which dominate per-session
> cost. Backed by a separate `episodes_json` column (migration 002) and the
> new `SessionDataSource::load_episodes(id)` trait method (3 impls); see
> [ADR-0020](../../docs/internals/adr-0020-aggregate-dualpath.md). Use
> `--no-cache` to fall back to single-path adapter for debugging or in
> environments where the cache isn't trustworthy.

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
| `--export` | `md` | `md` / `json` / `csv` / `html` / `tui` (✅ M1.6.3 activates `tui` — static cross-session aggregate TUI; for live-refresh use `agentprof watch aggregate ...` instead) |
| `--output` | stdout | Write to file instead of stdout (ignored with `--export tui`) |
| `--low-utilization-threshold` | `20.0` | Day bucket warn threshold; rows below are flagged |
| `--tokens-per-tool` | `200` | M1.6.6 — heuristic token cost per MCP tool when no sidecar covers a tool. Only consulted by `--by mcp-server`. |
| `--tool-descriptions` | _(none)_ | M1.6.6 — sidecar path (file or dir) with per-tool descriptions for exact token counts. See `analyze --tool-descriptions` for the on-disk schema. Only consulted by `--by mcp-server`. |

> **M2.5**: `--by model` and `--by day` automatically add 4 cache columns
> (`CacheCr` / `CacheRd` / `Hit%` / `NetSaved`) across md / csv / html.
> `--by tool` and `--by mcp-server` deliberately do **not** — cache tokens
> are prompt-level (accumulated per API call, not per tool invocation),
> so per-tool / per-server attribution is undefined. See
> [ADR-0023 D-3](../../docs/internals/adr-0023-cache-metrics.md).

**Per-key output**:

| `--by` | Columns |
|---|---|
| `tool` | Tool, Source, Calls, Success, Fail, Total, p50, p95, Sessions |
| `mcp-server` | Server, Tools, Calls, Failures, Total, Sessions, **Unused tools**, **Sessions w/0 calls**, **Wasted tokens** (last three M1.6.5/M1.6.6 — populated by `aggregate_waste`-style per-session reduction inside `aggregate_by_mcp_server`; `Wasted tokens` is rendered with a leading `≈` in md/html/tui because the v0.1.x aggregate path always treats cross-session sums as heuristic; CSV header is `wasted_tokens (approx)`) |
| `day` | Date (UTC), Sessions, Wall, Tool time, Out tokens, Utilization% (⚠ on low rows), **CacheCr, CacheRd, Hit%, NetSaved** (last four M2.5 / ADR-0023 D-3, D-5 — cells blank when no cache activity; CSV header keys are `cache_creation` / `cache_read` / `hit_pct_honest` / `saved_net`) |
| `model` | Model, Sessions, Turns, Out tokens, Total wall, **CacheCr, CacheRd, Hit%, NetSaved** (last four M2.5 / ADR-0023 D-3, D-5 — same semantics as `--by day`) |

**Notes**:
- **Sequential parse** of N sessions in the `--since` window (rayon parallelization deferred to a future perf milestone).
- **Fail-soft**: per-session parse failures degrade gracefully — successful rows still rendered; failures summarized to stderr. All-failure case exits `DataError` (2).
- **Empty window**: exits 0 with `no sessions matching --since=...` on stderr.
- **Day bucket UTC**: dates use UTC midnight boundaries (matches event timestamps). Future `--timezone` flag is a non-MVP extension.
- **TUI export**: ✅ shipped (M1.6.3) — `--export tui` opens a static cross-session aggregate TUI (no live refresh; one-shot view). For live-refresh use `agentprof watch aggregate --by ...` instead. Requires stdin + stdout to be TTYs.
- **Percentile recomputation**: aggregate p50/p95 is re-computed from the pooled per-call durations across all sessions (NOT averaged from per-session p50s, which would be statistically wrong).
- **failure_count caveat**: as of M1.6.2 the Copilot adapter doesn't propagate the `success: false` bit, so the Failures column may always be 0. Tracked upstream as a deferred fix.

See [ADR-0008](../../docs/internals/adr-0008-aggregate-report-and-utilization.md) for the data model + utilization metric design.

## `agentprof watch` (M1.6.3)

Live-refresh TUI on top of a file-system watcher (kernel events via
`notify-debouncer-mini`, not polling). Two sub-modes:

### Single-session

```sh
agentprof watch                                   # latest session, 250 ms debounce
agentprof watch --session latest
agentprof watch --session <uuid>
agentprof watch --session ./path/to/events.jsonl
agentprof watch --debounce-ms 500
```

Watches one `events.jsonl` non-recursively. Locks to the initial session
at startup ([ADR-0009 D-5](../../docs/internals/adr-0009-watch-runner-and-notify.md));
newer sessions are NOT auto-followed (`q` + restart to switch). Auto-redraws
within ~`--debounce-ms` of any append (default 250 ms).

### Cross-session aggregate

```sh
agentprof watch aggregate --by tool                         # default --since 30d
agentprof watch aggregate --by mcp-server --since 7d
agentprof watch aggregate --by day --since 30d --low-utilization-threshold 25
agentprof watch --debounce-ms 500 aggregate --by model      # debounce-ms BEFORE `aggregate`
```

Re-aggregates on any change recursively under `--root` (or the adapter
default session-state root). Reuses every flag of `agentprof aggregate`
(`--by` / `--since` / `--limit` / `--low-utilization-threshold`).

> **Note:** `--export` / `--output` are **rejected** when used with
> `watch aggregate` (exits `UserError` = 1). The watch output is always
> the interactive TUI; if you want one-shot export, drop `watch` and use
> `agentprof aggregate --by ... --export md|json|csv|html|tui` instead.
>
> Also: `agentprof`-level options (like `--debounce-ms`) MUST appear
> **before** the `aggregate` subcommand on the command line. clap parses
> them positionally.

### Requirements

- TTY on both stdin and stdout (exits `OutputError` = 3 if not).
- A `notify`-supported platform (Linux / macOS / Windows). No polling
  fallback — if notify init fails, exits `DataError` = 2 with an
  actionable message suggesting `agentprof analyze --export md` for
  headless one-shot output ([ADR-0009 D-15](../../docs/internals/adr-0009-watch-runner-and-notify.md)).

### Behaviour

- Reload failures (e.g. transient parse error during writer mid-flush)
  populate a red footer banner; the watch loop continues
  ([ADR-0009 D-13](../../docs/internals/adr-0009-watch-runner-and-notify.md)).
- Cross-session reload blocks the TUI thread for the reload duration
  (100+ sessions → multi-second pause; future perf milestone).

### Manual smoke

```sh
agentprof watch --session latest
# In another terminal:
echo '{"id":"x","timestamp":"2026-06-01T19:00:00Z","type":"user.message","data":{"turnId":"99","text":"hi"}}' \
    >> ~/.copilot/session-state/<workspace>/<session-uuid>/events.jsonl
# Observe the watch terminal redraw within ~250 ms.
```

See [ADR-0009](../../docs/internals/adr-0009-watch-runner-and-notify.md)
for the full architecture (`WatchRunner` + `WatchData` + `Event::Refresh`
+ watcher-thread-in-cli decision) and `crates/agentprof-tui/README.md`
`## WatchRunner (M1.6.3)` for the runner contract.

## `agentprof mcp-waste` (M1.6.5)

Cross-session report of MCP tools loaded into the context window but
never called. Reads `mcp.json` for the declared toolset and walks
adapter session-state to count actual invocations; tools with zero
calls across the time window are surfaced as waste.

```sh
agentprof mcp-waste                                     # 7d window, md to stdout
agentprof mcp-waste --since 30d --top 50 --export json  # CI-friendly
agentprof mcp-waste --mcp-config ./mcp.json --export html --output waste.html
```

| Flag | Default | Meaning |
|---|---|---|
| `--root` | adapter default | Adapter session-state root override |
| `--since` | `7d` | Time-window filter (`<N>d/h/m/s` or `all`) |
| `--top` | `20` | Cap on the "Always unused" table |
| `--mcp-config` | `~/.copilot/mcp.json` | Override mcp config path (`~/` expanded) |
| `--export` | `md` | `md` / `json` / `html` (**no `tui`** — spec §7.3 / §10; use the `[5] McpWaste` view inside `agentprof analyze --export tui` instead) |
| `--output` | stdout | Output file |
| `--tokens-per-tool` | `200` | M1.6.6 — heuristic token cost per MCP tool when no sidecar covers a tool. Folded into Summary `≈X wasted tokens`, per-tool, and per-server columns. |
| `--tool-descriptions` | _(none)_ | M1.6.6 — sidecar path (file or dir) with per-tool descriptions for exact token counts. Same on-disk schema as `analyze --tool-descriptions`. Loaded once outside the per-session loop. |

Pipeline: `cmd::mcp_waste::run()` → build a
[`SessionDataSource`](../agentprof-core/src/datasource.rs) via
`crate::data_source_factory::build_data_source` (dual-path `adapter +
SQLite` when storage opens cleanly and `--no-cache` is not set;
adapter-only otherwise) → `ds.discover(since)` → per-session
`ds.load_session(id)` (cache hit short-circuits adapter re-parse) →
per-session `agentprof_core::analyzer::compute_waste` (reads
`AnalysisReport.loaded_mcp_tools` directly per M2.1 T5.2.5; no separate
`Episodes`/raw-event pass needed) → cross-session reduce via
`aggregate_waste` → renderer dispatch (`md` / `json` / `html`). Failed
sessions are surfaced as a stderr summary; the command still emits a
report for the successful subset and exits `0`. Accumulated dual-path
divergence warnings are drained to stderr after the loop unless the
global `--quiet` flag is set (M2.1 T5.2.6). The shared
`resolve_mcp_config_path` helper is also consumed by
`analyze --section mcp-waste` so the two surfaces agree on `~/`
expansion and default path.

See spec
[`docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md`](../../docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md)
§7.3 for the full user-facing contract and
[ADR-0015](../../docs/internals/adr-0015-mcp-waste-architecture.md)
for the architecture (data-source provenance, sort order, the
"shipped without `tui` export" decision).

## `db` subcommand family (M2.1 T6)

Lifecycle and inspection commands for the SQLite cache introduced in
M2.1. All six actions honor the global `--storage-path` flag so they
can be pointed at a per-invocation DB file (essential for hermetic
integration tests after the T5.x cache-pollution fix).

```sh
agentprof db init                                     # create + migrate
agentprof db stats [--export table|json]              # default: table
agentprof db ingest --agent copilot --all             # or --since 7d / --session ID
agentprof db prune  --before 30d [--dry-run]          # FK CASCADE deletes children
agentprof db vacuum                                   # reclaim free pages; prints before/after
agentprof db export <SESSION_ID> [--format json|jsonl] [--output PATH]
```

| Action | Key flags | Notes |
|---|---|---|
| `init`   | — | Idempotent; creates parent dirs as needed. |
| `stats`  | `--export {table,json}` | Reads `page_count * page_size` for size; `oldest_started`/`newest_started` rendered as RFC3339 / `-`. |
| `ingest` | `--agent` + one of `--since`/`--all`/`--session` (required group) | Per-session failures logged via `tracing` + counted. Exits `0` on partial success, **`2` (DataError) on full failure** — all discovered sessions failed (M2.1 audit P1-4). Uses [`AdapterDataSource`](../agentprof-adapters/src/datasource.rs) via the new `load_session_by_ref(&AdapterRef)` fast path (M2.1 audit P1-3: O(N) instead of O(N²); reuses the `AdapterRef`s from one up-front `discover`). |
| `prune`  | `--before <DUR>` `--dry-run` | Returns count matched/deleted. Cascades to `tools_loaded` / `turn_buckets` via FK `ON DELETE CASCADE`. |
| `vacuum` | — | Prints `before=N bytes after=M bytes`. In-memory DBs always report `0/0` (SQLite quirk). |
| `export` | `<SESSION_ID>` `--format` `--output` | `json` = single pretty-printed `AnalysisReport`; `jsonl` = one `{"<key>": <value>}` line per top-level report field. Unknown id → exit `1`. |

Integration tests in [`tests/cli_db.rs`](tests/cli_db.rs) cover all
six actions plus prune cascade and ingest arg-group validation.
Every test pins `--storage-path <tempdir>/test.sqlite` per the
post-T5.x hermeticity requirement.

## `agentprof ingest-otlp` (M2.2)

Runs the embedded OTLP receiver — gated on the `otlp` cargo feature
(included in the default `full` feature). Binds gRPC (`127.0.0.1:4317`)
and HTTP/protobuf (`127.0.0.1:4318`) listeners, decodes OTLP envelopes
via the M2.2 T5.2 mapper, groups them by `session.id` through the M2.2
T6.1 `SessionRouter`, and persists each finalized buffer to the same
`SQLite` store used by `analyze` via the M2.2 T7.1
`StorageFlushSink`. On SIGINT / SIGTERM (Unix), drains every open
buffer through the sink before exiting (no partial sessions are
lost).

```sh
# Bind both listeners on loopback, write to the resolved storage path:
agentprof ingest-otlp

# Bind gRPC on a non-default address, disable HTTP, require a bearer:
agentprof ingest-otlp --grpc 0.0.0.0:14317 --no-http --bearer-token s3cret

# Terminate mTLS on both listeners:
agentprof ingest-otlp \
    --tls-cert /etc/agentprof/server.pem \
    --tls-key  /etc/agentprof/server.key \
    --client-ca /etc/agentprof/clients.pem
```

| Flag | Default | Notes |
|---|---|---|
| `--grpc <ADDR>` / `--no-grpc` | `127.0.0.1:4317` | At least one of `--grpc` / `--http` must be enabled (or just don't pass `--no-*`). |
| `--http <ADDR>` / `--no-http` | `127.0.0.1:4318` | HTTP/protobuf transport. |
| `--bearer-token <TOKEN>` | none (auth disabled) | Also reads `AGENTPROF_OTLP_TOKEN` env. |
| `--tls-cert` / `--tls-key` | none (plain TCP) | Both required together (enforced by clap `requires =`). |
| `--client-ca <PATH>` | none | Implies mTLS; requires `--tls-cert`. |
| `--max-session-bytes <N>` | `16 777 216` (16 MiB) | Force-flush threshold per `SessionBuffer`. |
| `--max-session-events <N>` | `100 000` | Force-flush threshold per `SessionBuffer`. |
| `--max-logs-request-bytes <BYTES>` | `8 388 608` (8 MiB) | M2.4 ADR-0022 D-2: max decoded protobuf size on `POST /v1/logs` / `LogsService.Export`. Overflows → gRPC `OutOfRange`/`ResourceExhausted`; HTTP `413`. |
| `--max-metrics-request-bytes <BYTES>` | `2 097 152` (2 MiB) | M2.4: same for Metrics. |
| `--max-traces-request-bytes <BYTES>` | `8 388 608` (8 MiB) | M2.4: same for Traces. |
| `--max-open-sessions <N>` | `1024` | M2.4 ADR-0022 D-3: max distinct sessions; exceed triggers LRU evict with `CloseReason::CapacityEvict`. |
| `--idle-seconds <SECONDS>` | `300` | Idle flush threshold (sweeper interval is 30 s). |
| `--store <PATH>` | resolved via `[storage] path` | Overrides the resolved `SQLite` location. |

### `[otlp]` config-file block (T8.2)

On startup `ingest-otlp` looks for an `[otlp]` block in the agentprof
TOML config (resolved from `$AGENTPROF_CONFIG` if set, else
`~/.config/agentprof/config.toml` via the platform's XDG conventions).
Values present in the block become the defaults for every field, and
CLI flags overlay them per-field. Resolution priority (highest first):

1. **CLI flag** — `--grpc`, `--http`, `--bearer-token`, `--tls-cert`,
   `--tls-key`, `--client-ca`, `--idle-seconds`, plus the explicit
   `--no-grpc` / `--no-http` disable toggles.
2. **`AGENTPROF_OTLP_TOKEN` env var** — `clap` folds it into
   `--bearer-token` automatically, so it shares priority 1.
3. **`[otlp]` config-file block** below.
4. **Built-in defaults** documented above.

A missing file or missing `[otlp]` block is silently treated as "no
overrides"; a malformed file logs a `warn` and falls through to CLI +
defaults so the receiver can still start.

```toml
[otlp]
# Listener addresses ("" disables the listener, omit → loopback defaults).
listen_grpc = "127.0.0.1:4317"
listen_http = "127.0.0.1:4318"

# Shared bearer auth (also overridable via env AGENTPROF_OTLP_TOKEN).
listen_token = "shared-secret"

# Server TLS — both required as a pair when TLS is on.
# tls_cert      = "/etc/agentprof/server.pem"
# tls_key       = "/etc/agentprof/server.key"
# mTLS client CA (implies server TLS above is configured).
# tls_client_ca = "/etc/agentprof/clients-ca.pem"

# Humantime-style durations (Ns / Nm / Nh).
session_idle_timeout = "5m"
shutdown_grace       = "10s"

# M2.4 hardening (v0.3.0, ADR-0022): per-signal request size caps + session cap.
max_logs_request_bytes    = 8388608    # 8 MiB (ADR-0022 D-2)
max_metrics_request_bytes = 2097152    # 2 MiB
max_traces_request_bytes  = 8388608    # 8 MiB
max_open_sessions         = 1024       # ADR-0022 D-3 (LRU evict above this)
```

The `max_session_bytes` / `max_session_events` buffer caps stay
CLI-only for now — they belong to `SessionBufferCaps`, not
`OtlpServerConfig`.

## `agentprof serve` (M2.3 ✅, feature `web`)

Live localhost dashboard backed by the SQLite store. Auto-polls every
5 s by default; toolbar lets the user pause or change interval (1 s /
2 s / 5 s / 10 s / 30 s; choice persists in `localStorage` under key
`agentprof.interval`).

```bash
agentprof serve \
  --storage-path ~/.local/share/agentprof/store.db
```

Then visit `http://127.0.0.1:4329/sessions` (the browser opens
automatically unless `--no-open` is passed).

### Flags

| Flag | Default | Notes |
|---|---|---|
| `--storage-path <PATH>` | (required) | SQLite store path. **File must exist** — exits `ExitKind::UserError` otherwise with a `agentprof db ingest` hint. |
| `--bind <ADDR>` | `127.0.0.1:4329` | Loopback default. Non-loopback bind emits a `tracing::warn!` recommending reverse-proxy auth. |
| `--interval-default <SECS>` | `5` | Browser-side poll interval. Overridable in the toolbar to any value in `1..=60`. |
| `--no-open` | off | Skip the "launch browser on start" behavior (suppresses the `open` crate call). |
| `--quiet` | off | Silence the start-up banner that prints the listening URL. |

### `[serve]` config-file block

```toml
[serve]
bind             = "127.0.0.1:4329"
interval_default = 5
auto_open        = true
```

Loaded from `$AGENTPROF_CONFIG` or
`$XDG_CONFIG_HOME/agentprof/config.toml`. **Precedence:** CLI flag > env >
file > built-in defaults — same semantics as the `[otlp]` block (M2.2
T8.2).

### Views

| Route | Notes |
|---|---|
| `GET /sessions` | Recent sessions list (30 d window, capped 200 rows). |
| `GET /session/:id` | Per-session detail with embedded analytical report (flame graph + turn / tool / hook tables + cache section). Renders `format::html::render_body_only` inside the dashboard chrome. |
| `GET /aggregate?by=model\|tool\|day&since=7d` | Cross-session aggregate. `--by=mcp-server` returns **HTTP 400** — use `/mcp-waste` instead (see [ADR-0024](../../docs/internals/adr-0024-web-dashboard-architecture.md) Consequences). |
| `GET /mcp-waste?since=7d` | MCP server waste list (**heuristic-only** — no sidecar). Banner directs users to `agentprof mcp-waste --tool-descriptions ...` for accurate counts. |
| `GET /mcp-waste/:server?since=7d` | Per-server tool usage detail. |
| `GET /healthz` | Liveness probe — `200 OK` with body `healthy`. |
| `GET /static/:name` | Bundled CSS / JS / favicon (compile-time `include_str!` / `include_bytes!`). |

Each dashboard view also exposes a `/api/<path>.html` chunk endpoint
(e.g. `/api/sessions.html`, `/api/session/:id.html`,
`/api/aggregate.html`, `/api/mcp-waste.html`,
`/api/mcp-waste/:server`) — the JS poller fetches the chunk and replaces
`document.getElementById('main').innerHTML`. No JSON parsing client-side.

### Security note

The dashboard ships with **no authentication** in v0.3.3 (ADR-0024 D-6).
Default loopback bind mitigates the obvious risk; if you bind to a
non-loopback address put it behind a reverse proxy (nginx / caddy /
Cloudflare Tunnel / etc.) — the startup `tracing::warn!` will remind you.

### Where to read more

- Architecture rationale + 7 decisions + 4 implementation notes:
  [ADR-0024](../../docs/internals/adr-0024-web-dashboard-architecture.md)
- Cross-crate feature index:
  [`docs/features/web-dashboard.md`](../../docs/features/web-dashboard.md)
- L1 CLI protocol entry: [`docs/architecture.md`](../../docs/architecture.md) §8

## Public interface

This crate produces a binary, not a library. The user-facing protocol is the CLI itself:

```text
agentprof analyze    [--agent copilot] [--session ...] [--root ...]
                     [--export md|json|tui|speedscope|html] [--output ...] [--section ...]
                     [--tokens-per-tool 200] [--tool-descriptions ...]      # ✓ shipped (M1.4 + M1.5 tui + M1.6.4 speedscope|html + M1.6.6 tokens)
agentprof list       [--agent copilot] [--root ...]
                     [--since <N>d|h|m|s|all] [--limit N]                 # ✓ shipped (M1.6.1)
agentprof aggregate  [--agent copilot] [--root ...] [--by tool|mcp-server|day|model]
                     [--since <N>d|h|m|s|all] [--limit N]
                     [--export md|json|csv|html|tui] [--output ...]
                     [--low-utilization-threshold 20]
                     [--tokens-per-tool 200] [--tool-descriptions ...]      # ✓ shipped (M1.6.2 + M1.6.3 tui + M1.6.6 tokens on --by mcp-server)
agentprof watch      [--agent copilot] [--session ...] [--root ...] [--debounce-ms 250]
                     [aggregate --by ... [...all aggregate flags]]          # ✓ shipped (M1.6.3)
agentprof mcp-waste  [--root ...] [--since 7d] [--top 20] [--mcp-config ...]
                     [--tokens-per-tool 200] [--tool-descriptions ...]
                     [--export md|json|html] [--output ...]                 # ✓ shipped (M1.6.5 + M1.6.6 tokens)
agentprof ingest-otlp [--grpc 127.0.0.1:4317] [--http 127.0.0.1:4318]
                      [--bearer-token TOKEN] [--tls-cert ...] [--tls-key ...]
                      [--client-ca ...] [--max-session-bytes N]
                      [--max-session-events N] [--idle-seconds N]
                      [--max-logs-request-bytes N] [--max-metrics-request-bytes N]
                      [--max-traces-request-bytes N] [--max-open-sessions N]
                      [--store PATH]                                          # ✓ shipped (M2.2; M2.4 hardening; feature: otlp)
agentprof serve       [--storage-path PATH] [--bind 127.0.0.1:4329]
                      [--interval-default 5] [--no-open] [--quiet]            # ✓ shipped (M2.3; feature: web)
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
| `cmd::list` | The `list` subcommand: cheap session discovery + 8-column table (incl. `Cache%`, M2.5). Since M2.1 T5.2 routes through `build_data_source(...)` for dual-path provenance and drains `DualPathWarning`s to stderr (suppressed by `--quiet`). | ✓ shipped (M1.6.1, dual-path wired M2.1 T5.2) |
| `cmd::aggregate` | The `aggregate` subcommand: cross-session group-by (4 keys × 4 export formats + `tui` since M1.6.3); exposes `pub fn compute_aggregate(&CopilotAdapter, &AggregateCmd) -> Result<(AnyAggregateReport, usize)>` so both `--export tui` and `watch aggregate` reload can share the load + compute pipeline (the second tuple element = total refs scanned, used by the empty-window warning). | ✓ shipped (M1.6.2 + M1.6.3 tui) |
| `cmd::format::aggregate_md` / `aggregate_csv` / `aggregate_html` | Per-format renderers for `AnyAggregateReport` | ✓ shipped (M1.6.2) |
| `cmd::watch` | The `watch` subcommand: single-session + `watch aggregate` cross-session live-refresh TUI. Owns the `notify-debouncer-mini` thread and drives `agentprof_tui::watch::WatchRunner` via an mpsc channel + reload closure. | ✓ shipped (M1.6.3) |
| `cmd::mcp_waste` | The `mcp-waste` subcommand: cross-session report of MCP tools loaded but never called. Per-session `compute_waste` + cross-session `aggregate_waste` + md/json/html renderers. Also owns the shared `resolve_mcp_config_path` helper consumed by `analyze --section mcp-waste`. | ✓ shipped (M1.6.5) |
| `exit` | `ExitKind` enum + `classify_error` downcast | ✓ shipped (M1.4) |
| `data_source` | `DualPathDataSource` composer — fans out `SessionDataSource` calls to an adapter + optional `SQLite` store, merges by session id (adapter wins), and records divergence warnings. (An async re-upsert callback was prototyped in M2.1 T4.2 and removed by the M2.1 audit followup — see `data_source.rs` module docs; proper async refresh is deferred to M2.1.1.) | ✓ shipped (M2.1 T4.2; re-upsert callback removed in audit followup) |
| `data_source_factory` | `build_data_source(agent, root, &StorageConfig, no_cache) -> anyhow::Result<(Box<dyn SessionDataSource>, WarningsHandle)>` — single composition seam used by every subcommand. Returns a `DualPathDataSource` when storage is reachable, falls back to a bare `AdapterDataSource` when `--no-cache` is set **or** storage open fails (`tracing::warn!` + graceful degradation, never a hard error). The second tuple element is an `Arc<Mutex<Vec<DualPathWarning>>>` shared with the inner dual-path source (empty for adapter-only returns) — callers drain it and emit one stderr line per warning unless `--quiet`. | ✓ shipped (M2.1 T5.1, warnings handle M2.1 T5.2) |
| `cmd::ingest_otlp` | The `ingest-otlp` subcommand: reads the `[otlp]` block from `~/.config/agentprof/config.toml` (or `$AGENTPROF_CONFIG`), folds CLI flags + env on top to build an `OtlpServerConfig` (priority: CLI > env > file > defaults), opens the SQLite store, wires `StorageFlushSink → SessionRouter → IngestPipeline → serve_{grpc,http}` + `spawn_idle_sweeper`, and drains gracefully on SIGINT / SIGTERM. Feature-gated under `otlp`; dispatcher in `main.rs` spins a multi-thread tokio runtime via `block_on` so the rest of the CLI stays sync. A hidden `--sweeper-interval-seconds` flag (test-only, omitted from `--help`) overrides the 30 s sweeper tick so end-to-end tests in `tests/cli_ingest_otlp_e2e.rs` can fire idle flushes within a few seconds. | ✓ shipped (M2.2 T8.1 + T8.2; e2e tests M2.2 T9.1) |
| `cmd::serve` (feature `web`) | The `serve` subcommand: reads the `[serve]` block (`bind` / `interval_default` / `auto_open`), folds CLI flags on top, opens the SQLite store, builds the axum `Router` (12 routes: 5 dashboard pages × 2 chunk endpoints + `/healthz` + `/static/:name` + `/` → `/sessions` redirect), and drives `axum::serve(...).with_graceful_shutdown(...)` until SIGINT / SIGTERM. Submodules: `mod` (clap + entry + signal), `state` (`Arc<Mutex<Db>>` + `interval_default`), `router`, `handlers` (5 views × 2 routes), `static_assets` (`include_str!` / `include_bytes!` bundling). Templates under `templates/dashboard/**`. Adds `format::html::render_body_only` + `format::aggregate_html::render_body_only` for embedding the existing static HTML report markup inside the dashboard chrome, plus `cmd::aggregate::compute_aggregate_from_store` and `cmd::mcp_waste::compute_aggregate_waste_from_store` (store-mode parallels to the existing adapter-mode `compute_*` fns). See [ADR-0024](../../docs/internals/adr-0024-web-dashboard-architecture.md). | ✓ shipped (M2.3; e2e tests `tests/cli_serve_e2e.rs`) |
| `cmd::config` | One module per planned subcommand | planned (Phase 2) |
| `config` | TOML loader / writer for `~/.config/agentprof/config.toml` | planned (M1.5+) |
| `report_html` | `askama` templates for the HTML report | planned (M1.5+) |
| `main` | clap dispatch + tracing init + panic hook | ✓ shipped (M1.4) |

## Features

| Feature | Default | Effect |
|---|---|---|
| `full` | on | Enables `anthropic-api`, `otlp`, and `web`. |
| `anthropic-api` | via `full` | Forwards to `agentprof-core/anthropic-api`. |
| `otlp` | via `full` | Forwards to `agentprof-storage/otlp`; required for the `ingest-otlp` subcommand. |
| `web` | via `full` | M2.3 — compiles `cmd::serve` + dashboard templates + bundled CSS/JS. Pulls in `axum`, `tower`, `tower-http`, `tokio`, `open` (axum / tower / tokio shared with `otlp`). Required for the `serve` subcommand. See [ADR-0024](../../docs/internals/adr-0024-web-dashboard-architecture.md). |

To build a minimal binary: `cargo build -p agentprof-cli --no-default-features`.

## Dependencies

- Workspace internal: every other `agentprof-*` crate
- External: `clap`, `anyhow`, `tracing`, `tracing-subscriber`, `tracing-appender 0.2` (added in M1.6.4 for the TUI auto-redirect's rolling-file writer; see "Tracing & logging" below), `serde`, `serde_json`, `chrono`, `directories`, `askama 0.16` (re-activated in M1.6.4 for the HTML report template; md renderer remains hand-rolled string-building), `csv` (added in M1.6.2 for `aggregate --export csv`), `notify-debouncer-mini 0.4` (added in M1.6.3 for the `watch` file watcher — pulls `notify` v6.1.1 transitively, see [ADR-0009 D-4](../../docs/internals/adr-0009-watch-runner-and-notify.md))

## Local commands

```sh
cargo run -p agentprof-cli -- --help
cargo test -p agentprof-cli --all-features
cargo doc  -p agentprof-cli --no-deps --open
```

Integration tests live under `tests/cli.rs` and use `assert_cmd` + `predicates`.

## Tracing & logging (M1.6.4)

`agentprof` uses `tracing` 0.1 as its **single canonical** diagnostic /
warning / debug output channel (no more `eprintln!`). All configuration is
resolved by `agentprof_cli::observability::LogConfig::resolve_from_env_and_flags`
and applied by `init_tracing`, which installs a `tracing_subscriber::fmt`
layer behind a `reload::Layer` so the writer can be swapped at runtime
(used by the TUI auto-redirect path).

### Global CLI flags

```
--log-level <LEVEL>    Tracing level filter (trace|debug|info|warn|error)
                       or full env-filter syntax (e.g. "warn,agentprof_core=debug").
                       Default: env AGENTPROF_LOG_LEVEL / AGENTPROF_LOG, then "warn".
--log-file <PATH>      Trace events to file. "-" forces stderr (overrides TUI auto-redirect).
                       Default: non-TUI = stderr; TUI = $XDG_STATE_HOME/agentprof/agentprof.log.
--no-cache             (v0.2.0 / M2.1 T4.3) Skip all storage I/O — degrades the dual-path
                       data source to a single-path adapter view. Useful for one-shot
                       inspection without touching the SQLite cache.
--storage-path <PATH>  (v0.2.0 / M2.1 T4.3) Override the resolved storage DB path.
                       Beats both the `[storage]` config-file value and the XDG default.
--quiet                (v0.2.0 / M2.1 T4.3) Suppress per-session "adapter vs storage"
                       divergence warning lines on stderr. Structured `tracing` events
                       are unaffected.
```

Both flags are clap `global = true` — they work on every subcommand
(`analyze`, `list`, `aggregate`, `watch`, `watch aggregate`).

### v0.2.0 storage config (M2.1 T4.3)

The CLI now parses an optional `[storage]` section in the agentprof TOML
config (resolution path lands in a follow-up task). Schema lives in
`agentprof_storage::config::PartialStorageConfig`; the CLI merges it via
[`agentprof_cli::config::resolve_storage_config`] which also honours the
`--storage-path` override above (flag wins over config-file value, per
`docs/architecture.md` §10).

```toml
[storage]
mode            = "cache"            # or "store"
path            = "/custom/db.sqlite"  # omit to use XDG default
auto_prune_days = 30                 # 0 disables auto-pruning
```

### Write-through & long-lived storage handles (M2.1 T5.3)

- `analyze` runs the in-memory pipeline first, then write-through-caches
  the resulting `AnalysisReport` into the SQLite store via
  `agentprof_storage::upsert::upsert_report`. The write is a pure side
  effect: failures are logged at `tracing::warn` and **never** alter the
  command's exit status or stdout. Suppress with the global `--no-cache`.
- `watch` (single-session) opens **one** `agentprof_storage::Db` handle
  at session start and holds it for the watch lifetime (spec §10.2 —
  long-lived conn, never re-opened per refresh). The initial report is
  flushed once on entry; per spec §8 there is **no** automatic write per
  refresh tick to avoid high-freq disk churn. `watch aggregate` ignores
  these flags (no per-session report to persist).

### Env vars

| Var | Effect |
|---|---|
| `AGENTPROF_LOG` | Backwards-compatible level filter (same syntax as `--log-level`). |
| `AGENTPROF_LOG_LEVEL` | Alias for `--log-level`; flag wins. |
| `AGENTPROF_LOG_FILE` | Alias for `--log-file`; flag wins. |
| `AGENTPROF_LOG_FULL_PATHS` | If `1`, emit raw session paths instead of `hash_path` short-hashes. **System-wide**: `hash_path` itself reads the env var on every call, so the opt-out applies at all 4 span layers (cli `cmd.*`, adapters `adapter.*`, core `analyzer.*` / `aggregator.*`). |

### TUI auto-redirect

TUI mode (`analyze --export tui`, `watch`, `watch aggregate`) auto-switches
the tracing writer to a rolling daily log file under
`$XDG_STATE_HOME/agentprof/agentprof.log` (via `tracing-appender`'s
non-blocking rolling appender). On clean exit the path is printed to
stdout. This prevents the alt-screen corruption that motivated the M1.6.3
`tracing::warn!` → `debug!` workaround in `watch.rs`.

Pass `--log-file -` (or `AGENTPROF_LOG_FILE=-`) to force stderr even in
TUI mode (you own the alt-screen pollution risk).

### Soft-fall policy

Any tracing init failure (file permission denied, XDG path not writable,
env-filter syntax error, etc.) **soft-falls** to the default stderr
writer — tracing **never** blocks CLI startup
([ADR-0010 D-13](../../docs/internals/adr-0010-tracing-infrastructure.md)).

### Span topology (4 layers, 13 spans)

| Layer | Span | Emitted at |
|---|---|---|
| 1 (cli) | `cmd.{analyze, list, aggregate, watch}` (`info_span!`) | `agentprof-cli::cmd::*::run` |
| 2 (adapters) | `adapter.{discover, parse, load_meta}` (`debug_span!`) | `agentprof-adapters` |
| 3 (core) | `analyzer.{derive_episodes, analyze}`, `aggregator.group_by{tool,mcp,day,model}` (`debug_span!`) | `agentprof-core` |
| 4 (events) | `tracing::{trace, debug, info, warn, error}!` | anywhere (replaces every `eprintln!`) |

Full design + decision log: [ADR-0010](../../docs/internals/adr-0010-tracing-infrastructure.md)
+ [spec](../../docs/superpowers/specs/2026-06-02-tracing-design.md).

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) — entries prefixed `cli:`.
