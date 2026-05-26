---
title: "ADR-0002: CopilotEvent enum — 17-variant clean-room schema from events.jsonl observation"
status: "Accepted"
date: "2026-05-26"
authors: "@verdenmax (project owner), AI assistant (Copilot CLI session 252068e5)"
tags: ["architecture", "decision", "data-model", "copilot-adapter", "wire-format", "serde", "clean-room"]
supersedes: ""
superseded_by: ""
---

# ADR-0002: CopilotEvent enum — 17-variant clean-room schema from events.jsonl observation

## Status

**Accepted**

## Context

Per ADR-0001, agentprof's MVP first-shipping adapter is `CopilotAdapter`, which reads `~/.copilot/session-state/<uuid>/events.jsonl`. This file is an internal telemetry log produced by GitHub Copilot CLI (`/usr/lib/node_modules/@github/copilot/`, version 1.0.54 at time of writing). The file format has the following properties:

1. **Undocumented publicly.** No GitHub-published wire-format reference exists for `events.jsonl`. The official Copilot CLI SDK at `/usr/lib/node_modules/@github/copilot/copilot-sdk/` declares Zod schemas for **only 8** session-lifecycle event types (`session.created` / `session.deleted` / `session.foreground` / `session.background` / `session.idle` / `session.error` / `session.updated` / `assistant.message`). The SDK is for **SDK consumers** subscribing to session-level signals, not for parsing the internal telemetry stream.
2. **`events.jsonl` actually contains 17+ event types** observed in real data on the user's machine:
   ```
   session.{start, info, mode_changed, model_change, plan_changed, shutdown}
   assistant.{turn_start, turn_end, message}
   user.message
   tool.{execution_start, execution_complete, user_requested}
   hook.{start, end}
   skill.invoked
   system.message
   abort
   ```
3. **Format evolves between Copilot versions.** A March-2026 session (`2fcbfbca-…`) lacks `hook.*`, `skill.invoked`, `session.model_change`, `system.message` entirely. The current-version session (`252068e5-…`, this brainstorming session itself) has all 17 plus emits ~50/50 split of `hook.start` / `hook.end` events as a high-frequency baseline (33 pairs per session in observed sample).
4. **License constraint.** `/usr/lib/node_modules/@github/copilot/LICENSE.md` §3 prohibits "Modify, adapt, translate, or create derivative works of the Software." Translating the SDK's `.d.ts` files or Zod schemas into Rust types would violate this clause.
5. **However, reverse-engineering one's own session data is legal.** The user's events.jsonl files are artifacts the user owns; observing them and writing parser code based on those observations is clean-room work, not derivative work.
6. **Multi-agent forward path.** `Adapter` trait was originally designed (`docs/architecture.md` §3) to support multiple agents (Claude, Codex, Copilot, Gemini, ...). Each agent's wire format will differ structurally (Claude jsonl has `tools` system blocks, Copilot has `hook.start`/`skill.invoked`, Codex emits its own variant); a single normalized `Event` enum that fits all of them would force lossy abstraction.

The decision is: **what shape should the Copilot event data model take, in `agentprof-adapters::copilot`?**

## Decision

**Define a 17-variant `CopilotEvent` enum in `agentprof-adapters/src/copilot/event.rs`, derived from clean-room observation of the user's own session data. Make it the associated `Event` type of `CopilotAdapter`; per-agent adapters keep their own native event enums; shared analysis happens at the `Episode` layer (see `agentprof-core::episode`).**

### Concrete decisions

1. **Clean-room observation, not SDK translation.** All field names, types, and presence/optionality come from `python3 ... events.jsonl | shape()` analysis of two sessions on the user's machine. No line of `.d.ts` or `.js` from `/usr/lib/node_modules/@github/copilot/copilot-sdk/` was copied or translated.

2. **`#[serde(tag = "type", rename_all = "snake_case")]` discriminated union** keyed on the wire-format `type` field. Each variant takes a payload struct wrapped in a common `WithEnvelope<D>` carrying outer fields (`id`, `timestamp`, `parentId`, `ephemeral?`).

3. **`#[serde(other)] Unknown` variant** as forward-compatibility safety net. New event types appearing in future Copilot versions deserialize to `Unknown` instead of triggering parse failures.

4. **All payload fields whose presence is not confirmed across multiple observed sessions are `Option<T>`.** Missing in real data → `None`. This costs zero bytes if absent (serde `skip_serializing_if`).

5. **Each payload struct is `#[non_exhaustive]`** so adding a newly-discovered field in future patches doesn't break downstream pattern matches.

6. **Per-agent enum, not single normalized.** `CopilotEvent` lives in `agentprof-adapters::copilot`. Future `ClaudeEvent` (post-Phase 2) will live in `agentprof-adapters::claude`. Shared analysis is the `Episode` layer (`Turn`, `ToolEpisode`, `HookEpisode`, `SkillEpisode`, `ModeSegment`) in `agentprof-core::episode` — analyzed once, populated from each adapter's native events.

### Wire format reference (the source of truth)

Each event in events.jsonl has the shape:

```json
{
  "type": "<discriminator>",
  "data": { /* type-specific payload */ },
  "id": "<event UUID>",
  "timestamp": "<ISO-8601 with millis>",
  "parentId": "<parent event UUID>",
  "ephemeral": true   // optional, only on some events
}
```

#### Variant table

All Rust types use `chrono::DateTime<Utc>` for ISO-8601 strings and `u64` (Unix epoch ms) for raw integer timestamps. `bool` and `u32`/`u64` integers map directly. `String` for free-form text. `Option<T>` for absent-in-some-sessions fields.

| Variant | `type` value | `data` field shape (observed) |
|---|---|---|
| `SessionStart` | `session.start` | `sessionId: String, version: u32, producer: String, copilotVersion: String, startTime: DateTime<Utc>, context: SessionContext, alreadyInUse: bool` where `SessionContext = { cwd: String, gitRoot: Option<String>, branch: Option<String>, headCommit: Option<String>, repository: Option<String>, hostType: Option<String> }` |
| `SessionInfo` | `session.info` | `infoType: String, message: String` |
| `ModeChanged` | `session.mode_changed` | `previousMode: String, newMode: String` (mode values seen: `interactive`, `plan`, `autopilot`) |
| `ModelChange` | `session.model_change` | `newModel: String` (no `previousModel`; derive prior from preceding `ModelChange` or `AssistantMessage.model`) |
| `PlanChanged` | `session.plan_changed` | `operation: String` (values seen: `update`) |
| `Shutdown` | `session.shutdown` | `shutdownType: String, totalPremiumRequests: u32, totalApiDurationMs: u64, sessionStartTime: u64, codeChanges: CodeChanges, modelMetrics: BTreeMap<String, ModelMetrics>, currentModel: String` where `CodeChanges = { linesAdded: u32, linesRemoved: u32, filesModified: Vec<String> }`, `ModelMetrics = { requests: serde_json::Value, usage: serde_json::Value }` (deeply-nested model-specific shape; keep as `Value` until needed) |
| `UserMessage` | `user.message` | `content: String, transformedContent: Option<String>, source: String, attachments: Vec<serde_json::Value>, interactionId: String` |
| `TurnStart` | `assistant.turn_start` | `turnId: String, interactionId: String` |
| `AssistantMessage` | `assistant.message` | `messageId: String, model: String, content: String, toolRequests: Vec<ToolRequest>, interactionId: String, turnId: String, reasoningOpaque: Option<String>, reasoningText: Option<String>, encryptedContent: Option<String>, outputTokens: u32, requestId: Option<String>, serviceRequestId: Option<String>` where `ToolRequest = { toolCallId: String, name: String, arguments: serde_json::Value, type: String, intentionSummary: Option<String> }` |
| `TurnEnd` | `assistant.turn_end` | `turnId: String` |
| `ToolExecStart` | `tool.execution_start` | `toolCallId: String, toolName: String, arguments: serde_json::Value` (arguments shape varies per tool — keep as Value) |
| `ToolExecComplete` | `tool.execution_complete` | `toolCallId: String, model: String, interactionId: String, turnId: Option<String>, success: bool, result: ToolResult, toolTelemetry: ToolTelemetry, error: Option<ToolError>` where `ToolResult = { content: String, detailedContent: String }`, `ToolTelemetry = { properties: BTreeMap<String, String>, metrics: BTreeMap<String, u64>, restrictedProperties: serde_json::Value }`, `ToolError = { message: String }` |
| `ToolUserRequested` | `tool.user_requested` | `toolCallId: String, toolName: String, arguments: ToolUserArgs` where `ToolUserArgs = { command: String, description: String }` |
| `HookStart` | `hook.start` | `hookInvocationId: String, hookType: String, input: HookInput` where `HookInput = { sessionId: String, timestamp: u64 /* unix ms */, cwd: String, source: String, initialPrompt: Option<String> }` (input may carry hook-kind-specific extras) |
| `HookEnd` | `hook.end` | `hookInvocationId: String, hookType: String, output: Option<HookOutput>, success: bool` where `HookOutput = { additionalContext: Option<String> }` (output may carry hook-kind-specific extras) |
| `SkillInvoked` | `skill.invoked` | `name: String, path: String, content: String, source: String, pluginName: Option<String>, pluginVersion: Option<String>, description: String, trigger: String` (source values: `plugin`, `project`, `builtin`; trigger values: `agent-invoked`, `user-invoked`) |
| `SystemMessage` | `system.message` | `role: String, content: String` |
| `Abort` | `abort` | `reason: String` |
| `Unknown` | `#[serde(other)]` | — (forward-compat sink for future Copilot event types) |

#### Privacy-relevant fields

Three categories of fields will leak private data if serialized for fixtures / public artifacts:

1. **Free-form user content**: `UserMessage.content`, `UserMessage.transformedContent`, `HookStart.input.initialPrompt`, `SystemMessage.content`.
2. **Filesystem identifiers**: `SessionStart.context.cwd`, `SessionStart.context.gitRoot`, `SessionStart.context.repository`, `SessionStart.context.branch`, `SessionStart.context.headCommit`, `HookStart.input.cwd`, `SessionInfo.message`.
3. **Tool I/O**: `ToolExecStart.arguments` (file paths, code, shell commands), `ToolExecComplete.result.content` / `.detailedContent` (file contents, command stdout), `ToolUserRequested.arguments.command`, `AssistantMessage.content` / `.reasoningText` (LLM thinking that may quote user code).

Fixture strategy (ADR-0003) restricts committed test data to synthetic content; **the parser type definitions themselves carry no private data** (only structural metadata).

#### Opaque fields

`AssistantMessage.reasoningOpaque` (~400 chars base64) and `AssistantMessage.encryptedContent` (~20KB base64) appear to be GitHub-encrypted blobs containing internal reasoning state. agentprof:

- Deserializes them as `Option<String>` (preserve presence + length for analytics, e.g., "this assistant turn had heavy hidden reasoning")
- Does **not** attempt to decrypt or interpret
- Does **not** serialize them in any export format unless `--include-encrypted-blobs` flag is set (Phase 2 escape hatch for users who consent)

### `WithEnvelope` wrapper

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WithEnvelope<D> {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    pub data: D,
}
```

### `CopilotEvent` enum surface

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CopilotEvent {
    #[serde(rename = "session.start")]          SessionStart(WithEnvelope<SessionStartData>),
    #[serde(rename = "session.info")]           SessionInfo(WithEnvelope<SessionInfoData>),
    #[serde(rename = "session.mode_changed")]   ModeChanged(WithEnvelope<ModeChangeData>),
    #[serde(rename = "session.model_change")]   ModelChange(WithEnvelope<ModelChangeData>),
    #[serde(rename = "session.plan_changed")]   PlanChanged(WithEnvelope<PlanChangeData>),
    #[serde(rename = "session.shutdown")]       Shutdown(WithEnvelope<ShutdownData>),
    #[serde(rename = "user.message")]           UserMessage(WithEnvelope<UserMessageData>),
    #[serde(rename = "assistant.turn_start")]   TurnStart(WithEnvelope<TurnRefData>),
    #[serde(rename = "assistant.message")]      AssistantMessage(WithEnvelope<AssistantMessageData>),
    #[serde(rename = "assistant.turn_end")]     TurnEnd(WithEnvelope<TurnRefData>),
    #[serde(rename = "tool.execution_start")]   ToolExecStart(WithEnvelope<ToolExecData>),
    #[serde(rename = "tool.execution_complete")] ToolExecComplete(WithEnvelope<ToolResultData>),
    #[serde(rename = "tool.user_requested")]    ToolUserRequested(WithEnvelope<ToolUserRequestedData>),
    #[serde(rename = "hook.start")]             HookStart(WithEnvelope<HookStartData>),
    #[serde(rename = "hook.end")]               HookEnd(WithEnvelope<HookEndData>),
    #[serde(rename = "skill.invoked")]          SkillInvoked(WithEnvelope<SkillData>),
    #[serde(rename = "system.message")]         SystemMessage(WithEnvelope<SystemMessageData>),
    #[serde(rename = "abort")]                  Abort(WithEnvelope<AbortData>),
    #[serde(other)] Unknown,
}
```

## Consequences

### Positive

- **POS-001**: **Zero license risk.** Clean-room work derived from user's own data, never touching `/usr/lib/node_modules/@github/copilot/copilot-sdk/` source files. LICENSE.md §3 untouched.
- **POS-002**: **Forward compatibility built in.** `#[serde(other)] Unknown` makes the parser robust to Copilot adding new event types without our code changes. `#[non_exhaustive]` on enum and payload structs lets us add variants/fields without breaking downstream match arms.
- **POS-003**: **Truthful types.** Every field is documented based on observed wire format, not aspirational documentation. Fields marked `Option<T>` were genuinely absent in some real sessions.
- **POS-004**: **`serde_json::Value` escape hatches** for genuinely-variable-shape sub-fields (`ToolExecStart.arguments`, `ShutdownData.modelMetrics`, `UserMessage.attachments`) keep the parser working even when sub-shapes vary, while letting analyzers introspect with `Value::as_*()` accessors.
- **POS-005**: **Multi-agent path is clean.** When ClaudeAdapter joins post-Phase-2, it gets its own `ClaudeEvent` enum with Claude-specific variants (e.g., `thinking_block`, `tool_use_inside_content`); shared analysis lives in `Episode` layer. No "forcibly unify everything" trap.
- **POS-006**: **Documentation lives with code.** Variant table in this ADR + rustdoc on each payload struct gives both auditors and AI agents a single reference. No "schema doc" drift from "code reality."
- **POS-007**: **Test fixture authoring is straightforward.** Each variant has a known shape — fixture writers fill in real-looking values, type-check via `serde_json::from_str::<CopilotEvent>(...)`.

### Negative

- **NEG-001**: **17 variants is a lot to maintain.** Adding a new field discovered in some future session means: (a) updating the variant table in this ADR, (b) updating the payload struct, (c) updating fixtures, (d) updating any analyzer that uses the field. Mitigated by `#[non_exhaustive]` + `Option<T>` defaults.
- **NEG-002**: **`serde_json::Value` escape hatches push type-safety pain to call sites.** Analyzers handling `ToolExecStart.arguments` will branch on tool name + introspect Value at runtime. Type errors here are caught at test time, not compile time.
- **NEG-003**: **No formal schema contract from GitHub.** Format may change in any Copilot CLI release. We mitigate by: (a) `Unknown` variant absorbs new types, (b) `Option<T>` absorbs disappearing fields, (c) MSRV testing matrix should include "latest Copilot CLI" smoke test (manually triggered, not blocking CI).
- **NEG-004**: **Opaque blobs (`reasoningOpaque`, `encryptedContent`) eat space in `RawSession`.** ~20KB per assistant turn × 100 turns = 2MB per session in memory. Mitigation: parser can be configured to drop these (`--skip-opaque` flag at load time, Phase 2).
- **NEG-005**: **Privacy surface large.** Just deserializing events.jsonl pulls user prompts, file paths, tool args, code, reasoning into memory. Mitigation: never serialize these to disk without explicit user opt-in; synthetic fixtures only (ADR-0003); no telemetry uploads (NG-11 in spec).
- **NEG-006**: **`#[serde(other)]` swallows unknown types silently.** A future event with semantic value (say `session.cost_alert` carrying real-time spend signals) would land in `Unknown` and never be analyzed. Mitigation: parser emits `tracing::trace!` for `Unknown` events; periodic smoke test reports their wire-format `type` strings.

## Alternatives Considered

### Translate `/usr/lib/node_modules/@github/copilot/copilot-sdk/types.d.ts` Zod schemas into Rust

- **ALT-001**: **Description**: Use ts-rs or hand-translation to convert the SDK's `.d.ts` declarations and Zod schemas into matching Rust types. Get GitHub-blessed schema for free.
- **ALT-002**: **Rejection Reason**: (a) `LICENSE.md §3` of the SDK explicitly forbids "translate, or create derivative works"; (b) SDK only declares 8 of 17+ actual event types — incomplete; (c) SDK types are for **SDK consumers** (live-session events) not internal telemetry stream — wrong abstraction layer.

### Single normalized `Event` enum that all agent adapters target

- **ALT-003**: **Description**: Define one `agentprof_core::Event` enum that's the union of every possible event across all current and future agents (Copilot, Claude, Codex, Gemini, ...). Each adapter maps its native wire format to this enum.
- **ALT-004**: **Rejection Reason**: (a) Copilot has `hook.start`, Claude doesn't — does Claude's adapter emit `Event::Hook` never? Then it's a Copilot field on a "common" enum; (b) Claude has `thinking` blocks, Copilot uses `reasoningOpaque`/`encryptedContent` — semantically different, would force lossy mapping; (c) future agents with unique signals (Codex with its `apply_patch` ceremony, Gemini with its multimodal flows) force enum changes that ripple across all downstream code; (d) per-agent native enum + shared Episode layer (this ADR's choice) achieves the same goal without forcing premature unification.

### Untyped `serde_json::Value` throughout — let analyzers do their own field probing

- **ALT-005**: **Description**: Don't define a typed enum at all. Parse each line as `serde_json::Value`. Analyzers branch on `value["type"]` and dig into `value["data"][field]` at runtime.
- **ALT-006**: **Rejection Reason**: (a) loses all compile-time benefits (typos in field names, type errors, completeness checks); (b) `agentprof-cli` would need duplicate "what events look like" knowledge spread across analyzer/exporter/TUI; (c) rustdoc and IDE support degrade — discoverability of the wire format vanishes; (d) violates `.github/instructions/rust.instructions.md` rule "prefer strong types over stringly-typed data."

### Generate types from a JSON schema we maintain by hand

- **ALT-007**: **Description**: Write a JSON Schema for events.jsonl in `crates/agentprof-adapters/schemas/copilot-events.schema.json`. Use `schemars` or `typify` to generate Rust types at build time.
- **ALT-008**: **Rejection Reason**: (a) extra build-time dependency for marginal value; (b) we still have to hand-write the schema (just shifts where the duplication lives); (c) JSON Schema lacks expressiveness for "this field is `Option` because it's missing in March-2026 sessions but present in May-2026" — comments needed anyway; (d) hand-written Rust types are read by humans more often than the schema would be.

### Use `prost`-style protobuf or other IDL

- **ALT-009**: **Description**: Define events.jsonl shape in `.proto` or similar IDL. Generate types. Maintain proto file in sync with observations.
- **ALT-010**: **Rejection Reason**: (a) wire format is JSON, not protobuf — translation friction at every variant; (b) Copilot CLI's source isn't using protobuf either (it's emitting JSON from JS); (c) overkill for ~20 variants that change rarely.

## Implementation Notes

- **IMP-001**: **`event.rs` should be split into smaller files** if it grows beyond ~500 lines. Suggested split: `event.rs` (enum definition), `event/payloads.rs` (data structs), `event/wire.rs` (deserialize helpers). Refactor only when needed.
- **IMP-002**: **Each payload struct gets rustdoc with `# Examples`** showing the JSON representation (per `.github/instructions/rust.instructions.md` rustdoc rules and `missing_docs = "error"` in workspace lints). Doctest skeleton:
  ```rust
  /// Tool invocation begin event.
  ///
  /// # Examples
  ///
  /// ```
  /// use agentprof_adapters::copilot::CopilotEvent;
  /// let json = r#"{"type":"tool.execution_start","data":{"toolCallId":"tc1","toolName":"bash","arguments":{}},"id":"e1","timestamp":"2026-05-26T10:00:00Z","parentId":null}"#;
  /// let evt: CopilotEvent = serde_json::from_str(json).unwrap();
  /// matches!(evt, CopilotEvent::ToolExecStart(_));
  /// ```
  ```
- **IMP-003**: **Add `EventKind` enum mirroring variant tags** for cheap `match` in `derive_episodes`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum EventKind {
      SessionStart, SessionInfo, ModeChanged, ModelChange, PlanChanged, Shutdown,
      UserMessage, TurnStart, AssistantMessage, TurnEnd,
      ToolExecStart, ToolExecComplete, ToolUserRequested,
      HookStart, HookEnd, SkillInvoked, SystemMessage, Abort, Unknown,
  }
  impl CopilotEvent { pub fn kind(&self) -> EventKind { ... } }
  ```
  This lets `derive_episodes` match on `EventKind` (cheap copy) instead of borrowing into the full enum.
- **IMP-004**: **Implement `Event` trait** (from `agentprof-core::adapter::Event`):
  ```rust
  impl Event for CopilotEvent {
      fn kind(&self) -> EventKind { self.kind() }
      fn timestamp(&self) -> chrono::DateTime<chrono::Utc> { /* extract from envelope */ }
      fn parent_id(&self) -> Option<&str> { /* extract */ }
  }
  ```
- **IMP-005**: **Fixture authors must validate** each new fixture line via `serde_json::from_str::<CopilotEvent>(line)` (round-trip test). Catches typos early.
- **IMP-006**: **`tracing::warn!`** on `CopilotEvent::Unknown` deserialization in the parser to surface unrecognized event types in logs. Counted in `ParseWarning` aggregate.
- **IMP-007**: **Smoke test** (`#[ignore]`) that loads every `events.jsonl` under `$AGENTPROF_LOCAL_FIXTURES_DIR` (developer's local `~/.copilot/session-state/`) and asserts `Unknown` count == 0. Catches schema drift when Copilot adds new event types.
- **IMP-008**: **Variant table in this ADR is the single source of truth** for the wire format. Any code change touching variant shapes MUST update this table in the same commit (per `.github/instructions/update-docs-on-code-change.instructions.md`).

## References

- **REF-001**: `ADR-0001` — events-first MVP pivot that motivates this schema
- **REF-002**: `ADR-0003` — synthetic fixture strategy that consumes this schema
- **REF-003**: `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md` §4 FR-1 + §6.4 CopilotEvent table + §11 OQ-1
- **REF-004**: `crates/agentprof-adapters/src/copilot/event.rs` (to be created in M1.2) — implementation site
- **REF-005**: `crates/agentprof-adapters/tests/fixtures/copilot/` (to be created in M1.2) — fixture set validating this schema
- **REF-006**: `/usr/lib/node_modules/@github/copilot/package.json` — Copilot CLI v1.0.4+ artifact whose telemetry stream this schema describes (current local install is v1.0.54)
- **REF-007**: `/usr/lib/node_modules/@github/copilot/LICENSE.md` §3 — clean-room boundary
- **REF-008**: `~/.copilot/session-state/2fcbfbca-1d5e-4432-bc26-63942387df2c/events.jsonl` — older session, baseline observation (no `hook.*` / `skill.invoked`)
- **REF-009**: `~/.copilot/session-state/252068e5-ca16-4186-a181-719462643d83/events.jsonl` — current-session observation (all 17 variants present including `hook.*` / `skill.invoked` / `session.model_change` / `system.message`)
- **REF-010**: `.github/instructions/rust.instructions.md` — strong-typing preferences enforced by Stage 0 always-on rules
- **REF-011**: `.github/copilot-instructions.md` §10 rule 5 — "不要在 L1 文档里写函数级细节——那是 rustdoc 的事；不要在 rustdoc 里写跨 crate 决策——那是 L1/internals 的事" (this ADR sits in L3 internals; wire-format details are correct here, not in L1 architecture.md)
