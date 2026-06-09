# ADR-0017: Unify session id namespace across adapter and storage

- **Status**: Accepted
- **Date**: 2026-06-10
- **Deciders**: M2.1 hotfix (P0 dual-path bug discovered in T7.2)
- **Related**: ADR-0015 (mcp-waste architecture), M2.1 spec
  `docs/superpowers/specs/2026-05-20-m2-1-sqlite-persistence-plan.md`

## Context

`DualPathDataSource::merge_refs` (in `crates/agentprof-cli/src/data_source.rs`)
joins the adapter-discovered `SessionRef`s with the storage-discovered
`SessionRef`s on the `id` field. The dual-path freshness compare
(`diff_fields`) only runs when both corpora share an `id`, and the
`agentprof: warn: session <id>: N fields differ …` line + `--quiet`
suppression are the entire user-facing surface area of this mechanism.

In M2.1 T7.2 (the dual-path integration tests), the `dualpath_warns_on_stale_db`
case was marked `#[ignore]` after the subagent discovered the join was always
empty:

| Source                                              | Value of `SessionRef.id`           |
|-----------------------------------------------------|------------------------------------|
| `CopilotAdapter::discover_sessions` (paths.rs)      | directory name (`with-mcp-waste`)  |
| `upsert_report` → `SqliteDataSource::discover`      | UUID parsed from `data.sessionId` inside `events.jsonl` (`00000000-…-099`) |

The two id namespaces never intersect → `diff_fields` is never invoked →
divergence warnings never fire → `--quiet` is dead code → the entire
dual-path consistency mechanism is silently broken.

Confirmed by re-enabling the ignored test: with the old code, even after
`UPDATE sessions SET raw_mtime = raw_mtime - 1000000`, no warn line is
emitted. With the fix below, the warn fires as designed.

## Decision

**Make the adapter's `SessionRef.id` match the canonical UUID parsed from
`data.sessionId` in the first event of `events.jsonl`** — the same id that
`SessionMeta::id` carries into `upsert_report`.

Implementation: a new helper
`agentprof_adapters::copilot::paths::extract_session_id_from_first_event`
opens `events.jsonl`, reads the first line via a single
`BufReader::read_line`, and extracts `data.sessionId`. Both
`discover_sessions` and `analyze::resolve_session_by_path` consume it.

Fallback: if the file is unreadable, empty, or malformed JSON, the directory
name is still used as a synthetic id — broken sessions remain discoverable
so the user sees them in `list` and can act on them.

Additionally, `diff_fields` was relaxed to treat `Option::None` as "no
opinion" rather than disagreement. Originally the adapter did not parse
`startTime` eagerly (returning `None` for `started_at_ms`), so without
this relaxation every fresh scan would have spuriously flagged
`started_at_ms` as a divergence between the adapter (`None`) and storage
(`Some(_)`). A real disagreement now requires both sides to assert a
value and disagree.

> **Update 2026-06-09** — commit `fb96414` reverses the *return-`None`*
> half of that compromise: the new helper
> `agentprof_adapters::copilot::paths::extract_session_start_ms_from_first_event`
> reads `data.startTime` (or envelope `timestamp`) from the first event
> line — same cheap `BufReader::read_line` pass the id extractor uses.
> `AdapterDataSource::adapter_ref_to_datasource_ref` now populates
> `started_at_ms` eagerly. The relaxed `diff_fields` semantics are
> retained (defense-in-depth + still useful when a future adapter can't
> get a cheap timestamp), but in practice both sides now agree on the
> ms-precision logical start and divergence flags only fire on real
> drift (e.g. a stale ingested row with an older timestamp from before
> the source `events.jsonl` was reflowed).
>
> This unblocks deterministic `list`/`aggregate` ordering across CI
> runners regardless of fixture mtime — the original M2.1 P0 fix only
> covered the id namespace; the M2.1 CI fix on `fb96414` covers the
> ordering namespace.

## Considered alternatives

### A. Change `upsert_report` to key on the directory name (rejected)

Would force storage to abandon the well-defined UUID identifier in
`SessionMeta::id` for a filesystem-coupled name. Loses cross-host stability,
breaks every existing `agentprof db *` workflow keyed on UUID, and
contradicts every other crate's understanding of "session id".

### B. Add a second `dir_name` field to `SessionRef`, join on both (rejected)

Adds a public API surface change to `agentprof-core::SessionRef`, requires
churn across all three adapters, and still leaves "which is the canonical
id?" ambiguous. The dual-path mechanism becomes harder to reason about.

### C. Parse first event line in discover (accepted)

One `open` + one `BufReader::read_line` per session directory. For 100
sessions at ~1 KB per first line, well under 100 ms total — negligible
against the existing whole-file parses that follow during analysis.
Aligns adapter id-space with the canonical UUID storage already uses.

## Consequences

### Positive

- Dual-path freshness compare now actually runs.
- `agentprof: warn: …` divergence line fires on real staleness.
- `--quiet` flag is no longer dead code.
- `agentprof analyze --root R --session <uuid>` now resolves the same id
  that `agentprof db ingest` writes — previously the user had to know the
  filesystem layout to fish out the right `--session`.

### Negative

- `agentprof list` output now shows UUIDs (longer than dir names). Width
  of the ID column grows from ~24 chars to 36 chars.
- The `cli_nocache_regression::list_no_cache_stable` snapshot was regenerated
  to reflect the new IDs. The test still serves its purpose (lock against
  future regression of single-path output) — this change is a deliberate,
  one-time correction. (Test file renamed from `cli_nocache_compat.rs` →
  `cli_nocache_regression.rs` in M2.1 audit P2-5.)
- One previously committed fixture (`with-session-shutdown`) shared a
  `sessionId` (`…-000099`) with `with-mcp-waste`. The shutdown fixture
  was re-stamped to `…-000019` to avoid a real collision in the now-merged
  id-space.

## Re-enabled tests

- `cli_dualpath::dualpath_warns_on_stale_db` — was `#[ignore]`, now passes.

## Snapshot / fixture deltas

- `crates/agentprof-cli/tests/snapshots/cli_nocache_regression__list_no_cache_stable.snap`
  (file renamed from `cli_nocache_compat__list_no_cache_stable.snap` in M2.1 audit P2-5)
- `crates/agentprof-adapters/tests/fixtures/copilot/with-session-shutdown/events.jsonl`
  (`sessionId` `…-000099` → `…-000019`)
