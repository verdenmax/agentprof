# ADR-0018: `SessionDataSource` trait abstraction + dual-path semantics

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: M2.1 Phase 2 (SQLite persistence + dual-path read)
- **Related**: ADR-0017 (id-namespace unification — the prerequisite that
  makes dual-path actually function); M2.1 spec
  `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md` §3.2 / §7;
  M2.2 (OTLP receiver, follow-on)

> **ADR numbering note**: spec §13 originally reserved ADR-0017 for this
> decision and ADR-0018 for hybrid storage (now ADR-0019). The M2.1 T7
> hotfix consumed ADR-0017 for the id-namespace fix (a P0 bug discovered
> in T7.2), shifting both decisions by one. The id-namespace ADR is the
> structural prerequisite to dual-path actually working, so its earlier
> numbering is correct.

## Context

M2.1 needed a unified API surface that the cli could use to read sessions
without caring whether the bytes came from the on-disk Copilot adapter,
from the SQLite cache, or — once M2.2 lands — from a live OTLP receiver.
Each cli subcommand also needs the option to fan out reads to **both** the
adapter and storage so it can:

1. Render the freshest result (adapter always wins on conflict).
2. Warn the user when the storage entry has drifted from the on-disk
   source (e.g. file was re-edited after the last `db ingest`).
3. Opportunistically re-upsert the storage entry in the background so the
   next read is hot. *(Note: prototyped in M2.1 T4.2, removed in the
   M2.1 audit followup — see the "Behaviour" subsection below and
   `crates/agentprof-cli/src/data_source.rs` module docs.
   Re-instating this safely is deferred to M2.1.1.)*

Without a single abstraction every command (`list`, `mcp-waste`, future
ones) would have to switch on the data-source kind inline, duplicate the
merge logic, and re-derive its own divergence reporting. The future OTLP
receiver would either have to bolt on a third bespoke path or pretend to
be an adapter.

The M2.1 spec resolved this with a trait at the leaf crate level (so all
implementors can depend on it without cycling back into cli) plus a single
composer that lives in cli (the only place that legitimately knows about
*multiple* data sources at once).

## Decision

A new `SessionDataSource` trait in **`agentprof-core::datasource`**
(the leaf crate — see ADR for crate dependency rules in
`docs/architecture.md` §3) with three concrete implementations:

1. **`AdapterDataSource<A: Adapter>`** in `agentprof-adapters::datasource`
   — wraps any `Adapter` impl and runs the full
   `discover → load → derive_episodes → analyze` pipeline inline.
   Two-argument constructor: `AdapterDataSource::new(adapter: Arc<A>, root: PathBuf)`
   so the discovery root is bound at construction (the adapter trait's
   `discover_sessions(&Path)` takes the root per call; the data source
   needs to remember it).
2. **`SqliteDataSource`** in `agentprof-storage::datasource` — wraps a
   shared `Arc<Mutex<Db>>` and serves reads out of the SQLite cache by
   deserialising the stored `analysis_report_json` blob (see ADR-0019 §5
   for the schema).
3. **`DualPathDataSource`** in `agentprof-cli::data_source` (the only
   non-leaf impl by design) — composes the above two and owns:
   - the **adapter-wins** conflict resolution policy;
   - the per-divergence **warning sink** (drained to stderr unless
     `--quiet`);
   - the freshness compare (`diff_fields`) that drives the warning.

   (An async **re-upsert** background fan-out was prototyped and
   removed in the M2.1 audit followup; see the rolled-back note
   under "Behaviour" below.)

The trait surface itself is intentionally minimal — `name`, `discover`,
`load_session` — matching the read shape every subcommand actually uses.

## Considered options

### A. Trait abstraction in `agentprof-core` (chosen)

- Leaf-crate placement keeps all implementors free of cyclic dependencies
  (`agentprof-storage` → `agentprof-core` ← `agentprof-adapters` is the
  existing dependency shape).
- OTLP-ready: M2.2's `OtlpReceiver` slots in as a fourth impl with no
  structural change — the cli's data-source factory just learns one more
  branch.
- Symmetric with the existing `Adapter` trait pattern (also in core).
- Testable via lightweight mock impls in cli's
  `tests/cli_dualpath.rs` — no need to spin up a real SQLite file to
  exercise merge logic.

### B. Inline duplication across cli cmds (rejected)

Every cmd would carry its own copy of "open adapter, open db, query both,
diff, pick fresher, log warning, re-upsert async". For 4 subcommands
that's 4 × ~80 LOC of nearly-identical logic — drifting versions
guaranteed within a release cycle. Also blocks M2.2: the OTLP receiver
would need to be added in 4 places at once.

### C. Decorator wrap on the existing `Adapter` trait (rejected)

Would force `SqliteDataSource` and the future `OtlpReceiver` to implement
the full `Adapter` contract (including `discover_sessions(&Path)`,
`load_session(&SessionRef) -> RawSession<E>`, the parametric event type
`E`, etc.) even though those concepts don't apply — storage doesn't have
events any more, just a serialised `AnalysisReport`. The mismatch is
worse than the duplication that would result.

## Rationale

- **Leaf-rule compliance**: the trait lives where any future implementor
  can implement it without dragging cli/storage/adapter into each other.
- **OTLP-ready**: pre-shapes the surface for M2.2 with zero refactor cost.
- **Symmetry with `Adapter`**: same pattern, same crate, same idioms.
- **Testability via mocks**: tests in T7.2 (`tests/cli_dualpath.rs`)
  construct stub `SessionDataSource` impls to exercise every branch of
  the merge — silent / warn / write-through / quiet — without touching
  the filesystem.
- **Single point-of-truth for "what counts as a session source"**: any
  future change to discovery semantics (e.g. pagination, push-mode for
  OTLP) is one trait edit + downstream type-check fan-out.

## Dual-path semantics codified

The cli composer (`DualPathDataSource`) encodes the following invariants:

- **Conflict resolution: adapter wins.** The on-disk session is always
  the authority. If `diff_fields` reports any divergence with the
  storage entry, the cli emits **one** human-readable warning line per
  diverged session to stderr (
  `agentprof: warn: session <id>: N fields differ (adapter newer)`),
  suppressed wholesale by the global `--quiet` flag. Structured
  `tracing` events at `WARN` level fire regardless of `--quiet`.
- **Async re-upsert via `std::thread::spawn`** *(prototyped, then
  removed in M2.1 audit followup, 2026-06-10).* The original design
  spawned a detached background thread to refresh the storage entry
  after divergence. The audit caught two problems: (a) the CLI
  factory never wired any callback through, leaving the surface as
  dead production code, and (b) a `std::thread::spawn` at the tail
  of a one-shot CLI invocation is killed when the process exits —
  the cache refresh almost never lands. Proper async refresh
  (`join`-on-exit, or in-process synchronous flush after `discover`)
  is deferred to **M2.1.1**. The `ReUpsertFn` type alias,
  `new_with_reupsert` constructor, `re_upsert` field, callback fan-
  out in `merge_refs`, and accompanying test were removed in the
  audit-followup PR. The dual-path source still records divergence
  warnings to stderr; only the background-rewrite path was deleted.
- **Id-namespace unification (ADR-0017) is the prerequisite.** Without
  matching `SessionRef.id` between adapter discovery and storage rows,
  the inner join in `merge_refs` is always empty and the entire
  dual-path mechanism silently no-ops. ADR-0017 was written as a
  hotfix after T7.2 caught this; it must remain a hard invariant for
  the trait abstraction to function.

## Consequences

### Positive

- Single point-of-truth for "what is a session source" — future
  receivers (OTLP, replay-from-recording, in-memory test fixture) slot
  in as additional trait impls with no cli refactor.
- Existing subcommands consume the trait via a single
  `build_data_source(...)` factory in cli (`data_source_factory.rs`),
  so adding the dual-path was a per-cmd 2-line change.
- Mockable: cli's dual-path integration tests use stub impls of the
  trait rather than spinning real SQLite + filesystem fixtures for
  every assertion.
- Symmetric with `Adapter` — easy to teach, easy to extend.

### Negative

- One extra layer of abstraction between cli subcommands and the
  underlying corpus. New contributors have to read both the adapter
  trait and the data-source trait to follow a read end-to-end.
- The cli now owns the composer (it was a free-function pattern in
  earlier sketches). This is the right home, but it means `cli` grew
  a small `data_source` + `data_source_factory` module pair that has
  to be kept in sync with the trait surface.

### Neutral

- `aggregate` is **not** dual-path-wired in M2.1 (see spec drift
  table). It needs `Episodes` data which doesn't currently live in
  `AnalysisReport`; hoisting that is M2.1.1's job. Users running
  `agentprof aggregate ...` therefore see no speed-up from the SQLite
  cache yet — they go through the adapter path on every invocation.
  This limitation is documented in `docs/architecture.md` §8 and
  `crates/agentprof-cli/README.md`.

## References

- Spec: `docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`
  §3.2 (trait shape) / §7 (read-path lifecycle) / §10.3 (async re-upsert)
- ADR-0017 — id-namespace unification (the structural prerequisite)
- ADR-0019 — hybrid storage mode (the cache-vs-store decision this trait
  abstracts over)
- Implementation:
  - `crates/agentprof-core/src/datasource.rs` (trait definition)
  - `crates/agentprof-adapters/src/datasource.rs` (`AdapterDataSource`)
  - `crates/agentprof-storage/src/datasource.rs` (`SqliteDataSource`)
  - `crates/agentprof-cli/src/data_source.rs` (`DualPathDataSource`
    composer)
  - `crates/agentprof-cli/src/data_source_factory.rs` (single
    composition seam)
