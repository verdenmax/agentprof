# Privacy Redaction — `--privacy` flag (L-1)

| Field | Value |
|---|---|
| Date | 2026-06-28 |
| Status | Approved — entering writing-plans |
| Author | L-1 closure (`tasks/ROADMAP.md` §6.1) |
| Triggered by | L-1 (🔴 HIGH PII exposed by default in `analyze` / `aggregate` reports) |
| Supersedes | the `--redact` / `--anonymize` draft in `docs/features/privacy.md` §4 |
| Touches ADRs | candidate **ADR-0026** (report redaction layer + level semantics) |
| Target release | v0.4.0 (minor — new opt-in flag; default `none` = zero behavior change) |

## 1. Problem statement

`agentprof analyze` (and `aggregate`) reports carry 🔴 HIGH tier PII by
default — `meta.cwd`, `meta.branch`, internal/preview `model` names, the
session UUID, and ~800 per-turn UUIDs (full inventory: `docs/features/privacy.md`
§2). Sharing a report publicly (issues, Discussions, blog posts) today
requires the manual `sed` / `jq` cheat sheet in privacy.md §3, which is
error-prone and easy to forget.

privacy.md §4 sketches `--redact` / `--anonymize` flags but they are
**not yet implemented**. This is tracked limitation **L-1 (HIGH severity)**.

This spec defines and implements opt-in report redaction. It does **not**
touch the separate *log-output* PII surface, which already has a
shipped, default-on hashing model since M1.6.4 (privacy.md §7).

## 2. Scope

### In scope

- New core module `agentprof_core::analyzer::redact` with:
  - `PrivacyLevel` enum (`None` / `Redact` / `Anonymize`), `clap::ValueEnum`
    behind the existing `clap-derive` feature (same pattern as `AgentKind`).
  - `AnalysisReport::redact(level) -> (AnalysisReport, RedactionMap)`.
  - `AggregateReport::redact(level) -> (AggregateReport, RedactionMap)`.
  - Shared helpers: `UuidRedactor`, `model_family(&str) -> String`,
    `hash_mcp_server(&str) -> String`.
- `--privacy <none|redact|anonymize>` flag on `analyze` **and** `aggregate`.
- `RedactionMap` + `agentprof-redaction-map.json` sidecar (anonymize only).
- All export formats inherit automatically (md / json / html / csv /
  speedscope / tui) — they serialize the same redacted report.
- ~25 tests (core unit + cli integration + insta snapshots + privacy
  regression guard).
- **ADR-0026** codifying the redaction-layer choice + level semantics.
- L1 + L2 doc sync (architecture §8/§10, privacy.md §4 "implemented",
  cli README, ROADMAP §6.1 L-1 → FIXED).

### Out of scope

- `list` subcommand redaction — smaller PII面 (per-session rows), rarely
  shared; deferred (note in privacy.md as follow-up).
- `xtask audit-pii` / CI `/home/<user>/` grep guard (L-11 / L-12) —
  separate fixture-hygiene work.
- Log-output redaction (already shipped M1.6.4, privacy.md §7).
- Tool-argument scrubbing in `ToolCall.arguments` (privacy.md §8, separate RFC).

## 3. Design decisions

- **D-1 — Redaction lives in the core report layer** (not per-format, not
  post-serialization regex). Rationale: DRY (one transform, every export
  format inherits), type-safe, unit-testable on report structure, and the
  `RedactionMap` falls out naturally from the same pass. (Rejected: format
  layer = 6+ duplicated sites; regex = fragile UUID/model matching.)
- **D-2 — Single mutually-exclusive enum flag** `--privacy <none|redact|anonymize>`
  rather than two booleans, so `redact` ⊂ `anonymize` ordering is encoded
  in the type and can't be given contradictorily.
- **D-3 — Two levels.** `Redact` = strip 🔴 HIGH. `Anonymize` = superset:
  also strips `agent_version` / `copilot_version` / `started_at`, hashes
  MCP server names, and emits the sidecar map.
- **D-4 — `model → family` applies at BOTH levels** (`split('-')[0:2]`),
  because internal/preview model identifiers are 🔴 HIGH (privacy.md §2).
- **D-5 — UUIDs map to stable `<uuid-N>`** in first-seen order; same UUID
  always maps to the same replacement (percentile rows / turn cross-refs
  stay internally consistent).
- **D-6 — MCP hashing keeps the tool segment** (`mcp__<hash8>__<tool>`):
  the server is the identifying part; the tool verb is useful + non-PII.
- **D-7 — `aggregate --by day` bucket keys are NOT redacted** (the date is
  the aggregation dimension; redacting it makes the report meaningless,
  and day-granularity is lower risk than precise `started_at`).
- **D-8 — `redact()` is a pure function** returning `Self` (not `Result`):
  it degrades safely on odd input (empty model kept, non-UUID strings still
  mapped) and never panics. Only the sidecar *file write* can fail.

## 4. Redaction rules

| Field | `redact` | `anonymize` |
|---|---|---|
| `meta.cwd` | `<redacted>` | `<redacted>` |
| `meta.branch` | `<redacted>` | `<redacted>` |
| `meta.id` (session UUID) | `<uuid-0>` (stable) | `<uuid-0>` |
| `turn_summary[i].turn_id` | `<uuid-N>` (stable, per-session) | same |
| `turn_summary[i].model` + `model_metrics` keys | family | family |
| `meta.agent_version` / `meta.copilot_version` | keep | `<redacted>` |
| `meta.started_at` | keep | `<redacted>` |
| `tool_rank[i].name` where `mcp__*` | keep | `mcp__<hash8>__<tool>` |
| sidecar `agentprof-redaction-map.json` | — | written |

**Always preserved** (🟢 LOW — the whole point of redaction is keeping the
ROI signal): all counts / durations / percentiles, builtin tool names
(`bash`/`view`/…), hook names, status/enum flags.

## 5. Mapping algorithms

### 5.1 UUID stable map

```text
UuidRedactor { counter: usize, map: BTreeMap<String, String> }

redact_uuid(orig) -> String:
    if let Some(r) = map.get(orig): return r            # cached → stable
    let r = format!("<uuid-{}>", counter); counter += 1
    map.insert(orig, r); return r
```
Walk order fixes numbering: `meta.id` first (→`<uuid-0>`), then
`turn_summary` in slice order.

### 5.2 model → family

```text
model_family(m) = m.split('-').take(2).join('-')
# claude-opus-4.7-1m-internal → claude-opus
# gpt-5-mini                   → gpt-5
# o1                           → o1   (fewer than 2 segments: kept as-is)
```

### 5.3 MCP server hash (anonymize)

```text
# mcp__github__search_issues  → mcp__<hash8>__search_issues
parse "mcp__{server}__{tool}", replace server with
  agentprof_core::observability::pii::hash_short(server)   # sha256[..8], reused
```

## 6. RedactionMap + sidecar

```rust
pub struct RedactionMap {                       // empty for `redact`; filled for `anonymize`
    pub uuids:       BTreeMap<String, String>,  // "<uuid-0>" → real UUID  (replacement→original, for un-redacting)
    pub models:      BTreeMap<String, String>,  // "claude-opus" → "claude-opus-4.7-1m-internal"
    pub mcp_servers: BTreeMap<String, String>,  // "<hash8>" → "github"
}
```

- Each redactor's internal map is `original→replacement` (fast lookup
  during the walk); the exported `RedactionMap` **inverts** it to
  `replacement→original` so a holder of the shared report can un-redact
  via the sidecar.
- `<redacted>` fields (cwd/branch/version/started_at) are **one-way**, not
  in the map (nothing to restore).
- Sidecar `agentprof-redaction-map.json` is written **only** for
  `--privacy anonymize`:
  - with `--output X.md` → sibling `agentprof-redaction-map.json`;
  - to stdout (no `--output`) → write to CWD + one stderr line with the path.
  - **Write failure is non-fatal**: exit code 3 + stderr warning, but the
    redacted report itself has already been emitted.

## 7. aggregate coverage

`AggregateReport` has a far smaller PII面 (no cwd/branch/session-UUID/turn-UUID —
aggregation removed them). Redacted targets:

- `--by model`: bucket key model → family (`redact`).
- `--by tool` / `--by mcp-server`: MCP names → hash (`anonymize`).
- `--by day`: **not redacted** (D-7).
- `model_metrics` keys → family (same as analyze).

`AggregateReport::redact` reuses the same `UuidRedactor` (no-op here) /
`model_family` / `hash_mcp_server` helpers from `analyzer::redact`.

## 8. Error handling

- `--privacy none` (default) → `redact()` is never called; zero overhead,
  identical output, fully backward-compatible.
- `redact()` cannot fail (pure, returns `Self`).
- Sidecar write failure → exit code 3 (I/O error, per architecture §8.1)
  + stderr warning; report already written (consistent with the
  exit-code policy).

## 9. Testing strategy

- **core unit** (`analyzer::redact` tests):
  - `Redact`: cwd/branch=`<redacted>`, UUID=`<uuid-N>` stable, model=family,
    counts/durations unchanged.
  - `Anonymize`: + version/started_at redacted, MCP hashed, `RedactionMap` filled.
  - UUID stability: same UUID at multiple sites → same replacement.
  - `model_family` edge cases: 1 / 2 / many segments.
  - MCP hash: server hashed, tool segment kept.
  - `PrivacyLevel::None` = identity (report unchanged).
  - aggregate bucket-key redaction per `--by` mode; `--by day` untouched.
- **cli integration** (`assert_cmd`):
  - `analyze --privacy redact` → grep stdout has no `/home/`, no real UUID,
    no full internal model name.
  - `analyze --privacy anonymize --output X` → sidecar exists + round-trips.
- **insta snapshots**: redacted md + json output.
- **privacy regression guard**: committed fixture → redact → assert no
  🔴 HIGH residue (permanent guard, mirrors privacy.md §6 P0 posture).

## 10. File-change manifest

- **Create** `crates/agentprof-core/src/analyzer/redact.rs` (`PrivacyLevel`,
  `RedactionMap`, `UuidRedactor`, `model_family`, `hash_mcp_server`,
  `AnalysisReport::redact`, `AggregateReport::redact`) + `pub mod redact;`.
- **Modify** `crates/agentprof-cli/src/cmd/analyze.rs` + `aggregate.rs`:
  add `--privacy` arg; call `redact()` before render; write sidecar.
- **Modify** `crates/agentprof-core/src/analyzer/mod.rs` (or meta) only if a
  field accessor is needed for the walk.
- **Tests**: `crates/agentprof-core/tests/redact.rs`,
  `crates/agentprof-cli/tests/cli_privacy.rs`, snapshots.
- **Docs**: ADR-0026; privacy.md §4 → "implemented"; architecture §8
  (analyze/aggregate flag) + §15.4 if a feature gate is involved (none —
  reuses `clap-derive`); cli README; CHANGELOG; ROADMAP §6.1 L-1 → ✅ FIXED.

## 11. Open questions / future work

- `list --privacy` (deferred — note as follow-up in privacy.md).
- `xtask audit-pii <report.json>` automation (L-11).
- Whether `redact` should also family-ize `--by tool` MCP names (currently
  only `anonymize` hashes them; `redact` keeps MCP tool names since they
  are 🟡 MEDIUM, not 🔴 HIGH — consistent with privacy.md §2 tiers).
