# ADR-0019: Hybrid storage mode (cache vs store)

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: M2.1 Phase 2 (SQLite persistence)
- **Related**: ADR-0018 (`SessionDataSource` trait — the read-path that
  this storage backs); M2.1 spec
  `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md` §5–§6

## Context

M2.1 introduced a SQLite layer in `agentprof-storage`. The persistence
**policy** — what guarantees we make about durability and what guarantees
we make about *not* needing to back the file up — is orthogonal to the
schema and has user-visible consequences.

Two natural ends of the spectrum exist:

- **Pure cache** — the SQLite file is a derivative of the on-disk session
  jsonl logs. Deleting it is always safe; the next `analyze` /
  `db ingest` reconstructs everything. Auto-pruning is reasonable.
- **Pure store** — the SQLite file is the authoritative record. The
  user is expected to back it up. Deleting it is data loss.

Each implies a different default XDG location, a different attitude to
`auto_prune_days`, and a different communication contract with the user.

M2.1 needs to pick one — but the two audiences split cleanly:

- **Individual dogfooders** (current v0.1.x usage shape): want
  near-zero setup, want "rm it and it rebuilds", do not want to start
  doing rsync of an opaque sqlite file.
- **Teams or long-running dogfood programs**: want a durable
  multi-month archive, will absolutely set up backups, do not want
  auto-prune silently dropping last quarter's audit data.

## Decision

**Hybrid.** Ship both modes in M2.1; default to **cache**; let users
opt into **store** with a single config edit.

```toml
# ~/.config/agentprof/config.toml
[storage]
mode = "store"          # default: "cache"
# path = "/custom/db.sqlite"   # optional override
auto_prune_days = 30    # cache mode only; ignored in store mode
```

| Aspect                  | Cache (default)                          | Store (opt-in)                              |
| ----------------------- | ---------------------------------------- | ------------------------------------------- |
| Config trigger          | absent or `mode = "cache"`               | `mode = "store"`                            |
| Default path            | `$XDG_CACHE_HOME/agentprof/cache.sqlite` | `$XDG_DATA_HOME/agentprof/store.sqlite`     |
| Typical Linux path      | `~/.cache/agentprof/cache.sqlite`        | `~/.local/share/agentprof/store.sqlite`     |
| `rm <path>` impact      | safe — rebuilt on next read              | **data loss** — pruned jsonl history gone   |
| `auto_prune_days`       | enabled (default 30)                     | disabled (any value is ignored)             |
| User backup expected    | no                                       | yes (rsync / restic / similar)              |
| `analyze` overwrite     | latest wins (never "append")             | latest wins (never "append")                |
| Suitable for            | individuals dogfooding                   | teams / multi-month audit trail             |

CLI overrides (per-invocation, do not change config):

- `--storage-path <PATH>` — beats both `[storage].path` and the XDG
  default. Useful for `db export` to an alt file, CI / multi-tenant
  hermetic tests.
- `--no-cache` — skip storage entirely; fall back to single-path
  adapter reads. Debugging aid.
- ❌ `--storage-mode {cache,store}` — **not** added. Mode is a
  "set and forget" property; flipping it ad-hoc would invite the
  destructive direction by accident. Switching is a deliberate
  config edit, not a flag.

## Considered options

### A. Pure cache only (rejected)

Simplest possible story: file under `$XDG_CACHE_HOME`, safe to delete,
auto-prune always on. Cannot serve the "team archive" / "multi-month
dogfood" use case at all — and that case is real (the spec §1.2
maintainer-environment audit covers a user who already keeps 6+ months
of Copilot session-state by hand). Forcing them onto an external solution
(or telling them to disable auto-prune by setting it to a huge value)
externalises complexity that the storage layer is already 90% set up to
handle.

### B. Pure store only (rejected)

Would surface "you have a new file under `$XDG_DATA_HOME` and you need
to back it up or you'll lose history" to every first-time user — for a
tool whose **default** invocation just wants a quick `analyze` and exit.
Pessimises the common case for the benefit of the rare one.

### C. Hybrid (chosen)

Two-way switch, single config knob, two XDG paths. The asymmetry between
the two modes is real (one is delete-safe, one is data loss) and the
right place to encode that asymmetry is **at the config edge** rather
than inside every cli call site.

## Rationale

- **Cache → Store upgrade is additive** (set `mode = "store"`, optionally
  copy the existing cache file across, done) — no schema change, no data
  migration, no breakage of in-flight commands.
- **Reverse direction is destructive** (`store` → `cache` would activate
  auto-prune on a file the user expected to keep forever). Keep that
  direction behind a deliberate config edit, not a flag — see why
  `--storage-mode` was rejected above.
- **XDG split** mirrors the semantic difference users already understand:
  `$XDG_CACHE_HOME` = "OS may evict, you can `rm -rf` it";
  `$XDG_DATA_HOME` = "yours forever, back it up".
- **Most users start with cache.** A small minority (teams, audit) opt
  into store. Defaults should favour the majority.
- **`auto_prune_days` belongs to cache.** Pruning a "store" would
  contradict the store's promise. The config field is silently ignored
  in store mode (not an error — switching back to cache mode would
  immediately re-activate it).

## Consequences

### Positive

- Defaults preserve "rm it and it rebuilds" semantics — first-time
  users can't shoot themselves in the foot.
- Power users get an authoritative archive with a single config line.
- Both modes share the same schema, same DDL, same migrations
  (`crates/agentprof-storage/migrations/001_initial.sql`) — the only
  difference is where the file lives and whether `auto_prune_days`
  is honoured.
- `agentprof db stats` surfaces the active mode + path on the first
  line, so users always know which contract they're under.

### Negative

- **Two XDG paths to maintain.** The storage crate's
  `StorageConfig::resolve_path` and the cli's
  `data_source_factory::build_data_source` both have to branch on
  mode. Tests in `agentprof-storage/tests/config_resolve.rs` cover
  both branches.
- **`auto_prune_days` is silently ignored in store mode.** A user who
  flips to `store` without removing `auto_prune_days = N` from their
  config will see no warning. The store-mode contract documents this
  as "any value is ignored"; we considered a startup warning but
  decided against the noise — the prune simply never runs.
- Future docs / FAQ has to explain the cache-vs-store distinction;
  it's the single most likely point of user confusion in v0.2.0.

### Neutral

- `analyze` always overwrites (latest-wins) regardless of mode. There
  is no "append history" mode in M2.1; multiple analyses of the same
  session keep only the most recent `AnalysisReport`. If a user wants
  per-analysis history they can `db export` and snapshot externally.
  Per-analysis history is on the post-v0.2.0 backlog (spec §12).

## References

- Spec: `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`
  §5 (final schema DDL) / §6 (this mode matrix) / §6.2 (config schema) /
  §6.3 (cli override flags)
- ADR-0018 — `SessionDataSource` trait + dual-path read semantics
  (the abstraction this storage backs)
- `docs/architecture.md` §9 (final SQLite schema, normative) / §10
  (`[storage]` config block)
- Implementation:
  - `crates/agentprof-storage/src/config.rs` — `StorageConfig` /
    `StorageMode` / `PartialStorageConfig` (XDG resolution + merge)
  - `crates/agentprof-storage/src/db.rs` — `Db::open` + pragmas
  - `crates/agentprof-storage/src/admin.rs` — `prune_before` honours
    cache-mode semantics
  - `crates/agentprof-cli/src/data_source_factory.rs` — mode-aware
    factory for the dual-path read
