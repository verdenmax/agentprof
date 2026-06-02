# ADR-0010: Tracing infrastructure (structured observability)

**Status**: Accepted
**Date**: 2026-06-02
**Milestone**: M1.6.4
**Spec**: [`docs/superpowers/specs/2026-06-02-tracing-design.md`](../superpowers/specs/2026-06-02-tracing-design.md)
**Plan**: [`docs/superpowers/plans/2026-06-02-m1.6.4-tracing.md`](../superpowers/plans/2026-06-02-m1.6.4-tracing.md) *(written next, after this ADR)*

## Context

agentprof has had `tracing` 0.1 and `tracing-subscriber` 0.3 wired in
every crate since M1.4 (`docs/architecture.md:33,541`), with an
`AGENTPROF_LOG` env-filter `init_tracing()` in `cmd::main`. Despite that
plumbing, the M1.6.3 codebase contains **two** actual `tracing::*!`
call sites (both `debug!` in `cmd/watch.rs`) and **~14** scattered
`eprintln!` calls under `cmd/{analyze,list,aggregate,watch}.rs` that
carry the actual diagnostic and warning load.

Three concrete pain points surfaced across recent reviews:

- M1.6.3 T2 fix-up had to downgrade a `tracing::warn!` to `tracing::debug!`
  inside the notify debouncer callback because the background-thread
  warning corrupted the TUI alt-screen (see
  `crates/agentprof-cli/src/cmd/watch.rs:253` rustdoc).
- M1.6.3 quality review #6 found `eprintln!("warning: no sessions ...")`
  fires on every reload tick of `watch aggregate` on an empty root —
  same alt-screen-corruption class.
- `full-review-cli-adapters` review found that
  `compute_aggregate` still emits per-session parse failures via
  `eprintln!` (`crates/agentprof-cli/src/cmd/aggregate.rs:~278`),
  which will also corrupt the alt-screen mid-watch-loop.

The pattern is clear: ad-hoc `eprintln!` is no longer adequate. Users and
developers need: (a) one canonical place to look for diagnostic output,
(b) structured fields they can grep across, (c) span correlation to tie
a "warning: ..." to its session / command / phase, and (d) a sink choice
(stderr vs file) that does NOT trash interactive UI.

The shipped architecture already names tracing as the canonical mechanism;
M1.6.4 finally implements it.

## Decisions

(Each row maps 1:1 to a decision in the M1.6.4 spec §2 — D-1 through
D-16. Re-opening a decision requires either editing this ADR or
recording a new ADR that explicitly supersedes the affected D-rows.)

- **D-1** Scope: medium — convert all production `eprintln!` to
  `tracing::*` AND add `#[tracing::instrument]` on the key
  per-subcommand / per-adapter / per-analyzer entry points. Rejected
  "light" (eprintln-only convert) because cross-call correlation is the
  primary user value; rejected "heavy" (per-event spans) because
  signal-to-noise without OTLP is poor.
- **D-2** TUI behavior: when entering an interactive TUI
  (`analyze --export tui`, `watch`, `watch aggregate`), automatically
  switch the tracing writer to a rolling file under
  `$XDG_STATE_HOME/agentprof/agentprof.log`; on clean exit,
  `println!` the path to stdout (after `terminal::leave()`). Rejected
  auto-silence (kills debuggability); rejected stderr-always (exact
  M1.6.3 T2-fix-up bug).
- **D-3** Config UX: keep the existing `AGENTPROF_LOG` env var; add two
  global CLI flags `--log-level <LEVEL>` and `--log-file <PATH>` with
  `clap` `global = true`; precedence is **flag > env > default**, with
  the default = `warn`-level stderr. Rejected env-only (poor
  discoverability via `--help`); rejected flag-only (breaks existing
  AGENTPROF_LOG users).
- **D-4** Span topology (4 layers): `cmd.{analyze,list,aggregate,watch}`
  (info_span) → `adapter.{discover,parse,load_meta}` (debug_span) →
  `analyzer.{derive_episodes,analyze}` / `aggregator.group_by_*`
  (debug_span) → events (trace/debug/info/warn/error). Rejected lighter
  (top-2 only) because adapter-level pivot is high-value; rejected
  heavier (per-event/per-tool) until OTLP exporter ships.
- **D-5** PII handling: session paths default to a stable
  `sha256[..8]` hex short-hash; `AGENTPROF_LOG_FULL_PATHS=1` opts out.
  Documented in the rustdoc on `hash_path`: collision probability ≈
  50 % at √(2³²) ≈ 65 536 distinct paths, which is fine for a developer
  tool typically running over < 1 000 sessions. Rejected always-full
  (leaks `/home/<user>/...` into logs); rejected always-redacted
  (kills user-side debugging); rejected `[..16]` (overkill).
- **D-6** OTLP relationship: M1.6.4 emits structured tracing locally
  only. NO OTLP exporter wiring. But the emission API stays
  OTLP-ready: no stdout-only assumptions, no synchronous side-effects
  inside spans. Rejected wiring OTLP in M1.6.4 because it requires
  `agentprof-storage::otlp` work that is explicitly Phase 2.
- **D-7** eprintln replacement: production code emits via `tracing::*`
  only. All ~14 existing `eprintln!` in `cmd/*` get converted to
  `tracing::warn!` / `info!` / `error!`. `println!` is reserved for
  **user-expected output** (the `list` table, the `analyze --export md`
  / `--export json` / `--export csv` reports, and the final TUI-exit
  log-path-hint line). Rejected per-call-site split between "internal"
  vs "user-facing" — arbitrary, ambiguous.
- **D-8** Module layout: `agentprof-core::observability::pii` (lib leaf,
  pub) + `agentprof-cli::observability::{config, init, tui_guard}`
  (binary-only). The PII hash helper lives in core because adapters
  and tui both consume it; init / writer / TUI guard logic lives in
  cli because cli is the orchestrator. Rejected a new
  `agentprof-observability` crate (YAGNI; single-shot work that
  doesn't justify a sixth workspace crate); rejected pure scattered
  changes (PII helper duplication + bloated `main.rs`).
- **D-9** New direct deps: `sha2 = "0.10"` (workspace; used only by
  `agentprof-core` for the hash) and `tracing-appender = "0.2"`
  (workspace; used only by `agentprof-cli` for non-blocking
  rolling-file writer). Rejected a hand-rolled `std::hash::Hasher`
  hash (nonstandard length, no `DefaultHasher` stability guarantee);
  rejected `tracing-subscriber::fmt::writer::BoxMakeWriter` only
  (synchronous disk I/O would block the cli main thread). Transitive
  deps for sha2 (`digest`, `block-buffer`, `crypto-common`,
  `generic-array`, `typenum`, `cpufeatures`) are all MIT or
  MIT-OR-Apache-2.0 — already in the existing `deny.toml` allowlist;
  no `deny.toml` change required.
- **D-10** CLI flag placement: `--log-level` and `--log-file` go on the
  top-level `Cli` struct with `#[arg(global = true, env = "...")]`,
  NOT on each subcommand. Rejected per-subcommand because of the
  exact arg-ordering trap that bit `watch aggregate` in M1.6.3 (T2
  review #2: `agentprof watch aggregate --debounce-ms 500` failed
  because clap parsed everything after the subcommand into the
  subcommand).
- **D-11** Log file rotation: `tracing-appender::rolling::daily(dir,
  "agentprof.log")` with retention left to the user. No intra-session
  rotation. Rejected hourly (too noisy); rejected size-based
  (complexity over benefit for a developer tool).
- **D-12** Worker-thread guard: `tracing_appender::non_blocking`'s
  `WorkerGuard` is returned wrapped inside a
  `cli::observability::TracingHandle` struct held by `main()` for the
  entire process lifetime; dropping it earlier loses buffered events.
  Rejected forgetting about the guard (drops last buffer batch).
- **D-13** init failure mode: ANY tracing init failure — bad env-filter
  string, log-file dir create error, `set_global_default` conflict,
  appender init — is soft. Fall back to default stderr subscriber,
  emit one `tracing::warn!`, continue. Rejected hard-failing with
  DataError(3) because tracing is observability infra and must never
  be the reason the CLI cannot run.
- **D-14** Span field convention: session paths emitted as
  `session = %hash_path(p)`, file paths as `file = %hash_path(p)`,
  record counts as `events = N` / `episodes = N` / `buckets = N`.
  No raw `PathBuf`, no raw home-dir paths in any production-path
  emission. Rejected mixed conventions (inconsistent grep, accidental
  PII leaks).
- **D-15** Tests: unit tests for `hash_path` (determinism, distinct
  inputs, invalid-UTF8 path tolerance, empty path) and `LogConfig`
  precedence (flag > env > default, invalid level fallback, XDG path
  resolution, `--log-file -` forces stderr); one integration test
  file `crates/agentprof-cli/tests/cli_tracing.rs` (5 scenarios per
  spec §6.2). NO snapshot tests of trace output (timestamps make them
  brittle). NO new CI gate (existing test suite covers).
- **D-16** Documentation produced as part of M1.6.4: this ADR, L1
  `docs/architecture.md` updates (§4 cli row + new §15.4 observability
  paragraph + §8 mention of new global flags), L2
  `crates/agentprof-{core,cli}/README.md` updates, L3 rustdoc on all
  new pub items, `CHANGELOG.md` `[Unreleased]` entries
  (Added / Changed / Dependencies).

## Consequences

### Positive

- Single canonical place (`tracing::*`) for all diagnostic / warning /
  info output across all 5 crates; existing scattered `eprintln!` is
  removed.
- Span correlation: every warning carries its `cmd.* / adapter.* /
  analyzer.*` context, making support-style debugging much faster
  (e.g. `"WARN aggregator.group_by{key=tool}: skipping malformed
  bucket"` instead of `"warning: ..."` with no caller info).
- TUI mode is alt-screen-safe by construction (D-2 + D-12 guard
  lifecycle), removing the class of bugs that needed M1.6.3 T2-fix-up
  and the residual `m1.6.3-t2-followup-residual-stderr-in-watch`
  ticket.
- PII surface is reduced by default (D-5): logs no longer leak
  `/home/<user>/...` paths; opt-out is a single env var.
- Future OTLP exporter wiring (Phase 2) is a localized change because
  emission API stayed OTLP-ready (D-6) — only the subscriber stack
  in `cli::observability::init` adds a tonic exporter layer.
- Existing `AGENTPROF_LOG` env-var users keep working (D-3 backward
  compatibility).

### Negative

- 2 new direct workspace dependencies (`sha2`, `tracing-appender`)
  add ~150 KB to the release binary. Acceptable for a developer
  tool; smaller than the existing ratatui surface.
- Behavior change for existing users that grep stderr by prefix:
  `eprintln!("agentprof: warning: ...")` becomes
  `WARN agentprof_cli::cmd::analyze: ...` (tracing-subscriber fmt
  format). Documented in CHANGELOG with the new grep recipe.
- New XDG_STATE log file may surprise TUI users unfamiliar with the
  path. Mitigated by explicit `println!` on TUI exit (D-2) and a
  CONTRIBUTING / README mention.
- Hash collisions are theoretically possible at ~65 k distinct paths
  (D-5). Rare in practice; documented workaround
  (`AGENTPROF_LOG_FULL_PATHS=1`).
- Init failures soft-fall to stderr (D-13). Means a misconfigured
  `--log-file` does NOT exit non-zero, which a strict-CI user might
  prefer; this is an explicit tradeoff for "tracing must never block
  CLI startup". A future opt-in `--log-strict` flag could change this
  if needed (out of scope for M1.6.4).
- No OTLP exporter (D-6): users wanting distributed-trace export must
  wait for Phase 2 / `agentprof ingest-otlp` companion work.

## Alternatives considered

(Already enumerated row-by-row in the Decisions section above; each
D-row carries its rejected alternative inline. The four largest:)

- **A new `agentprof-observability` crate** (alternative to D-8) —
  would centralize init / hash / future-OTLP wiring in one place but
  adds a sixth workspace crate for what is single-shot M1.6.4 work.
  YAGNI.
- **OTLP exporter wired in M1.6.4** (alternative to D-6) — would
  deliver real distributed tracing in one shot, but requires
  `agentprof-storage::otlp` server-side work (Phase 2). Scope creep;
  push to its own milestone.
- **`agentprof watch` keeps the current `tracing::debug!` workaround
  forever** (status quo to D-2) — works in the narrow watch case but
  doesn't generalize to `analyze --export tui` (which is now hitting
  the same alt-screen-corruption class via post-load warnings, per
  `full-review-cli-adapters` Important #3).
- **Per-subcommand `--log-level` flag** (alternative to D-10) —
  rejected because of the exact clap arg-ordering UX trap that bit
  `watch aggregate` in M1.6.3 (T2 review #2). The `global = true`
  attribute side-steps this entirely.

## Implementation notes

- New pub API surface enumerated in spec §3.2: `agentprof-core`
  exposes `observability::pii::{hash_path, hash_short}`;
  `agentprof-cli` exposes
  `observability::{LogConfig, LogWriter, TracingHandle, TuiLogGuard,
  init_tracing, enter_tui_log_guard}`. All `pub` items must carry
  rustdoc + `# Examples` per workspace lints (`missing_docs = warn` +
  `-D warnings`).
- 6 implementation tasks T0–T6 in spec §7.1. Followed by writing-plans
  expansion into bite-sized steps in
  `docs/superpowers/plans/2026-06-02-m1.6.4-tracing.md` (next pipeline
  stage).
- All 4 workspace gates must remain green: `cargo fmt --check` /
  `cargo clippy --workspace --all-targets --all-features -D warnings`
  / `cargo test --workspace --all-features` / `RUSTDOCFLAGS=-Dwarnings
  cargo doc --no-deps --workspace`. No new gates added.
- `deny.toml` is **not** modified (D-9: all new transitive licenses
  are MIT / MIT-OR-Apache-2.0, already allowed).
- Acceptance criteria (spec §9) provide the post-implementation
  verification checklist for the final review.
- Out-of-scope items (spec §8): `--log-format json`, per-PR clippy
  lint enforcing PII hash, SQLite-store linkage, removing the
  `cmd::list` table `println!`. All deferred to M1.6.5+ or Phase 2.

## References

- Predecessor ADR-0009 (M1.6.3): WatchRunner + notify-based file
  watcher — provides the panic-safe TUI lifecycle pattern that
  `enter_tui_log_guard` (D-2) integrates with.
- Predecessor ADR-0006 (M1.5): panic-safe TUI — defines the
  terminal::leave-on-panic contract; M1.6.4 must not weaken it (the
  tracing-appender WorkerGuard must drop AFTER the panic hook runs
  terminal::leave).
- Predecessor ADR-0008 (M1.6.2): aggregate report — defines
  `AnyAggregateReport`; M1.6.4 instruments `aggregate::group_by_*`
  without changing its shape.
- M1.6.3 T2 follow-up ticket
  `m1.6.3-t2-followup-residual-stderr-in-watch` is closed by
  D-2 + D-13.
- M1.6.3 quality review issue #6 (T2 review) — `eprintln!` in reload
  closure — closed by D-7.
- `full-review-cli-adapters` Important #3 (one-shot
  `aggregate --export tui` empty-root warning corrupts alt-screen) —
  closed by D-2 + D-7.
- spec self-review item §12 — internal consistency confirmed at spec
  write time; this ADR adds no new inconsistencies (every D-row
  appears identically here and in the spec).
