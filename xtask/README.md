# xtask

> Build / maintenance / release tasks for agentprof. Follows the [`cargo-xtask`](https://github.com/matklad/cargo-xtask) convention: a normal workspace crate driven via `cargo run -p xtask -- <task>`.

This crate is **not** published to crates.io (`publish = false`).

## Planned tasks

| Task | Purpose |
|---|---|
| `anonymize` | Strip paths / emails / tokens from a real session log to produce a test fixture |
| `dist-check` | Verify release-profile build for all platforms before tagging |
| `release-notes` | Generate `CHANGELOG` excerpt from the most recent `feat:` / `fix:` / `BREAKING:` commits |

## Local commands

```sh
cargo run -p xtask -- --help
```

## Change history

See [`CHANGELOG.md`](../CHANGELOG.md) — entries prefixed `xtask:`.
