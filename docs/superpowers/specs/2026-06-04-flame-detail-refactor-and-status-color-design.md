# Design — FlamegraphView detail-block refactor + T-id status color coding

**Status:** approved
**Date:** 2026-06-04
**Scope:** `agentprof-tui::views::flamegraph`
**Tickets:** F1.9 (detail block) + F1.10 (T-id status color)

---

## 1. Motivation

After F1 (TurnDetailView), F1.5 (thinking-only Blue marker), F1.6 (per-turn
output tokens), F1.7 (Models view), F1.8 (sticky header) shipped, the
FlamegraphView's bottom 3-row bordered "Detail" block had become low-value
real estate. It displayed:

```
Turn 00000000-0000-0000-0000-000000000202 | model=gpt-5-mini mode=Interactive out_tokens=10 tools=2
```

Three of the five fields are **fully redundant with content visible
elsewhere on screen**:

| Field | Where else it's shown |
|---|---|
| `Turn <UUID>` | The flame row's `Tn` label (1-based, far more user-friendly) |
| `out_tokens=N` | Flame row's `OutTK` column (since F1.6) |
| `tools=N` | Footer line enumerates each tool by name + duration |

Only `model=...` and `mode=...` were truly new information for that row.
3 rows of screen real estate (1 content + 2 borders) for 2 fields of new
data is wasteful — every other terminal row could fit one more turn in
the gantt.

Separately, the user reported the column header `Token` was ambiguous
(F1.8 → renamed to `OutTK` in `2028504`). The same kind of friction
applied to status visibility: aborted turns were marked with a `UNDERLINE`
modifier which is hard to distinguish from selected (`REVERSED`) on many
terminal themes, and open / in-flight turns had no distinguishing
marker at all. Watch-mode users had no way to see "this turn is still
running" vs "this turn ended" without reading the duration column.

This spec consolidates both into a single brainstorm + design pass:

- **F1.9** — replace the 3-row bordered Detail block with a single
  no-border meta line that surfaces new high-value fields and drops
  the duplicates.
- **F1.10** — color-code the 5-char T-id portion of each flame row's
  prefix to encode turn status (Aborted = Red, Open = DarkGray) and
  tighten F1.5's Blue marker to the same 5-char region.

---

## 2. Goals / non-goals

### F1.9 — detail block refactor

**Goals:**

1. Reclaim 2 rows of vertical space (3-row detail → 1-row meta).
2. Eliminate the three redundant fields (`Turn <UUID>`, `out_tokens`,
   `tools=N`).
3. Surface 3 new high-value fields the user cannot get elsewhere on
   the same screen: relative start time, precise turn duration
   (uncompacted), tool call count.
4. Preserve `model` and `mode` (the two valuable fields of the old
   detail block).
5. Graceful degradation on narrow terminals (≤ 80 cols).
6. No new dependencies, no breaking API changes outside the tui crate.

**Non-goals:**

- Surfacing turn-level **input** or **cache** tokens. Copilot wire
  does not expose these per-turn (see ADR-0012); only session-level
  via the Models view.
- Surfacing tool call **success / error** counts. The data path needs
  more work (separate from this spec); a future M4 follow-up.
- Restoring the bordered block as an option.

### F1.10 — T-id status color

**Goals:**

1. Visually distinguish at-a-glance the three most common "weird"
   turn states: aborted, in-flight, thinking-only.
2. Keep the existing `PREFIX_WIDTH=24` invariant — no growth, no
   sigil character that would force a `gantt_width -= 1` change.
3. Compose additively with existing `REVERSED` (selected) and
   `UNDERLINED` (aborted) modifiers. Color-blind users still see
   aborted via `UNDERLINED`.
4. Tighten F1.5's thinking-only Blue to the same 5-char region for
   consistency (currently colors the whole 24-char prefix — overkill).

**Non-goals:**

- Coloring the `█` / `░` / `·` cell characters by status. Cell colors
  are owned by `cell_style` for per-source coloring; status belongs
  in the row prefix.
- Per-tool-call status (a Yellow Errored bucket). Same data-path
  argument as F1.9 non-goal.
- Background color encoding. Adds visual clutter for one signal.

---

## 3. Architecture

Both changes are scoped to `crates/agentprof-tui/src/views/flamegraph.rs`.

### 3.1 F1.9 layout change

```
                        ┌────────────────────────────┐
                        │ Block " Flamegraph (1/3) " │
                        │ ┌────────────────────────┐ │
   inner / chunks[0]    │ │ header(1)              │ │  (existing F1.8 header)
                        │ │ ─────────────────────  │ │
                        │ │ rows(min)              │ │  (gantt rows; +2 visible)
                        │ │ ─────────────────────  │ │
                        │ │ footer(1)              │ │  (selected_turn_footer_line)
                        │ └────────────────────────┘ │
                        └────────────────────────────┘
   chunks[1] (NEW)       <single no-border 1-row meta line>
```

**Render layout:**

- **Old:** `[Constraint::Min(1), Constraint::Length(3)]`
- **New:** `[Constraint::Min(1), Constraint::Length(1)]`

The `Block::default().borders(Borders::ALL).title(" Detail ")` widget at
chunks[1] is **deleted**; replaced with a plain `Paragraph` (no Block,
no borders).

### 3.2 F1.9 meta-line format

```
T<n> · +<rel> in · <dur> · <N> calls · model=<m> · mode=<Plan|Interactive>
```

**Example:**
```
T3 · +1m24s in · 5.0s · 3 calls · model=gpt-5-mini · mode=Interactive
```

**Field semantics:**

| Field | Source | Formatter | Notes |
|---|---|---|---|
| `T<n>` | `state.flame_selected + 1` | direct | 1-based, parity with flame row label |
| `+<rel> in` | `turn.started_at - session.started_at` | `human_short` | "+0s" if first turn / unknown |
| `<dur>` | `turn.ended_at - turn.started_at` (or "—" if open) | `human_short` | Uncompacted (flame row is fixed-width) |
| `<N> calls` | `turn.tool_calls.len()` | decimal | 0 → `0 calls` (not omitted) |
| `model=<m>` | `turn_summary_row.model` | direct (string) | "model=—" if `None` |
| `mode=<m>` | `turn_summary_row.mode` | `{:?}` debug | "mode=—" if `None` |

**Separator** = `" · "` (matches footer convention).

**Empty / fallback:** if `state.flame_selected` out of range, render
`"(no turn selected)"` (parity with existing footer fallback).

### 3.3 F1.9 narrow-terminal truncation

When the formatted line exceeds `meta_rect.width`, drop fields **from
the right** in this priority order:

```
mode  →  model  →  N calls  →  dur  →  +rel in  →  T<n>
(low priority)                                  (high priority)
```

Implementation: build a `Vec<String>` of pre-formatted segments, then
call a reused `fit_entries` analog (the footer already has this pattern
in `selected_turn_footer_line` → `fit_entries`; we extract a generic
helper).

### 3.4 F1.10 prefix span split

`build_row` currently produces ONE styled span for the entire 24-char
prefix, with `prefix_style` carrying composed modifiers (REVERSED if
selected, Blue fg if thinking-only).

**Change:** split the prefix into **two spans**:

- `span_tid` = the first **5 chars** of the prefix (the right-aligned
  T-id portion, e.g. `"  T39"`). Carries `t_id_style`.
- `span_rest` = the remaining **19 chars** (` <Duration> <OutTK>  `).
  Carries `rest_style`.

`PREFIX_WIDTH = 24` invariant unchanged (5 + 19 = 24).

### 3.5 F1.10 color precedence

Computed in one pass within `build_row`, applied to `span_tid` only:

```rust
let t_id_fg = match turn {
    Some(t) if matches!(t.status, TurnStatus::Aborted(_)) => Some(Color::Red),
    Some(t) if t.ended_at.is_none()                       => Some(Color::DarkGray),
    Some(t) if t.tool_calls.is_empty()                    => Some(Color::Blue), // thinking-only
    _                                                     => None,
};
```

Precedence (highest → lowest):
1. **Aborted** → Red
2. **Open / in-flight** (no `ended_at`) → DarkGray
3. **Thinking-only** (closed + no tools) → Blue (existing F1.5 behavior,
   now confined to 5-char region)
4. **Default** (closed + has tools) → no fg override

### 3.6 F1.10 composition with existing modifiers

| Modifier | Applies to | Source |
|---|---|---|
| `REVERSED` | both spans (entire prefix) | `selected` row in flame view |
| `UNDERLINED` | both spans | `TurnStatus::Aborted(_)` (preserved as color-blind backup) |
| `Color::Red` fg | span_tid only | F1.10 — status |
| `Color::DarkGray` fg | span_tid only | F1.10 — status |
| `Color::Blue` fg | span_tid only | F1.10 (was F1.5 — full prefix) |

All three compose additively. A selected-aborted-thinking-only turn
shows: REVERSED + UNDERLINED full prefix + Red T-id (Red wins over
Blue per precedence rule).

---

## 4. Test plan

### 4.1 F1.9 unit tests (new)

| Test | Asserts |
|---|---|
| `meta_line_includes_all_fields_when_full_width` | Wide width (120) → all 6 fields present |
| `meta_line_drops_mode_first_on_narrow_width` | Width = 50 → mode dropped, model preserved |
| `meta_line_keeps_t_id_relative_dur_when_extremely_narrow` | Width = 20 → only top 3 priority fields |
| `meta_line_open_turn_renders_dash_for_duration` | `ended_at = None` → duration = "—" |
| `meta_line_no_model_renders_dash_for_model` | `model = None` → `model=—` |
| `meta_line_fallback_for_out_of_range_selection` | Returns `"(no turn selected)"` |

### 4.2 F1.10 unit tests (new)

| Test | Asserts |
|---|---|
| `t_id_aborted_turn_has_red_fg` | `TurnStatus::Aborted(_)` → span_tid fg = Red |
| `t_id_open_turn_has_darkgray_fg` | `ended_at.is_none()` → span_tid fg = DarkGray |
| `t_id_aborted_open_turn_red_wins_over_darkgray` | Both true → Red (precedence) |
| `t_id_thinking_only_blue_only_on_5_chars` | Thinking-only → only span_tid Blue; span_rest no Blue |
| `t_id_completed_with_tools_default_color` | Normal turn → no fg override |
| `t_id_aborted_underline_still_applied` | Aborted → span gets UNDERLINED in addition to Red fg |

### 4.3 F1.5 test updates (regression)

Existing `build_row_thinking_only_turn_has_blue_prefix` and friends
(currently in `flamegraph::tests`) assert Blue is on the **whole**
prefix span. Update to assert Blue is on **span_tid only** (per F1.10
tightening).

### 4.4 Snapshot tests

| Snapshot | Update reason |
|---|---|
| `views__flamegraph__with_aborts` | (1) Detail block 3-row → 1-row meta; (2) aborted turn T-id now has Red fg in the styled snapshot |
| `views__flamegraph__cross_turn_tool` | (1) Detail block 3-row → 1-row meta |

Both snapshots will be regenerated and accepted in the implementation
commits.

---

## 5. Documentation

- **L2 README** (`crates/agentprof-tui/README.md`):
  - `views::flamegraph` row in modules table: drop "Detail" block
    mention, add "1-row meta line below" + status color rules.
  - Help overlay legend (if it describes status colors) update.
- **rustdoc**:
  - `render()`: new section on meta line + status-color span split.
  - `build_row()`: status precedence table.
  - New `format_meta_line()` (or whatever helper name): full doc +
    `# Examples`.
- **CHANGELOG `[Unreleased] / Added`**:
  - F1.9 entry (detail block refactor)
  - F1.10 entry (T-id status color)
- **CHANGELOG `[Unreleased] / Changed`**:
  - F1.5 thinking-only Blue marker now applies to the 5-char T-id
    only (was whole 24-char prefix).

---

## 6. Implementation order

Two independent commits, F1.9 first then F1.10.

**Commit A — F1.9 detail-block refactor:**
1. Extract a generic `fit_segments(prefix, segs, budget)` helper if
   the existing `fit_entries` doesn't fit cleanly. Otherwise reuse.
2. Add `format_meta_line(state, max_width) -> String` pure function.
3. Replace chunks layout + render Paragraph at chunks[1].
4. Delete `Block::default().borders(Borders::ALL).title(" Detail ")`.
5. Add 6 unit tests (§4.1).
6. Regenerate + accept 2 snapshots.
7. CHANGELOG + L2 README + rustdoc.

**Commit B — F1.10 T-id status color:**
1. Refactor `build_row` to produce 2 prefix spans instead of 1.
2. Add `t_id_status_color(turn) -> Option<Color>` helper.
3. Update F1.5 thinking-only logic to use the same helper (precedence
   integration).
4. Add 6 unit tests (§4.2) + update F1.5 tests (§4.3).
5. Regenerate + accept 2 snapshots (Red T-id appears).
6. CHANGELOG + L2 README + rustdoc.

---

## 7. Alternatives considered

### 7.1 Detail block fate (F1.9)

| Option | Verdict | Reason |
|---|---|---|
| A. Delete entirely; merge model+mode into footer | Rejected | Footer would overflow on narrow terminals; mixing per-call detail with row-level meta confuses semantics |
| **B. Compress to 1-line no-border, surface new fields** | **Selected** | Maximum space recovery (+2 rows) with no semantic mixing; clean separation |
| C. Keep 3-row but multi-column dashboard | Rejected | No space recovery; only marginally more useful than today |

### 7.2 Status sigil location (F1.10)

| Option | Verdict | Reason |
|---|---|---|
| **A. Color-code T-id, keep PREFIX_WIDTH=24** | **Selected** | No invariant break; faster visual scan than character matching; uses ratatui built-ins; existing UNDERLINED preserved as color-blind backup |
| B. Grow PREFIX_WIDTH 24 → 26 for `T3 ✓` sigil | Rejected | Breaks PREFIX_WIDTH invariant; gantt loses 2 cells; Unicode rendering varies across terminals (✓ vs ✓ on Linux console) |
| C. Both color + sigil | Rejected | Overkill; visual noise; combinatorial style space |

### 7.3 Status scope (F1.10)

| Option | Verdict | Reason |
|---|---|---|
| **A. Aborted + Open + (existing) thinking-only** | **Selected** | Highest user value per LOC; both visible at-a-glance signals |
| B. + Errored (turns with failed tool calls) → Yellow | Deferred to M4 | Data path needs new trait method to walk tool call success; semantic ambiguity ("does 1 failed grep make the turn 'errored'?") |
| C. Only Aborted → Red | Rejected | Misses open / in-flight which is the highest watch-mode value |

---

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Meta line truncation lands on a fragment that confuses (e.g. "model=gpt-5-min...") | `fit_segments` drops whole fields, never truncates within a field — same convention as `selected_turn_footer_line` already has |
| Color precedence rule wrong (e.g. user expects Open in an aborted-open turn) | Closed loop: precedence table in spec §3.5 + 6 unit tests in §4.2 lock the rule |
| Theme conflict (red on red bg) | ratatui inherits terminal bg; users on red bg already have bigger problems. Test snapshots are theme-neutral. |
| F1.5 test regressions when narrowing Blue to 5-char | Explicit §4.3 entry; update test assertions in same commit |

---

## 9. Out of scope (M4+ follow-ups)

- Errored turn coloring (Yellow) — needs tool-call success/error data path
- Per-turn input / cache token columns — needs upstream Copilot wire change
- Session-summary banner above flame block — separate F-ticket
- Compaction event markers — separate F-ticket
- Hook segment rendering in gantt — separate M-ticket

---

## 10. References

- [F1.5 design](2026-05-30-m1.5-tui-design.md) — original Blue thinking-only marker
- [F1.6 commit `edfbdb8`](../../../crates/agentprof-tui/src/views/format.rs) — OutTK column origin
- [F1.7 spec](2026-06-03-f1.7-models-view-design.md) + [ADR-0012](../../internals/adr-0012-session-model-metrics-and-models-view.md) — session-level tokens (Models view)
- [F1.8 commit `9f47b4e`](../../../crates/agentprof-tui/src/views/flamegraph.rs) — sticky header (extracted `PREFIX_WIDTH` constant)
