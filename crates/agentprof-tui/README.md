# agentprof-tui

> Ratatui-based terminal views for agentprof: per-turn flamegraph, interactive tool ROI rank, single-session aggregate.

**Status:** M1.5 ✅ shipped (2026-05-30). See [`docs/superpowers/specs/2026-05-30-m1.5-tui-design.md`](../../docs/superpowers/specs/2026-05-30-m1.5-tui-design.md) for the design spec and [`docs/internals/adr-0006-panic-safe-tui.md`](../../docs/internals/adr-0006-panic-safe-tui.md) for the panic-safety contract.

## Position in the agentprof architecture

Depends only on `agentprof-core`. Owns all `ratatui` / `crossterm` use; no other lib crate touches these.
See [`docs/architecture.md`](../../docs/architecture.md) §3 (system layering) and §4 (crate table).

## Public interface

| Item | Purpose |
|---|---|
| `AppRunner::new(&AnalysisReport, &Episodes)` | Construct a TUI session bound to a parsed report + per-call timing |
| `AppRunner::run(&mut Terminal<B>) -> Result<(), TuiError>` | Event loop; returns on `q` / Ctrl-C |
| `app::terminal::install_panic_hook` | Idempotent (`Once`); MUST be called before `enter()` |
| `app::terminal::enter() -> Result<TuiTerminal>` | Refuses non-tty with `TuiError::NotATerminal` |
| `app::terminal::leave(&mut TuiTerminal)` | Best-effort restore; idempotent |
| `views::View` | `Flamegraph` / `Roi` / `Aggregate` |

```rust
// CLI usage shape (see crates/agentprof-cli/src/cmd/analyze.rs::run_tui):
// agentprof_tui::app::terminal::install_panic_hook();
// let mut term = agentprof_tui::app::terminal::enter()?;
// let res = agentprof_tui::AppRunner::new(&report, &episodes).run(&mut term);
// let _ = agentprof_tui::app::terminal::leave(&mut term);
// res?
```

## Modules

| Module | Purpose |
|---|---|
| `app::terminal` | `install_panic_hook` + `enter` + `leave` (idempotent) |
| `app::event` | `Event` enum + crossterm event mapper |
| `app::state` | `AppState` + `dispatch` (pure-logic state machine) |
| `app` (root) | `AppRunner` (wires state + views + event loop) |
| `views::flamegraph` | Per-turn horizontal gantt + `segment_layout` |
| `views::roi` | Interactive tool rank with sort cycling + `recent_calls` |
| `views::aggregate` | By-Mode + By-Hook tables (single session) + `group_by_mode` |
| `views::format` | Shared display helpers (`human_short`) |
| `theme` | `ToolSource → Color` + status modifiers |
| `error` | `TuiError` (`#[non_exhaustive]` thiserror) |

## Key bindings

| Key | Action |
|---|---|
| `q` / Ctrl-C | Quit (clean leave + exit 0) |
| `1` / `2` / `3` | Switch view |
| `Tab` / Shift-Tab | Cycle views |
| `↑` / `↓` | Scroll / select |
| `t` / `c` / `s` / `p` (in Roi) | Cycle sort key (total / calls / success% / p50) |
| Viewport | Auto-scrolls to keep selected row visible in Flamegraph and Roi |
| `?` | Help overlay |

## Panic safety

**Hard rule:** TUI must never leave the terminal in raw mode. `install_panic_hook` wraps the default panic hook so any panic during `run()` first restores cooked mode + leaves the alternate screen, then re-emits the panic message. Full rationale + ratatui-pattern citation in [`docs/internals/adr-0006-panic-safe-tui.md`](../../docs/internals/adr-0006-panic-safe-tui.md).

## Local commands

```sh
cargo test  -p agentprof-tui
cargo test  -p agentprof-tui --test views        # 3 insta snapshots
cargo insta review -p agentprof-tui              # review snapshot deltas
cargo run   -p agentprof-cli -- analyze --session <path> --export tui
cargo doc   -p agentprof-tui --no-deps --open
```

## Dependencies

- Workspace internal: `agentprof-core`
- External (runtime): `ratatui 0.29`, `crossterm 0.28`, `chrono`, `thiserror`, `tracing`
- External (dev): `insta`, `agentprof-adapters` (snapshot tests load fixtures via `CopilotAdapter`)

## Change history

- **2026-05-30 — M1.5 ✅ shipped**: 3 views + panic-safe lifecycle + 3 insta snapshots + 2 CLI tests. See `CHANGELOG.md` `[Unreleased]`.
