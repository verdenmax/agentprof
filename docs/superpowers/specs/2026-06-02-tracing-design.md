# M1.6.4 — Tracing infrastructure (structured observability) — Design Spec

> **Status:** Draft → awaiting user spec-review approval.
> **Milestone:** M1.6.4 (next after M1.6.3 watch shipped).
> **Date:** 2026-06-02.
> **Pipeline stage:** 1 (Discovery / Design). Next: §5.2 Stage 2 (ADR) then Stage 3 (writing-plans).
> **Author:** Copilot brainstorming session 252068e5.
> **Branch:** `feat/m1.6.4-tracing`.

---

## 1. Why

agentprof has a fully wired `tracing` dependency in every crate, a working
`AGENTPROF_LOG` env-filter setup in `main.rs` — and **two** actual
`tracing::*!` call sites in the entire codebase (both in `watch.rs`,
both `debug!`). Meanwhile `~14` `eprintln!` calls scattered across
`cmd/{analyze,list,aggregate,watch}.rs` carry diagnostic / warning load.

The M1.5 / M1.6.x review passes consistently surfaced:

- TUI alt-screen corruption from stray stderr writes (T2 fix-up in M1.6.3
  downgraded one `tracing::warn!` to `debug!` exactly because the
  background callback would otherwise splat the watch screen).
- Empty-root warning "no sessions matching" emitting on every reload tick
  in `watch aggregate` mode (filed as
  `m1.6.3-t2-followup-residual-stderr-in-watch`).
- Per-session parse failures use `eprintln!` from inside
  `compute_aggregate`, also alt-screen-hostile.
- No correlation between user-visible warnings and the actual code path
  (a "warning: --output is ignored" line gives no caller / phase context).

The fix is the standard one: **structured tracing with proper sinks**.
This spec captures the M1.6.4 build of that.

`docs/architecture.md:541` and `:33` already prescribe `tracing` as the
canonical logging mechanism and `RUST_LOG`-style env control. M1.6.4
fulfills that promise.

---

## 2. Decision log

(Each decision was locked during the 2026-06-02 brainstorming session;
re-opening one requires an updated ADR.)

| # | Decision | Alternative considered |
|---|---|---|
| **D-1** | **Scope:** medium — convert all `eprintln!` to `tracing::*`, add `#[instrument]` on key entry points, structured spans 4 layers deep. | Light (just convert eprintln) — rejected: leaves cross-call correlation impossible. Heavy (per-event spans) — rejected: signal-to-noise too low without OTLP. |
| **D-2** | **TUI behavior:** when entering an interactive TUI (`analyze --export tui` or `watch`), automatically switch tracing writer to a rolling file under `$XDG_STATE_HOME/agentprof/agentprof.log`; on exit, `println!` the path. | Auto-silence (drop subscriber) — rejected: kills debuggability. Stderr always — rejected: alt-screen corruption (the exact bug M1.6.3 already had to work around). |
| **D-3** | **Config UX:** keep `AGENTPROF_LOG` env var; add global `--log-level <LEVEL>` and `--log-file <PATH>` flags (clap `global = true`); flag > env > default. | Env-only — rejected: discoverable via `--help` is important for new users. Flag-only — rejected: breaks existing AGENTPROF_LOG users. |
| **D-4** | **Span topology (4 layers):** `cmd.{analyze,list,aggregate,watch}` → `adapter.{discover,parse,load_meta}` → `analyzer.{derive_episodes,analyze}` / `aggregator.group_by` → events. | Lighter (top-2 layers only) — rejected: removes the per-adapter / per-analyzer pivot users need most. Heavier (per-event/per-tool spans) — rejected: M1.6.4 doesn't have OTLP exporter to consume that volume. |
| **D-5** | **PII handling:** session paths default to `sha256[..8]` short-hash; `AGENTPROF_LOG_FULL_PATHS=1` opts out. | Always-full-paths — rejected: leaks `/home/<user>/.cache/...`. Always-redacted — rejected: kills user's own debuggability. Bigger-hash (sha256[..16]) — rejected as overkill; 32-bit collision space is fine for the typical < 100 sessions per run. |
| **D-6** | **OTLP relationship:** M1.6.4 emits structured tracing locally only. Do NOT wire an OTLP exporter. But keep emission API OTLP-ready (no stdout-only assumptions, no synchronous side-effects from spans). | Wire OTLP exporter in M1.6.4 — rejected: scope creep, requires storage::otlp work that's Phase 2. |
| **D-7** | **eprintln replacement:** production code emits via `tracing::*` only. All 14 existing `eprintln!` calls → `tracing::warn!` / `info!` / `error!`. `println!` is reserved for **user-expected output** (list table, md/json/csv reports, the final TUI-exit path-hint line). | Keep eprintln for "user-facing" warnings, tracing for "internal" debug — rejected: arbitrary line; users want one place to look for what went wrong. |
| **D-8** | **Module layout:** `agentprof-core::observability::pii` (lib leaf) + `agentprof-cli::observability::{config, init, tui_guard}` (binary-only). Hash helper in core because adapters/tui both consume it; init logic stays in cli (orchestrator). | New `agentprof-observability` crate — rejected: YAGNI, single-shot work. Pure scattered changes — rejected: PII hash duplication / TUI guard logic bloats main.rs. |
| **D-9** | **New direct deps:** `sha2 = "0.10"` (workspace; only core uses it for hash_path) + `tracing-appender = "0.2"` (workspace; only cli uses it for non-blocking file writer + rolling). | Custom SHA via `std::hash::Hasher` — rejected: nonstandard length / no `DefaultHasher` stability guarantees across Rust versions. `tracing-subscriber::fmt::writer::BoxMakeWriter` only — rejected: would block the cli main thread on disk writes. |
| **D-10** | **CLI flag placement:** `--log-level` and `--log-file` at the top-level `Cli` struct with `#[arg(global = true, env = "...")]`, NOT per-subcommand. | Per-subcommand — rejected: same arg-ordering trap that bit `watch aggregate` in M1.6.3 (T2 review #2). |
| **D-11** | **Log file rotation:** `tracing-appender::rolling::daily(dir, "agentprof.log")`, retention left to user. No intra-session rotation. | Hourly rotation — rejected: too noisy. Size-based — rejected: complexity > benefit for a developer tool. |
| **D-12** | **Worker-thread guard:** the `tracing_appender::non_blocking` `WorkerGuard` is returned from `init_tracing` and held by `main` until the process exits (via the returned `TracingHandle` struct). Dropping mid-program would lose buffered events. | Forget about the guard — rejected: drops the last batch of events on exit. |
| **D-13** | **init failure mode:** ANY tracing init failure (subscriber install, file dir create, etc.) is **soft** — fall back to default stderr subscriber, emit one `tracing::warn!`, do NOT block CLI startup. | Hard-fail with DataError(3) — rejected: tracing is observability infra, not user data; it must never be the reason the CLI can't run. |
| **D-14** | **Span field convention:** session paths emitted as `session = %hash` (short hash, `Display`), file paths as `file = %hash`, record-counts as `events = N` / `episodes = N`. No raw PathBuf, no raw home-dir paths, no Display-formatted timestamps inside fields (those go in the span's `time` column). | Mixed conventions per call site — rejected: inconsistent grep. |
| **D-15** | **Tests:** unit tests for `hash_path` / `LogConfig` precedence; one integration test file `crates/agentprof-cli/tests/cli_tracing.rs` covering flag-level filter / file output / fallback on bad path. NO snapshot tests of trace output (timestamps make them brittle). NO new CI gate (existing test suite covers). | Snapshot trace output — rejected. Drop integration tests — rejected: precedence rules in §3 need a regression net. |
| **D-16** | **Documentation produced:** ADR-0010 (this decision set), L1 `docs/architecture.md` updates (§4 cli row + §15.4 add observability section), L2 `crates/agentprof-{core,cli}/README.md` updates, L3 rustdoc on all new pub items, CHANGELOG entries (Added: structured tracing / Changed: eprintln→tracing / Dependencies: sha2 + tracing-appender). | Skip ADR — rejected: §5.5 ADR triggers fire (≥2 alternatives + new pub API + new crate-level concern). |

---

## 3. Architecture

### 3.1 Module layout

```
agentprof-core/                                 # leaf lib
└── src/
    └── observability/                          # NEW
        ├── mod.rs                              # pub use pii::*
        └── pii.rs                              # hash_path, hash_short

agentprof-cli/                                  # binary
└── src/
    ├── main.rs                                 # MODIFIED (delegate to observability::init_tracing)
    └── observability/                          # NEW
        ├── mod.rs                              # pub use {init_tracing, TuiLogGuard, LogConfig, TracingHandle}
        ├── config.rs                           # LogConfig + Cli merge logic
        ├── init.rs                             # init_tracing() with Stderr or rolling-file writer
        └── tui_guard.rs                        # TuiLogGuard RAII: on Drop print path
```

### 3.2 Public API (M1.6.4 additions)

**agentprof-core**:

```rust
/// Stable observability helpers safe to call from any crate; zero workspace deps.
pub mod observability {
    pub mod pii {
        /// Stable 8-char hex prefix of SHA-256(path bytes). Deterministic;
        /// collision probability ≈ 50% at √(2^32) ≈ 65 536 distinct paths.
        /// For developer-tool logs that typically hold << 1 000 sessions
        /// this is sufficient. For larger fleets set `AGENTPROF_LOG_FULL_PATHS=1`.
        pub fn hash_path(p: &std::path::Path) -> String;

        /// Stable 8-char hex prefix of SHA-256(str bytes). Same trade-offs as `hash_path`.
        pub fn hash_short(s: &str) -> String;
    }
}
```

**agentprof-cli** (binary; `pub` ≡ `pub(crate)` for link visibility):

```rust
pub mod observability {
    /// Resolved + frozen log configuration. Construct via `LogConfig::resolve(&Cli)`.
    #[non_exhaustive]
    pub struct LogConfig {
        pub level_filter: tracing_subscriber::EnvFilter,
        pub writer: LogWriter,
        pub full_paths: bool,
    }

    #[non_exhaustive]
    pub enum LogWriter { Stderr, File(std::path::PathBuf) }

    impl LogConfig {
        /// Merge Cli flags + env vars + defaults into a frozen config.
        /// Never fails: `Default` config is `level=warn, writer=Stderr, full_paths=false`.
        pub fn resolve(cli: &Cli) -> Self;
    }

    /// Returned by `init_tracing`; owns the appender `WorkerGuard`. Drop on process exit.
    #[non_exhaustive]
    pub struct TracingHandle { /* WorkerGuard + bookkeeping */ }

    /// Install a tracing subscriber per `cfg`. Soft-fails to default-stderr on any error.
    pub fn init_tracing(cfg: &LogConfig) -> TracingHandle;

    /// Returned by `enter_tui_log_guard`; on Drop, prints "agentprof: trace log at <path>"
    /// to stdout iff the writer was a file. Idempotent.
    #[non_exhaustive]
    pub struct TuiLogGuard { /* internal */ }

    /// Switch the active tracing writer to a rolling file under `$XDG_STATE_HOME/agentprof/`
    /// before entering a TUI, unless `cfg.writer` already pins one. Returns RAII guard.
    pub fn enter_tui_log_guard(cfg: &LogConfig) -> TuiLogGuard;
}
```

### 3.3 Span topology (D-4)

```
cmd.<subcommand>                                 (info level, info_span!)
├── session = <hash8>                            field
├── agent = "copilot"                            field
├── adapter.discover                             (debug level)
│   ├── root = <hash8>
│   └── (event) found = N sessions
├── adapter.parse                                (debug level)
│   ├── session = <hash8>
│   └── (event) parse warnings: N
├── adapter.load_meta
├── analyzer.derive_episodes                     (debug level)
│   ├── events = N
│   └── (event) episodes = N
├── analyzer.analyze                             (debug level)
└── aggregator.group_by                          (debug level, one per --by variant)
    ├── key = "tool" | "mcp-server" | "day" | "model"
    └── sessions = N
```

Event-level: `tracing::{trace,debug,info,warn,error}!` used inline; never
emitted under `cmd.*` spans without context (the span provides correlation).

### 3.4 Data flow

```
Cli (clap parsed)
  │
  └─ LogConfig::resolve(&Cli)             # D-3 precedence: flag > env > default
       │
       └─ init_tracing(&cfg) -> TracingHandle
            │
            │ writer = Stderr (default) | File(rolling-daily, non-blocking)
            │
            └─ subscriber installed (try_init; soft-fail to default on conflict)

                Subcommand entry (run)
                  │
                  ├─ cmd::analyze::run     #[instrument(skip_all, fields(session, agent, export))]
                  │  └─ entries tracing::info!  "started analyze"
                  │
                  ├─ (TUI path) let _guard = enter_tui_log_guard(&cfg);
                  │             ratatui terminal::enter()
                  │             ... AppRunner::run() ...
                  │             terminal::leave()
                  │             ;  _guard drop → println!("agentprof: trace log at ...")
                  │
                  └─ ... eventually each lib crate's instrumented fn fires ...

Exit → main returns → TracingHandle drops → WorkerGuard flushes appender → process exit
```

### 3.5 Dependencies added

| Dep | Where | Why | License |
|---|---|---|---|
| `sha2 = "0.10"` | workspace; `[dependencies]` in `agentprof-core` only | `hash_path` impl | MIT OR Apache-2.0 ✅ already allowed |
| `tracing-appender = "0.2"` | workspace; `[dependencies]` in `agentprof-cli` only | non-blocking rolling-file writer | MIT ✅ already allowed |

Transitive: `digest`, `block-buffer`, `crypto-common`, `generic-array`,
`typenum`, `cpufeatures` (sha2's standard chain). All MIT or
MIT-OR-Apache-2.0; already allowed by `deny.toml`. No new license
allowlist entries required.

### 3.6 Existing wiring touched

- `agentprof-cli/src/main.rs` `init_tracing()` — DELETED in favor of
  `observability::init_tracing`. Top-level `Cli` struct gains 2 fields.
- `agentprof-cli/src/cmd/{analyze,list,aggregate,watch}.rs` — `run` fns
  gain `#[instrument(skip_all, fields(...))]` and TUI variants call
  `enter_tui_log_guard(&cfg)` immediately before the panic-safe enter.
- `agentprof-core/src/analyzer/mod.rs` `analyze` + 4 `aggregate/group_by_*`
  fns gain `#[instrument(skip_all, fields(...))]`. Their existing
  `assert_eq!` length-mismatch guards stay; tracing adds context.
- `agentprof-adapters/src/copilot/{parser, paths}.rs`
  `parse_events_jsonl` + `discover_sessions` gain `#[instrument]`.
- 14 `eprintln!` call sites identified in §1 → `tracing::warn!` / `info!` /
  `error!` per D-7.

---

## 4. CLI surface changes

### 4.1 New global flags (D-3, D-10)

```
agentprof [GLOBAL] <COMMAND> [...]

  --log-level <LEVEL>    Tracing level filter (trace|debug|info|warn|error)
                         or full env-filter syntax (e.g. "warn,agentprof_core=debug").
                         Default: env AGENTPROF_LOG, then "warn".
                         [env: AGENTPROF_LOG_LEVEL]

  --log-file <PATH>      Write trace events to this file instead of stderr.
                         Use "-" to force stderr (overrides TUI auto-redirect).
                         Default: stderr for non-TUI; auto under
                         $XDG_STATE_HOME/agentprof/agentprof.log for TUI/watch.
                         [env: AGENTPROF_LOG_FILE]
```

(Existing `AGENTPROF_LOG` env var keeps working; merged with `--log-level`
via D-3 precedence.)

### 4.2 New env vars

| Var | Default | Effect |
|---|---|---|
| `AGENTPROF_LOG` | unset (= "warn") | Existing; tracing-subscriber env-filter syntax. |
| `AGENTPROF_LOG_LEVEL` | unset | Mirror of `--log-level`; flag wins. |
| `AGENTPROF_LOG_FILE` | unset | Mirror of `--log-file`; flag wins. |
| `AGENTPROF_LOG_FULL_PATHS` | unset (= hashed) | If `1`, emit raw paths instead of `hash_path()`. |

### 4.3 TUI exit message (D-2)

When `cmd::{analyze --export tui, watch, watch aggregate}` exits cleanly
and the writer is `File`, before `main` returns:

```
agentprof: trace log at /home/<user>/.local/state/agentprof/agentprof.log
```

(Goes to stdout, after terminal::leave(), so no alt-screen risk.)

---

## 5. Error handling (D-13)

| Failure | Behavior |
|---|---|
| `EnvFilter::try_new(level_str)` fails | log warn once + fall back to `EnvFilter::new("warn")`; continue |
| Log file dir create fails | warn once + fall back to stderr writer; continue |
| File `OpenOptions::create + append` fails | warn once + fall back to stderr; continue |
| `tracing::subscriber::set_global_default` returns error (already initialized) | swallow; this is fine on subsequent inits |
| `enter_tui_log_guard` fails to switch writer | warn once + use the previously-installed subscriber as-is |

The cardinal rule: **tracing init never blocks CLI startup** (D-13).

---

## 6. Testing strategy (D-15)

### 6.1 Unit tests

`crates/agentprof-core/src/observability/pii.rs`:
- `hash_path_is_deterministic` — same input → same output.
- `hash_path_distinguishes_inputs` — different paths → different outputs.
- `hash_path_handles_invalid_utf8` — `OsString` from bytes path, no panic.
- `hash_path_handles_empty` — `PathBuf::new()`, returns a stable string.
- `hash_short_is_deterministic`.

`crates/agentprof-cli/src/observability/config.rs`:
- `flag_wins_over_env_level`
- `env_wins_when_flag_absent`
- `default_is_warn`
- `invalid_level_string_falls_back_to_warn`
- `xdg_state_path_resolution` (with `XDG_STATE_HOME` set and unset)
- `dash_log_file_forces_stderr_even_in_tui`

### 6.2 Integration test: `crates/agentprof-cli/tests/cli_tracing.rs` (NEW)

| Test | Assertion |
|---|---|
| `analyze_log_level_debug_shows_cmd_span` | stderr contains `"cmd.analyze"` (span name) |
| `analyze_log_level_warn_hides_debug_events` | stderr does NOT contain `"adapter.parse"` |
| `aggregate_log_file_writes_to_path` | given `--log-file /tmp/test.log`, file contains expected span names after command |
| `analyze_log_file_invalid_path_fallback_to_stderr` | invalid `/no-such-dir/x.log` → stderr contains both the original output AND a fallback warning |
| `tui_exit_prints_log_path_hint` | running `analyze --export tui` (with TTY mock) and exiting prints `"agentprof: trace log at"` on stdout |

### 6.3 Manual smoke (D-15)

Spec §12-equivalent items:

```
1. agentprof watch --log-level debug
   → ~/.local/state/agentprof/agentprof.log is created.
   → Edit events.jsonl in another terminal.
   → Inside the log: see `cmd.watch` span nested with `adapter.parse`.
   → Quit (q) → stdout shows "agentprof: trace log at <path>".

2. AGENTPROF_LOG_FULL_PATHS=1 agentprof analyze --log-level debug 2>&1 | grep session
   → Output mentions actual session path (not hash).

3. agentprof aggregate --log-file - --by tool --log-level info > /dev/null
   → stderr shows `cmd.aggregate` info span (because of "-" forcing stderr
     even though pipe is non-TTY).
```

### 6.4 Out of test scope

- `tracing-appender`'s rotation correctness (upstream).
- OTLP exporter (D-6 — M1.6.4 doesn't ship one).

---

## 7. Migration plan

### 7.1 Sequenced tasks (informs writing-plans)

1. **T0** — branch + baseline gates green (already done; `feat/m1.6.4-tracing`).
2. **T1** — `agentprof-core::observability::pii` + tests; add `sha2` workspace dep.
3. **T2** — `agentprof-cli::observability::{config,init,tui_guard}` + tests; add `tracing-appender` workspace dep + add `--log-level`/`--log-file` global flags.
4. **T3** — wire `enter_tui_log_guard` into `cmd::analyze::run_tui` + `cmd::aggregate::run_tui_for_aggregate` + `cmd::watch::enter_and_run`; add `cli_tracing.rs` integration tests.
5. **T4** — convert ~14 existing `eprintln!` to `tracing::*` (per D-7); add `#[instrument]` on cmd::{analyze,list,aggregate,watch}::run and storage-irrelevant pub fns (per D-4 layer 1).
6. **T5** — add `#[instrument]` on adapter::{discover_sessions, parse_events_jsonl, load_meta} and on core::analyzer::{derive_episodes, analyze} + 4 aggregate::group_by_*; **all session-path fields go through `hash_path` per D-5**.
7. **T6** — docs L1 + L2 + L3 sync + ADR-0010 + CHANGELOG.

### 7.2 Backward compatibility

- `AGENTPROF_LOG` keeps working (existing scripts unaffected).
- Default stderr writer + `warn` level → behavior change: previous `eprintln!` warnings now appear via tracing's fmt format (timestamp + level + module). User-visible diff: **format** changes, not the content. Document in CHANGELOG.
- Adding global `--log-level` / `--log-file` is purely additive at the clap level (no conflict with existing per-subcommand args).

### 7.3 Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| User scripts grep stderr for `"warning:"` prefix | Medium | tracing-subscriber `fmt` will produce `"WARN agentprof_cli::cmd::analyze: ..."`. Document in CHANGELOG that the prefix changes; provide example `agentprof ... 2>&1 \| grep WARN`. |
| TUI mode users surprised by log file in unfamiliar XDG path | Medium | Explicit `println!` on TUI exit shows the path; explicit doc in README + CONTRIBUTING. |
| sha2 + tracing-appender add ~150 KB to final binary | Low | acceptable for a developer tool; smaller than current ratatui surface. |
| Hash collisions surface in support-style debugging | Very low (D-5) | doc rustdoc note; `AGENTPROF_LOG_FULL_PATHS=1` workaround. |

---

## 8. Out of scope (post-M1.6.4)

- OTLP exporter wiring (D-6; pushed to Phase 2 / `agentprof ingest-otlp` companion work).
- `--log-format json` (M1.6.5+).
- Per-PR clippy lint enforcing PII hash (CI grep is the M1.6.4 mechanism).
- Linking tracing spans to the SQLite store (storage-side work).
- Migrating `println!` from `cmd::list` table renderer (those are user-expected output per D-7).
- TUI footer banner for tracing events (would need M1.6.6+ tracing-layer integration).

---

## 9. Acceptance criteria (informs Final Acceptance in writing-plans)

A reviewer should be able to verify ALL of:

- [ ] All 4 hard gates green: `cargo fmt --check` / `cargo clippy --workspace --all-targets --all-features -- -D warnings` / `cargo test --workspace --all-features` / `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace`.
- [ ] `grep -rn 'eprintln!' crates/*/src/` returns 0 hits in `cmd::*` files (some `eprintln!` in non-prod paths like `#[cfg(test)]` are OK).
- [ ] `AGENTPROF_LOG=debug agentprof analyze --session <path>` produces stderr with `cmd.analyze` span name visible.
- [ ] `agentprof watch` creates `$XDG_STATE_HOME/agentprof/agentprof.log` and reports the path on quit.
- [ ] `cargo tree --workspace --depth 1` shows `sha2` only as a dep of `agentprof-core` and `tracing-appender` only as a dep of `agentprof-cli`.
- [ ] `deny.toml` unchanged (D-9: new licenses already allowed).
- [ ] CHANGELOG `[Unreleased]` has the 3 expected sections (Added / Changed / Dependencies).
- [ ] ADR-0010 created with all 16 D-decisions.
- [ ] All `pub` items in new modules have rustdoc + `# Examples`.
- [ ] `cli_tracing.rs` integration tests all pass.

---

## 10. Open questions

None at this stage — all 16 D-decisions are explicitly locked above with
rejected alternatives. Future deltas should be captured as new ADRs that
explicitly Supersede their relevant D-decisions in ADR-0010.

---

## 11. Cross-references

- `docs/architecture.md` §3 (crate boundaries) — modules to add to §4.
- `docs/architecture.md` §8 (CLI 协议) — `--log-level`/`--log-file` to add.
- `docs/architecture.md` §15.4 — new `observability` feature flag if any (none planned for M1.6.4).
- `docs/internals/adr-0009-watch-runner-and-notify.md` — same-style ADR sibling to follow.
- `tasks/ROADMAP.md` §2.2 — M1.6.4 row needs to flip from "currently undefined" to this spec on landing.
- `tasks/001-mvp-agent-token-profiler.md` §10 — same.
- `crates/agentprof-cli/src/cmd/watch.rs:253` rustdoc — the existing T2-fixup rationale for `debug` instead of `warn` in the debouncer callback foreshadows D-2 (TUI alt-screen safety). M1.6.4 generalizes that one fix into a system-wide guarantee.

---

## 12. Spec self-review notes

(After writing, per brainstorming checklist step 7.)

- ✅ No "TBD" / "TODO" / vague requirements remaining.
- ✅ §2 D-decisions ↔ §3-§5 design ↔ §9 acceptance criteria are internally consistent.
- ✅ Scope: single spec, focused on M1.6.4 only; OTLP exporter explicitly out of scope (D-6).
- ✅ Ambiguity check: each `--log-level` / `--log-file` / `AGENTPROF_LOG_*` precedence rule is explicit in D-3; TUI auto-redirect override rule is explicit in §4.1 (`-` forces stderr).
- ✅ Lib leaf rule preserved: `agentprof-core` only depends on `sha2` (a leaf crate itself).
- ✅ No new workspace crate (D-8 explicitly rejects that).
