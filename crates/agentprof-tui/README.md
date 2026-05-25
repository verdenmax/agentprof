# agentprof-tui

> Ratatui-based terminal views for agentprof: flamegraph, ROI table, aggregate dashboard.

## Position in the agentprof architecture

Depends only on `agentprof-core`. Owns all `ratatui` / `crossterm` use; no other crate touches these. See [`docs/architecture.md`](../../docs/architecture.md) §3 (system layering) and §5 (visualization output forms).

## Public interface

- `app::AppRunner` — drives the event loop and view switching
- `views::flamegraph` — per-session token flamegraph (system / tools_schema / history / ...)
- `views::roi` — Tool ROI matrix
- `views::aggregate` — cross-session aggregates (MCP server waste board, utilization trend)

```rust
// (will become a doctest once Phase 1 lands)
// let runner = agentprof_tui::app::AppRunner::new(&report)?;
// runner.run()?;
```

## Modules (planned)

| Module | Purpose |
|---|---|
| `app` | Event loop, view switching, panic-safe terminal setup |
| `views::flamegraph` | Per-turn stacked token chart |
| `views::roi` | Tool ROI matrix with sorting / filtering |
| `views::aggregate` | Cross-session views |
| `theme` | Color palette and styling primitives |

## Panic safety

**Hard rule** (see [`docs/architecture.md`](../../docs/architecture.md) §16, rule 11): the TUI must not panic. `AppRunner` installs a `std::panic::set_hook` to restore terminal raw mode before re-emitting the panic. All fallible operations in this crate return `Result<_, TuiError>`.

## Dependencies

- Workspace internal: `agentprof-core`
- External: `ratatui`, `crossterm`, `thiserror`, `tracing`

## Local commands

```sh
cargo test -p agentprof-tui
cargo doc  -p agentprof-tui --no-deps --open
```

Snapshot tests use `ratatui::backend::TestBackend` with `insta`; run `cargo insta test` to review.

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) — entries prefixed `tui:`.
