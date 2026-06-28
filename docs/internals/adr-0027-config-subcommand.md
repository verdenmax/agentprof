# ADR-0027 — `agentprof config` subcommand (`path` / `show` / `edit` / `init`)

**Status:** Accepted (2026-06-29)
**Supersedes:** —
**Superseded by:** —
**Owner:** `agentprof-cli` crate (`cmd::config` + `config::resolve_config_path`).
**Related:** ADR-0019 (hybrid storage mode — the `[storage]` block `show` renders), ADR-0021 / ADR-0022 (OTLP receiver — the `[otlp]` block + its prior duplicated path lookup), ADR-0024 (web dashboard — the `[serve]` block + its prior duplicated path lookup).

## Context

`agentprof` already loads `~/.config/agentprof/config.toml` (or
`$AGENTPROF_CONFIG`): the `[storage]` / `[otlp]` / `[serve]` blocks are
modelled by `agentprof_cli::config::PartialConfig` and consumed by the
`ingest-otlp` and `serve` commands. But there was **no CLI surface to manage
that file** — a user could not ask where it lives, what configuration is
actually in effect, edit it, or generate a starter template. This was tracked
limitation **L-4 (MEDIUM)** — the last planned subcommand not yet shipped
(`tasks/ROADMAP.md` §6.1).

Two latent problems compounded L-4:

1. **doc-vs-reality contradiction.** `docs/architecture.md` §10 advertised a
   `config.toml` schema with `[paths]`, `[tokenizer]`, and `[pricing]` blocks.
   But `PartialConfig` uses `#[serde(deny_unknown_fields)]` and only models
   `storage` / `otlp` / `serve` — so a user copying the documented `[paths]`
   block got a **parse failure**, not the advertised behavior. Those three
   blocks are un-wired paper schema.
2. **duplicated path resolution.** The `$AGENTPROF_CONFIG` → XDG
   `config_dir()/agentprof/config.toml` lookup was implemented **twice**
   (`cmd/ingest_otlp.rs`, `cmd/serve/mod.rs`).

This ADR codifies the design now shipped: a four-action `config` subcommand
scoped to the real wired blocks, a single path resolver, and the schema-doc
fix. Spec: `docs/superpowers/specs/2026-06-28-config-subcommand-design.md`;
plan: `docs/superpowers/plans/2026-06-28-config-subcommand.md`.

## Considered options

### What does `config show` print?

- **Effective values + source annotation** (chosen, D-1). Merge built-in
  defaults with file overrides; tag each line `(default)` / `(from file)`.
  Answers "what is in effect AND what did I set?" in one view, and reuses the
  real per-block resolvers so the displayed defaults cannot drift from runtime
  behavior.
- **Raw `cat`** (rejected). Shows only what the user typed; cannot answer
  "what default am I getting?" and leaks nothing about effective behavior.
- **Normalized re-emit** (rejected). Round-tripping through serde loses the
  default-vs-set distinction and invents a canonical form the user never wrote.

### How is the config path resolved across commands?

- **One `resolve_config_path()` helper** (chosen, D-2). Single source of truth
  in `config.rs`, shared by `config`, `ingest-otlp`, and `serve`.
- **Per-command copies** (rejected — the status quo that caused the two
  divergent lookups above).

## Decisions

### D-1: `config show` prints *effective* values with source annotation

`show` merges built-in defaults with file overrides and annotates each field
`(default)` or `(from file)` by inspecting the corresponding `PartialConfig`
field's `Some` / `None`. It **reuses the real resolvers**
(`resolve_storage_config` for `[storage]`, `OtlpServerConfig::from_partial`
for `[otlp]`) so the shown defaults are exactly what the runtime would use and
cannot silently drift. Only `[serve]`'s three defaults (`bind` /
`interval_default` / `auto_open`) are inlined, because no public
partial-only resolver exists for that block; they mirror `serve/mod.rs`. A
file present but malformed → exit 2 (data error); `show` deliberately does
**not** call `validate()`, so an invalid-but-parseable effective config is
displayed for the user to fix rather than rejected.

### D-2: Unified `resolve_config_path()`

`agentprof_cli::config::resolve_config_path() -> Option<PathBuf>` is the single
`$AGENTPROF_CONFIG` → XDG `config_dir()/agentprof/config.toml` lookup. The two
prior copies in `ingest-otlp` and `serve` were removed and now call it. It
returns `Option` (not `Result`): the absence of any config dir is a normal
state, not an error, until an action actually needs to write/read.

### D-3: Scoped to the wired `storage` / `otlp` / `serve` blocks

`config` only knows the three blocks `PartialConfig` actually models. The same
change fixes the architecture §10 schema-vs-`deny_unknown_fields` contradiction:
the canonical TOML example now lists only the wired blocks, and
`[paths]` / `[tokenizer]` / `[pricing]` are demoted to an explicit "🚧 planned,
not yet wired — currently rejected by `deny_unknown_fields`" note. This stops
the docs from advising a block that parse-fails on load.

### D-4: `edit` self-heals; an empty editor var is treated as unset

`edit` prefers `$VISUAL`, then `$EDITOR`. An **empty** value (`VISUAL=`) is
treated as unset and falls through, rather than spawning `""` (which would
mis-report as an I/O failure). With neither set → exit 1 (user error). When
the file is absent, `edit` writes the starter template **first**, then opens
the editor — first-time `edit` lands on a parseable, commented file instead of
an empty buffer. It never re-templates an existing file. A non-zero editor
status propagates as exit 1; a spawn failure is exit 3.

### D-5: Feature-gated blocks degrade gracefully in `show`

When the `otlp` / `web` feature is not compiled into the running binary, `show`
prints the block header with `(feature not enabled in this build)` instead of
fabricating values, so the output always reflects the actual binary.

### D-6: No `config set`; mutation goes through `edit` (YAGNI)

There is no `config set <key> <value>`. `config edit` covers file mutation, and
the inline `CONFIG_TEMPLATE` covers first-time scaffolding. `ConfigAction`
carries a `#[non_exhaustive]` marker as a forward-compat signal — though it is
inert while the enum is private (a new variant still requires updating `run`'s
`match`). `init` writes a template with only `[storage]`
active (`[otlp]` / `[serve]` commented) so the file parses in any feature
build; it refuses an existing file without `--force` (exit 1).

## Exit codes (project convention)

| Code | Meaning | `config` cases |
|---|---|---|
| 0 | success | `path` / `show` OK; `init` wrote; `edit` returned 0 |
| 1 | user error | `init` exists w/o `--force`; no `$EDITOR` / `$VISUAL`; editor non-zero |
| 2 | data error | `show`: config file fails to parse or a block fails to resolve |
| 3 | I/O / env | cannot determine config dir; write / read / spawn failure |

## Consequences

**Positive:**

- Closes L-4: the last planned CLI surface ships; the config file is now
  discoverable (`path`), inspectable (`show`), editable (`edit`), and
  scaffoldable (`init`).
- `show` reusing the real resolvers means the documented/displayed defaults
  cannot drift from the values `ingest-otlp` / `serve` actually apply.
- One `resolve_config_path` removes two divergent lookups; future precedence
  changes happen in one place.
- The §10 schema now matches `deny_unknown_fields` reality — no more
  parse-failing `[paths]` / `[tokenizer]` / `[pricing]` advice.

**Negative:**

- `[serve]`'s three defaults are inlined in `render_serve` (no public
  partial-only resolver to reuse), so adding a `[serve]` field requires
  touching `config show` too. Documented in the rustdoc on `render_serve`.

**Neutral:**

- No SQLite migration, no new feature gate (reuses `clap-derive`); the
  feature-gated `[otlp]` / `[serve]` rendering is behind the existing
  `otlp` / `web` features.
- `[paths]` / `[tokenizer]` / `[pricing]` wiring, `config set`, CLI-flag merge
  in `show`, and secret masking in `show` are all explicitly out of scope
  (spec §2 / §3 D-6) and remain future work.

## References

- Spec: `docs/superpowers/specs/2026-06-28-config-subcommand-design.md`
- Plan: `docs/superpowers/plans/2026-06-28-config-subcommand.md`
- CLI module: `crates/agentprof-cli/src/cmd/config.rs`
- Path resolver: `crates/agentprof-cli/src/config.rs` (`resolve_config_path`)
- Schema doc fixed by this change: `docs/architecture.md` §8, §10
- L-4 tracking: `tasks/ROADMAP.md` §6.1
