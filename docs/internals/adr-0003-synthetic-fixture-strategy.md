---
title: "ADR-0003: Synthetic-only fixture strategy — hand-crafted test data, no anonymizer tool, no real-data fixtures in git"
status: "Accepted"
date: "2026-05-26"
authors: "@verdenmax (project owner), AI assistant (Copilot CLI session 252068e5)"
tags: ["architecture", "decision", "testing", "privacy", "fixtures", "tdd"]
supersedes: ""
superseded_by: ""
---

# ADR-0003: Synthetic-only fixture strategy — hand-crafted test data, no anonymizer tool, no real-data fixtures in git

## Status

**Accepted**

## Context

The `CopilotAdapter` (ADR-0002) parses `~/.copilot/session-state/<uuid>/events.jsonl`. These files contain a wide privacy surface (ADR-0002 §"Privacy-relevant fields"):

1. **User prompts** (`UserMessage.content`, `UserMessage.transformedContent`, `HookStart.input.initialPrompt`) — natural-language input from the user, potentially containing trade secrets, code, project context.
2. **Filesystem identifiers** (`SessionStart.context.{cwd,gitRoot,branch,headCommit,repository}`, `HookStart.input.cwd`, `SessionInfo.message`) — local paths and git metadata that can de-anonymize the user.
3. **Tool I/O** (`ToolExecStart.arguments`, `ToolExecComplete.result.content` / `.detailedContent`) — actual code, file contents, shell command stdout. Highest sensitivity.
4. **Assistant reasoning** (`AssistantMessage.reasoningText`, `AssistantMessage.content`) — LLM chain-of-thought, which often quotes user code or contains sensitive deliberation.
5. **Opaque GitHub-encrypted blobs** (`AssistantMessage.reasoningOpaque`, `AssistantMessage.encryptedContent`) — encrypted by GitHub, contents not interpretable by us; redistributing them in fixtures has ambiguous license status.

To test the parser, derivation algorithms, TUI rendering, exporters, and CLI integration, we need **test fixtures** that the CI test suite consumes. These fixtures:

- live in `crates/agentprof-adapters/tests/fixtures/copilot/<scenario>/events.jsonl`
- are committed to `git`
- are pulled by every contributor when cloning
- become part of the public artifact when the repo is open-sourced

Hence the central question: **how do we produce fixtures that are realistic enough to catch parser bugs without leaking the developer's private session data into the public repo?**

During Stage 1 brainstorming, four candidate strategies were enumerated and one was selected.

## Decision

**Use 100% synthetic, hand-crafted fixtures. No anonymizer tool. Fixtures committed to git contain zero bytes derived from real user sessions.**

### Concrete decisions

1. **9 fixture scenarios committed**, each in its own directory under `crates/agentprof-adapters/tests/fixtures/copilot/`:
   | Fixture | Purpose |
   |---|---|
   | `minimal/` | Smallest valid session: start → user → turn → shutdown |
   | `builtin-tools-only/` | Only `bash` + `str_replace_editor` etc. |
   | `with-mcp-calls/` | `mcp__github__*`, `mcp__filesystem__*` tool calls |
   | `with-skill-invoked/` | `skill.invoked` events + subsequent tool attribution |
   | `with-hooks-heavy/` | 30+ `hook.start`/`hook.end` pairs |
   | `with-aborts/` | Abort events at different points (during tool, hook, turn) |
   | `with-mode-transitions/` | Interactive → Plan → Autopilot transitions |
   | `live-truncated/` | No shutdown, has `inuse.<pid>.lock`, last line incomplete |
   | `corrupt/` | One broken JSON line + valid events |

2. **Each fixture directory contains**:
   - `events.jsonl` — hand-authored JSONL, every line independently parseable (except where intentionally broken for `corrupt/`)
   - `expected.json` — serialized `Episodes` for snapshot comparison via `insta`
   - `README.md` — one-paragraph description: what this fixture proves, which FRs it covers

3. **Hand-authored synthetic content rules**:
   - Paths use `/tmp/agentprof-fixture/<project-slug>` (clearly fake)
   - User messages use placeholder text like `"[fixture-prompt-1]"` or short non-sensitive English sentences
   - Tool arguments use minimal viable values (`{"path":"/tmp/agentprof-fixture/example.rs"}` etc.)
   - Tool results use short, well-known synthetic strings (`"file not found"`, `"3 matches found"`)
   - Reasoning text replaced with `"[synthetic reasoning placeholder]"`
   - Opaque/encrypted blobs use short base64-looking strings (e.g., `"AAAA..."` repeating) with realistic lengths
   - All UUIDs are stable hex like `00000000-0000-0000-0000-000000000001` for reproducibility

4. **No anonymizer tool in `xtask`.** The originally-suggested `cargo xtask anonymize <real-log>` command is **not built**. Reasons:
   - With synthetic-only fixtures, an anonymizer is not needed for the committed corpus
   - For developers who want to smoke-test against real local data (see IMP-002), the developer's own machine already has real `events.jsonl` — no transformation needed
   - Avoids maintaining anonymizer logic that, if bugged, could leak private data into commits — a single missed field is a privacy breach

5. **Local smoke tests (`#[ignore]`)** allow developers to run the test suite against their own real `~/.copilot/session-state/` data:
   ```rust
   #[test]
   #[ignore = "requires AGENTPROF_LOCAL_FIXTURES_DIR set to local Copilot session-state"]
   fn smoke_real_local_sessions() {
       let dir = std::env::var("AGENTPROF_LOCAL_FIXTURES_DIR").expect("...");
       for events_jsonl in walk(&dir, "events.jsonl") {
           let raw = parse_events_jsonl(&events_jsonl, /* is_live */ false);
           assert!(raw.is_ok(), "parser failed on real session {events_jsonl:?}");
           let unknown_count = count_unknown_events(&raw.unwrap());
           assert_eq!(unknown_count, 0, "schema drift: found Unknown events in real session");
       }
   }
   ```
   These run only when explicitly requested (`cargo test --include-ignored`). Output never committed. Directory used (`$AGENTPROF_LOCAL_FIXTURES_DIR`) added to `.gitignore`'s example block.

6. **Fixture authorship discipline**:
   - Every new variant added to `CopilotEvent` (ADR-0002) requires updating at least one fixture to exercise it
   - Every new derivation rule in `derive_episodes` requires a fixture line that exercises the rule
   - Fixture changes go through code review like any other code

## Consequences

### Positive

- **POS-001**: **Zero privacy risk in committed test data.** No bytes of real user data leave the user's machine via the git repo. The privacy promise of agentprof ("nothing leaves your machine") is consistent end-to-end including its development artifacts.
- **POS-002**: **No anonymizer maintenance burden.** Anonymizers are infamously prone to "missed one field" bugs. By not having one, we eliminate an entire category of privacy failure modes.
- **POS-003**: **Tests are deterministic and human-readable.** A hand-authored 5-line `events.jsonl` is easier to reason about than a 200-event anonymized real session. Snapshot comparison failures point to the exact synthetic event that changed behavior.
- **POS-004**: **CI does not require user data.** Anyone forking the repo can run the full test suite without first having a Copilot CLI installation or session history. Lowers contributor onboarding friction to zero.
- **POS-005**: **Scenario coverage is explicit and reviewable.** Each fixture directory's README states the scenario being tested. Adding a new edge case (e.g., "what if abort fires during a hook") means adding a new fixture, which surfaces in code review.
- **POS-006**: **Round-trip discipline**: every line of every fixture must `serde_json::from_str::<CopilotEvent>` cleanly. Forces fixture authors to mirror the wire format precisely; mismatches caught immediately, not at runtime.
- **POS-007**: **Real-world catch via smoke tests is opt-in but available.** Developers worried about schema drift between Copilot CLI versions can periodically run smoke tests against their actual data; this provides a safety net without committing to maintaining real-data fixtures.

### Negative

- **NEG-001**: **Synthetic fixtures may miss real-world quirks.** The Copilot CLI's actual output may have undocumented edge cases (extra fields, surprising null patterns) that hand-authored fixtures don't replicate. Mitigation: smoke tests (IMP-002) catch drift; production users are encouraged to file issues with anonymized excerpts.
- **NEG-002**: **Initial fixture-authoring cost.** 9 scenarios × ~30 lines × careful field-by-field authorship = ~half a day of work in M1.2, before parser tests can run. Compensated by faster TDD cycles after fixtures exist.
- **NEG-003**: **Fixture maintenance grows with `CopilotEvent` schema.** Adding a variant requires updating fixtures. Without an anonymizer, no "generate from real session" shortcut. Mitigation: only 17 variants now; growth rate is slow (Copilot doesn't add events frequently); ADR-0002 IMP-008 makes wire-format/fixture sync a code-review check.
- **NEG-004**: **`live-truncated/` and `corrupt/` fixtures need creativity** to model "partial last line being written" and "broken mid-stream" without anonymizing a real interrupted session. Authoring requires understanding the parser's error recovery paths well — easier once parser exists, slightly chicken-and-egg.
- **NEG-005**: **No "anonymize my real session and contribute it back" workflow** for community contributors. Mitigation: clear documentation in fixture `README.md` says "send us synthetic fixtures or describe the scenario in an issue; we'll author the synthetic version."
- **NEG-006**: **`expected.json` snapshot files can drift if fixture or analyzer changes.** Standard `insta` workflow handles this: `cargo insta review` shows diffs; reviewer accepts intentional changes, rejects regressions.

## Alternatives Considered

### Anonymize real local sessions, then commit anonymized files

- **ALT-001**: **Description**: Build `cargo xtask anonymize <real-log>` that reads a real `events.jsonl`, redacts known-sensitive fields (paths → `/tmp/proj`, prompts → lorem ipsum, tool args → `<REDACTED>`, paths in tool results, etc.), and writes a safe version. Commit the safe version as fixture. Have a test suite that asserts the anonymizer correctly scrubs all sensitive fields.
- **ALT-002**: **Rejection Reason**: (a) **single missed field = privacy breach**; anonymizer logic must be perfect to be safe, and "perfect" is an unreachable target for free-form JSON with deeply-nested arbitrary content; (b) anonymizer becomes a critical-correctness dependency requiring its own extensive test coverage; (c) anonymizer logic must be updated every time Copilot adds a new event type with new fields — implicit version-coupling; (d) opaque blobs (`reasoningOpaque`, `encryptedContent`) have ambiguous license status — distributing them, even from "user's own session", is questionable; (e) anonymized prose still reveals authorship style, code conventions, project domain — partial anonymization is theatrical security.

### Hybrid: synthetic for CI + anonymizer for local dev smoke tests

- **ALT-003**: **Description**: Synthetic fixtures (this ADR's choice) committed for CI, **plus** `cargo xtask anonymize` for developers to generate local fixtures from their real data, gitignored.
- **ALT-004**: **Rejection Reason**: (a) the local-smoke-test purpose is fully served by reading the developer's **own real** sessions directly (no anonymization step needed — the files never leave their machine), which is what IMP-002 specifies; (b) building anonymizer infrastructure for the local-dev case adds code without changing privacy properties; (c) keeping it minimal — "real data when local, synthetic data when committed" — eliminates one tool from the maintenance burden.

### Schema-only fixtures: replace all string content with `<REDACTED:len=N>` placeholders

- **ALT-005**: **Description**: Parse a real `events.jsonl`, replace every string field's value with `<REDACTED:len=N>` where N is the original length. Commit the resulting structurally-correct but content-stripped fixture.
- **ALT-006**: **Rejection Reason**: (a) string fields like `tool.toolName` are NOT private (they're tool identifiers like `bash`, `view`, `mcp__github__search`) and replacing them strips real test value; (b) `ToolEpisode` tests assert correctness around `name` strings; stripping them prevents most analyzer testing; (c) loses event-type names (`session.start` etc.) which are needed for `serde_json::from_str` to discriminate variants; (d) what's left is mostly metadata — not enough signal to validate parser correctness; (e) basically forces fall-back to scenario-based hand-crafted fixtures anyway, with extra steps.

### No fixtures — rely on integration tests run by developers locally

- **ALT-007**: **Description**: Don't commit any fixtures. Tests are all `#[ignore]` and require local Copilot data to run. CI just does compilation + clippy + a few unit tests on pure functions.
- **ALT-008**: **Rejection Reason**: (a) defeats the purpose of CI — regressions in parser/analyzer would only be caught when a developer happens to remember to run smoke tests; (b) contributors without Copilot CLI can't verify their changes; (c) snapshot tests can't run; (d) violates `.github/copilot-instructions.md` §5 Stage 4 TDD principle — needs reliable test infrastructure.

## Implementation Notes

- **IMP-001**: **Fixture authorship happens in M1.2** before parser implementation. Order:
  1. Hand-author `minimal/events.jsonl` (5 lines, simplest valid).
  2. Implement `CopilotEvent` enum + parser to load it.
  3. Author next fixture (e.g., `builtin-tools-only/`), extend parser as needed.
  4. Iterate. Each fixture exercises one new parser code path.

- **IMP-002**: **Local smoke test setup** documented in `crates/agentprof-adapters/README.md`:
  ```bash
  # Run smoke tests against your own real local data:
  export AGENTPROF_LOCAL_FIXTURES_DIR=~/.copilot/session-state
  cargo test -p agentprof-adapters --test smoke -- --include-ignored
  ```
  These tests assert (a) every real `events.jsonl` parses without error, (b) no `Unknown` events appear (schema-drift detector), (c) no `ParseWarning::Unclosed*` for non-live sessions (state-machine correctness).

- **IMP-003**: **`.gitignore` add** (in M1.2 commit):
  ```
  # Local smoke-test target directories (developer-specific; never commit)
  /local-fixtures/
  /smoke-data/
  ```
  Plus the comment block in `CONTRIBUTING.md` explaining the contract: "fixtures in `crates/*/tests/fixtures/` are always synthetic; for real-data smoke tests, set `AGENTPROF_LOCAL_FIXTURES_DIR` to your own untouched data."

- **IMP-004**: **Fixture round-trip validation test** (runs in CI):
  ```rust
  #[test]
  fn every_fixture_line_parses_as_copilot_event() {
      for fixture_path in walk("tests/fixtures/copilot", "events.jsonl") {
          if fixture_path.parent().unwrap().file_name() == Some("corrupt".as_ref()) { continue; }
          for (line_no, line) in BufReader::new(File::open(&fixture_path).unwrap()).lines().enumerate() {
              let line = line.unwrap();
              if line.trim().is_empty() { continue; }
              let parsed: Result<CopilotEvent, _> = serde_json::from_str(&line);
              assert!(parsed.is_ok(), "fixture {fixture_path:?} line {line_no} failed to parse: {parsed:?}");
          }
      }
  }
  ```
  Catches typos in fixture authorship before any other test runs.

- **IMP-005**: **Fixture `README.md` template**:
  ```markdown
  # Fixture: with-aborts

  ## Purpose
  Validates abort attribution: links `abort` events to the most-recently-opened Turn/Tool/Hook.

  ## Scenarios covered
  - Abort during tool execution
  - Abort during hook execution
  - Abort between turns (attaches to session-level `Episodes.aborts`)

  ## Functional Requirements exercised
  - FR-2.8 (abort attribution)
  - FR-1.8 (parse warnings don't abort file)

  ## Expected snapshots
  See `expected.json` for the `Episodes` output that this fixture should produce.
  Compared via `insta::assert_json_snapshot!`.
  ```

- **IMP-006**: **`expected.json` review process**: when fixture or analyzer logic changes, `cargo insta review` shows the diff. Reviewer must judge: intentional change (accept) or regression (reject, fix code). Document this workflow in `CONTRIBUTING.md`.

- **IMP-007**: **Synthetic skill content** for `with-skill-invoked/`: use a short fake skill text (≤200 chars) like `"# Skill: synthetic-example\nThis is a placeholder skill for fixture testing.\nFollow these steps: 1. ... 2. ... 3. ...\n"`. **Never** copy real skill text from `~/.copilot/installed-plugins/` — those are vendored from `obra/superpowers` and have their own license; while reuse may be allowed, keeping fixtures synthetic eliminates any provenance question.

- **IMP-008**: **Schema-drift response**: when smoke tests fail because Copilot CLI emits a new event type or field, the response order is:
  1. Document the new shape in `ADR-0002` (§Variant table) — same commit as parser update
  2. Add `Unknown`-handling test ensuring forward-compat fallback still works
  3. Add or update a fixture exercising the new shape
  4. Update parser + analyzer to handle the new variant (if semantically meaningful)
  5. CHANGELOG entry `feat(adapters): support <new event type> in Copilot adapter`

## References

- **REF-001**: `ADR-0001` — events-first pivot establishing the testing-discipline context
- **REF-002**: `ADR-0002` — `CopilotEvent` enum that fixtures must exercise
- **REF-003**: `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md` §9 Testing Strategy + §11 OQ-10 (privacy of `HookStart.input.initialPrompt`)
- **REF-004**: `crates/agentprof-adapters/tests/fixtures/copilot/` — directory created in M1.2
- **REF-005**: `crates/agentprof-adapters/README.md` (M1.2 doc update) — documents local smoke test usage
- **REF-006**: `CONTRIBUTING.md` (M1.2 doc update) — adds "fixtures are synthetic, never anonymize real data" rule
- **REF-007**: `.gitignore` (M1.2 update) — excludes any local-fixtures directories
- **REF-008**: `.github/copilot-instructions.md` §5 Stage 4 — TDD requirement; fixture authorship is part of the test-first discipline
- **REF-009**: `xtask/README.md` — confirms scope: **no** `anonymize` subcommand; xtask stays minimal for MVP
- **REF-010**: `insta` crate documentation — snapshot testing tool used for `expected.json` comparison

---

## Update §2026-05-30: fixture count + variant count

This ADR's §NEG-003 referenced a 17-variant `CopilotEvent` and 9 committed fixtures (the M1.2 baseline). After M1.3 + M1.4 + 4 followups, the variant count is **28 named + `Unknown` = 29** (see ADR-0002 Update) and the fixture count is **12** (`crates/agentprof-adapters/tests/fixtures/copilot/`):

| Added after M1.2 | Purpose |
|---|---|
| `orphan-events/` | M1.3 — `derive_episodes` orphan / abort path |
| `cross-turn-tool/` | M1.4 — ADR-0005 D-2 commit-call-turn-divergence fix |
| `with-post-tool-use-hooks/` | M1.4 post-output-audit — three `Option<String>` schema fixes (HookInput.source / UserMessageData.source / AssistantMessageData.turn_id) |

The synthetic-only premise of this ADR is unchanged; xtask anonymize still does not exist and remains "by-convention only" (see [`docs/features/privacy.md`](../features/privacy.md) §5).
