# ADR-0022: OTLP Receiver Capacity Caps + LRU Session Eviction

**Status:** Accepted (2026-06-10)
**Context:** M2.4 hardening wave (post-M2.2 audit)
**Implements:** Audit findings F2 + F3
**Supersedes:** None
**Superseded by:** None
**Related:** [ADR-0021](adr-0021-otlp-receiver-architecture.md) (parent — M2.2 OTLP receiver architecture)

## Context

The M2.2 OTLP receiver (ADR-0021) shipped with per-session OOM caps
(16 MiB / 100 000 events / 5 min idle) but no caps at two higher
layers:

1. **Per-request decoded message size** — neither tonic's
   `max_decoding_message_size` nor axum's `DefaultBodyLimit` was
   configured. A single ~4 MiB protobuf bomb decodes into hundreds of
   MiB of `TypedEvent` instances before any per-session check fires.
2. **Number of distinct sessions in `SessionRouter`** — `DashMap<SessionId, SessionBuffer>`
   grows unboundedly as new `session.id` values arrive. UUID-spam
   exhausts memory faster than the 30 s idle sweeper can evict.

Combined, these allow a small number of crafted requests to OOM the
receiver process. This ADR resolves both gaps via per-signal request
size caps (audit F2) and an LRU session eviction policy (audit F3).

This ADR also covers two secondary defenses (`session.id` length cap
at the mapper layer; new `CloseReason::CapacityEvict` variant for
observability) that fall out of the F3 fix.

## Decisions

### D-1: F3 eviction strategy — LRU evict oldest

When `SessionRouter` already holds `max_open_sessions` distinct
buffers and a new `session.id` arrives:

- The router immediately flushes the least-recently-active buffer
  with `CloseReason::CapacityEvict`.
- The new session is then admitted and its first event pushed.
- No error is surfaced to the OTLP client.

**Rationale:** smooth degradation under load. Legitimate burst traffic
(e.g. 3 agents starting simultaneously) is unaffected because the
evicted buffer is also persisted — only its in-flight unflushed events
are lost, and only when the evicted buffer happens to be active rather
than truly oldest. The new `CloseReason::CapacityEvict` value gives
operators a clear DB-side signal to identify evicted sessions.

**Alternatives considered:**

- **Refuse new** — return `tonic::Code::ResourceExhausted` / HTTP 503.
  Honest about saturation but rejects legitimate burst traffic during
  a sweep cycle. Pushes load-shedding to the OTLP client, which often
  lacks the right backoff policy.
- **Hybrid (LRU evict with grace period)** — try LRU evict first; if
  even the oldest is fresh (< 1 s), refuse new. Combines both
  approaches but adds branching complexity for marginal benefit at
  default caps of 1024.
- **No enforcement (log warn only)** — keep current behavior; trust
  the idle sweeper. Fails because an attacker can keep buffers warm
  with trickle traffic to defeat the timer.

### D-2: F2 cap values — per-signal split (8/2/8 MiB)

Three independent configuration fields are applied symmetrically to
both transports:

| Field                        | Default     |
|------------------------------|-------------|
| `max_logs_request_bytes`     | `8 * 1024 * 1024` (8 MiB) |
| `max_metrics_request_bytes`  | `2 * 1024 * 1024` (2 MiB) |
| `max_traces_request_bytes`   | `8 * 1024 * 1024` (8 MiB) |

On gRPC, applied via `Server::add_service(svc.max_decoding_message_size(N))`
per-service. On HTTP, applied via `Router::route(path, post(handler).layer(DefaultBodyLimit::max(N)))`
per-route.

**Rationale:** OTel metric envelopes are routinely 10–100× smaller
than logs/traces in production traffic. Tightening metrics independently
is essentially free defense-in-depth.

**Alternatives considered:**

- **Uniform 4 MiB** — matches tonic default; simpler config (one
  knob); but weaker DoS protection on metrics path.
- **Uniform 8 MiB** — generous, uniform mental model; lets metrics
  bombs through.
- **Uniform 16 MiB** — aligns with per-session buffer cap ("one
  request can fill one buffer"); weakest against decode amplification.

### D-3: F3 default `max_open_sessions` = 1024

With the M2.2 per-session cap of 16 MiB, worst-case `SessionRouter`
memory is 1024 × 16 MiB = 16 GiB. Appropriate for team-shared
deployments. Single-user setups (typically ≤ 5 concurrent agents)
never approach the cap. Operators with stricter memory budgets
override via `--max-open-sessions`.

**Alternatives considered:**

- **256** — laptop-safe (4 GiB worst case); covers 99% of
  single-developer setups; but reasonable team deployments would
  hit the cap.
- **64** — tight; would trigger LRU evict frequently in CI runs.
- **4096** — only sensible for dedicated server / k8s pod with
  explicit `resources.limits.memory`.

### D-4: F1 primitive — `subtle` crate

Bearer-token equality uses `subtle::ConstantTimeEq::ct_eq` on the
raw byte slices, never the default `==` operator on `str` (which
short-circuits at the first mismatching byte).

`subtle = "2"` is added as a direct workspace dependency.

**Alternatives considered:**

- **Hand-rolled constant-time u8 slice compare** — ~10 lines of
  code, no new dep; but reinvents a solved problem and is harder
  to review.
- **`ring::constant_time::verify_slices_are_equal`** — already in
  the dependency tree transitively via rustls; avoids a new direct
  dep but couples agentprof to ring's intentionally minimal public
  API surface, which may shift between versions.

`subtle` was chosen for clarity, audit lineage (dalek-cryptography),
zero transitive deps, and MSRV compatibility (1.60 ≤ our 1.78).

### D-5: `SessionId` length cap — 256 bytes, enforced in mapper

Session IDs longer than 256 bytes are rejected at the mapper layer
with a new `MapperError::SessionIdTooLong { signal, len }` variant.
256 bytes accommodates UUIDv4 (36 chars) with 7× headroom and
covers any sane organization prefix scheme.

**Rationale:** the mapper is the only layer with both (a) source
signal metadata for a useful error message and (b) cheap access to
the candidate `SessionId` string before any router buffer is allocated.

### D-6: Release cadence — three tags (v0.2.0 → v0.2.1 → v0.3.0)

- `v0.2.0` — M2.1 SQLite persistence + M2.1.1 aggregate dual-path.
- `v0.2.1` — M2.2 OTLP receiver as-shipped, with a prominent SECURITY
  NOTICE flagging F1/F2/F3 and pointing at v0.3.0.
- `v0.3.0` — M2.4 hardening: this ADR's decisions implemented.

**Rationale:** the v0.2.1 → v0.3.0 minor bump (not patch) signals
"this is not just a bug fix — there are new operator-facing config
flags". Skipping v0.2.1 entirely would hide the fact that the
unhardened M2.2 was ever on main, complicating bug triage for users
who checked out `main` between merges.

## Consequences

**Positive:**

- DoS-class memory exhaustion on the OTLP receiver becomes
  effectively impossible at default caps (modulo OS-level
  pathological cases). Worst case is bounded: 16 GiB resident
  with 1024 × 16 MiB sessions × 8 MiB request decode peaks.
- Bearer-token attackers gain no timing oracle.
- `CloseReason::CapacityEvict` gives operators a DB-side breadcrumb
  for diagnosing capacity issues.

**Negative:**

- Operators have 4 new flags / 4 new config keys to understand.
- Worst-case eviction may flush a buffer with several minutes of
  in-flight unflushed events. Mitigation: defaults (1024 sessions)
  are well above realistic single-user load; eviction is an
  exceptional path, not a normal one.
- `subtle` crate adds a new direct workspace dep, requiring
  per-release `cargo deny check` (already in CI gate).

**Neutral:**

- No SQLite schema migration; `CloseReason::CapacityEvict` is
  in-memory tracing only.
- No breaking API change; all new `OtlpServerConfig` fields are
  additive (Default fall-back).

## Implementation notes

The router LRU index uses `parking_lot::Mutex<lru::LruCache<SessionId, ()>>`
or an equivalent in-tree structure (final choice deferred to the
implementation PR — see plan §T8). The `DashMap` is kept as the
primary buffer store; the LRU index only tracks ordering, not buffer
data, so eviction logic remains O(1) amortized.

`SessionRouter::ingest` checks `buffers.len() >= cap.max_open_sessions`
BEFORE attempting to insert a new entry; if hit, evict the oldest
LRU entry first via `close_buffer(oldest, CapacityEvict)`. Touch
the LRU index on every event ingest (not just session creation) so
"recency" reflects actual activity, not just admission time.

**Addendum (M2.4 T11.5, post-merge fix `cf33b91`):** the as-shipped
admission check was rewritten to use `DashMap::entry()` + the
`Entry::Vacant` arm rather than a `len()` pre-check + later
`insert_or_modify`. The earlier shape was racy under concurrent first
events from the same `session.id` (two tasks could both observe
`len() < cap` and admit, briefly overshooting the cap by 1). Gating
admission on the `Vacant` variant collapses the check + insert into a
single atomic per-shard operation, eliminating the race. The user-
visible semantics are unchanged: at most one buffer is evicted per
admission, and `CapacityEvict` is still reported for the evicted
session. See `crates/agentprof-storage/src/otlp/router.rs` around
the `Entry::Vacant` match arm.

## References

- Parent design ADR: [ADR-0021](adr-0021-otlp-receiver-architecture.md)
- Audit report: this session's `m22-post-audit` agent run (recorded in checkpoints)
- Spec: `docs/superpowers/specs/2026-06-10-m2.4-otlp-hardening-design.md`
- `subtle` crate: https://docs.rs/subtle/2/subtle/trait.ConstantTimeEq.html
- tonic `max_decoding_message_size`: https://docs.rs/tonic/0.12/tonic/server/struct.Server.html#method.max_decoding_message_size
- axum `DefaultBodyLimit`: https://docs.rs/axum/0.7/axum/extract/struct.DefaultBodyLimit.html
