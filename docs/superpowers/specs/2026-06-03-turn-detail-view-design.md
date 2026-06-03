# F1 — TurnDetailView + tool arguments plumbing — Design Spec

> **Status:** Draft → awaiting user spec-review approval.
> **Milestone:** M1.6.4 follow-up wave Phase 2 (TUI UX continuation; data layer + UI).
> **Date:** 2026-06-03.
> **Pipeline stage:** 1 (Discovery / Design). Next: Stage 2 (ADR-0011 — see §11) then Stage 3 (writing-plans).
> **Author:** Copilot brainstorming session 252068e5.
> **Branch:** direct-to-main (follow-up wave convention; one cohesive PR).
> **Scope decision (Q4):** Option **B** — plumb args through `ToolCall` (real feature, not micro-fix).

---

## 1. Why

The 2026-06-03 FlamegraphView UX wave shipped a `T3 selected: bash(120ms) read_file(85ms) +K more` footer. Real-session feedback (57-turn `~/.copilot/session-state/...` profile) exposed two gaps:

1. **`+K more` truncation hides the full tool list** for any turn with >3 calls or long-named tools (`mcp:postgres::execute_query`). Users cannot see what those K calls are.
2. **`bash(120ms)` answers "when" but not "what"**. The natural follow-through — "what command did bash run?" — is invisible. Tool arguments are exactly the right answer for this.

The first gap is a pure UI problem (add a drill-down view). The second is a **data plumbing** problem: `agentprof-core::episode::ToolCall` currently stores only `{ span, status, turn_id, user_requested }` — the arguments JSON parsed by the Copilot adapter (`ToolRequest::arguments` at `event.rs:364`) is **discarded** by `derive_episodes`. We fix both in one PR.

---

## 2. What — User-facing contract

### 2.1 New TUI affordance

In `AppRunner` (single-session `analyze --export tui`) and the single-session `WatchRunner` (`watch ...`), when the active view is `View::Flamegraph` and a turn row is selected (via `↑/↓/j/k/G/gg`):

- **`Enter`** opens a full-screen `TurnDetailView` for the selected turn.
- Inside the detail view:
  - Header: `Turn T3 — 5.2s wall · thinking 2.1s (40%) · 3 tool calls`
  - One block per tool call, sorted by duration descending:
    ```
    ▶ bash             120ms  ✓  builtin
       └ args: { "command": "ls -la" }
    ```
  - Tool name colored by `ToolSource` via `theme::tool_source_color`
    (Builtin=cyan, MCP=magenta, Skill=yellow), matching FlamegraphView.
  - `▶` marker shows the currently selected tool call.
  - `args` line single-line, truncated to **80 characters** + `…`.
  - When tool args data is unavailable (e.g. derived from `<orphan>` sentinel or no matching `payload_tool_requests` event), the `└ args:` line is replaced with `└ args: (not captured)` in dim gray.
  - Long tool names (e.g. `mcp:postgres::execute_query`) wrap at the second colon onto a second line, indented to match the name column; the duration/status/source line stays on the first row.
  - Footer hint: `selected: bash · Enter expand · Esc return · j/k G/gg navigate`
- Keys:

  | Key | Action |
  |---|---|
  | `↑` `↓` `j` `k` | Move selected tool_call up/down |
  | `G` | Jump to last tool_call |
  | `gg` (two-key vim) | Jump to first tool_call |
  | `Enter` | Toggle args expansion (full text, word-wrapped) on selected tool_call |
  | `Esc` | Return to `FlamegraphView`, original turn selection preserved |
  | `1` / `2` / `3` | Equivalent to `Esc` + switch to that top-level view |
  | `q` | Global quit (unchanged) |
  | `?` | Help overlay (extended with detail-view rows) |

### 2.2 Data exposure (no CLI surface change)

The plumbing change is internal but visible to JSON/HTML/Speedscope renderers if they choose to read `ToolCall.arguments`. For F1 we update:

- **JSON export**: `arguments` field naturally serializes (it's a public field on a `#[derive(Serialize)]` struct). Schema gets a new optional key. **No breaking change** because consumers should ignore unknown keys.
- **Speedscope export**: NOT touched (frame names already carry tool identity; args would bloat the JSON dramatically). Out of scope.
- **HTML / markdown / CSV exports**: NOT touched (tables are tool-aggregated, not per-call). Out of scope.

### 2.3 Privacy posture

Arguments often contain user paths, search queries, file contents, SQL. They are **not redacted** in F1 — same posture as the existing PII model where adapter output is "trust the caller". Documented in `docs/features/privacy.md` §8 (new). The `AGENTPROF_LOG_FULL_PATHS` env var does NOT apply (that gates *logging*, not data fields).

---

## 3. Architecture

### 3.1 Data flow

```
Copilot adapter                  agentprof-core derive_episodes              agentprof-tui
─────────────────                ───────────────────────────────             ──────────────
ToolRequest.arguments  ───┐
                          ├──► Event::payload_tool_requests() ──┐
ToolUserRequested.args ───┘                                     │
                                                                ▼
                                           args_by_call_id: BTreeMap<String, Value>
                                                                │
                                           on_tool_complete:    │
                                              build ToolCall ◄──┘
                                              + ToolCall.arguments
                                                                │
                                                                ▼
                                             Episodes / AnalysisReport
                                                                │
                                                                ▼
                                                       TurnDetailView reads
                                                       turn.tool_calls[i] (CallRef)
                                                       → tools[ref.name].calls[ref.index]
                                                       → render arguments
```

### 3.2 `agentprof-core` changes

**File: `crates/agentprof-core/src/adapter.rs`** — add to `pub trait Event`:

```rust
/// Adapter-specific (tool_call_id, arguments) pairs declared by this
/// event. Returns empty for events without tool-request payloads.
///
/// Used by `derive_episodes` to populate `ToolCall.arguments` —
/// the args data point lives separately from the span on the wire
/// (Copilot: `assistant.message.tool_requests[*]` and
/// `tool.user_requested.arguments`), so derive needs a first-pass map
/// keyed by `tool_call_id` before it can attach args to the matching
/// span on close.
///
/// Default returns empty `Vec`; adapters override for relevant
/// payload-bearing variants.
fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
    Vec::new()
}
```

**File: `crates/agentprof-core/src/episode/tool.rs`** — add to `pub struct ToolCall`:

```rust
#[non_exhaustive]
pub struct ToolCall {
    pub span: Span,
    pub turn_id: Option<String>,
    pub status: ToolCallStatus,
    pub user_requested: bool,
    /// Tool arguments JSON value, when the adapter captures and
    /// emits it via `Event::payload_tool_requests()`. `None` when
    /// either (a) the adapter does not implement that method for
    /// the relevant variant, or (b) the `tool_call_id` had no
    /// matching tool-request event in the session (e.g. orphan
    /// completes, mid-session resume).
    pub arguments: Option<serde_json::Value>,
}
```

**File: `crates/agentprof-core/src/episode/derive.rs`** — modify `derive_episodes` flow:

```rust
pub fn derive_episodes<E: Event>(events: &[E], meta: &SessionMeta) -> Episodes {
    // PASS 0 (new, single linear scan): collect args by tool_call_id.
    let mut args_by_call_id: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for ev in events {
        for (call_id, args) in ev.payload_tool_requests() {
            // Insert-only-if-absent: first occurrence wins. Defends
            // against degenerate retries / replays.
            args_by_call_id.entry(call_id).or_insert(args);
        }
    }

    // PASS 1 (existing): walk events, build state. Threads
    // args_by_call_id into the state machine so `on_tool_complete`
    // can stamp ToolCall.arguments.
    // …
}
```

`State::on_tool_complete` change (pseudocode):

```rust
let arguments = self.args_by_call_id.get(&tool_call_id).cloned();
let mut tool_call = ToolCall::new(span);
tool_call.turn_id = current_turn_id.clone();
tool_call.status = status;
tool_call.user_requested = user_requested;
tool_call.arguments = arguments;
```

Snapshot stability: derive remains pure + total. PASS 0 walks events once to build the map; PASS 1 walks them once to drive the state machine. Total complexity stays `O(N_events × avg_requests_per_event) + O(N_events)` ⊆ `O(N_events × max_requests_per_event)` — same big-O class as before.

### 3.3 `agentprof-adapters` changes

**File: `crates/agentprof-adapters/src/copilot/event.rs`** — implement on `CopilotEvent`:

```rust
fn payload_tool_requests(&self) -> Vec<(String, serde_json::Value)> {
    match self {
        Self::AssistantMessage(payload) => payload
            .data
            .tool_requests
            .iter()
            .map(|tr| (tr.tool_call_id.clone(), tr.arguments.clone()))
            .collect(),
        Self::ToolUserRequested(payload) => vec![(
            payload.data.tool_call_id.clone(),
            // ToolUserArgs is a struct, serialize to Value.
            // Lossy: it has fields like `prompt`, `choices`, `allow_freeform`.
            serde_json::to_value(&payload.data.arguments).unwrap_or(serde_json::Value::Null),
        )],
        _ => Vec::new(),
    }
}
```

`serde_json::to_value` only fails on `Map<K, _>` with non-string keys → `ToolUserArgs` is a normal struct → `unwrap_or` is dead code but kept for type total-ness. Document the rationale with `// SAFETY` comment.

### 3.4 `agentprof-tui` changes

**New file: `crates/agentprof-tui/src/views/turn_detail.rs`** (~280 LOC including helpers + tests):

```rust
pub struct TurnDetailState {
    pub turn_id: String,
    pub selected_tool_idx: usize,
    pub expanded_tools: HashSet<usize>,
    pub viewport_top: u16,
    pub pending_gg: bool,
}

impl TurnDetailState {
    pub fn new(turn_id: impl Into<String>) -> Self { /* … */ }
    pub fn move_up(&mut self);
    pub fn move_down(&mut self, max: usize);
    pub fn jump_first(&mut self);
    pub fn jump_last(&mut self, max: usize);
    pub fn toggle_expand(&mut self);
    pub fn handle_gg(&mut self) -> GgAction; // pending vs trigger
}

pub fn render_turn_detail(
    f: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    state: &TurnDetailState,
    app_state: &crate::app::state::AppState<'_>, // borrows report + episodes
);

// Pure helpers (rustdoc + Examples + tested):
pub fn format_args_preview(args: Option<&serde_json::Value>, max_chars: usize) -> String;
pub fn wrap_args_full(args: Option<&serde_json::Value>, width: usize) -> Vec<String>;
pub fn tool_call_block_lines(
    name: &str,
    source: ToolSource,
    duration: chrono::Duration,
    status: &ToolCallStatus,
    args: Option<&serde_json::Value>,
    is_selected: bool,
    is_expanded: bool,
    width: u16,
) -> Vec<ratatui::text::Line<'static>>;

fn render_empty_state(f: &mut ratatui::Frame<'_>, area: Rect);
fn render_header_line(turn: &Turn, area: Rect) -> Line<'static>;
fn render_footer_line(state: &TurnDetailState, selected_name: &str, area: Rect) -> Line<'static>;
```

CallRef resolution inside `render_turn_detail`: walk `app_state.episodes.turns` for the entry where `t.id == state.turn_id`, then for each `CallRef` in `turn.tool_calls`, look up `app_state.episodes.tools.get(&ref.name).and_then(|e| e.calls.get(ref.index))`. This is the standard pattern used by FlamegraphView.

**Modified: `crates/agentprof-tui/src/app/state.rs`** — add field + key dispatch:

```rust
pub struct AppState<'a> {
    // existing fields…
    // (`report: &'a AnalysisReport`, `episodes: &'a Episodes` already present!)
    pub detail_view: Option<TurnDetailState>,
}

// dispatch order in handle_key:
// 1. If detail_view.is_some(): try detail keys first (Esc/Enter/j/k/G/gg).
//    1/2/3 pop detail and fall through to view switch.
//    q/? fall through unchanged.
// 2. Else: existing FlamegraphView/RoiView/AggregateView dispatch.
// 3. NEW: View::Flamegraph + KeyCode::Enter + valid selection → enter detail.
```

> **Self-review correction (2026-06-03):** Earlier draft proposed adding `pub episodes: Episodes` to `AnalysisReport` to give TUI access to per-call data. This was **unnecessary** — `AppState<'a>` already holds `episodes: &'a Episodes` via the existing `AppState::new(&report, &episodes)` ctor. Dropped from this spec; D-6 in ADR-0011 is marked superseded by this same finding.

**Modified: `crates/agentprof-tui/src/app/mod.rs`** — render dispatch:

```rust
if let Some(detail) = self.state.detail_view.as_ref() {
    render_turn_detail(f, area, detail, &self.state);
} else {
    /* existing view dispatch */
}
```

The `render_turn_detail` signature takes `&AppState` (which already carries both `&AnalysisReport` and `&Episodes`); no extra parameters needed.

Help overlay (`fn draw_help_overlay`) gets 5 new lines (detail-view keys + entry hint); overlay height 22 → 27.

**Modified: `crates/agentprof-tui/src/watch.rs`** — single-session reload safety + transient AppState round-trip:

`WatchRunner` re-creates a transient `AppState` on every render frame and every key dispatch (existing line 398/457 pattern). To persist `detail_view` across these reconstructions, follow the existing `pending_gg` / `help_overlay` round-trip pattern:

```rust
// In WatchViewState (persistent across renders + reloads):
pub struct WatchViewState {
    pub help_overlay: bool,
    pub pending_gg: bool,
    pub detail_view: Option<TurnDetailState>,  // NEW
    // …other fields…
}

// On every transient AppState construction (render path, key dispatch path):
let mut transient = AppState::new(report, episodes);
transient.help_open = self.view_state.help_overlay;
transient.detail_view = self.view_state.detail_view.clone();  // NEW

// After dispatch (key handling path only — render does not mutate):
self.view_state.help_overlay = transient.help_open;
self.view_state.detail_view = transient.detail_view;  // NEW

// After do_reload(), validate detail_view against fresh episodes:
if let WatchData::Single { episodes, .. } = &self.data {
    if let Some(ref dv) = self.view_state.detail_view {
        if !episodes.turns.iter().any(|t| t.id == dv.turn_id) {
            let id = dv.turn_id.clone();
            self.view_state.detail_view = None;
            self.last_error = Some(format!("turn {id} disappeared after reload"));
        }
    }
}
```

`TurnDetailState` clone is cheap (one `String` + `HashSet<usize>` typically <8 entries + small primitives). Cross-session `WatchData::Cross` does NOT support detail view (aggregates have no per-turn structure). No change there.

---

## 4. Visual mockup (4-tool turn, `read_file` selected & expanded)

```
┌ Turn T3 — 5.2s wall · thinking 2.1s (40%) · 4 tool calls ─────────────────┐
│                                                                            │
│    bash             1.2s   ✓  builtin                                      │
│       └ args: { "command": "rg -n pattern --type rust" }                   │
│                                                                            │
│  ▶ read_file         850ms ✓  builtin                                      │
│       └ args (expanded):                                                   │
│          {                                                                 │
│            "path": "/home/user/proj/src/main.rs",                          │
│            "view_range": [1, 200]                                          │
│          }                                                                 │
│                                                                            │
│    mcp:postgres::    420ms ✓  mcp:postgres                                 │
│       execute_query                                                        │
│       └ args: { "query": "SELECT * FROM users WHERE id=$1", "p…           │
│                                                                            │
│    write             180ms ✗  builtin                                      │
│       └ args: { "path": "/tmp/out.json", "content": "…(+2.3KB)" }          │
│                                                                            │
│ selected: read_file · Enter collapse · Esc return · j/k G/gg navigate      │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Tests

### 5.1 New tests in `agentprof-core`

`crates/agentprof-core/src/adapter.rs` (inline):
- `payload_tool_requests_default_empty` — default trait impl returns `vec![]`

`crates/agentprof-core/src/episode/derive.rs` (inline):
- `args_attached_to_tool_call_when_payload_tool_requests_seen`
- `args_none_when_no_matching_tool_request_event`
- `args_first_occurrence_wins_on_duplicate_call_id`
- `args_attached_for_user_requested_tools` (e.g. ask_user)
- `args_handled_correctly_for_orphan_completes` (no panic, None set)

`crates/agentprof-core/src/episode/tool.rs` (inline):
- `tool_call_default_arguments_is_none`
- `tool_call_serde_roundtrip_with_arguments` (json roundtrip)
- `tool_call_arguments_skipped_when_none_in_json` (`#[serde(skip_serializing_if = "Option::is_none")]`)

### 5.2 New tests in `agentprof-adapters`

`crates/agentprof-adapters/src/copilot/event.rs` (inline):
- `payload_tool_requests_assistant_message_multi`
- `payload_tool_requests_tool_user_requested_single`
- `payload_tool_requests_other_variants_empty`

`crates/agentprof-adapters/tests/copilot_integration.rs` (or wherever):
- One end-to-end fixture-based test verifying ToolCall.arguments is populated for a known event in `builtin-tools-only` fixture.

### 5.3 New tests in `agentprof-tui`

`crates/agentprof-tui/tests/turn_detail.rs` (new file):
- `render_4_tool_turn_snapshot` (color-blind buffer_to_symbol_grid)
- `render_empty_tool_calls_shows_no_tool_calls`
- `args_preview_truncates_at_80_chars`
- `args_none_shows_not_captured`
- `args_expansion_toggle_via_enter`
- `args_full_wraps_at_terminal_width`
- `vim_keys_jk_G_gg_navigate_tool_calls`
- `esc_returns_to_flamegraph_preserves_turn_selection`
- `enter_from_flamegraph_on_empty_turn_still_works`
- `reload_drops_detail_view_when_turn_disappears`
- `reload_preserves_detail_view_when_turn_still_present`
- `view_switch_keys_123_pop_detail_then_switch`

`crates/agentprof-tui/src/views/turn_detail.rs` inline:
- `format_args_preview_exact_80_chars_no_truncation`
- `format_args_preview_81_chars_truncates_with_ellipsis`
- `format_args_preview_none_yields_not_captured`
- `format_args_preview_pretty_collapsed_to_single_line`
- `wrap_args_full_indents_pretty_json`
- `tool_call_block_lines_selected_renders_marker`
- `tool_call_block_lines_expanded_uses_wrap_args_full`
- `handle_gg_two_key_sequence_triggers_jump_first`

### 5.4 Snapshot delta target

- 5 new snapshot files (4 in `turn_detail.rs` tests, 1 reload-aware in WatchRunner integration if applicable)
- 0 existing snapshots affected (new view; existing tests don't touch detail path)
- 0 fixture changes (existing fixtures suffice; `builtin-tools-only` has args in raw events already)

### 5.5 Test count delta target

- core: +9 tests
- adapters: +4 tests
- tui: +18 tests
- **Total: +31 tests** (533 → ~564)

---

## 6. Backwards / forward compatibility

| Surface | Change | Compat |
|---|---|---|
| `Event` trait | New method `payload_tool_requests` with default impl returning empty `Vec` | ✅ Backwards compatible — existing trait impls compile unchanged |
| `ToolCall` struct | New field `arguments: Option<serde_json::Value>`; struct is `#[non_exhaustive]` | ✅ Non-breaking (callers can't pattern-match exhaustively); `ToolCall::new(span)` ctor defaults the new field to `None` |
| JSON export schema | New optional field `arguments` on each tool call | ✅ Forward-compat for unknown-keys consumers; documented in CHANGELOG |
| `AppState<'a>` struct | New field `detail_view: Option<TurnDetailState>`; struct is `#[non_exhaustive]` | ✅ Non-breaking; `AppState::new()` ctor defaults to `None` |
| `WatchViewState` struct | New field `detail_view: Option<TurnDetailState>` | ✅ Non-breaking (struct is internal to `agentprof-tui`); default constructed `None` |
| TUI key bindings | `Enter` on Flamegraph row was previously unbound — no semantics conflict | ✅ |
| CLI surface | None | ✅ |

CHANGELOG marks: `### Added` (3 entries: trait method, struct field, TurnDetailView). `### Changed` (1 entry: args field in JSON export).

---

## 7. Out of scope (deferred)

- **Result content display** — PII risk. Requires `--show-results` CLI flag + in-view toggle. Defer to follow-up RFC.
- **Hook / skill call rows in detail view** — separate UX exploration; B-6 / B-7 fixtures landed but no UI consumer.
- **Token counts per turn** — M2 territory.
- **Args in HTML / markdown / CSV / Speedscope exports** — visually awkward in those formats; only JSON benefits.
- **Args redaction** — separate privacy feature; document the current "trust adapter output" posture in `docs/features/privacy.md` §8.
- **Wide-char (CJK / emoji) tool name / args width** — `unicode-width` not pulled; document ASCII-friendly limitation.
- **`tool_call_id` on `ToolCall`** — not added (no consumer needs it directly; args lookup happens once at derive time).
- **F2 ask_user pending detection** — separate brainstorm next (`f2-askuser-pending-brainstorm` todo).

---

## 8. Alternatives considered

| Data plumbing alternative | Verdict |
|---|---|
| **(B) Add `payload_tool_requests` to `Event` trait + new `ToolCall.arguments` field** | ✅ Chosen — symmetrical with existing `payload_name/model/output_tokens/mode` pattern, surgical |
| (B') Add `Adapter::tool_arguments(&[Event]) -> Map` method | ❌ — bypasses the per-event `Event` trait contract; awkward signature change for derive_episodes |
| (B'') Adapter pre-attaches args into a side `RawSession.tool_args_map` field | ❌ — pollutes `RawSession`, asymmetric with other facts |
| (B''') Pass `RawSession` to TUI; TUI re-walks events | ❌ — TUI gains adapter awareness, breaks the L1 "core is leaf" rule |
| (A) Ship TurnDetailView without args data | ⏸ — viable fallback if scope blows up; less complete user value (rejected in Q4) |
| (C) Two-phase: ship args plumbing alone first, then detail view | ⏸ — clean but doubles the spec/plan cycle for one cohesive user-facing feature |

| UI form (Q1) | Verdict |
|---|---|
| **Full-screen TurnDetailView** | ✅ |
| Modal overlay | ❌ — args + 4+ tools overflow |
| Split panel | ❌ — compresses both views |

| Detail-internal Enter (Q3) | Verdict |
|---|---|
| **Expand/collapse args full** | ✅ — recursive "Enter = deeper" mental model |
| No-op | ❌ — wastes the key |
| Expand args + result | ❌ — PII risk re-enters |

---

## 9. Risks

- **State leakage on reload**: `WatchRunner` swap could leave `detail_view` pointing at a vanished turn id. Mitigated in §3.4 — `do_reload()` validates `view_state.detail_view.turn_id` against fresh `episodes.turns` and drops + footer-banners on mismatch.
- **`expanded_tools: HashSet<usize>` indices**: stable within one detail session; cleared on Esc or reload. OK.
- **Snapshot fragility**: tool ordering is duration-desc; ties use original `Episodes.tools[name].calls[index]` order. Deterministic.
- **Wide-char CJK args / tool names**: `format_args_preview` uses `chars().take()` (not byte count); width off by ±1 cell on wide CJK / emoji. Documented in rustdoc with explicit ASCII-friendliness note.
- **`TurnDetailState` clone cost in WatchRunner**: copied across transient `AppState` boundary every render + every key event. Struct is small (`String` + `HashSet<usize>` typically <8 entries + 3 primitives) — cost is negligible relative to a single TUI frame.
- **`serde_json::Value::Null` for `to_value` failure on `ToolUserArgs`**: dead code path (struct serializes deterministically) but documented + asserted in a debug_assert via unit test.

---

## 10. Decision log

Decisions reached via Copilot CLI `ask_user` interactive sequence 2026-06-03 15:35–15:55 in session 252068e5:

1. **Q1 (UI form)** → (b) Full-screen TurnDetailView
2. **Q2 (Field set)** → (b) MVP + args preview
3. **Q3 (Detail-internal Enter)** → (b) Expand current tool_call args full
4. **Q4 (Data plumbing scope, surfaced post-spec-self-review)** → (B) Plumb args through ToolCall in same PR

All four votes recorded in session transcript before this spec was committed.

---

## 11. ADR triggers (Stage 2)

Per `.github/copilot-instructions.md` §5.5:

**ADR-0011 (provisional): "Tool arguments plumbing and TurnDetailView state model"**.

Rationale:
- Multiple real alternatives considered for data plumbing (B / B' / B'' / B''' / A / C) → §5.5 row 1 ✅
- Introduces new public trait method + struct field + new public view module → §5.5 row 2 ✅
- Establishes the precedent "tool arguments are part of the episode model, not adapter-private" — future adapters (Claude / Codex) must implement `payload_tool_requests` to opt into rich detail. Worth recording.
- Establishes the UX precedent "Enter = drill deeper one level" for future detail-of-detail views (result viewer, frame viewer).

ADR will be written in Stage 2 immediately after this spec is approved, before Stage 3 (writing-plans).

---

## 12. Open follow-ups (not blocking F1)

- **F2 ask_user pending detection** — next brainstorm (`f2-askuser-pending-brainstorm` todo). Possible integration once F1 ships: detail view shows `⏸ pending — awaiting user reply` badge if `tool_call.status == ToolCallStatus::OrphanEnd` AND `tool_call.user_requested == true` (or some other signal — needs F2 brainstorm to define).
- **Args sort hotkeys in detail view** (`t`/`c`/`p` mimicking RoiView) — wait for usage feedback.
- **Hook/skill row interleaving** — separate UX exploration.
- **Other adapters (Claude / Codex)** must implement `payload_tool_requests` when they land — document in `docs/adapters.md` as recommended-but-optional method for full TurnDetailView UX.

---
