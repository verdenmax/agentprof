# agentprof-cli

> The `agentprof` binary — the only assembly layer in the workspace. Owns the CLI subcommands, config loading, HTML templating, and the `main()` entry point.

## Position in the agentprof architecture

This is the **assembly** crate: it depends on every other workspace crate, but **no lib crate is allowed to depend on it**. See [`docs/architecture.md`](../../docs/architecture.md) §3 (dependency rule) and §8 (CLI protocol).

## M1.4 status

`agentprof analyze` is **shipped** as of M1.4 (commit on `feat/m1.4-cli-and-analyzer`). It covers:

- Subcommand wiring: `--agent` (default `copilot`), `--session` (`latest` / `previous` / `<uuid>` / `<path>`), `--root`, `--export md|json`, `--output <file>`, `--section turn-summary,tool-rank,hook-rank`.
- Renderers: markdown (Session header + Turn Summary + Tool Rank + Hook Rank + Warnings; human-friendly durations) and JSON (`serde_json::to_string_pretty(&AnalysisReport)`, stable integer-ms `Duration`).
- Structured exit codes via `ExitKind` (UserError=1, DataError=2, OutputError=3) carried through `anyhow::Error::msg().context()` + `classify_error` downcast in `main.rs`.
- ADR-0005 D-2 commit-call-turn-divergence fix verified at four independent layers (derive unit / episode snapshot / analyzer snapshot / CLI snapshot + JSON assertion).

All other subcommands (`list` / `aggregate` / `watch` / `ingest-otlp` / `export` / `config`) remain **planned** for M1.5+.

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

## Public interface

This crate produces a binary, not a library. The user-facing protocol is the CLI itself:

```text
agentprof analyze    [--agent copilot] [--session ...] [--root ...]
                     [--export md|json] [--output ...] [--section ...]    # ✓ shipped (M1.4)
agentprof list       [--agent ...] [--since 7d]                            # planned (M1.5+)
agentprof aggregate  [--by tool|mcp-server|day|model] [--since 30d]        # planned (M1.5+)
agentprof watch      [--agent ...]                                         # planned (M1.5+)
agentprof ingest-otlp [--listen 0.0.0.0:4317]   # feature: otlp            # planned (M1.5+)
agentprof export <session> --format ...                                    # planned (M1.5+)
agentprof config     [show | edit | path]                                  # planned (M1.5+)
```

See [`docs/architecture.md`](../../docs/architecture.md) §8 for the canonical specification and exit codes.

## Modules

| Module | Purpose | Status |
|---|---|---|
| `cmd::analyze` | The `analyze` subcommand: session discovery, load+derive+analyze, render dispatch | ✓ shipped (M1.4) |
| `cmd::format::md` | Markdown renderer for `AnalysisReport` | ✓ shipped (M1.4) |
| `cmd::format::json` | JSON renderer for `AnalysisReport` | ✓ shipped (M1.4) |
| `exit` | `ExitKind` enum + `classify_error` downcast | ✓ shipped (M1.4) |
| `cmd::list` / `aggregate` / `watch` / `ingest_otlp` / `export` / `config` | One module per planned subcommand | planned (M1.5+) |
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
- External: `clap`, `anyhow`, `tracing`, `tracing-subscriber`, `serde`, `serde_json`, `chrono`, `directories`, `askama`

## Local commands

```sh
cargo run -p agentprof-cli -- --help
cargo test -p agentprof-cli --all-features
cargo doc  -p agentprof-cli --no-deps --open
```

Integration tests live under `tests/cli.rs` and use `assert_cmd` + `predicates`.

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) — entries prefixed `cli:`.
