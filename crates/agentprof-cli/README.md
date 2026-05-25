# agentprof-cli

> The `agentprof` binary — the only assembly layer in the workspace. Owns the CLI subcommands, config loading, HTML templating, and the `main()` entry point.

## Position in the agentprof architecture

This is the **assembly** crate: it depends on every other workspace crate, but **no lib crate is allowed to depend on it**. See [`docs/architecture.md`](../../docs/architecture.md) §3 (dependency rule) and §8 (CLI protocol).

## Public interface

This crate produces a binary, not a library. The user-facing protocol is the CLI itself:

```text
agentprof analyze    [--agent ...] [--session ...] [--export ...]
agentprof list       [--agent ...] [--since 7d]
agentprof aggregate  [--by tool|mcp-server|day|model] [--since 30d]
agentprof watch      [--agent ...]
agentprof ingest-otlp [--listen 0.0.0.0:4317]   # feature: otlp
agentprof export <session> --format ...
agentprof config     [show | edit | path]
```

See [`docs/architecture.md`](../../docs/architecture.md) §8 for the canonical specification and exit codes.

## Modules (planned)

| Module | Purpose |
|---|---|
| `cmd::analyze` / `list` / `aggregate` / `watch` / `ingest_otlp` / `export` / `config` | One module per subcommand |
| `config` | TOML loader / writer for `~/.config/agentprof/config.toml` |
| `report_html` | `askama` templates for the HTML report |
| `main` | clap dispatch + tracing init + panic hook |

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
