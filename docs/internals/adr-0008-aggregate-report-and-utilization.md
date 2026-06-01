# ADR-0008 — AggregateReport<B> + utilization metric

> **Status:** Accepted
> **Date:** 2026-06-01
> **Deciders:** agentprof team
> **Supersedes:** —
> **Superseded by:** —
> **Spec:** [`2026-06-01-m1.6.2-aggregate-design.md`](../superpowers/specs/2026-06-01-m1.6.2-aggregate-design.md)
> **Plan:** [`2026-06-01-m1.6.2-aggregate.md`](../superpowers/plans/2026-06-01-m1.6.2-aggregate.md)

## Context

M1.6.2 introduces the `agentprof aggregate` subcommand: cross-session
roll-ups across **four canonical group-by keys** — `tool`, `mcp-server`,
`day`, `model` — exported as `md` / `json` / `csv` / `html`. Unlike `analyze`
(single session, rich derived data) and `list` (cheap per-session
metadata table), `aggregate` reads N sessions, derives `Episodes` +
`AnalysisReport` for each, and then folds the four bucket types out of
the per-session results.

Three design tensions had to be resolved:

1. The four bucket types have **completely different schemas** (a tool
   bucket has p50/p95 + success/fail; a day bucket has utilization%;
   etc.). A single `AggregateReport` struct would be either a
   string-typed bag or an unmaintainable union. But the CLI needs **one**
   serde boundary for `--export json`.
2. Computing `p50` / `p95` across sessions cannot just average the
   per-session percentiles — that is statistically incorrect (mean of
   medians ≠ median). The aggregator needs access to the **per-call
   duration vectors** of every session, not just the rolled-up
   `AnalysisReport`.
3. The `day` bucket needs a meaningful single scalar to flag "this day
   was mostly waiting on the LLM rather than doing work". We pick
   `utilization_pct = tool_time / wall_time × 100`, and let the CLI
   caller decide the warn threshold (default `20.0`).

## Decision

### D-7: Generic `AggregateReport<B>` + outer `AnyAggregateReport` enum

Internally, the core exposes a **type-parameterized** report:

```rust
pub struct AggregateReport<B> {
    pub window: AggregateWindow,
    pub buckets: Vec<B>,
    pub meta: AggregateMeta,   // counts of skipped / parsed sessions, etc.
}
```

with one concrete bucket per group-by key:

| `B` | Where | Schema highlights |
|---|---|---|
| `ToolBucket` | `analyzer::aggregate::bucket` | tool name, source, calls, success, fail, total / p50 / p95 (re-computed), session_count |
| `McpServerBucket` | same | server, tool_count, calls, failures, total, session_count |
| `DayBucket` | same | date (UTC), session_count, wall_time, tool_time, out_tokens, `utilization_pct`, `is_low_utilization` |
| `ModelBucket` | same | model, sessions, turns, out_tokens, total_wall |

For the CLI/serde boundary we wrap them in a serde-tagged outer enum:

```rust
#[derive(Serialize)]
#[serde(tag = "by")]
#[non_exhaustive]
pub enum AnyAggregateReport {
    Tool(AggregateReport<ToolBucket>),
    McpServer(AggregateReport<McpServerBucket>),
    Day(AggregateReport<DayBucket>),
    Model(AggregateReport<ModelBucket>),
}
```

Rejected alternatives:

- **Single concrete `AggregateReport`** with an `enum BucketRow` —
  forces stringly-typed access at every call site and obscures the
  per-key schema; would have prevented `--export csv` from generating
  consistent column headers.
- **Trait objects** (`Vec<Box<dyn Bucket>>`) — adds dynamic dispatch
  + boxing on a hot path that benefits zero from late binding, and
  makes serde derivation hostile.

### D-12: Session model = first-turn model (representative value)

Within a single Copilot session, every turn may technically run on a
different model (e.g. a user mid-session switches `--model`). For the
`--by model` aggregator, the bucket key is the **first turn's** model
string. Rationale:

- It is the model the session was "started with" — the closest analog
  to the wall-clock context the user perceives.
- The alternative (counting a session once per distinct model) inflates
  session counts and double-counts wall time.
- The information loss (mid-session switches) is rare in practice and
  surfaced separately by the per-turn `analyze` report.

### D-5 / D-6: `--by day` utilization metric + `is_low_utilization`

Per-day rows carry **two scalars** that don't exist in the other
bucket types:

- `utilization_pct: f32` — `sum(tool_time_ms) / sum(wall_time_ms) × 100`
  across all sessions whose `started_at` falls on that UTC date.
  Captures "how much of the day's wall clock was spent doing tool work"
  vs. waiting on LLM thinking time / user input.
- `is_low_utilization: bool` — set when `utilization_pct < threshold`.
  The threshold is **caller-supplied** at construction time (default
  `20.0` from CLI); core does **not** hard-code a project-wide
  threshold, because different agents (Claude vs. Copilot vs. Codex)
  have very different baseline tool-call rates.

The CLI renders rows with `is_low_utilization == true` using a `warn-row`
CSS class (`--export html`) or a `⚠` glyph (`--export md`).

### D-2: Sequential parsing for MVP (rayon deferred)

`aggregate` parses N sessions sequentially. For typical windows
(`--since 30d` = O(50) sessions), this is well under one second on warm
disk. Rayon parallel parsing is a clean **future-perf milestone**:

- The aggregator functions are already pure over `&[AnalysisReport]` +
  `&[Episodes]`, so swapping in `par_iter` is mechanical.
- Parallelism does not change the result (commutative folds), so no
  test churn is expected.
- Snapshot stability is easier to verify with the sequential baseline
  shipped first.

### D-9: UTC dates for the day bucket

`DayBucket.date` is a `chrono::NaiveDate` derived from the session's
`started_at` in **UTC**, not the host's local timezone. Reasons:

- Matches the event-stream timestamp convention (event JSONL is RFC3339
  UTC).
- Avoids snapshot non-determinism across CI runners and contributor
  machines.
- A future `--timezone <tz>` flag is a non-breaking additive extension.

### Percentile recomputation (NOT averaging per-session percentiles)

`ToolBucket.p50_ms` / `p95_ms` are computed from the **pooled per-call
durations across all sessions in the window**, NOT from the average of
per-session `p50` values. The mean of medians is not the median;
averaging percentiles is a statistical fallacy that materially distorts
ranks when session sizes differ.

Concretely, the aggregator takes `&[(AnalysisReport, Episodes)]` rather
than just `&[AnalysisReport]`, so it can re-walk the per-call duration
vectors of each `ToolEpisode` and feed them into `analyzer::percentile`
(nearest-rank semantics, fixed in T1 fix-up `a0e2dd6`).

The same principle applies to `compute_wall` — see T1 fix-up commit for
the hooks/skills traversal correction (was previously walking only
tools, undercounting wall time for hook-heavy sessions).

### `#[non_exhaustive]` discipline + `unreachable!` wildcards (T2 fix-up)

All four bucket types and `AnyAggregateReport` carry `#[non_exhaustive]`
so we can add a 5th group-by key (e.g. `--by hook` for M1.6.5) without
breaking downstream consumers. The CLI dispatch sites
(`AnyAggregateReport::meta()`, `fill_metadata`, `truncate_buckets`)
match all four variants explicitly and use `unreachable!` for the
wildcard arm — chosen over silent `_ => {}` so that adding a new variant
**fails to compile** until the dispatch is updated. (Without the
wildcard, the compiler would reject the `match` due to
`#[non_exhaustive]` from the same crate; the explicit `unreachable!`
arm makes the forward-compat intent self-documenting.)

This change shipped as T2 fix-up commit `549fb40`.

### `--since all` rendering as "all" sentinel (T2 fix-up)

`--since all` is implemented internally as a 100-year `Duration`
sentinel (`AggregateWindow::All` is conceptually distinct, but in the
date-math layer it lives as `since_secs = 100 * 365 * 86400`). Naively
rendering this in the human-readable header would emit
`Window: 876000.0 h` (or the `2562047788015.2 h` u64-max overflow seen
before the fix-up). The renderers special-case the sentinel and write
`Window: all` for `md` / `csv` / `html`.

Shipped as T2 fix-up commit `549fb40`.

## Consequences

- **Type safety**: each bucket variant has its own concrete schema; CSV
  / HTML / md renderers each dispatch over `AnyAggregateReport` and emit
  per-key column sets. No stringly-typed bucket rows.
- **Statistical correctness**: aggregate p50 / p95 reflect the true
  pooled distribution. Users comparing tools across the `--since`
  window get medians they can defend.
- **Forward-compat**: adding `--by hook` (or any new key) is a 4-step
  change: add bucket struct → add enum variant → add aggregator
  function → update the four dispatch sites. The `unreachable!` arms
  break the build until step 4 is done.
- **Performance**: O(N) sequential parse is acceptable for MVP windows;
  rayon swap is a single-line change when ROI justifies it.
- **failure_count caveat**: the Copilot derive layer doesn't currently
  propagate the `success: false` bit, so `ToolBucket.failure_count` and
  `McpServerBucket.failure_count` are effectively always 0 today. The
  aggregator code is correct; only the input data is success-only.
  Tracked as upstream follow-up
  `m1.6.2-followup-copilot-failure-bit`.

## Implementation

- **Module**: `agentprof-core::analyzer::aggregate` —
  - `mod.rs` (~200 LOC): `AggregateReport<B>`, `AnyAggregateReport`,
    `AggregateKey`, `AggregateWindow`, `AggregateMeta`, `fill_metadata`,
    `truncate_buckets`.
  - `bucket.rs` (~150 LOC): `ToolBucket`, `McpServerBucket`,
    `DayBucket`, `ModelBucket` (all `#[non_exhaustive]`).
  - `group_by_{tool,mcp,day,model}.rs` (~100 LOC each): pure aggregator
    functions taking `&[AnalysisReport]` + `&[Episodes]`.
  - Total core surface ≈ 700 LOC including tests.
- **CLI**: `agentprof-cli::cmd::aggregate::run` (~370 LOC) —
  - Local `AggBy` enum mirrors `AggregateKey` for `clap::ValueEnum`
    derivation (the core type intentionally does NOT impl `ValueEnum`
    to keep `clap` out of the core dep graph).
  - Dispatches to `cmd::format::aggregate_md` /
    `cmd::format::aggregate_csv` / `cmd::format::aggregate_html`
    (askama template `templates/aggregate.html` + `styles.css`
    `.warn-row` rule).
- **Templates**: `crates/agentprof-cli/templates/aggregate.html`
  + `templates/styles.css` (`.warn-row` for low-utilization day rows).
- **Tests** (21 new total):
  - `agentprof-core/tests/aggregate.rs` (2): shape + serde round-trip.
  - `agentprof-adapters/tests/aggregate_on_fixtures.rs` (8): four
    aggregator functions × multiple fixture combinations. Placed in
    adapters (not core) to avoid a dev-dep cycle, mirroring the
    `episode_derive_on_fixtures` and `analyzer_on_fixtures` precedents.
  - `agentprof-cli/tests/aggregate.rs` (11): integration + insta
    snapshots. Snapshot names are `aggregate__*.snap` (insta uses the
    test file stem `aggregate.rs`), not `cli__aggregate_*.snap`.
  - Core lib added +7 tests (compute_wall walks +2, percentile +5).
- **Deferred follow-ups** (tracked in session SQL, 9 items):
  - `m1.6.2-followup-copilot-failure-bit` (upstream Copilot derive)
  - `m1.6.2-followup-compute-wall-shared` (hoist helper)
  - `m1.6.2-followup-i3-model-skip-test` (coverage gap)
  - `m1.6.2-followup-i4-total-wall-test` (coverage gap)
  - `m1.6.2-followup-i3-fixture-isolation` (snapshot fixture isolation)
  - `m1.6.2-followup-m1-serde-unit-doc` (serde unit docs)
  - `m1.6.2-followup-m2-pub-use-aggregators` (ergonomics)
  - `m1.6.2-followup-m3-mcp-zip-enumerate` (idiomatic iter)
  - `m1.6.2-followup-m5-utilization-precision` (f32 → f64 if needed)
