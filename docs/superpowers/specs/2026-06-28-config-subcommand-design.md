# Config Subcommand — `agentprof config` (L-4)

| Field | Value |
|---|---|
| Date | 2026-06-28 |
| Status | Approved — entering writing-plans |
| Author | L-4 closure (`tasks/ROADMAP.md` §6.1) |
| Triggered by | L-4 (the only planned CLI surface still unimplemented) |
| Touches ADRs | candidate **ADR-0027** (config subcommand + `show` effective-value semantics + path-resolution unification) |
| Target release | v0.4.0 (minor — new subcommand, additive) |

## 1. Problem statement

`agentprof` already loads `~/.config/agentprof/config.toml`: the
`[storage]` / `[otlp]` / `[serve]` blocks are modelled by
`agentprof_cli::config::PartialConfig` and consumed by the `ingest-otlp`
and `serve` commands. But there is **no CLI surface to manage that file**:
a user cannot ask where the file lives, what configuration is actually in
effect, edit it, or generate a starter template. This is tracked
limitation **L-4 (MEDIUM)** — the last planned subcommand not yet shipped.

Two latent problems compound this:

1. **doc-vs-reality contradiction.** `docs/architecture.md` §10 documents a
   `config.toml` schema that includes `[paths]`, `[tokenizer]`, and
   `[pricing]` blocks. But `PartialConfig` uses `#[serde(deny_unknown_fields)]`
   and only models `storage` / `otlp` / `serve` — so a user who copies the
   documented `[paths]` block gets a **parse failure**, not the advertised
   behavior. Those three blocks are un-wired paper schema.
2. **duplicated path resolution.** The `$AGENTPROF_CONFIG` → XDG
   `config_dir()/agentprof/config.toml` lookup is implemented **twice**
   (`cmd/ingest_otlp.rs:375`, `cmd/serve/mod.rs:309`).

This spec defines and implements the `config` subcommand around the
**real, wired** blocks, unifies the path lookup, and fixes the schema
contradiction.

## 2. Scope

### In scope

- New bin module `agentprof-cli/src/cmd/config.rs` (anyhow, bin-only) with
  four actions under `agentprof config <ACTION>`:
  - `path` — print the effective config-file path + existence marker.
  - `show` — print the **effective** configuration (built-in defaults
    merged with file overrides), each value annotated `(default)` or
    `(from file)`.
  - `edit` — open the file in `$VISUAL` / `$EDITOR`; if absent, write the
    starter template first, then open.
  - `init [--force]` — write a commented default template to the path.
- New public helper `agentprof_cli::config::resolve_config_path() ->
  Option<PathBuf>` (single source of truth for `$AGENTPROF_CONFIG` → XDG).
  `ingest-otlp` and `serve` refactored to call it (dedup).
- Starter template (inline `const`) covering `storage` / `otlp` / `serve`
  blocks with comments + default-value examples.
- `clap` wiring: `ConfigCmd { #[command(subcommand)] action: ConfigAction }`
  on the main command enum.
- Exit-code mapping per project convention (§5).
- **ADR-0027** (effective-value `show` semantics + source annotation +
  path-resolution unification + the decision to scope to wired blocks).
- L1 + L2 doc sync:
  - **Fix architecture §10** — keep only the wired `storage`/`otlp`/`serve`
    schema; move `[paths]`/`[tokenizer]`/`[pricing]` into an explicit
    "🚧 planned, not yet wired (rejected by `deny_unknown_fields`)" note.
  - architecture §8 CLI table + `config` row (drop "🚧 规划中").
  - `crates/agentprof-cli/README.md` + root `README.md` add `config`.
  - `tasks/ROADMAP.md` L-4 → config shipped.
  - `CHANGELOG.md`.
- Tests: cmd unit (path resolution, source annotation) + `assert_cmd`
  integration (all four actions, success + error exit codes), isolated via
  `tempdir` + `$AGENTPROF_CONFIG`.

### Out of scope

- **Wiring `[paths]` / `[pricing]` / `[tokenizer]`** so adapters / tokenizer
  actually consume them — a separate feature; this spec only documents them
  as planned and makes the *real* blocks manageable.
- **`config set <key> <value>`** (mutating individual keys from the CLI) —
  YAGNI; `config edit` covers file mutation. `#[non_exhaustive]` on
  `ConfigAction` keeps it addable later.
- Merging **CLI flags** into `show` output — `show` is a standalone command
  with no per-subcommand flags; it reflects file + built-in defaults only.
- Secret redaction in `show` (e.g. `[otlp].listen_token`) — see §3 D-6.

## 3. Design decisions

- **D-1 — `show` prints *effective* values, not raw file.** Merge built-in
  defaults with file overrides and annotate each field `(default)` /
  `(from file)` by inspecting the corresponding `PartialConfig` field's
  `Some`/`None`. Rationale: answers "what is actually in effect and what did
  I set?" in one view. (Chosen over raw-`cat` and normalized re-emit.)
- **D-2 — single path resolver.** Extract `resolve_config_path()` into
  `config.rs`; `ingest-otlp` + `serve` call it. Removes two divergent
  copies; `config path`/`show`/`edit`/`init` all share it.
- **D-3 — scope to wired blocks + fix the schema doc.** `config` only knows
  `storage`/`otlp`/`serve`. The same PR corrects architecture §10 so the
  documented schema matches `deny_unknown_fields` reality (no more
  parse-failing `[paths]` advice).
- **D-4 — `edit` self-heals.** If the file is absent, write the starter
  template first, then open the editor — first-time `edit` is guided rather
  than dropping the user into an empty buffer.
- **D-5 — feature-gated blocks degrade gracefully in `show`.** When the
  `otlp` / `web` feature is not compiled into the running binary, `show`
  prints the block header with `(feature not enabled in this build)` instead
  of fabricating values.
- **D-6 — `show` does not mask secrets, but never *invents* them.** Values
  like `[otlp].listen_token` are printed only when `(from file)`; a
  `(default)` token is shown as the literal built-in default (or `""`
  when none). We do not add masking now (the file is local, user-owned);
  noted as a possible follow-up, not in scope.
- **D-7 — bin-only, anyhow.** All logic in `agentprof-cli`; no lib crate
  gains a dependency. `resolve_config_path` returns `Option` (not a
  `Result`) — absence of any config dir is a normal state, not an error.

## 4. Command behavior

### `config path`
- Resolve via `resolve_config_path()`.
- Print the path; suffix `[exists]` or `[not found]`. If `$AGENTPROF_CONFIG`
  is set, note the override source.
- Exit `0` even when the file is absent (querying the path is not an error).
- If no config dir can be determined at all (`None`) → stderr explanation,
  exit `3` (I/O / environment).

### `config show`
- Resolve path; if present, read + `parse_toml` → `PartialConfig`; if
  absent, use an empty `PartialConfig` (all defaults).
- For each wired block, compute effective values + source:
  - `[storage]` — `resolve_storage_config(&partial.storage, None)` →
    `mode` / `path` / `auto_prune_days`; source from
    `partial.storage.<field>.is_some()`.
  - `[otlp]` (feature `otlp`) — resolve against `PartialOtlpServerConfig`
    defaults; else header `(feature not enabled in this build)`.
  - `[serve]` (feature `web`) — `resolve_serve_config` defaults; else as
    above.
- Output: a `# Effective configuration (path: … [exists|not found])`
  header, then TOML-ish blocks with trailing `(default)` / `(from file)`
  per line.
- Parse failure (malformed TOML / unknown field) → friendly `ConfigError`
  to stderr with the file path, exit `2` (data error).

### `config init [--force]`
- Resolve path; create parent dir (`create_dir_all`) if needed.
- If the file exists and `--force` is absent → stderr "already exists; pass
  --force to overwrite", exit `1` (user error).
- Write the inline template; print the written path; exit `0`.
- Write failure → exit `3`.

### `config edit`
- Pick editor: `$VISUAL` then `$EDITOR`; if neither set → stderr "set
  $EDITOR or $VISUAL", exit `1`.
- If the file is absent → write template first (D-4).
- Spawn `editor <path>`, inherit stdio, wait. Propagate a non-zero editor
  status as exit `1`. Spawn failure → exit `3`.

## 5. Exit codes (project convention)

| Code | Meaning | `config` cases |
|---|---|---|
| 0 | success | path/show OK; init wrote; edit returned 0 |
| 1 | user error | init exists w/o `--force`; no `$EDITOR`/`$VISUAL`; editor non-zero |
| 2 | data error | show: config file failed to parse |
| 3 | I/O / env | cannot determine config dir; write/read/spawn failure |

## 6. Documentation impact

- **L1** `docs/architecture.md`: §8 CLI table (`config` shipped, not 🚧);
  §10 schema corrected to wired blocks + explicit "planned/un-wired" note
  for `[paths]`/`[tokenizer]`/`[pricing]`.
- **L2** `crates/agentprof-cli/README.md` (subcommand list + `config`
  section); root `README.md` quickstart mention.
- **L3** rustdoc on `cmd::config` actions + `resolve_config_path`
  (`# Examples`, `# Errors`); **ADR-0027**.
- `tasks/ROADMAP.md` L-4 row → config shipped; `CHANGELOG.md` `feat`.

## 7. Test plan

- **Unit (`cmd/config.rs` / `config.rs`):**
  - `resolve_config_path`: `$AGENTPROF_CONFIG` override wins; XDG fallback;
    `None` when no dir.
  - source annotation: `(from file)` when `PartialConfig` field is `Some`,
    `(default)` when `None`.
- **Integration (`tests/cli_config.rs`, `assert_cmd`, `tempdir` +
  `$AGENTPROF_CONFIG`):**
  - `path`: exists vs not-found markers.
  - `show`: with a file (mixed default/from-file lines), without a file
    (all defaults), and with a malformed file (exit 2).
  - `init`: fresh write (exit 0, file appears); existing w/o `--force`
    (exit 1); existing w/ `--force` (overwrite, exit 0).
  - `edit`: `$EDITOR=true` happy path (exit 0; template created when
    absent); no editor env (exit 1).
- Feature-gated `show` lines: assert `(feature not enabled in this build)`
  appears for `[otlp]`/`[serve]` when built without those features (or
  assert resolved values when built with them).

## 8. Self-review

- **Placeholders:** none. **Contradictions:** none — `show` semantics (D-1)
  consistent across §3/§4/§7; scope (§2) excludes `set`/wiring consistently.
- **Scope:** single implementation plan — four thin actions + one extracted
  helper + doc fix. No decomposition needed.
- **Ambiguity:** `edit`-absent behavior pinned (D-4: write template first);
  feature-gated `show` pinned (D-5); exit codes tabulated (§5).
