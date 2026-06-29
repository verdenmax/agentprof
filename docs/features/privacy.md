# Privacy considerations for `agentprof` reports

> **Scope.** This document describes which fields of an `agentprof analyze`
> report carry potentially-sensitive metadata and what to do about it
> when sharing reports publicly (issues, GitHub Discussions, blog posts,
> etc.). It does **not** describe any security boundary inside `agentprof`
> itself — every report is computed on local data the user already had on
> disk; nothing is exfiltrated, sent over the network, or written outside
> the user's chosen `--output` path.
>
> **Status.** The *report* surface now ships opt-in redaction via
> `--privacy <none|redact|anonymize>` on `analyze` + `aggregate`
> (see [§4](#4---privacy-noneredactanonymize-shipped-l-1)). `md` / `json` /
> `csv` — **aggregate**'s table-only `html` — and now **analyze**'s `html` /
> `speedscope` flamegraph frames are all fully redacted (F-10): episodes are
> redacted through the same context as the report, so turn-ids stay consistent
> and no original turn-id or raw MCP server name survives.
> The manual cheat sheet in [§3](#3-manual-redaction-cheat-sheet) remains
> useful for `list` (not yet covered) and for ad-hoc one-off scrubbing.
>
> The separate *log output* surface (`tracing` stderr / `--log-file` /
> TUI-mode `$XDG_STATE_HOME/agentprof/agentprof.log`) **does** carry a
> shipped, default-on PII model since **M1.6.4** — see
> [§7. Log output PII model](#7-log-output-pii-model-m164) below.

## 1. What `agentprof analyze` does NOT carry

`AnalysisReport` is a rollup of **timings, counts, status flags, and
identifiers**. The following content from the underlying session is
**never** included in the report:

| Source-of-truth content | Where it lives | Why it's excluded |
|---|---|---|
| User prompts (`user.message.content`) | wire `events.jsonl` | Analyzer rolls calls into counts; prompt text is never read |
| Tool arguments (`tool.execution_start.arguments`) | wire `events.jsonl` | Analyzer tracks tool *name* + duration only |
| Tool results (`tool.execution_complete.result`) | wire `events.jsonl` | Same as above |
| Assistant text (`assistant.message.content`) | wire `events.jsonl` | Analyzer reads `outputTokens` + `model` only |
| Reasoning traces (`reasoning_text`, `reasoning_opaque`) | wire `events.jsonl` | Not consumed at all |
| Hook input payloads (e.g. `toolResult.textResultForLlm`) | wire `events.jsonl` | Hook rank uses `hook_type` + duration only |
| MCP server URLs / auth tokens | wire `events.jsonl` headers | Not consumed |
| Subagent prompts (`subagent.started.prompt`) | wire `events.jsonl` | Future post-MVP work may surface counts; never text |

If you're worried about a particular field, grep your report for it —
if it's not literally in the JSON / md output, it's not there.

## 2. PII tiers in the current report

The following table summarizes every field in `AnalysisReport` that may
carry personally-identifying information (PII) or session-identifying
information (SII), graded by sensitivity.

### Tier 🔴 HIGH — likely to identify the human or the project

| Field | Tier | Example | Why it's sensitive |
|---|---|---|---|
| `meta.cwd` | 🔴 HIGH | `/home/<user>/<projects>/agentprof` | Leaks Unix username + project path layout |
| `meta.branch` | 🔴 HIGH | `feat/internal-secret-feature` | Leaks branch naming conventions + WIP feature codenames |
| `meta.model` (inside `turn_summary[i].model`) | 🔴 HIGH | `claude-opus-4.7-1m-internal` | Identifies internal / preview model access |
| `meta.id` | 🔴 HIGH | `252068e5-ca16-4186-a181-719462643d83` | Persistent UUID, cross-references local session-state dir |
| `turn_summary[i].turn_id` | 🔴 HIGH | full UUIDs per turn | ≈ 800 UUIDs per session, allow cross-session correlation |

### Tier 🟡 MEDIUM — fingerprint of toolchain / habits

| Field | Tier | Example | Why it's sensitive |
|---|---|---|---|
| `tool_rank[i].name` (MCP entries) | 🟡 MEDIUM | `mcp__github__search_issues` | Reveals which MCP servers the user has configured |
| `meta.agent_version` | 🟡 MEDIUM | `1.0.54` (or unreleased build IDs) | Pinpoints CLI build + can leak preview-channel membership |
| `meta.started_at` | 🟡 MEDIUM | `2026-05-26T02:43:43Z` | Reveals working hours / timezone via offset patterns |
| `turn_summary[i].status` when `Aborted` (`AbortInfo.at`) | 🟡 MEDIUM | `2026-05-26T16:00:05Z` | Nested abort wall-clock instant — same working-hours / timezone leak as `started_at` |
| `meta.producer` | 🟡 MEDIUM | `copilot-cli` (wire `producer`) | Identifies which agent CLI produced the session (the wire `copilot_version` maps to `meta.agent_version`, not a separate field) |

### Tier 🟢 LOW — generic engineering signal, usually safe

| Field | Tier | Comment |
|---|---|---|
| All durations / counts / percentiles | 🟢 LOW | Pure numerics |
| `tool_rank[i].name` for builtin tools (`bash`, `view`, …) | 🟢 LOW | Public vocabulary |
| `hook_rank[i].name` (`sessionStart`, `postToolUse`) | 🟢 LOW | Public vocabulary |
| `*.is_user_blocking`, `*.status`, `*.synthesized_*` | 🟢 LOW | Boolean / enum flags (exception: `status = Aborted` embeds a 🟡 MEDIUM `AbortInfo.at` wall-clock instant — see MEDIUM tier above) |
| `parse_warnings`, `warnings` counts and `error` text | 🟢 LOW | Error messages may quote line numbers + serde-level "missing field X" strings; no payload values |
| `analysis_report.model_metrics[<model>].{input_tokens, output_tokens, cache_read_tokens, cache_write_tokens}` (F1.7) | 🟢 LOW | Aggregate per-model counters; not attributable to specific prompts/users. See [§10](#10-per-model-token-metrics-modelmetrics-f17) for full detail |

## 3. Manual redaction cheat sheet

Until `--privacy` covered every format ([§4](#4---privacy-noneredactanonymize-shipped-l-1)),
manual scrubbing was the only option. It remains useful for `list` (not yet
covered) and for ad-hoc one-off scrubbing; the following
patterns cover most cases for sharing a report publicly.

### 3.1 `--export md` output

```bash
agentprof analyze --agent copilot --session <id> --export md \
  | sed -E \
      -e 's#/home/[^/[:space:]]+#/home/USER#g' \
      -e 's#- CWD: .*#- CWD: <redacted>#' \
      -e 's#- Branch: .*#- Branch: <redacted>#' \
      -e 's#[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}#<uuid>#g' \
  > report-redacted.md
```

What this does:

1. Replaces every `/home/<username>/...` path with `/home/USER/...`.
2. Blanks out the `CWD:` and `Branch:` lines in the Session header.
3. Replaces every UUID-shaped string (session id, all turn ids) with the literal `<uuid>`.

It does **not** redact model names — if you're worried about leaking
internal model identifiers, append `-e 's#claude-opus-[^|[:space:]]*#<model>#g'`
(or `gpt-5-[^|[:space:]]*`, etc.) tailored to your case.

### 3.2 `--export json` output

For JSON, prefer `jq` to strip keys cleanly:

```bash
agentprof analyze --agent copilot --session <id> --export json \
  | jq '
      .meta.cwd            = "<redacted>" |
      .meta.branch         = "<redacted>" |
      .meta.id             = "<uuid>" |
      .turn_summary       |= map(.turn_id = "<uuid>" | .model = "<model>")
    ' \
  > report-redacted.json
```

## 4. `--privacy <none|redact|anonymize>` (shipped, L-1)

`analyze` and `aggregate` carry an opt-in `--privacy` flag that redacts the
report **in the core report layer** before any export format is rendered, so
every report-derived format inherits the same transform (see
[ADR-0026](../internals/adr-0026-report-redaction.md)). The default is
`none` — byte-identical, fully backward-compatible output, zero overhead.

```bash
agentprof analyze   --agent copilot --export md   --privacy redact
agentprof analyze   --agent copilot --export json --privacy anonymize --output report.json
agentprof aggregate --agent copilot --by model --export md --privacy redact
```

### 4.1 Field rules

| Field | `redact` | `anonymize` |
|---|---|---|
| `meta.cwd` | `<redacted>` | `<redacted>` |
| `meta.branch` | `<redacted>` | `<redacted>` |
| `meta.repository` | `<redacted>` | `<redacted>` |
| `meta.id` (session UUID) | `<uuid-0>` (stable) | `<uuid-0>` |
| `turn_summary[i].turn_id` | `<uuid-N>` (stable, per-session) | same |
| `turn_summary[i].model` + `model_metrics` keys | family (`claude-opus`) | family |
| `meta.agent_version` / `meta.producer` | keep | `<redacted>` |
| `meta.started_at` + per-turn `started_at` (incl. `status = Aborted`'s `AbortInfo.at`) | keep | zeroed (1970 sentinel) |
| `tool_rank[i].name` where `mcp__*` | keep | `mcp__<hash8>__<tool>` |
| `warnings` / `parse_warnings` | cleared | cleared |
| sidecar `agentprof-redaction-map.json` | — | written |

`model → family` (`split('-').take(2)`) applies at **both** levels because
internal / preview model identifiers are 🔴 HIGH (§2). UUIDs map to stable
`<uuid-N>` in first-seen order (`meta.id` → `<uuid-0>`), so percentile rows
and turn cross-references stay internally consistent.

**Always preserved** (🟢 LOW — the ROI signal is the point of redaction): all
counts / durations / percentiles, builtin tool names (`bash` / `view` / …),
hook names, status / enum flags (the `Aborted` variant is kept, but its
nested `AbortInfo.at` instant is zeroed at `anonymize` — see §4.1 table),
and Skill names.

### 4.2 The `anonymize` sidecar

`--privacy anonymize` writes a reverse map `agentprof-redaction-map.json`
(`replacement → original` for `uuids` / `models` / `mcp_servers`) so a holder
of the shared report can un-redact locally. It is written as a sibling of
`--output` (or in CWD when streaming to stdout, with a one-line stderr
notice). A sidecar write failure is **non-fatal**: the report is already
emitted; the failure surfaces as exit code 3 + a stderr warning. The
`<redacted>` fields (cwd / branch / repository / version / started_at) are
one-way and are **not** in the map.

> **Caution.** `agentprof-redaction-map.json` de-anonymizes the report — anyone
> holding both files can reverse the mapping back to the original UUIDs / model
> names / MCP server names. **Delete it after sharing the report**, or add it to
> your `.gitignore`. `agentprof` does **not** auto-gitignore or auto-delete the
> sidecar.

### 4.3 Coverage by export format

| Format | Coverage |
|---|---|
| `md` / `json` | ✅ **fully redacted** — safe for public sharing |
| **aggregate** `csv` | ✅ **fully redacted** — aggregate-only export; safe for public sharing |
| **aggregate** `html` (table-only) | ✅ **fully redacted** — `aggregate_html::render` takes **no** `episodes`, so there is no flamegraph anywhere and nothing to leak |
| **analyze** `html` (flamegraph) | ✅ **fully redacted** — episodes are redacted through the same context as the report (F-10), so frames carry no original turn-id or raw MCP server name |
| **analyze** `speedscope` | ✅ **fully redacted** — report meta + frame table redacted; episodes share the report's redaction context (F-10) |
| `tui` | **not redacted** — local-only surface; `--privacy` + `--export tui` warns |

> **analyze html / speedscope are now fully redacted (F-10):** the flamegraph
> is built from `episodes`, which are redacted through the **same**
> `RedactionContext` as the report — table and flamegraph share turn-ids and no
> original turn-id or raw MCP server name leaks. `aggregate`'s html is
> table-only (no flamegraph) and likewise fully redacted, as are aggregate
> `csv` / `md` / `json`.

### 4.4 Not yet covered (future work)

- **`list --privacy`** — `list` has a far smaller per-session-row PII面 and is
  rarely shared; deferred.

Tracked in [`tasks/ROADMAP.md`](../../tasks/ROADMAP.md) §6.2.

## 5. Defense in depth — workspace conventions

For repository contributors. **Note: enforcement is currently by-convention
only.** The items below describe what reviewers and committers should check
manually; there is no automated CI guard nor an `xtask anonymize` helper
(both are planned for future milestones — see issue tracker).

1. **Never** commit a real `events.jsonl` file. The fixtures in
   `crates/agentprof-adapters/tests/fixtures/copilot/` are
   **synthetic-only** per [ADR-0003 §3](../internals/adr-0003-synthetic-fixture-strategy.md):
   they must use the reserved session UUID range
   `00000000-0000-0000-0000-0000000000NN…NNNN` and the synthetic CWD
   prefix `/tmp/agentprof-fixture/<fixture-slug>`. Reviewers must
   verify this by reading every new fixture; nothing in CI currently
   enforces it.
2. **Never** commit a real `agentprof analyze` report under `docs/`.
   For documentation that needs a sample report, generate one from a
   fixture (the only ones whose `cwd` is `/tmp/agentprof-fixture/...`).
   Reviewers must spot-check any new committed report; nothing in CI
   currently enforces it.
3. **Privacy regression watch.** Any PR that touches
   `crates/agentprof-core/src/analyzer/` or
   `crates/agentprof-cli/src/cmd/format/` should be checked against
   §2 above to confirm no new field surfaced from the wire layer is
   carrying user content (prompts, tool args, tool results, assistant
   text, reasoning text). The only safe additions are timings,
   counts, status enums, and tool/hook/skill names from the public
   vocabulary.

Future automation tracked in roadmap:
- An `xtask audit-pii <report.json>` command that flags 🔴 HIGH fields
  in any report.
- A CI step that grep-fails any committed file containing a `/home/<word>/`
  path (the most common accidental PII leak).
- A pre-commit hook that scans new fixture files for non-reserved UUIDs.

## 6. Reporting a leak

If you discover that `agentprof analyze` is emitting a field this document
doesn't account for (or any field that crosses the line from "ROI rollup"
to "user content / tool output"), open a `[privacy]` issue on the
repository. Such issues are P0 and will receive a fix + a new
`PrivacyWarning` test fixture within one release cycle.

## 7. Log output PII model (M1.6.4)

`agentprof` emits structured `tracing` logs (see
[ADR-0010](../internals/adr-0010-tracing-infrastructure.md) and
[`docs/architecture.md`](../architecture.md) §15.5). This surface is
**separate** from the analyzer report surface described in §1–§5: log
output is for diagnostics (the `--log-level` / `--log-file` global flags
and the TUI auto-redirect to `$XDG_STATE_HOME/agentprof/agentprof.log`),
not for sharing analysis results.

Because log output can land in bug-report attachments, `agentprof`
applies a **default-on** PII redaction to one specific class of fields:
**session paths** carried in span attributes (e.g. the `session = ...`
field on `cmd.analyze` / `cmd.watch` / `adapter.discover` /
`adapter.parse` spans).

### What is redacted

| Field shape | Default rendering | Implementation |
|---|---|---|
| `session = %path` (any span where a session-state path is attached) | First 8 hex chars of `sha256(path.to_string_lossy())` | [`agentprof_core::observability::pii::hash_path`](../../crates/agentprof-core/src/observability/pii.rs) |
| Any other short string the caller chooses to hash | Same 8-hex shape | [`hash_short`](../../crates/agentprof-core/src/observability/pii.rs) |

Hashing is **deterministic** within a process invocation and across
invocations on the same host (no per-run salt), so a support reader can
still cross-reference two `session = abc12345` lines as referring to the
same session — without learning the underlying path.

### Opt-out

Set `AGENTPROF_LOG_FULL_PATHS=1` in the environment to bypass the hash
and emit the raw path. Use this when reproducing a bug locally where you
need to grep `~/.copilot/session-state/...` from the log itself; **do
not** ship the resulting log unredacted.

The opt-out is implemented inside
[`agentprof_core::observability::pii::hash_path`](../../crates/agentprof-core/src/observability/pii.rs)
itself (it reads `AGENTPROF_LOG_FULL_PATHS` per call), so the opt-out
applies system-wide at every emission layer — L1 `cmd.*` (cli), L2
`adapter.*` (adapters), L3 `analyzer.*` / `aggregator.*` (core). It is
the **only** mechanism that disables the path hash — there is no
config-file equivalent, by design (one-shot env var keeps the opt-out
visible to ops on the command line).

### What is NOT covered by this hashing

The hash applies only to fields the call sites explicitly wrap. Plain
log message text (`tracing::warn!("failed to parse {path}: ...")`) and
error chain bodies are emitted verbatim — call sites must hash before
formatting when needed. Coverage today:

- ✅ `cmd.analyze` / `cmd.list` / `cmd.aggregate` / `cmd.watch` spans —
  `session` attribute is hashed.
- ✅ `adapter.discover` / `adapter.parse` spans — `session` attribute is hashed.
- ⚠️ Free-form warning / error messages constructed via `format!`
  inside the code path may still carry raw paths. Per the cheat sheet in
  §3.1, redact log files with the same `sed` recipe used for `--export md`
  output before sharing.

The analyzer report fields enumerated in §2 are **unaffected** by this
hashing — they are computed and serialized separately and are governed by
the `--privacy` flag ([§4](#4---privacy-noneredactanonymize-shipped-l-1)).

## 8. Tool arguments in `ToolCall.arguments` (F1)

As of F1 (2026-06-03), `agentprof_core::episode::ToolCall` carries an
optional `arguments: serde_json::Value` field populated from
`Event::payload_tool_requests()`. For the Copilot CLI adapter this
includes the raw JSON args of every `tool_request` and `tool.user_requested`
event — e.g.:

- `bash` calls carry `{ "command": "rg pattern --type rust" }`
- `read_file` carries `{ "path": "/home/user/project/src/main.rs" }`
- `mcp:postgres::execute_query` carries `{ "query": "SELECT * FROM ..." }`
- `ask_user` carries the prompt + choice list the agent presented
  (user replies are tracked separately as `tool_result`, not
  `arguments` — so the user's typed response is NOT in args)

**No redaction is performed in v1.** The args data is passed through
as-is to:

1. The TUI `TurnDetailView` (shown to anyone viewing the report).
2. **Args do NOT currently appear in the JSON export** (`analyze --export
   json`). The JSON export serializes `AnalysisReport`, which aggregates
   tool data into `tool_rank` (per-tool-name rollups) without per-call
   args. The `ToolCall.arguments` field IS populated end-to-end in the
   in-memory `Episodes` (consumed by `TurnDetailView`), but is not
   surfaced by any export format in F1. Adding per-call args to JSON
   export is reserved for a future enhancement (likely via a new
   `--export episodes-json` surface or by extending `AnalysisReport`
   with raw episode access).

This matches the existing posture on tool names, raw event content, and
turn timing data: agentprof trusts whatever the adapter emits and does
not introspect payload contents to scrub sensitive substrings.
**This posture is recorded in [ADR-0011](../internals/adr-0011-turn-detail-and-args-plumbing.md) D-13.**

**Note**: the `AGENTPROF_LOG_FULL_PATHS` environment variable governs
*logging fields* (e.g. `session = %hash`), NOT payload data. It has no
effect on `ToolCall.arguments` rendering or serialization.

**Future**: a `--show-results` / args-redaction feature is reserved for
a future privacy RFC. Until then, users should be aware that:

- Sharing `analyze --export json` output is safe regarding args (they
  are not in the JSON export today — see item 2 above). It WILL expose
  tool names, durations, success/failure rates, and turn timing, which
  may be sensitive in their own right.
- Recording a `watch` TUI session on screen captures args.
- HTML / Markdown / CSV / Speedscope exports do NOT include args (those
  format exports are tool-aggregated or frame-named, not per-call —
  Speedscope frame names already convey tool identity; args would
  multiply file size dramatically. See ADR-0011 D-12.)

## 9. Reporting a leak

(Same as §6 above; preserved for direct linking. If you find a `tracing`
span attribute that emits a raw session path or any other 🔴 HIGH
content from §2 without going through `hash_path` / `hash_short`, file
the same `[privacy]` issue.)

## 10. Per-model token metrics (`model_metrics`, F1.7)

As of F1.7 (2026-06), `agentprof_core::analyzer::AnalysisReport` carries
an optional `model_metrics: Option<BTreeMap<String, ModelUsage>>` field
populated from `Event::payload_model_metrics()` (currently only
implemented by the Copilot CLI adapter — it reads
`session.shutdown.modelMetrics`). Each `ModelUsage` exposes four `u64`
counters: `input_tokens`, `output_tokens`, `cache_read_tokens`,
`cache_write_tokens`.

### Risk

🟢 **LOW** — these are aggregate per-model token counters scoped to a
single session. They are not attributable to a specific prompt, file,
or user input; they leak only "which models were used by this session
and roughly how much".

### Surfaces

- **TUI Models view** (key `4` in `analyze --export tui`, `watch`) —
  interactive display only, not persisted.
- **JSON export** (`analyze --export json`) — emitted under the
  `model_metrics` key. Field is
  `#[serde(skip_serializing_if = "Option::is_none")]`, so the key is
  absent entirely for adapters that don't implement
  `Event::payload_model_metrics` (Claude, Codex today) or for sessions
  without a `session.shutdown` event. Example:

  ```json
  {
    "model_metrics": {
      "claude-opus-4.7-1m-internal": {
        "input_tokens": 98327,
        "output_tokens": 47523,
        "cache_read_tokens": 3444639,
        "cache_write_tokens": 721860
      }
    }
  }
  ```

- **HTML / Markdown / CSV exports** — surfaced via the same
  `AnalysisReport` rollup (any format that serializes the full report).

### Adjacent fields

The `<model>` map keys are model identifiers as reported by the adapter
(e.g. `claude-opus-4.7-1m-internal`). These overlap with the existing
🔴 HIGH-tier `turn_summary[i].model` field (see §2) — internal /
preview model identifiers leak the same way through either surface.

### Mitigation

Strip from JSON before sharing:

```bash
agentprof analyze --agent copilot --session <id> --export json \
  | jq 'del(.model_metrics)'
```

Or anonymize model names while preserving token counts:

```bash
... | jq '
  .model_metrics |= (
    to_entries
    | map(.key |= (split("-")[0:2] | join("-")))
    | from_entries
  )
'
```

### Provenance

Token values come straight from Copilot CLI's
`session.shutdown.modelMetrics` free-form `serde_json::Value` tree;
`agentprof` walks it with `.get("usage")?.get("<key>").and_then(as_u64)`
per [ADR-0012 D-7](../internals/adr-0012-session-model-metrics-and-models-view.md).
No transformation, sampling, or estimation is applied — what the CLI
reports is what gets serialized.
