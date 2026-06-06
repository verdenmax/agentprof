# Design — F2 ask_user pending detection

**Status:** approved
**Date:** 2026-06-05
**Scope:** `agentprof-core::analyzer::pending` (new) + `agentprof-tui::views::{flamegraph, roi}` + `agentprof-tui::watch`
**Tickets:** F2.1 (core helper) + F2.2 (Flame T-id Pending Yellow) + F2.3 (RoiView column + footer banner)

---

## 1. Motivation

User reported (recurring complaint dating back to early sessions):

> ask_user 比较特殊，有时候用户没有确认 AI 的回复就会一直在这里卡着

`ask_user` is the only tool in `USER_BLOCKING_TOOLS` today. When
Copilot CLI invokes `ask_user`, it emits a `tool.execution_start` for
`ask_user` and then BLOCKS waiting for the user to type a response in
the terminal. If the user is away from the keyboard, the session sits
stalled — the wire emits no further events until the user comes back.

In agentprof's `watch` mode this manifests as "latest turn never
progresses" with no obvious signal that *the agent is waiting on
you*, not stuck doing work. The user wants a visual hint
(`"⚠ ask_user pending for 1m23s"`) that breaks through their
tab-switch amnesia.

Sibling case: any tool stuck in `ToolCallStatus::OpenAtEndOfSession`
for an extended time without a matching `tool.execution_complete` is
also "pending" — though the threshold for `bash` running a test suite
(30 minutes is plausible) differs by an order of magnitude from
`ask_user` (where 30 seconds is already long).

This spec adds a single shared "is this call pending right now?"
helper in `agentprof-core` plus three TUI surfaces that consume it
(footer banner in watch mode, Flamegraph T-id color, RoiView Tool
cell color + counter in detail strip).

---

## 2. Goals / non-goals

### Goals

1. **Watch-mode live detection** — when the user runs
   `agentprof watch <session>`, a pending `ask_user` (> 30 s) or a
   pending non-blocking tool (> 5 min) surfaces as a footer banner so
   the user notices the moment they tab back to the terminal.

2. **Postmortem detection** — `analyze --export tui` colors past
   pending calls (calls that ended in `OpenAtEndOfSession` and crossed
   the per-tool threshold by session end) so users can see "I got
   stuck waiting N times last session".

3. **Zero schema change** — pending is a derived property
   `(call_status, now, threshold) → bool`, not a persistent state.

4. **Threshold-by-tool-class** — `USER_BLOCKING_TOOLS` (currently
   `["ask_user"]`) → 30 s threshold; any other tool → 5 min threshold.

5. **Single source of truth** — one `is_pending(call, tool_name, now)`
   helper used by all three TUI surfaces, parametrized on `now` so
   watch passes `Utc::now()` and postmortem passes session-end time.

### Non-goals

- **Hook pending detection** — hooks fire and complete near-instantly
  in real sessions; adding a separate threshold class for hooks adds
  surface area without value.

- **Config-file thresholds** — for this wave the thresholds are
  `pub const` so users compile-time-adjustable; future config wave
  can promote to `~/.config/agentprof/config.toml` (YAGNI for v0.1.0).

- **Multi-tool prioritization cleverness** — if both `ask_user` and
  `bash` are pending simultaneously, the banner lists both ranked by
  `is_user_blocking` then by elapsed time. No "ask_user always wins"
  hard-coding.

- **Notification system** (desktop / terminal bell) — out of scope;
  visual hints are sufficient.

- **Anything in Cross mode** — `watch aggregate ...` is cross-session
  aggregation; per-session "pending" is meaningless when buckets span
  many sessions. Cross-mode renderers do not consume the pending
  helper.

---

## 3. Architecture

Three commits, each scoped to one layer.

```
                          +----------------------------+
                          | agentprof-core             |
                          |  analyzer::pending         |  <- F2.1
                          |   - ASK_USER_THRESHOLD     |
                          |   - DEFAULT_THRESHOLD      |
                          |   - is_pending(c, t, now)  |
                          |   - pending_calls(eps, now)|
                          +-------------+--------------+
                                        |
                       +----------------+----------------+
                       |                |                |
                       v                v                v
        +----------------------+ +--------------------+ +------------------+
        | tui::views::flame    | | tui::views::roi    | | tui::watch       |
        |  t_id_status_color   | |  Tool cell color   | |  footer banner   |
        |   + Pending Yellow   | |   + detail strip   | |   pending line   |
        |   (F2.2)             | |   (F2.3 part 1)    | |   (F2.3 part 2)  |
        +----------------------+ +--------------------+ +------------------+
```

### 3.1 F2.1 — `agentprof-core::analyzer::pending`

New module. Public surface:

```rust
use chrono::{DateTime, Duration, Utc};
use crate::episode::{Episodes, tool::{ToolCall, ToolCallStatus}};

/// Time an `ask_user` (or any USER_BLOCKING_TOOLS entry) can be
/// open before pending detection fires.
pub const ASK_USER_THRESHOLD: Duration = Duration::seconds(30);

/// Time any non-USER_BLOCKING tool can be open (e.g. long `bash`)
/// before pending detection fires.
pub const DEFAULT_THRESHOLD: Duration = Duration::minutes(5);

#[must_use]
pub fn threshold_for(tool_name: &str) -> Duration;

#[must_use]
pub fn is_pending(
    call: &ToolCall,
    tool_name: &str,
    now: DateTime<Utc>,
) -> bool;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PendingCall<'a> {
    pub tool_name: &'a str,
    pub turn_id: Option<&'a str>,
    pub started_at: DateTime<Utc>,
    pub elapsed: Duration,
    pub is_user_blocking: bool,
}

#[must_use]
pub fn pending_calls<'a>(
    episodes: &'a Episodes,
    now: DateTime<Utc>,
) -> Vec<PendingCall<'a>>;
```

**Algorithm for `is_pending`:**

1. If `call.status` is not `ToolCallStatus::OpenAtEndOfSession`, return
   false.
2. `elapsed = now - call.span.started_at`.
3. `threshold = threshold_for(tool_name)`.
4. Return `elapsed >= threshold`.

**Algorithm for `pending_calls`:**

1. Iterate `episodes.tools` BTreeMap.
2. For each (tool_name, ToolEpisode), iterate `episode.calls`.
3. For each call, evaluate `is_pending(call, tool_name, now)`. If true,
   construct a `PendingCall { ... }`.
4. Sort the result by: `is_user_blocking` desc (user-blocking first),
   then `tool_name` asc, then `started_at` asc. Deterministic order so
   renderers don't see flicker frame to frame.

**`now` parameter — the two use cases:**

- **watch mode** (live): `Utc::now()` at the top of each render frame.
  Pending status is re-evaluated each frame; a call becomes "pending"
  the frame after the threshold elapses.

- **postmortem** (analyze): `Utc::now()` would be wrong — the session
  ended hours / days ago, so every open call would be "pending". Use
  `meta.ended_at` if available, falling back to the last event's
  timestamp. For F2.1 we accept `now` as an explicit parameter; the
  callers in F2.2 / F2.3 decide the right value per context.

### 3.2 F2.2 — Flamegraph T-id `Pending` Yellow color

`views::flamegraph::t_id_status_color` (F1.10) gains a fourth status
slot. The precedence becomes (highest → lowest):

| Rank | Condition | Color |
|---|---|---|
| 1 | `TurnStatus::Aborted(_)` | `Color::Red` |
| 2 | **NEW: any tool_call in the turn is pending (now)** | `Color::Yellow` |
| 3 | Open / in-flight (`ended_at.is_none()`) | `Color::DarkGray` |
| 4 | Thinking-only (closed + no tool_calls) | `Color::Blue` |
| 5 | Otherwise | `None` (default) |

**Why above Open**: a turn with a pending `ask_user` IS open
(`ended_at = None`), but the more specific signal "pending" wins
over the generic signal "open".

**Why below Aborted**: an aborted turn that also had a pending call
is, in the end, aborted — `Red` is the more useful signal for the
user reviewing history.

**Implementation:**

Signature change: `t_id_status_color` gains 3 new parameters:

```rust
pub fn t_id_status_color(
    turn: Option<&Turn>,
    episodes: &Episodes,           // NEW (F2.2)
    now: DateTime<Utc>,            // NEW (F2.2)
) -> Option<Color>;
```

Inside, before the existing Open branch, iterate `turn.tool_calls`,
look up the matching `ToolEpisode.calls[ref.index]`, and call
`is_pending`. If any returns true → `Color::Yellow`.

Caller (`build_row`) already has `episodes: &Episodes` in scope and
passes `now` from `render_into` (new top-of-frame `Utc::now()`).

### 3.3 F2.3 part 1 — RoiView Tool cell color + counter

`views::roi::render_unified_table` consumes the F2.1 helper to color
the Tool cell of any tool currently with pending calls. The color hint
composes additively with the existing F1.13 `failure_severity_color`:

| failure | pending | result |
|---|---|---|
| > 50% Red | any | **Red** (failure wins — broken tool is worse than slow tool) |
| Yellow | any | **Yellow** (already Yellow regardless of pending) |
| None | true | **Yellow Bold** (pending hint) |
| None | false | default |

In other words: pending only changes color if no failure signal was
firing. A tool with both `> 50% fail` AND a pending call already
reads Red — adding "also pending" Bold doesn't help.

**Detail strip update**: when a row is selected, the detail strip's
existing "recent 5 calls" line gets a prefix `⚠ N pending` if any
calls are currently pending for that tool. Example: `⚠ 1 pending (45s) · t1 (120ms✓) t3 (85ms✓) ...`.

### 3.4 F2.3 part 2 — Watch footer banner

`watch.rs::render_into` already reserves a footer row when
`last_error.is_some()`. F2.3 part 2 extends that reservation to also
fire when `pending_calls(episodes, Utc::now())` returns non-empty.
Precedence:

1. If `last_error.is_some()` → reload-error banner (existing).
2. Else if `pending` non-empty → pending banner (NEW).
3. Else → no footer row.

**Banner format:**

- 1 pending call:
  ```
  ⚠ <tool_name> pending for <elapsed_human_short> — <hint>
  ```
  - `<hint>` = `"your input needed"` for `is_user_blocking`, else
    `"check this tool"`.
- N >= 2 pending:
  ```
  ⚠ <N> calls pending: <tool_name>(<elapsed>) <tool_name>(<elapsed>) ...
  ```
  - Truncates with `+K more` past the line budget (mirrors
    `selected_turn_footer_line`'s convention).

**Color**: `Color::Yellow + Modifier::BOLD` (matches the F2.2 T-id
color so users connect the two signals).

**Lifecycle**: re-evaluated every frame (cheap — iterates only the
already-bounded `episodes.tools` BTreeMap). No persistent state on
`WatchViewState`.

---

## 4. Test plan

### 4.1 F2.1 — `analyzer::pending::tests`

7 new unit tests:

| Test | Asserts |
|---|---|
| `is_pending_user_blocking_threshold_exact` | `ask_user`, elapsed = 30s → true |
| `is_pending_user_blocking_threshold_just_under` | `ask_user`, elapsed = 29.999s → false |
| `is_pending_default_threshold_exact` | `bash`, elapsed = 5min → true |
| `is_pending_default_threshold_just_under` | `bash`, elapsed = 4m59s → false |
| `is_pending_non_open_status_returns_false` | `ask_user` Success → false (even if 30 hours elapsed) |
| `is_pending_now_before_start_returns_false` | Defensive: `now < started_at` (clock skew) → false |
| `pending_calls_sorts_user_blocking_first_then_name_then_started` | Mixed pool: 1 ask_user + 2 bash (one started earlier) → ask_user first, then bash by started_at |
| `pending_calls_empty_episodes_returns_empty` | `Episodes::default()` → `vec![]` |

### 4.2 F2.2 — Flame T-id Pending Yellow

3 new unit tests in `views::flamegraph::tests`:

| Test | Asserts |
|---|---|
| `t_id_pending_turn_is_yellow` | Open turn with pending ask_user → `Color::Yellow` |
| `t_id_aborted_pending_red_wins_over_yellow` | Aborted turn with pending call → `Color::Red` (precedence) |
| `t_id_open_non_pending_still_darkgray` | Open turn with no pending calls → `Color::DarkGray` (regression guard for Open priority below Pending) |

### 4.3 F2.3 — RoiView color + footer banner

3 new tests:

| Test | Asserts |
|---|---|
| `roi_tool_cell_pending_only_is_yellow_bold` | Tool with pending call but 0 failures → Yellow Bold |
| `watch_runner_pending_banner_renders_when_calls_pending` | Inject WatchData::Single with a pending ask_user → render_into shows `⚠ ask_user pending` text |
| `watch_runner_pending_banner_suppressed_by_reload_error` | Both last_error AND pending → only the error banner renders |

### 4.4 Snapshot tests

No new snapshots required:
- Flamegraph snapshots use the char-buffer extractor — colors don't
  appear, so Yellow Pending doesn't break them.
- RoiView snapshots use the same extractor — color-only changes
  invisible.
- Watch footer text would appear in snapshots, but we have no
  existing watch render snapshot; F2.3 tests verify via direct
  buffer inspection.

If a future "watch render with pending banner" snapshot is desired,
add it under `tests/snapshots/views__watch_pending_banner.snap`.

---

## 5. Documentation

- **L1 architecture.md §5.1** — add `pending` to the rollups table:
  `pending_calls(&Episodes, now: DateTime<Utc>) -> Vec<PendingCall<'_>>`.
- **L2 README** (`crates/agentprof-tui/README.md`):
  - `views::flamegraph` row: T-id color table updated (Pending Yellow at rank 2).
  - `views::roi` row: append F2 description for the Tool-cell Pending color rule.
- **L2 README** (`crates/agentprof-core/README.md` if it exists, else
  module-level rustdoc in `analyzer/pending.rs`).
- **rustdoc**: each public item in `analyzer::pending` gets the
  standard `#[must_use]` + `# Examples` doctest.
- **Help overlay (`?`)**: add a row to the "RoiView Tool color"
  legend section: `Tool (yellow bold)  Tool has pending call(s) — see footer for details`.
- **CHANGELOG `[Unreleased] / Added`**: 1 entry covering all 3
  commits.

No ADR required per `.github/copilot-instructions.md` §5.5:
- No `>= 2 candidates seriously considered + needs documenting` —
  brainstorming Q1/Q2/Q3/Q4 all had clear recommendations user
  accepted without debate.
- No new pub trait method on `Event` / `Adapter`.
- Doesn't supersede an existing ADR.

---

## 6. Implementation order

3 commits, F2.1 → F2.2 → F2.3, each independently bisectable.

**Commit A — F2.1 core helper:**
1. New module `crates/agentprof-core/src/analyzer/pending.rs`.
2. `pub mod pending;` in `analyzer/mod.rs`.
3. 8 new unit tests (§4.1).
4. CHANGELOG entry (initial draft, finalize in F2.3 commit).

**Commit B — F2.2 Flame T-id Pending Yellow:**
1. `t_id_status_color` signature extended with `episodes` + `now`.
2. New precedence branch (rank 2) for Pending Yellow.
3. `build_row` already has `episodes` in scope; pass `now` from
   `render_into` (new top-of-frame `Utc::now()`).
4. 3 new unit tests (§4.2).
5. Update F1.10 tests that call `t_id_status_color(turn)` to pass
   the new 3-arg form (about 6 test sites).
6. Help overlay: update T-id color legend in `app/mod.rs`
   (4 entries → 5).

**Commit C — F2.3 RoiView + watch footer banner:**
1. RoiView Tool cell composition: F1.13 failure color OR Pending
   Yellow (failure wins per §3.3 table).
2. RoiView detail strip: prefix `⚠ N pending` when applicable.
3. Watch footer banner: `pending_calls(...)` evaluation in
   `render_into`; render Yellow text if non-empty AND no reload
   error.
4. Help overlay: add "RoiView Tool color" entry for Pending Yellow.
5. 3 new tests (§4.3).
6. L2 README + CHANGELOG final pass covering all 3 commits.

---

## 7. Alternatives considered

### 7.1 Detection scope

| Option | Verdict | Reason |
|---|---|---|
| **A. Both watch + postmortem** | **Selected** | Single helper, both use cases. Watch solves user's actual pain, postmortem is free. |
| B. Watch only | Rejected | Skips postmortem value; postmortem is ~30% extra work for 100% extra coverage. |
| C. Postmortem only | Rejected | Doesn't address user's actual complaint about live mode. |

### 7.2 Threshold model

| Option | Verdict | Reason |
|---|---|---|
| **A. Per-tool-class thresholds** | **Selected** | 30s for ask_user / 5min for bash matches reality; uniform threshold over-alerts on long bash. |
| B. Uniform 60s | Rejected | bash test suite would constantly alert. |
| C. Only ask_user | Rejected | Loses coverage of "MCP tool stuck" case. |

### 7.3 UI surfaces

| Option | Verdict | Reason |
|---|---|---|
| **A. Footer banner + Flame T-id + RoiView Tool** | **Selected** | 3 redundant signals = 100% notice; cheap because reuses F1.10/F1.13 frameworks. |
| B. Footer banner only | Rejected | User switching views misses the signal. |
| C. Flame T-id only | Rejected | Only visible in Flamegraph view. |

### 7.4 Schema impact

| Option | Verdict | Reason |
|---|---|---|
| **A. Reuse OpenAtEndOfSession + derived helper** | **Selected** | Pending is `(state, now, threshold)` — derived. Storing it persistently couples state with time. |
| B. New `ToolCallStatus::Pending { since }` variant | Rejected | Schema change; couples state + time; redundant with OpenAtEndOfSession. |
| C. Reuse status + analyzer-level `pending_tools(&Episodes)` fn | Rejected (taken half-way) | We have the fn; not adding a new persistent field. |

---

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Watch banner thrashes when ask_user oscillates around 30s threshold | `is_pending` uses `>=`, so 30.0001s stays true; clock jitter doesn't oscillate. |
| `Utc::now()` called many times per render frame causes minor perf hit | Cached at top of `render_into` as a local; passed to all callers (F2.2 signature change makes this explicit). |
| Postmortem `now` choice (meta.ended_at vs last event ts) ambiguity | Spec §3.1 makes this caller-choice. analyze passes `meta.ended_at` if available. |
| `t_id_status_color` signature change is breaking for external test code | Function is `pub` for crate use; no out-of-crate test crate consumes it. If it had, the migration would be `+ episodes + now` arguments — 2-line change per call site. |
| Pending detection across reload — call was pending before reload, no longer pending after | `is_pending` is re-evaluated each frame; reloads re-derive episodes; "no longer pending" naturally falls out. |

---

## 9. Out of scope (M3+ follow-ups)

- **Config-file threshold override** — promote consts to TOML field once config layer lands.
- **Terminal bell on pending transition** — `--bell-on-pending` flag.
- **Pending duration histogram** — would augment RoiView with "p50 pending duration" but needs aggregation across sessions (cross-session feature, not single-session).
- **Cross-mode pending surface** — defer until cross-mode has a per-session-drilldown UI.

---

## 10. References

- F1.10 (T-id status color framework): commit `6d44204`
- F1.13 (Tool cell failure-severity color): commit `90adecb`
- F1.7 Models view (`#[non_exhaustive]` patterns followed here): commit `3ef6797`
- `USER_BLOCKING_TOOLS` constant: `crates/agentprof-core/src/analyzer/tool_rank.rs`
- `ToolCallStatus::OpenAtEndOfSession`: `crates/agentprof-core/src/episode/tool.rs`
- Origin of user complaint: F1.5 brainstorm session, ~mid-conversation
