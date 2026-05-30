# Privacy considerations for `agentprof` reports

> **Scope.** This document describes which fields of an `agentprof analyze`
> report carry potentially-sensitive metadata and what to do about it
> when sharing reports publicly (issues, GitHub Discussions, blog posts,
> etc.). It does **not** describe any security boundary inside `agentprof`
> itself — every report is computed on local data the user already had on
> disk; nothing is exfiltrated, sent over the network, or written outside
> the user's chosen `--output` path.
>
> **Status.** Documentation-only. The planned `--redact` / `--anonymize`
> CLI flags listed at the bottom are **not yet implemented**; until then,
> users wishing to share reports must redact manually using the cheat
> sheet in [§3](#3-manual-redaction-cheat-sheet).

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
| Subagent prompts (`subagent.started.prompt`) | wire `events.jsonl` | Future M1.5 / M2 may surface counts; never text |

If you're worried about a particular field, grep your report for it —
if it's not literally in the JSON / md output, it's not there.

## 2. PII tiers in the current report

The following table summarizes every field in `AnalysisReport` that may
carry personally-identifying information (PII) or session-identifying
information (SII), graded by sensitivity.

### Tier 🔴 HIGH — likely to identify the human or the project

| Field | Tier | Example | Why it's sensitive |
|---|---|---|---|
| `meta.cwd` | 🔴 HIGH | `/home/verden/pfind/2026-spring/code/agentprof` | Leaks Unix username + project path layout |
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
| `meta.copilot_version` | 🟡 MEDIUM | matches `--version` output | Same as `agent_version` |

### Tier 🟢 LOW — generic engineering signal, usually safe

| Field | Tier | Comment |
|---|---|---|
| All durations / counts / percentiles | 🟢 LOW | Pure numerics |
| `tool_rank[i].name` for builtin tools (`bash`, `view`, …) | 🟢 LOW | Public vocabulary |
| `hook_rank[i].name` (`sessionStart`, `postToolUse`) | 🟢 LOW | Public vocabulary |
| `*.is_user_blocking`, `*.status`, `*.synthesized_*` | 🟢 LOW | Boolean / enum flags |
| `parse_warnings`, `warnings` counts and `error` text | 🟢 LOW | Error messages may quote line numbers + serde-level "missing field X" strings; no payload values |

## 3. Manual redaction cheat sheet

Until `--redact` lands ([§4](#4-planned-redact--anonymize-flags-not-yet-implemented)),
the following patterns cover most cases for sharing a report publicly.

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

## 4. Planned `--redact` / `--anonymize` flags (NOT YET IMPLEMENTED)

A future release will add two opt-in flags to the `analyze` subcommand
(and to `aggregate` once it lands in M1.5):

- `--redact` — strip the 🔴 HIGH tier fields by default. Replaces
  `meta.cwd` / `meta.branch` with `<redacted>`, replaces every UUID with
  `<uuid-N>` (stable per-session, so percentile rows still reference each
  other), and replaces model names with their family (`claude-opus`,
  `gpt-5`). Counts, durations, tool names (non-MCP), hook names remain
  untouched. Safe-by-default for public sharing.
- `--anonymize=full` — stronger variant for posting to issue trackers
  attached to bug reports. In addition to `--redact`, also strips
  `agent_version` / `copilot_version` / `started_at`, hashes MCP tool
  names (`mcp__<hash>__<original-tool>`), and emits a separate
  `agentprof-redaction-map.json` alongside the report (gitignored by
  default) so the user can locally cross-reference original ↔ anonymized
  values.

Tracked in: future M1.5+ milestone (see `docs/plan.md` roadmap).

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
