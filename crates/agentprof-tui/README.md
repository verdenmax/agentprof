# agentprof-tui

> Ratatui-based terminal views for agentprof: per-turn flamegraph, interactive tool ROI rank, single-session aggregate.

**Status:** M1.5 ✅ shipped (2026-05-30). See [`docs/superpowers/specs/2026-05-30-m1.5-tui-design.md`](../../docs/superpowers/specs/2026-05-30-m1.5-tui-design.md) for the design spec and [`docs/internals/adr-0006-panic-safe-tui.md`](../../docs/internals/adr-0006-panic-safe-tui.md) for the panic-safety contract.

## Position in the agentprof architecture

Depends only on `agentprof-core`. Owns all `ratatui` / `crossterm` use; no other lib crate touches these.
See [`docs/architecture.md`](../../docs/architecture.md) §3 (system layering) and §4 (crate table).

## Public interface

| Item | Purpose |
|---|---|
| `AppRunner::new(&AnalysisReport, &Episodes)` | Construct a TUI session bound to a parsed report + per-call timing (M1.5) |
| `AppRunner::run(&mut Terminal<B>) -> Result<(), TuiError>` | Event loop; returns on `q` / Ctrl-C |
| `WatchRunner::new_static(WatchData)` | Owned-data runner for static `aggregate --export tui` (M1.6.3) |
| `WatchRunner::with_watcher(WatchData, Receiver<RefreshKind>, reload)` | Live-refresh runner for `agentprof watch [aggregate ...]` (M1.6.3) |
| `watch::{WatchData, RefreshKind, ReloadError, AggSortKey, WatchViewState}` | Public types for the watch runner contract |
| `app::terminal::install_panic_hook` | Idempotent (`Once`); MUST be called before `enter()` |
| `app::terminal::enter() -> Result<TuiTerminal>` | Refuses non-tty with `TuiError::NotATerminal` |
| `app::terminal::leave(&mut TuiTerminal)` | Best-effort restore; idempotent |
| `views::View` | `Flamegraph` / `Roi` / `Aggregate` / `Models` |

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
| `app::state` | `AppState` + `dispatch` (pure-logic state machine); F1.7 adds `AppState.models_selected: usize` field tracking the Models view selection cursor (driven by `scroll_to_top` / `scroll_to_bottom` for `gg` / `G`) |
| `app` (root) | `AppRunner` (wires state + views + event loop) |
| `watch` (M1.6.3) | `WatchRunner` + `WatchData` enum + `RefreshKind` / `ReloadError` + cross-session `AggSortKey`. Owns the live-refresh event loop; the file watcher itself lives in `agentprof-cli`. |
| `views::flamegraph` | Per-turn horizontal gantt + `segment_layout` + `build_gantt_cells` (3-state row: `█` tool / `░` LLM thinking / `·` padding) + `build_styled_cells_with_source` (colors `█` by [`ToolSource`](../agentprof-core/src/model/tool_source.rs): Builtin=cyan, MCP=magenta, Skill=yellow; reuses `theme::tool_source_color`) + Blue `T-id` prefix marks thinking-only turns (empty `tool_calls`) + 5-char output-tokens column between duration and gantt (via `format_tokens_short`; `None` → centered dash) + `header_line()` sticky column-name header (F1.8 — labels `Turn` / `Duration` / `Token` aligned over the data columns + colored legend `█ tool ░ thinking · padding`; rendered in a reserved top strip when `inner.height >= 3`) + `selected_turn_footer_line` (footer beneath the gantt listing the selected turn's tool calls with per-call durations, e.g. `T3 selected:  bash(120ms) +2 more`; appends `· thinking only` when the selected turn has no tool calls) |
| `views::roi` | Interactive tool rank with sort cycling + `recent_calls` |
| `views::aggregate` | By-Mode + By-Hook tables (single session) + `group_by_mode`; M1.6.3 adds a cross-session arm rendering `AnyAggregateReport` for `aggregate --export tui` and `watch aggregate ...` |
| `views::models` (F1.7) | Session-level per-model token rollup table (input / output / cache_read / cache_write); sorted by input desc with bold-cyan totals footer row. Falls back to a centered placeholder + explanation when `report.model_metrics` is `None` / empty (session has not emitted `session.shutdown`). `format_token_u64_short(u64)` helper caps cells at 5 chars (k/M/G/T/P abbreviations) for `u64` totals exceeding `u32::MAX`. j/k/↑/↓/G/gg navigate; `Esc` returns to Flamegraph |
| `views::format` | Shared display helpers — `human_short` (duration), `format_tokens_short` (5-char-capped token count for FlamegraphView prefix), `format_tokens_detailed` (uncapped token count for TurnDetailView header) |
| `views::turn_detail` (F1) | `TurnDetailState` state struct + pure formatters (`format_args_preview`, `wrap_args_full`, `status_sigil`) + `render_turn_detail(frame, area, &TurnDetailState, &AppState)` full-screen renderer; wired into `AppRunner` via `AppState.detail_view: Option<TurnDetailState>` — `Enter` on a Flamegraph turn opens it, `Esc` returns, `j`/`k`/`G`/`gg` navigate, `Enter` toggles args expand, `1`/`2`/`3` pop and switch view |
| `theme` | `ToolSource → Color` + status modifiers |
| `error` | `TuiError` (`#[non_exhaustive]` thiserror) |

## Key bindings

| Key | Action |
|---|---|
| `q` / Ctrl-C | Quit (clean leave + exit 0) |
| `1` / `2` / `3` / `4` | Switch view (Flamegraph / Roi / Aggregate / Models) |
| `Esc` (in Models / TurnDetail) | Return to previous top-level view |
| `Tab` / Shift-Tab | Cycle views |
| `↑` / `↓` or `k` / `j` | Scroll / select (vim aliases) |
| `G` | Jump to last row |
| `gg` | Jump to first row (two-key vim sequence) |
| `t` / `c` / `s` / `p` (in Roi) | Cycle sort key (total / calls / success% / p50) |
| Viewport | Auto-scrolls to keep selected row visible in Flamegraph and Roi |
| `?` | Help overlay |

## Panic safety

**Hard rule:** TUI must never leave the terminal in raw mode. `install_panic_hook` wraps the default panic hook so any panic during `run()` first restores cooked mode + leaves the alternate screen, then re-emits the panic message. Full rationale + ratatui-pattern citation in [`docs/internals/adr-0006-panic-safe-tui.md`](../../docs/internals/adr-0006-panic-safe-tui.md).

## Tracing & logging

`agentprof-tui` is **intentionally outside** the M1.6.4 span topology — it
emits no `#[tracing::instrument]` spans and currently no `tracing::*!`
macro calls. The rationale is captured in
[ADR-0010 D-4](../../docs/internals/adr-0010-tracing-infrastructure.md):
the TUI runs after `terminal::enter()` puts the terminal into the
alternate screen + raw mode, so any subscriber writing to `stderr` would
corrupt the UI (the exact bug-class fixed in M1.6.3 T2). Instead,
`agentprof-cli` swaps the tracing writer to
`$XDG_STATE_HOME/agentprof/agentprof.log` via a reload-`Layer` for the
duration of `AppRunner::run` / `WatchRunner::run` — see the
"Tracing & logging" section of
[`crates/agentprof-cli/README.md`](../agentprof-cli/README.md) and
[`docs/architecture.md`](../../docs/architecture.md) §15.5.

The `tracing` workspace dependency is retained in `Cargo.toml` because
the crate is allowed to call `tracing::warn!` / `info!` for non-rendering
diagnostics that the cli-side reload-`Layer` will then route to the log
file — but no such call site exists today, and any future addition MUST
respect the "no `eprintln!`, no direct stderr write" rule.

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
- **2026-06-01 — M1.6.3 ✅ shipped**: `WatchRunner` + `WatchData` + `Event::Refresh` + cross-session arm in `views::aggregate`. See `## WatchRunner (M1.6.3)` below and [ADR-0009](../../docs/internals/adr-0009-watch-runner-and-notify.md).

## WatchRunner (M1.6.3)

Owned-data TUI runner for live-refresh watching and static cross-session
aggregate viewing. Coexists with `AppRunner` (the M1.5 borrow-based
runner) — pick the right one:

| Use case | Runner |
|---|---|
| Single static parse output (`analyze --export tui`) | `AppRunner::new(&report, &episodes)` |
| Cross-session static aggregate (`aggregate --export tui`) | `WatchRunner::new_static(WatchData::Cross(any_report))` |
| Live-refresh single-session watch (`agentprof watch`) | `WatchRunner::with_watcher(WatchData::Single { .. }, rx, reload)` |
| Live-refresh cross-session aggregate watch (`agentprof watch aggregate ...`) | `WatchRunner::with_watcher(WatchData::Cross(..), rx, reload)` |

### Data shape

```rust
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::analyzer::aggregate::AnyAggregateReport;
use agentprof_core::episode::Episodes;
use agentprof_core::model::SessionMeta;

pub enum WatchData {
    Single {
        report: AnalysisReport,
        episodes: Episodes,
        meta: SessionMeta,
    },
    Cross(AnyAggregateReport),
}
```

### Live-refresh contract

- Caller spawns a `notify::RecommendedWatcher` (typically wrapped in
  `notify-debouncer-mini`), holds it for the entire lifetime of
  `run()` — dropping it stops the watcher — and writes
  `RefreshKind::DataChanged` into the mpsc `channel::<RefreshKind>()`
  on each debounced event burst.
- Caller provides a `Box<dyn FnMut() -> Result<WatchData, ReloadError>>`
  reload closure that re-parses (single mode) or re-aggregates
  (cross mode) on demand. The closure is invoked on the TUI thread
  whenever a `RefreshKind::DataChanged` arrives; therefore cross-mode
  reload blocks rendering for the full reload duration (see
  [ADR-0009](../../docs/internals/adr-0009-watch-runner-and-notify.md)
  "Negative consequences").
- Reload errors populate a footer banner (red, one line); the event
  loop continues. Only terminal I/O failures bubble out of `run()`.

### Key bindings (cross-session aggregate mode)

| Key | Action |
|---|---|
| `q` / Ctrl-C | Quit (clean leave + exit 0) |
| `?` | Toggle help overlay |
| `c` / `t` / `s` / `p` | Sort by calls / total / sessions / p50 |
| `↑` / `↓` | Move selected row |

Single-session watch mode reuses the M1.5 `AppRunner` view bindings
(`1`/`2`/`3` switch view, `t`/`c`/`s`/`p` cycle Roi sort, etc.). The
Flamegraph selected-turn footer ends with `· Enter for detail` to
advertise the F1 TurnDetailView; the `?` help overlay (now 27 lines)
lists the same `Enter` / `Esc` / `j`/`k`/`G`/`gg` / `1`/`2`/`3` keys
under a "Detail view (Flamegraph → Enter):" section. Pressing `Enter`
on a turn with no tool calls also opens the detail view — it will
display `(no tool calls)` in the body. This is intentional: every
selectable turn is openable for consistency, and the empty-state
confirms there's nothing more to inspect rather than silently
no-op'ing.

**TurnDetailView in single-session watch mode** (F1 Task 8): `Enter` on a
selected Flamegraph turn opens the detail view exactly as in
`analyze --export tui`; `Esc` / `1` / `2` / `3` returns or pops to another
view; navigation (`j`/`k`/`G`/`gg`) and `Enter`-to-toggle-args-expand
behave identically. The detail-view state (`WatchViewState.detail_view`)
persists across reloads. If a reload removes the underlying turn (e.g.
the session file was truncated or the turn's events were dropped), the
detail view is dropped and the footer shows
`⚠ reload error: turn <id> disappeared after reload`. The banner is
**not sticky**: it persists only until the next reload — a subsequent
successful reload clears it, regardless of whether that reload sets
its own new banner.

**ModelsView in watch mode** (F1.7 Task 10): `4` opens the Models view exactly as in `analyze --export tui`. The selection cursor (`WatchViewState.models_selected`) and the active view (`WatchViewState.view`, defaults to `Aggregate` for M1.6.3 backward compat) both round-trip across the transient `AppState` on every render / dispatch tick, so number-key view switches persist across `RefreshKind::Reload` events. When `session.shutdown` arrives mid-watch, the next reload populates `AnalysisReport.model_metrics` and the Models view body switches from empty-state to the table on the next render. Cross-session aggregate mode (`watch aggregate ...`) does NOT surface the Models view (the metrics are session-level; cross-session aggregation is out of scope per ADR-0012 D-13).

**Known limitation (F1.7):** `WatchRunner::render_into` currently
dispatches only `View::Models` (new in F1.7) and falls back to
`views::aggregate::render` for all other views (`Flamegraph`, `Roi`,
`Aggregate`). This means pressing keys `1` / `2` / `3` in watch mode
updates the view state but the rendered output stays on aggregate
until `Tab` cycles to Models or back. This is a pre-existing M1.6.3
limitation that F1.7's `view` round-trip fix exposed (state now
persists across events but render dispatch is incomplete). Tracked as
F1.7.1 follow-up: extend `WatchRunner::render_into` to dispatch all
four `View::*` arms, matching `AppRunner::render_into`.

### CLI wiring

`agentprof watch` (in `agentprof-cli`) owns the file-watcher thread —
see [ADR-0009 D-10](../../docs/internals/adr-0009-watch-runner-and-notify.md)
for the dependency-graph reason. `agentprof-tui` has no `notify`
dependency at all (neither in `Cargo.toml` nor in any `use` statement);
the runner only sees an opaque `Receiver<RefreshKind>` plus the reload
closure, both injected by `agentprof-cli`.
