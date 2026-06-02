# ADR-0009: WatchRunner and notify-based file watcher

**Status**: Accepted
**Date**: 2026-06-01
**Milestone**: M1.6.3
**Spec**: [`docs/superpowers/specs/2026-06-01-m1.6.3-watch-and-aggregate-tui-design.md`](../superpowers/specs/2026-06-01-m1.6.3-watch-and-aggregate-tui-design.md)
**Plan**: [`docs/superpowers/plans/2026-06-01-m1.6.3-watch-and-aggregate-tui.md`](../superpowers/plans/2026-06-01-m1.6.3-watch-and-aggregate-tui.md)

## Context

After M1.6.2 shipped `agentprof aggregate` (md/json/csv/html), the only
remaining M1.6 work was a live-refresh TUI ("watch") so users could
monitor an active Copilot session without restarting `agentprof analyze`
on every change. We also deferred the cross-session aggregate TUI
(`aggregate --export tui`) from M1.6.2 — both features share the same
underlying need: an owned-data TUI runner that can swap its data
snapshot on demand.

The M1.5 `AppRunner<'a>` is borrow-based; its lifetime parameter ties
the runner to a single immutable parse output, which is a poor fit for
"reload on every filesystem event."

## Decisions

(Each maps to a decision row in the M1.6.3 spec §2.)

- **D-1** Ship `watch` + `aggregate --export tui` together — they share
  the cross-session aggregate code path.
- **D-2** `watch` is an independent subcommand, not a `--watch` flag on
  existing subcommands — clearer interaction semantics; avoids
  undefined "write to file every refresh?" behaviour.
- **D-3** Support both single-session AND cross-session sub-modes —
  both are real use cases.
- **D-4** Use `notify-debouncer-mini = "0.4"` for kernel-level
  filesystem events (inotify / FSEvents / ReadDirectoryChangesW); the
  `notify` crate (v6.1.1) is pulled in **transitively** via the
  debouncer's re-exports (`notify_debouncer_mini::notify::{
  RecommendedWatcher, RecursiveMode}`). A direct `notify` workspace
  dep was intentionally NOT declared, to avoid the risk of two
  notify-major versions co-existing in the dependency tree (e.g.
  the user adding `notify = "7"` later while debouncer-mini still
  pulls v6). Rejected polling because it adds latency and burns CPU.
- **D-5** `--session latest` locks to the initial session at startup;
  newer sessions are not auto-followed (`q` + restart instead).
- **D-6** Debounce window defaults to 250 ms (matches existing TUI
  poll cadence; coalesces append-burst storms).
- **D-7** New `WatchRunner` module coexists with `AppRunner` (do not
  retrofit borrow-based AppRunner).
- **D-8** Single `WatchRunner` struct carrying `enum WatchData { Single,
  Cross }` dispatches by variant — code reuse for the event loop,
  watcher channel, and refresh dispatch.
- **D-9** Existing `agentprof-tui::views::aggregate` gets a cross-
  session arm; do NOT fork a new file.
- **D-10** Watcher thread lives in `agentprof-cli`, NOT `agentprof-tui`
  — tui would otherwise need to depend on `agentprof-adapters` for
  reload, which breaks the lib leaf rule.
- **D-11** New `Event::Refresh` variant on `Event` enum (alongside
  reserved `Tick`); only `WatchRunner::run` emits it.
- **D-12** Watch mode requires TTY on both stdin and stdout.
- **D-13** Reload failures populate a footer banner; the watch loop
  continues (transient parse errors during writer mid-flush should not
  crash the watch session).
- **D-14** No real file-watcher integration test in CI — notify-based
  timing is flaky on CI runners. Covered by manual smoke in spec §12.
- **D-15** No polling fallback if notify init fails — YAGNI for MVP;
  print actionable error suggesting `--export md` and exit DataError.
- **D-16** This ADR documents the decisions.

## Consequences

### Positive

- Clean coexistence with M1.5 `AppRunner` (separate runner; no
  retrofit risk).
- Reuses existing view fns + the `compute_aggregate` pipeline (DRY).
- Owned-data model makes future enhancements (background reload
  thread; pause/resume) easier.
- Single struct + `WatchData` enum keeps the event loop simple.

### Negative

- One new direct external dep (`notify-debouncer-mini`) plus its
  transitive closure (`notify`, `crossbeam-channel`,
  `inotify`/`fsevent-sys`/`kqueue` platform backends, `mio`,
  `filetime`).
- No `deny.toml` changes required — all transitively-added crates
  (notify 6.1.1 in particular) use licenses already in the allowlist
  (MIT, Apache-2.0, ISC, CC0-1.0). The `Artistic-2.0` concern that
  arose during design only applies to notify 7.x; we don't ship it.
- No CI coverage for the watcher path itself (manual smoke only).
- Cross-session reload blocks the TUI for the full reload duration
  (100+ sessions → multi-second pause); future M1.6.6 perf milestone
  may move reload off the TUI thread.

## Alternatives considered

- **Polling watcher** — simpler implementation, no extra deps, but
  added latency (poll interval) and CPU burn. Rejected D-4.
- **`--watch` flag on existing subcommands** — would require defining
  what `--watch` means for `--export md` / `--export html` (re-write
  the file?). Rejected D-2.
- **Direct `notify = "7"` workspace dep** — would have required adding
  `Artistic-2.0` to the deny.toml allowlist (notify 7's dual-license
  CC0-1.0 / Artistic-2.0). Picking debouncer-mini's transitive
  `notify = "6.1.1"` avoids that license-policy expansion while still
  giving us debounce out of the box. Rejected D-4.
- **Auto-follow latest session** — would require a separate root-watcher
  for session-dir creation events + session-swap logic in the runner.
  Niche use case; deferred D-5.
- **Retrofit `AppRunner` to be owned-data** — would invalidate M1.5
  static tests and ripple through `AppState`. Rejected D-7.
- **Fork a new cross-session aggregate view file** — would duplicate
  the existing `views::aggregate` module. Rejected D-9.
- **Put watcher in `agentprof-tui`** — would force tui to depend on
  `agentprof-adapters` + `agentprof-core::adapter::Adapter`. Breaks
  lib leaf rule. Rejected D-10.

## References

- Spec: [`docs/superpowers/specs/2026-06-01-m1.6.3-watch-and-aggregate-tui-design.md`](../superpowers/specs/2026-06-01-m1.6.3-watch-and-aggregate-tui-design.md)
- Plan: [`docs/superpowers/plans/2026-06-01-m1.6.3-watch-and-aggregate-tui.md`](../superpowers/plans/2026-06-01-m1.6.3-watch-and-aggregate-tui.md)
- Predecessor ADR: [`adr-0008-aggregate-report-and-utilization.md`](adr-0008-aggregate-report-and-utilization.md) (M1.6.2)
- M1.5 panic-safe TUI: [`adr-0006-panic-safe-tui.md`](adr-0006-panic-safe-tui.md)
