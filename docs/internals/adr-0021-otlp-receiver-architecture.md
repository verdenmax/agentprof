# ADR-0021: OTLP receiver architecture

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: M2.2 milestone (OTLP receiver)
- **Related**: ADR-0017 (session-id namespace), ADR-0018 (`SessionDataSource` trait),
  ADR-0019 (Hybrid storage mode), ADR-0020 (aggregate dual-path)

## Context

M2.2 adds an OTLP receiver so agentprof can ingest live OpenTelemetry
traffic (the same protocol Anthropic Console, OTel Collector, and a
growing number of agent SDKs already emit). Until now ingestion was
strictly file-pull: each adapter reads a JSONL session log on disk.

The OTel push model is fundamentally different — long-lived gRPC/HTTP
streams, per-batch resource attributes, three signal families
(logs/metrics/traces), and emitters that may stay connected for the
entire lifetime of a session. Folding it cleanly into the existing
architecture without breaking M2.1's `SessionDataSource` contract,
without forcing every install to pay tonic/axum compile cost, and
without re-architecting storage, drives the decisions below.

The design space and trade-offs are captured in
`docs/superpowers/specs/2026-06-10-m2.2-otlp-receiver-design.md`
(referred to as **spec** below). This ADR codifies the resulting
choices so a future reader knows *why* they were made.

## Decision

Ten interlocking decisions, summarized first then expanded:

| # | Decision | Spec ref |
|---|----------|----------|
| 1 | OTLP lives as a submodule of `agentprof-storage::otlp`, not a new crate | §3.1 |
| 2 | All-maximalist surface: gRPC + HTTP + Logs + Metrics + Traces + Bearer + mTLS | §1, §4 |
| 3 | OTLP does **not** implement the `Adapter` / `SessionDataSource` trait | §3.1 |
| 4 | `session.id` derivation: resource attr → `claude.session_id` → record-level attr → drop | §5.3 |
| 5 | Per-session in-memory buffer with OOM caps (16 MiB / 100k events / 5 min idle) | §5.4 |
| 6 | Flush priority: ExplicitEnd > OOM > Idle > Shutdown | §5.4 |
| 7 | `FlushSink` is a **sync** trait (matches today's rusqlite storage) | §5.5 |
| 8 | Lossy mapping when a `TypedEvent` variant cannot be expressed in M2.1 schema | §6 |
| 9 | OTLP-sourced sessions use synthetic `raw_path = "otlp://<session_id>"` | §6 |
| 10 | Entire receiver is gated behind the `otlp` Cargo feature, disabled by default | §3.1 |

### Decision 1 — Submodule under `agentprof-storage::otlp`

**Options considered**

- (a) New crate `crates/agentprof-otlp` depending on `agentprof-storage`
- (b) Submodule `agentprof_storage::otlp` (chosen)
- (c) Put it in `agentprof-cli`

**Rationale.** The receiver needs a `Db` handle for every flush; option
(a) means either re-exporting `Db` (leak) or building an indirection
trait just to dodge a crate boundary. Option (c) violates the L1 rule
that lib logic does not live in the bin crate (CLAUDE.md §3, AGENTS
§3). Option (b) keeps the dependency graph acyclic, shares the existing
`Db` type by direct import, and slots naturally under the existing
`otlp` Cargo feature on `agentprof-storage`.

### Decision 2 — Maximalist surface (gRPC + HTTP, Logs + Metrics + Traces, Bearer + mTLS)

**Options considered**

- B+B+C+C (chosen): both transports, all three signals, both auth modes
- gRPC-only / HTTP-only
- Logs-only (smallest viable surface)
- No-auth or single-auth-mode

**Rationale.** Spec §1 and §4 + explicit milestone direction: the value
of an OTel receiver is *ecosystem compatibility*. Half-implementing the
matrix forces users into "agentprof works if your collector exporter
happens to emit X" surprises. The marginal cost of adding the second
transport / second signal is mostly proto codegen + a few hundred lines
of mapper; the cost of *not* having them is users routing through a
collector just to bridge formats. mTLS is non-optional for any team
shipping OTel in production.

### Decision 3 — OTLP does **not** implement the `Adapter` trait

**Options considered**

- (a) Implement `Adapter` / `SessionDataSource` on a `OtlpAdapter`
- (b) Build a parallel `IngestPipeline` / `Router` type tree (chosen)

**Rationale.** Spec §3.1: `Adapter` is a file-pull contract (`load(path)
-> Session`). OTLP is push-stream: there is no `path`, no terminal EOF,
and a single connection multiplexes many sessions. Shoehorning would
force `Adapter` to grow a "stream mode" flag that nine in-tree adapters
do not need, and would make the trait simultaneously describe two
disjoint lifecycles. A separate `Router → SessionBuffer → FlushSink →
Db` pipeline keeps both concepts cohesive.

**Note for ADR-0018.** ADR-0018 introduced `SessionDataSource` as the
*read* side of the universe; OTLP is on the *write* side and never
needs to satisfy that trait. T10.2 will add a footnote to ADR-0018
recording this scope boundary.

### Decision 4 — `session.id` derivation chain

**Options considered**

- (a) Only honor `resource.attributes["session.id"]` (strict)
- (b) Fallback chain (chosen): resource `session.id` → resource
  `claude.session_id` → record-level `session.id` → drop with
  `MapperError::MissingResourceAttr`
- (c) Auto-generate a UUID when none is present

**Rationale.** Spec §5.3. Today's Claude Code emitter writes
`claude.session_id` on the resource, not `session.id` — option (a)
would break the most important current source. Option (c) is the worst
choice: silently inventing sessions creates ghost rows that look real
in `list` and pollute aggregates with no recovery path. (b) is strict
about *eventually* requiring an explicit id (drop, do not invent) but
forgiving about *which* attribute name carries it.

### Decision 5 — Per-session in-memory buffer with OOM caps

**Options considered**

- (a) Per-record direct writes to SQLite
- (b) Bounded in-memory buffer per session id with flush triggers (chosen)

**Rationale.** Spec §5.4. OTel emitters batch frequently and small —
per-record writes would cause severe SQLite write amplification (BEGIN
+ N inserts + COMMIT per record) and steal the writer lock from the
adapter path. Buffering coalesces a session's worth of events into one
transaction. Caps (16 MiB raw bytes / 100 000 events / 5 min idle) are
chosen to bound worst-case memory at a few hundred MiB even with many
concurrent sessions, while still being generous enough that a normal
agent session never trips OOM before its natural end.

### Decision 6 — Flush priority order: ExplicitEnd > OOM > Idle > Shutdown

**Options considered**

- (a) Idle-only flush
- (b) No OOM cap
- (c) ExplicitEnd → OOM → Idle → Shutdown (chosen)

**Rationale.** Explicit end markers (e.g. an emitter sending
`agentprof.session.end=true`) must always win — they are user
contracts. OOM must outrank Idle because hitting the byte cap means the
process is in danger now, not "stale soon". Idle outranks Shutdown
because flushing idle sessions before shutdown begins keeps shutdown
fast and reduces the work the shutdown path needs to drain. (a) and
(b) were rejected because either alone leaves a degenerate failure
mode: idle-only never frees memory under load; no-cap can OOM the
host.

### Decision 7 — `FlushSink` is a sync trait

**Options considered**

- (a) `async fn flush(&self, ...) -> Result<...>` (Tokio-native)
- (b) `fn flush(&self, ...) -> Result<...>` (sync, chosen)

**Rationale.** `agentprof-storage` is built on `rusqlite`, which is
sync; `Db::upsert_report` is a sync call (see T7.1). The router itself
runs on a Tokio worker thread, so a sync sink is just "do work on the
thread you are already on" — no `block_on` is needed. An async trait
would force every implementer to either fake-async over rusqlite or
adopt `tokio-rusqlite` (a non-trivial dependency change). The sync
choice can be revisited cleanly if storage ever moves to async — the
trait stays narrow.

### Decision 8 — Lossy mapping when M2.1 schema cannot express a `TypedEvent` variant

**Options considered**

- (a) Extend the M2.1 `events` schema to be a superset
- (b) Reject the entire batch
- (c) Lossy mapping with `tracing::warn!` (chosen)

**Rationale.** Spec §6 explicitly permits lossy mapping. (a) would
churn the schema mid-milestone and force a migration on every existing
user just so OTLP-only fields have a column. (b) is hostile: one
unknown attribute key would drop a thousand valid records. (c)
preserves M2.1 schema stability, ingests everything the existing
analyzer/TUI/aggregate paths understand, and emits a structured warning
so users can see what was dropped without it breaking ingestion.

### Decision 9 — Synthetic `raw_path = "otlp://<session_id>"`

**Rationale.** Spec §6. The `sessions.raw_path` column is `NOT NULL`
(M2.1 schema, ADR-0019), and every read path uses it as the
human-visible "where did this come from" string. OTLP sessions have no
file path. A `otlp://<id>` URI satisfies the NOT NULL constraint,
re-uses an existing column without a migration, and is greppable so
users and `list --where` filters can distinguish ingest sources.

### Decision 10 — Disabled by default behind `otlp` Cargo feature

**Options considered**

- (a) Always on
- (b) Separate binary `agentprof-otlpd`
- (c) Opt-in feature flag `otlp` (chosen)

**Rationale.** tonic + axum + rustls + dashmap together add a
noticeable build-time and binary-size cost. The majority of agentprof
users today are file-pull only and would pay that cost for nothing
under (a). (b) re-introduces a separate crate/binary which Decision 1
already rejected on cycle-risk and Db-sharing grounds. (c) is the
established pattern in the workspace — every storage/OTLP/Anthropic
dependency is already feature-gated.

## Consequences

### Positive

- OTel ecosystem compatibility out of the box (gRPC + HTTP, three
  signals).
- Shared `Db` handle — OTLP-ingested sessions flow through the same
  read APIs (`load_session`, `load_episodes`) as adapter-loaded ones.
- Consistent CLI ergonomics: `agentprof ingest-otlp` mirrors the shape
  of existing subcommands (T8.1/T8.2).
- Strict-but-forgiving `session.id` semantics avoid both silent ghosts
  and over-strict drops.
- Feature-gated build keeps default installs lean.

### Negative

- Enabling `--features otlp` materially increases dependency surface
  (tonic, axum, rustls, dashmap) and compile time.
- Lossy mapping means OTLP-native attributes outside M2.1's schema are
  dropped (with `warn!`), so OTLP ingestion is **not** byte-for-byte
  faithful — users wanting "raw OTLP archive" must run a separate
  collector.
- `otlp://<session_id>` is a user-visible token (CLI output, JSON
  exports, filters). Once shipped, this string is part of the
  compatibility surface and cannot be renamed without breaking user
  queries.
- Two write paths now coexist (adapter file-pull and OTLP push), each
  needing its own integration tests; future schema changes must
  consider both.

## Alternatives Considered

Beyond the per-decision alternatives above, three project-shaping
alternatives were rejected:

### Single proto-codegen-only crate

- **Description**: Put just the `tonic-build`-generated OTLP types into
  a tiny `agentprof-otlp-proto` crate, depended on by both storage and
  cli.
- **Rejection**: The codegen artifacts are bulky and pull tonic
  transitively; isolating them gives no real benefit because the only
  consumer is the receiver, which already lives in storage. Adds a
  crate boundary to maintain for no payoff and creates the *exact*
  cycle risk Decision 1 was designed to avoid (cli → proto-crate, cli
  → storage → proto-crate).

### OTel Collector exporter → agentprof file ingest

- **Description**: Don't build a receiver at all. Tell users to run an
  OTel Collector with a file exporter, then point an agentprof adapter
  at the resulting files.
- **Rejection**: Spec §1.2 explicitly rejects this. It pushes a
  deployment burden onto every user (run + monitor a collector + a
  filesystem pipeline), doubles latency from "live" to "visible", and
  forfeits the live `watch`-style use cases M2.2 enables.

### HTTP-only, drop gRPC

- **Description**: Implement only the HTTP/protobuf OTLP variant. It
  is simpler — no tonic, no streaming, no h2 plumbing.
- **Rejection**: Anthropic-side emitters and the OTel Collector itself
  default to gRPC. Shipping HTTP-only means the most common emitters
  silently miss us until users learn to flip an exporter config —
  precisely the "works only if your stars align" outcome Decision 2
  exists to prevent.

## Notes

- ADR-0018 (`SessionDataSource` trait) will get a clarifying footnote
  in T10.2 noting that OTLP is on the write side and intentionally
  does not implement the trait — this prevents future readers from
  thinking it is an oversight.
- The 16 MiB / 100 000 events / 5 min idle caps in Decision 5 are
  starting values; they are configurable via the `[otlp]` config block
  (T8.2) and will be revisited if real-world traffic shows the
  defaults are mis-tuned.
- The `otlp://<session_id>` scheme in Decision 9 is treated as a
  stable user-visible token. Any future change (e.g. encoding
  endpoint info) must go through SemVer + CHANGELOG and provide a
  backward-compatible read path.

## References

- Spec: `docs/superpowers/specs/2026-06-10-m2.2-otlp-receiver-design.md`
- Plan: `docs/superpowers/plans/2026-06-10-m2.2-otlp-receiver.md` (T10.1)
- ADR-0018: `SessionDataSource` trait (file-pull read contract)
- ADR-0019: Hybrid storage mode (cache vs store, `Db` handle)
- ADR-0020: Aggregate dual-path (per-session episodes column)
- OpenTelemetry Protocol Specification:
  https://opentelemetry.io/docs/specs/otlp/
