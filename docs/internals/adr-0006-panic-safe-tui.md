# ADR-0006 — Panic-safe TUI lifecycle

**Status:** Accepted (2026-05-30, ships with M1.5).
**Supersedes:** —
**Superseded by:** —
**Owner:** `agentprof-tui` crate.

## Context

`agentprof-tui` uses `crossterm` for raw mode + alternate-screen handling. A panic in any TUI code path that does not restore those modes leaves the user's shell unusable until they manually run `reset` or close the terminal. This is unacceptable for a developer tool the user invokes interactively.

Two related failure modes:

1. **Panic** anywhere in the `AppRunner::run` loop or in a view's `render` function.
2. **Early `?` return** from `enter()` after raw mode is enabled but before `leave()` runs.

## Decision

The CLI (`crates/agentprof-cli/src/cmd/analyze.rs::run_tui`) is responsible for the lifecycle in this exact order:

```rust
agentprof_tui::app::terminal::install_panic_hook();   // 1. BEFORE enter()
let mut term = agentprof_tui::app::terminal::enter()?; // 2. enter raw + alt screen
let res = agentprof_tui::AppRunner::new(&report, &episodes).run(&mut term);
let _ = agentprof_tui::app::terminal::leave(&mut term); // 3. best-effort restore
res?                                                    // 4. propagate run error after restore
```

`install_panic_hook` wraps the existing `std::panic::set_hook`:

```rust
static PANIC_HOOK_INSTALLED: Once = Once::new();

pub fn install_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            original(info);   // print the panic message in cooked mode
        }));
    });
}
```

`enter()` also performs best-effort cleanup if a step succeeds but a later step fails (T1 review I-1 fix): if `EnterAlternateScreen` or `Terminal::new` fails after `enable_raw_mode` succeeded, `enter()` itself calls `disable_raw_mode` before returning the error. This means callers who get `Err` from `enter()` do NOT need to (and can NOT) call `leave()`.

`leave()` runs all 3 cleanup steps unconditionally (T1 review M-1 fix) — `disable_raw_mode` + `LeaveAlternateScreen` + `show_cursor` — returning the first error encountered. This guarantees full restore even if one step fails (e.g. `show_cursor` errors on an emoji-rich terminal that lost track of cursor position).

`Once` makes `install_panic_hook` idempotent so callers don't have to coordinate.

## Why not RAII (`Drop`)?

A custom `TerminalGuard` with `Drop` would auto-leave at scope exit, but:

- `Drop` runs **after** the panic hook has already printed (incorrectly) into raw mode.
- `Drop` cannot return errors, so any cleanup failure is silently swallowed.
- Coordinating `Drop` order with `Terminal<CrosstermBackend<Stdout>>` (which itself owns stdout) is fiddly.

The hook-first pattern is the one used by upstream ratatui examples and is well-known.

## Why not `signal_hook` / SIGINT trap?

`crossterm` already delivers Ctrl-C as a normal `KeyEvent`, which our `dispatch` maps to `Action::Quit`. A SIGINT trap would conflict with crossterm's signal handling on Linux and double-restore. The current `dispatch`-based Ctrl-C handling is sufficient for M1.5; signal trapping is reserved for daemon-style M2.x `watch` mode where the loop may block on I/O.

## Why a `Once` guard on the panic hook?

Tests call `install_panic_hook` multiple times (3 in `tests` modules) to verify idempotence. Multiple subscriptions would compound the wrapping (each call wraps the previous hook), causing duplicate `disable_raw_mode` calls on panic. `Once` guarantees a single installation.

## Consequences

**Pros:**

- Single `install_panic_hook` call works regardless of feature flags or test runner reentry.
- `enter()` + `leave()` are pure procedural functions — easy to test (snapshot tests don't call them; they use `TestBackend` directly).
- Panic hook restores terminal **before** the panic message prints, so the message shows in cooked mode.
- Idempotent across all three entry points (CLI, future `watch` daemon, test harness).
- Partial-failure safety: `enter()` itself cleans up if a later step fails; callers don't need to coordinate.

**Cons:**

- Caller must call the three functions in order — there is no compile-time enforcement.
- Mitigation: the CLI is the only entry point and has a single `run_tui` function that bakes in the order. Future entry points (M2.x `watch`) should reuse `run_tui` or copy the order verbatim.
- A panic in `install_panic_hook` itself (e.g. allocator OOM during `Box` construction) would abort with whatever hook is current. Acceptable: such conditions already prevent any sensible recovery.

## Test coverage

- `app::terminal::tests::install_panic_hook_is_idempotent` — calls 3×, no panic.
- `app::terminal::tests::enter_returns_not_a_terminal_when_stdout_is_piped` — guards against the non-tty case (raw mode never enabled).
- CLI integration `analyze_export_tui_flag_parses_and_short_circuits_under_non_tty` — guards the full path from `--export tui` to the `OutputError` exit code.

Full panic-during-run coverage is **manual** (would require a sub-process test fixture that intentionally panics inside a view). Documented as a known limitation; if a panic-during-run regression appears, write a subprocess test then.

## References

- Spec `docs/superpowers/specs/2026-05-30-m1.5-tui-design.md` §9 ("Panic safety").
- ratatui upstream examples: <https://github.com/ratatui/ratatui/blob/main/examples/apps/panic/src/main.rs> (the pattern this ADR follows).
- `crossterm` docs on `enable_raw_mode` / `EnterAlternateScreen` lifecycle: <https://docs.rs/crossterm/0.28/crossterm/terminal/index.html>.
