//! `FlamegraphView` — per-turn horizontal gantt of tool calls.
//!
//! Each row = one Turn (chronological order). The horizontal scale is set
//! to the **p95** of non-user-blocking turn durations (see
//! [`Turn::is_user_blocking`]); turns at or below p95 scale proportionally,
//! and the top 5% outliers clamp to gantt width via the `.min(gantt_w)`
//! guard so they still render as fully-filled rows. Within a row, each
//! [`agentprof_core::episode::ToolCall`] is drawn as a segment at
//! `(call.span.start - turn.start) / turn.duration` of row width, colored by
//! [`agentprof_core::model::ToolSource`].
//!
//! ## Rendered output
//!
//! Each turn row mixes three character types in its gantt area:
//!
//! - `█` (U+2588 FULL BLOCK) — a tool / hook / skill is executing during this time slice.
//!   Colored by [`agentprof_core::model::ToolSource`] (Builtin=cyan, MCP=magenta, Skill=yellow)
//!   via [`crate::theme::tool_source_color`]; Hook segments aren't yet color-coded (they live
//!   in `hook_calls`, not `tool_calls`).
//! - `░` (U+2591 LIGHT SHADE) — the turn is in-flight but no tool is running (LLM thinking time:
//!   reasoning, generating output tokens, interpreting tool output). This time IS part of the
//!   turn's wall-time and is real cost.
//! - `·` (U+00B7 MIDDLE DOT) — padding: the turn ended before this position. The character only
//!   exists because this row is shorter than the p95 non-blocking turn duration (which sets the
//!   horizontal scale). This time is NOT part of the turn.
//!
//! Below the gantt rows, a single-line footer (see [`selected_turn_footer_line`]) lists the
//! currently-selected turn's tool calls with per-call durations, e.g.
//! `T3 selected:  bash(120ms) read_file(85ms) +2 more · Enter for detail`. Truncates from the
//! right with `+K more` when the line exceeds the footer width; the trailing
//! `· Enter for detail` hint advertises the [`turn_detail`](crate::views::turn_detail) view
//! and may itself be truncated on narrow terminals (the `?` help overlay lists the same key).
//!
//! User-blocking turns (e.g. containing an `ask_user` call where the user spent minutes on
//! the keyboard or AFK) are excluded from the scale calculation per
//! [`Turn::is_user_blocking`]; otherwise such a turn's wall-time would dwarf others and
//! squash every normal turn's gantt-bar to ≤ 1 cell. We additionally use the **p95** of
//! the remaining non-blocking durations (rather than `max`) to resist agent-side outliers:
//! a single `task`/long-subagent turn running tens of minutes is *not* user-blocking, but
//! would have the same squashing effect under a max-based scale. The top 5% outliers
//! clamp to gantt width and remain visible as fully-filled rows. See ADR-0005 §6 for the
//! same split applied to the Tool Rank table.
//!
//! See `docs/superpowers/specs/2026-05-30-m1.5-tui-design.md` §6.1.

use agentprof_core::episode::{Episodes, Turn, TurnStatus};
use agentprof_core::model::ToolSource;
use chrono::Duration;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span as TextSpan};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::state::AppState;
use crate::theme::tool_source_color;
use crate::views::format::{format_tokens_short, human_short};

/// Width in chars of the flamegraph row prefix.
///
/// Composed of:
/// - 5 chars right-padded turn label (e.g. `"  T39"`)
/// - 1 space
/// - 10 chars right-padded duration (e.g. `"      9.6s"`)
/// - 1 space
/// - 5 chars right-padded tokens column (e.g. `" 2.3k"`)
/// - 2 trailing spaces (visual gutter before gantt)
///
/// MUST match the `format!("{:>5} {:>10} {:>5}  ", ...)` literal in
/// `build_row`. Use this constant in `render` for gantt-width
/// computation; the `build_row_prefix_width_matches_layout_constant`
/// test enforces the invariant.
pub const PREFIX_WIDTH: u16 = 24;

/// Build the sticky column-name header rendered above the gantt rows.
///
/// Returns a single [`Line`] that the [`render`] function places in a
/// reserved 1-cell strip at the top of the flamegraph block (above the
/// scrolling rows). It serves two purposes:
///
/// 1. Label the three prefix columns (`Turn` / `Duration` / `OutTK`) so
///    new users can read the row format at a glance.
/// 2. Provide a colored legend for the three gantt cell symbols
///    (`█` tool · `░` thinking · `·` padding), matching the colors used
///    by `cell_style` in data rows.
///
/// The first [`PREFIX_WIDTH`] chars mirror the column layout of
/// `build_row` **exactly** (same right-aligned 5 / 10 / 5 width
/// template) so the labels sit in the same character positions as the
/// data values below them. The
/// `header_line_prefix_matches_build_row_format` test enforces this
/// invariant — if you change the format literal in `build_row`, change
/// it here too (and bump [`PREFIX_WIDTH`] if the width changes).
///
/// The label `OutTK` (output tokens, 5 chars) is intentionally singular
/// and abbreviated to fit the fixed 5-char tokens column without
/// overflowing [`PREFIX_WIDTH`]. It surfaces the per-turn `output_tokens`
/// sum (from `assistant.message.outputTokens` events); per-turn input /
/// cache tokens are NOT available on the Copilot wire and only appear at
/// session level (see the Models view, key `4`). When the upstream wire
/// schema starts exposing per-turn input / cache, this label can widen
/// back to a more descriptive form (e.g. multiple columns).
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::flamegraph::{header_line, PREFIX_WIDTH};
/// let line = header_line();
/// let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
/// assert!(text.starts_with(&format!(
///     "{:>5} {:>10} {:>5}  ",
///     "Turn", "Duration", "OutTK"
/// )));
/// // The prefix portion occupies exactly PREFIX_WIDTH chars.
/// let prefix: String = text.chars().take(PREFIX_WIDTH as usize).collect();
/// assert_eq!(prefix.chars().count(), PREFIX_WIDTH as usize);
/// // The legend mentions all three gantt symbols.
/// assert!(text.contains('█'));
/// assert!(text.contains('░'));
/// assert!(text.contains('·'));
/// ```
#[must_use]
pub fn header_line() -> Line<'static> {
    let prefix = format!("{:>5} {:>10} {:>5}  ", "Turn", "Duration", "OutTK");
    Line::from(vec![
        TextSpan::raw(prefix),
        // Tool block — colored cyan to match the default Builtin
        // ToolSource color; data rows use per-source colors
        // (Builtin=cyan / MCP=magenta / Skill=yellow). The legend
        // simply shows the symbol shape.
        TextSpan::styled("█", Style::default().fg(Color::Cyan)),
        TextSpan::raw(" tool  "),
        // Thinking — DIM modifier matches cell_style('░', _).
        TextSpan::styled("░", Style::default().add_modifier(Modifier::DIM)),
        TextSpan::raw(" thinking  "),
        // Padding — DarkGray matches cell_style('·', _).
        TextSpan::styled("·", Style::default().fg(Color::DarkGray)),
        TextSpan::raw(" padding"),
    ])
}

/// Pure-math layout helper for `render`.
///
/// Given a turn (start/end), a list of `(call_start, call_end, segment_index)`
/// tuples, and the row width in cells, produce a `Vec` of
/// `(cell_start, cell_len, segment_index)` for each call clipped to the row.
///
/// Cells outside the row width are clipped. Zero-duration turns (e.g. open
/// turns with no `ended_at`) produce an empty `Vec`. Zero-length calls
/// (`call_start == call_end`) within a non-zero turn render as a 1-cell
/// segment so they remain visible.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::flamegraph::segment_layout;
/// use chrono::{TimeZone, Utc};
/// let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
/// let t10 = t0 + chrono::Duration::seconds(10);
/// let t3 = t0 + chrono::Duration::seconds(3);
/// let t5 = t0 + chrono::Duration::seconds(5);
/// // Turn 0..10s; one call at 3..5s; width = 20 cells → segment at cells 6..10.
/// let segs = segment_layout(t0, t10, &[(t3, t5, 0)], 20);
/// assert_eq!(segs, vec![(6, 4, 0)]);
/// ```
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub fn segment_layout(
    turn_start: chrono::DateTime<chrono::Utc>,
    turn_end: chrono::DateTime<chrono::Utc>,
    calls: &[(
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        usize,
    )],
    width: u16,
) -> Vec<(u16, u16, usize)> {
    let total = turn_end - turn_start;
    if total <= Duration::zero() || width == 0 {
        return Vec::new();
    }
    let total_ms = total.num_milliseconds().max(1) as f64;
    let w = f64::from(width);
    let mut out = Vec::with_capacity(calls.len());
    for (start, end, idx) in calls {
        let offset_ms = (*start - turn_start).num_milliseconds().max(0) as f64;
        let len_ms = (*end - *start).num_milliseconds().max(0) as f64;
        let cell_start_f = (offset_ms / total_ms * w).floor();
        let cell_end_f = ((offset_ms + len_ms) / total_ms * w).ceil();
        let cell_start = cell_start_f.clamp(0.0, w) as u16;
        let cell_end = cell_end_f.clamp(0.0, w) as u16;
        let len = cell_end
            .saturating_sub(cell_start)
            .max(1)
            .min(width.saturating_sub(cell_start));
        if len > 0 && cell_start < width {
            out.push((cell_start, len, *idx));
        }
    }
    out
}

/// Render the `FlamegraphView` into the given area.
///
/// Layout: vertical list of rows; each row has a fixed-width
/// [`PREFIX_WIDTH`]-cell prefix (turn label + duration + tokens columns)
/// followed by the gantt strip filling the remaining width. The selected
/// turn (`state.flame_selected`) is highlighted with reverse-video.
///
/// **Sticky header** (top 1 cell of the bordered block): see
/// [`header_line`]. Labels the prefix columns (`Turn` / `Duration` /
/// `OutTK`) and provides a colored legend for the gantt symbols
/// (`█` tool · `░` thinking · `·` padding) so the meaning of each
/// character in the rows below is self-evident. Reserved only when
/// `inner.height >= 3` (i.e. there is room for header + at least 1 row +
/// footer); on very tall-but-still-tiny windows (h == 2) the footer is
/// prioritized over the header because per-turn detail is more
/// information-dense than the legend.
///
/// **Meta line** (`chunks[1]`, F1.9): single no-border row below the
/// bordered Flamegraph block. Replaces the old 3-row bordered " Detail "
/// block (which had three fields fully redundant with content shown
/// elsewhere — Turn UUID, `out_tokens`, and `tools=N`). See
/// [`format_meta_line`] for the field list, priority order, and
/// narrow-terminal truncation rules.
///
/// **Viewport (edge-triggered)**: `state.flame_viewport_top` is persisted
/// across frames via [`std::cell::Cell`]. Render only adjusts it when
/// `flame_selected` leaves the visible window — pressing `↓` past the
/// bottom shifts viewport down by 1; pressing `↑` past the top shifts it
/// up by 1; movement within the visible window does not scroll. This is
/// the canonical scroll-to-follow pattern.
#[allow(clippy::too_many_lines)]
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        // F1.9: chunks[1] shrank from 3 rows (bordered " Detail " block)
        // to 1 row (no-border meta line). +2 rows reclaimed for gantt.
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Flamegraph (1/3) ");
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    let prefix_width: u16 = PREFIX_WIDTH;
    let gantt_width = inner.width.saturating_sub(prefix_width);

    // Reserve 1 line at the bottom of `inner` for the selected-turn footer
    // (e.g. "T3 selected:  bash(120ms) read_file(85ms) +2 more"). The
    // footer lives INSIDE the bordered Flamegraph block so the existing
    // Detail strip (`chunks[1]`) is unchanged.
    //
    // F1.8 (sticky header): when there is room for ≥3 lines, also reserve
    // the TOP line for [`header_line`] (column names + gantt legend).
    // On a degenerate h == 2 window we keep the original rows+footer
    // layout — per-turn detail is more information-dense than the legend.
    let (header_area, rows_area, footer_area) = if inner.height >= 3 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);
        (Some(split[0]), split[1], Some(split[2]))
    } else if inner.height >= 2 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);
        (None, split[0], Some(split[1]))
    } else {
        (None, inner, None)
    };

    if let Some(header_rect) = header_area {
        frame.render_widget(Paragraph::new(header_line()), header_rect);
    }

    let turns = &state.report.turn_summary;

    // Polish #1: build turn-by-id lookup once per render (was O(N×M) per-
    // row iter().find(); now O(N) build + O(M) lookups).
    let turn_by_id: std::collections::HashMap<&str, &agentprof_core::episode::Turn> = state
        .episodes
        .turns
        .iter()
        .map(|t| (t.id.as_str(), t))
        .collect();

    // Scaling strategy: use p95 of non-user-blocking turn durations.
    //
    // Why p95 instead of max:
    // - User-blocking turns (e.g. `ask_user` waiting on a human) are
    //   already excluded via `Turn::is_user_blocking()` (see b5c1429).
    // - BUT agent-self-blocking turns also exist: a single `task` /
    //   long subagent call can run for tens of minutes without any
    //   human involvement. These are NOT user-blocking but they're
    //   the same kind of long-tail outlier for visualization.
    // - Using max would let any single outlier (human OR agent) squash
    //   the visualization. Using p95 keeps 95% of turns at usable
    //   widths; the rare > p95 outliers get clamped to `gantt_w` by
    //   the existing `.min(gantt_w)` in the scaled cell calc, so they
    //   still render visibly (just at the cap).
    //
    // Standard practice in flamegraph tooling (Speedscope / flamegraph.pl
    // / gprof2dot all use percentile-based scaling for the same reason).
    //
    // Fallback: if EVERY turn is user-blocking (degenerate), or if
    // there are no completed turns, fall back to max-of-all-durations
    // or 1 ms — same fallback as before b5c1429.
    //
    // Mirrors ADR-0005 §6 + the user-blocking split in the Tool Rank
    // table (see [`agentprof_core::analyzer::tool_rank`]).
    let mut non_blocking_durs_ms: Vec<i64> = turns
        .iter()
        .filter_map(|row| {
            let turn = turn_by_id.get(row.turn_id.as_str())?;
            if turn.is_user_blocking() {
                None
            } else {
                row.duration.map(|d| d.num_milliseconds())
            }
        })
        .collect();
    non_blocking_durs_ms.sort_unstable();

    let max_dur: i64 = if non_blocking_durs_ms.is_empty() {
        // Degenerate: every turn is user-blocking. Fall back to
        // max-of-all so we don't divide by 1 ms.
        turns
            .iter()
            .filter_map(|t| t.duration.map(|d| d.num_milliseconds()))
            .max()
            .unwrap_or(1)
            .max(1)
    } else {
        // p95 of sorted non-blocking durations.
        // p95 index = ceil(0.95 * n) - 1, clamped to [0, n-1].
        // For n = 1, p95 = the single value.
        // For n = 57, p95 = idx 53 (the 54th turn ascending).
        let n = non_blocking_durs_ms.len();
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let p95_idx_raw = ((n as f64) * 0.95).ceil() as usize;
        let p95_idx = p95_idx_raw.saturating_sub(1).min(n - 1);
        non_blocking_durs_ms[p95_idx].max(1)
    };

    // Edge-triggered viewport: only scroll when selected leaves the window.
    // Persisted via Cell on AppState so the cursor can move freely within
    // the viewport (instead of being glued to the bottom edge every frame).
    let visible_rows = (rows_area.height as usize).max(1);
    let mut viewport_top = state.flame_viewport_top.get();
    if state.flame_selected < viewport_top {
        viewport_top = state.flame_selected;
    } else if state.flame_selected >= viewport_top + visible_rows {
        viewport_top = state.flame_selected + 1 - visible_rows;
    }
    // Defensive clamp (M1): if flame_selected somehow exceeds turns.len()
    // (e.g. state restored from a different session), keep viewport_top
    // within bounds so the slice below doesn't panic.
    let max_viewport_top = turns.len().saturating_sub(visible_rows);
    viewport_top = viewport_top.min(max_viewport_top);
    state.flame_viewport_top.set(viewport_top);
    let visible_end = (viewport_top + visible_rows).min(turns.len());

    let mut lines: Vec<Line<'_>> = Vec::with_capacity(visible_end.saturating_sub(viewport_top));
    // F2.2 — cache `now` once per frame so all `build_row` calls see
    // the same instant. For watch mode this is the live "now"; for
    // postmortem `analyze --export tui` it's also `Utc::now()`, which
    // means any `OpenAtEndOfSession` call from a session that ended N
    // hours ago renders as pending (elapsed = N hours >> threshold).
    // That's the desired behavior — "you abandoned this call N hours
    // ago" is the user-facing message.
    let now = chrono::Utc::now();
    for (offset, row) in turns[viewport_top..visible_end].iter().enumerate() {
        let abs_idx = viewport_top + offset;
        let turn = turn_by_id.get(row.turn_id.as_str()).copied();
        let line = build_row(
            abs_idx,
            abs_idx == state.flame_selected,
            row.duration,
            max_dur,
            gantt_width,
            turn,
            state.episodes,
            now,
        );
        lines.push(line);
    }

    let para = Paragraph::new(lines);
    frame.render_widget(para, rows_area);

    // Footer line: selected turn's tool calls with per-call durations.
    // Helps the user see *what* the highlighted row spent its time on,
    // not just *how much* time it spent.
    if let Some(footer_rect) = footer_area {
        let selected_turn = turns
            .get(state.flame_selected)
            .and_then(|row| turn_by_id.get(row.turn_id.as_str()).copied());
        let footer_text = selected_turn_footer_line(
            state.flame_selected,
            selected_turn,
            state.episodes,
            footer_rect.width,
        );
        let footer = Paragraph::new(TextSpan::styled(
            footer_text,
            Style::default().add_modifier(Modifier::DIM),
        ));
        frame.render_widget(footer, footer_rect);
    }

    // F1.9 meta line — replaces the old 3-row bordered " Detail " block.
    // Single row, no border, DIM modifier to visually separate from the
    // gantt rows above. See [`format_meta_line`] for content format and
    // narrow-terminal truncation rules.
    let meta_text = format_meta_line(state, chunks[1].width);
    frame.render_widget(
        Paragraph::new(TextSpan::styled(
            meta_text,
            Style::default().add_modifier(Modifier::DIM),
        )),
        chunks[1],
    );
}

/// Resolve the foreground color for the 5-char T-id portion of a flame
/// row prefix, encoding the turn's status (F1.10) and pending-call
/// state (F2.2).
///
/// Precedence (highest → lowest):
///
/// 1. `TurnStatus::Aborted(_)` → [`Color::Red`] — the most urgent
///    user-facing signal; pairs with the [`Modifier::UNDERLINED`]
///    applied to the whole row by `build_row` as a color-blind backup.
/// 2. **Pending** (F2.2) — any `ToolCall` in `turn.tool_calls` reports
///    pending via [`crate::views::flamegraph::is_turn_pending`]
///    (which delegates to
///    [`agentprof_core::analyzer::pending::is_pending`]) → [`Color::Yellow`].
///    Ranked above "Open" because a turn with a stuck `ask_user` IS
///    open, but "pending" is the more specific + actionable signal.
/// 3. **Open** / in-flight (`turn.ended_at.is_none()`) → [`Color::DarkGray`]
///    — distinguishes turns still running in watch mode from completed
///    turns at the bottom of the list.
/// 4. **Thinking-only** (closed turn with no tool calls — e.g. pure-text
///    replies, summary turns, plan/execute breakpoints) → [`Color::Blue`]
///    — the legacy F1.5 marker, now confined to the 5-char T-id span
///    instead of the full 24-char prefix (F1.10 tightening, for
///    consistency with Aborted / Open).
/// 5. Otherwise (closed turn with tool calls) → `None` (terminal default
///    fg). Status colors stack additively with [`Modifier::REVERSED`]
///    (selected row) and [`Modifier::UNDERLINED`] (aborted), so a
///    selected-aborted turn shows REVERSED + UNDERLINED + Red T-id.
///
/// Returns `None` when `turn` is `None` (e.g. a stale `flame_selected`
/// pointing past `episodes.turns.len()`).
///
/// **F2.2 signature change**: the `episodes` and `now` parameters are
/// required to evaluate pending state. Callers in `build_row` already
/// hold `episodes` in scope; `now` is cached once at the top of
/// `render` (single `Utc::now()` per frame).
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::{Episodes, Turn, TurnStatus};
/// use agentprof_tui::views::flamegraph::t_id_status_color;
/// use chrono::Utc;
/// use ratatui::style::Color;
///
/// let now = Utc::now();
/// let t = Turn::new("t1".into(), now);
/// // Open turn (no `ended_at`) with no pending calls → DarkGray.
/// assert_eq!(
///     t_id_status_color(Some(&t), &Episodes::default(), now),
///     Some(Color::DarkGray),
/// );
/// // No turn → None.
/// assert_eq!(t_id_status_color(None, &Episodes::default(), now), None);
/// ```
#[must_use]
pub fn t_id_status_color(
    turn: Option<&Turn>,
    episodes: &Episodes,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Color> {
    let t = turn?;
    if matches!(t.status, TurnStatus::Aborted(_)) {
        return Some(Color::Red);
    }
    // F2.2 — pending state above Open / Thinking / default. We check
    // pending BEFORE ended_at because a turn with a stuck ask_user IS
    // open; "pending" is the more specific signal.
    if is_turn_pending(t, episodes, now) {
        return Some(Color::Yellow);
    }
    if t.ended_at.is_none() {
        return Some(Color::DarkGray);
    }
    if t.tool_calls.is_empty() {
        return Some(Color::Blue);
    }
    None
}

/// Returns true if any `ToolCall` referenced by this turn is currently
/// pending (F2.2 — delegates per-call check to
/// [`agentprof_core::analyzer::pending::is_pending`]).
///
/// Helper extracted so [`t_id_status_color`] stays readable and so
/// tests can pin the per-turn aggregation independently of the color
/// precedence logic.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::{Episodes, Turn};
/// use agentprof_tui::views::flamegraph::is_turn_pending;
/// use chrono::Utc;
///
/// // Empty turn → no pending.
/// let t = Turn::new("t1".into(), Utc::now());
/// assert!(!is_turn_pending(&t, &Episodes::default(), Utc::now()));
/// ```
#[must_use]
pub fn is_turn_pending(
    turn: &Turn,
    episodes: &Episodes,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    for call_ref in &turn.tool_calls {
        let Some(ep) = episodes.tools.get(&call_ref.name) else {
            continue;
        };
        let Some(call) = ep.calls.get(call_ref.index) else {
            continue;
        };
        if agentprof_core::analyzer::pending::is_pending(call, &call_ref.name, now) {
            return true;
        }
    }
    false
}

#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn build_row<'a>(
    idx: usize,
    selected: bool,
    duration: Option<Duration>,
    max_dur_ms: i64,
    gantt_width: u16,
    turn: Option<&'a Turn>,
    episodes: &'a Episodes,
    now: chrono::DateTime<chrono::Utc>,
) -> Line<'a> {
    let dur_str = duration.map_or_else(|| "—".to_string(), human_short);
    // 5-char fixed-width tokens column inserted between duration and the
    // gantt. Renders the per-turn `output_tokens` sum (assistant.message
    // outputTokens). `None` → centered dash so it's distinguishable from
    // "0 reported". See `format_tokens_short` for bucket details.
    let tokens_str = format_tokens_short(turn.and_then(|t| t.output_tokens));
    let prefix = format!(
        "{:>5} {:>10} {:>5}  ",
        format!("T{}", idx + 1),
        dur_str,
        tokens_str,
    );
    debug_assert_eq!(
        prefix.chars().count(),
        PREFIX_WIDTH as usize,
        "build_row prefix must be exactly PREFIX_WIDTH chars"
    );

    // F1.10 — split the prefix into the 5-char T-id portion and the
    // remaining 19 chars (duration + space + tokens + 2-char gutter).
    // The T-id span carries the status color (Aborted = Red / Pending =
    // Yellow [F2.2] / Open = DarkGray / thinking-only = Blue / default);
    // the rest span never carries an fg override so the duration / OutTK
    // columns stay visually consistent across turn statuses.
    //
    // 5 + 19 = 24 = PREFIX_WIDTH. The split is by char count, not
    // byte count, to handle the rare case where the T-id contains
    // a multi-byte char (none today, but defensive).
    let prefix_tid: String = prefix.chars().take(5).collect();
    let prefix_rest: String = prefix.chars().skip(5).collect();

    let base_style = if selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    // Resolve status color via the documented precedence rule:
    // Aborted > Pending (F2.2) > Open > thinking-only > default.
    // See `t_id_status_color`.
    let tid_style = t_id_status_color(turn, episodes, now).map_or(base_style, |c| base_style.fg(c));
    let spans: Vec<TextSpan<'_>> = vec![
        TextSpan::styled(prefix_tid, tid_style),
        TextSpan::styled(prefix_rest, base_style),
    ];

    let gantt_w = gantt_width;
    let mut cells: Vec<TextSpan<'_>> = vec![TextSpan::raw(" ".repeat(gantt_w as usize))];
    if let Some(t) = turn {
        if let Some(ended) = t.ended_at {
            let started = t.started_at;
            // Scale this turn's gantt width relative to the p95 of
            // non-user-blocking turn durations. Rows above p95 clamp to
            // `gantt_w` via the `.min(gantt_w)` below, so outliers
            // render as fully-filled rows.
            let dur_ms = duration.map_or(0, |d| d.num_milliseconds()).max(0);
            let scaled = u16::try_from(dur_ms * i64::from(gantt_w) / max_dur_ms).unwrap_or(gantt_w);
            let scaled = scaled.min(gantt_w);
            let call_tuples: Vec<_> = t
                .tool_calls
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    let ep = episodes.tools.get(&r.name)?;
                    let call = ep.calls.get(r.index)?;
                    Some((call.span.started_at, call.span.ended_at, i))
                })
                .collect();
            let segs = segment_layout(started, ended, &call_tuples, scaled);
            // Per-cell ToolSource parallel to the char buffer, used to color
            // `█` cells by source (Builtin / MCP / Skill). `None` for cells
            // that are not part of any tool segment (thinking-time `░` or
            // padding `·`).
            let mut sources: Vec<Option<ToolSource>> = vec![None; gantt_w as usize];
            for (cs, cl, seg_idx) in &segs {
                let Some(call_ref) = t.tool_calls.get(*seg_idx) else {
                    continue;
                };
                let Some(source) = episodes
                    .tools
                    .get(&call_ref.name)
                    .map(|ep| ep.source.clone())
                else {
                    continue;
                };
                let start = (*cs as usize).min(sources.len());
                let end = (start + *cl as usize).min(sources.len());
                for slot in &mut sources[start..end] {
                    *slot = Some(source.clone());
                }
            }
            // Build a String of gantt_w cells; overlay segments.
            let buf = build_gantt_cells(scaled, gantt_w, &segs);
            cells = build_styled_cells_with_source(&buf, &sources);
        }
    }
    let mut all = spans;
    all.extend(cells);
    let abort_mod = if matches!(turn.map(|t| &t.status), Some(TurnStatus::Aborted(_))) {
        Modifier::UNDERLINED
    } else {
        Modifier::empty()
    };
    if !abort_mod.is_empty() {
        for s in &mut all {
            s.style = s.style.add_modifier(abort_mod);
        }
    }
    Line::from(all)
}

/// Build per-cell styled spans for one gantt row, coloring `█` cells by
/// [`ToolSource`].
///
/// `buf` contains the gantt characters as produced by [`build_gantt_cells`]
/// (`█` / `░` / `·`). `sources` is a parallel slice of the same length where
/// each entry is the [`ToolSource`] of the tool call occupying that cell, or
/// `None` for cells that are not part of any tool segment (thinking-time
/// `░` or padding `·`).
///
/// Cells are styled as follows:
///
/// - `█` + `Some(source)` → foreground = [`tool_source_color`] (Builtin → cyan,
///   MCP → magenta, Skill → yellow; matches `RoiView`).
/// - `█` + `None` → fallback cyan (defensive; should not occur in practice
///   because every `█` originates from a tool segment that has a known
///   [`ToolSource`]).
/// - `░` → [`Modifier::DIM`] gray (thinking time).
/// - `·` → dark-gray (padding past the turn's wall-time). No `DIM` modifier
///   because on dark terminal themes `DarkGray + DIM` collapses to invisible
///   against a black background; plain `DarkGray` keeps padding visible but
///   subtle.
///
/// Consecutive cells with the same style are merged into a single
/// [`TextSpan`] to keep the rendered output compact.
///
/// # Panics
///
/// Does not panic on a length mismatch between `buf` and `sources`: cells
/// past the shorter slice are treated as having no source.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::ToolSource;
/// use agentprof_tui::views::flamegraph::build_styled_cells_with_source;
/// let buf = ['░', '█', '█', '·'];
/// let sources = [None, Some(ToolSource::Builtin), Some(ToolSource::Builtin), None];
/// let spans = build_styled_cells_with_source(&buf, &sources);
/// // The two adjacent `█` Builtin cells merge into a single span.
/// assert_eq!(spans.len(), 3);
/// assert_eq!(spans[1].content, "██");
/// ```
#[must_use]
pub fn build_styled_cells_with_source<'a>(
    buf: &[char],
    sources: &[Option<ToolSource>],
) -> Vec<TextSpan<'a>> {
    let mut out: Vec<TextSpan<'a>> = Vec::new();
    let mut current: Option<(Style, String)> = None;
    for (i, ch) in buf.iter().enumerate() {
        let style = cell_style(*ch, sources.get(i).and_then(Option::as_ref));
        match &mut current {
            Some((s, run)) if *s == style => run.push(*ch),
            _ => {
                if let Some((s, run)) = current.take() {
                    out.push(TextSpan::styled(run, s));
                }
                let mut run = String::new();
                run.push(*ch);
                current = Some((style, run));
            }
        }
    }
    if let Some((s, run)) = current {
        out.push(TextSpan::styled(run, s));
    }
    out
}

/// Foreground/modifier style for a single gantt cell.
///
/// Pure helper extracted so [`build_styled_cells_with_source`] can be a
/// straightforward run-length compressor.
fn cell_style(ch: char, source: Option<&ToolSource>) -> Style {
    match ch {
        '█' => source.map_or_else(
            || Style::default().fg(Color::Cyan),
            |s| Style::default().fg(tool_source_color(s)),
        ),
        '░' => Style::default().add_modifier(Modifier::DIM),
        '·' => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

/// Build the selected-turn footer line shown below the gantt rows.
///
/// Format: `"T{n} selected:  bash(120ms) read_file(85ms) +K more · Enter for detail"`
/// where `n` is the 1-based turn index, durations come from
/// [`agentprof_core::episode::Span::duration`], and `+K more` appears if
/// truncation from the right was required to fit `max_width` columns. When
/// the selected turn has no tool calls, the line reads
/// `"T{n} selected:  (no tool calls) · thinking only · Enter for detail"`
/// — the `· thinking only` marker confirms what the colored prefix in the
/// gantt row already shows visually. When `turn` is `None` (e.g. an
/// out-of-range selection index), the line reads `"(no turn selected)"`
/// (no hint — there is nothing to open).
///
/// The trailing `" · Enter for detail"` discoverability hint surfaces the
/// [`turn_detail`](crate::views::turn_detail) affordance.
/// On narrow terminals where the hint plus the tool list exceeds
/// `max_width`, the line is truncated from the right and the hint may be
/// cut off — acceptable degradation, since the help overlay (`?`) lists
/// the same key binding.
///
/// Hooks and Skill invocations are intentionally omitted — only entries in
/// [`Turn::tool_calls`] are listed.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::Episodes;
/// use agentprof_tui::views::flamegraph::selected_turn_footer_line;
/// // No turn → fixed placeholder, no hint.
/// let line = selected_turn_footer_line(0, None, &Episodes::default(), 80);
/// assert_eq!(line, "(no turn selected)");
/// ```
#[must_use]
pub fn selected_turn_footer_line(
    turn_idx: usize,
    turn: Option<&Turn>,
    episodes: &Episodes,
    max_width: u16,
) -> String {
    let Some(t) = turn else {
        return "(no turn selected)".to_string();
    };
    let prefix = format!("T{} selected:  ", turn_idx + 1);
    let budget = usize::from(max_width);
    if t.tool_calls.is_empty() {
        let base = format!("{prefix}(no tool calls)");
        let with_mark = append_thinking_marker(base, budget);
        return append_detail_hint(with_mark, budget);
    }

    // Format each call as "name(human_short(dur))". Unknown calls (no
    // matching ToolEpisode entry) are skipped — they would render as
    // "name(?)" which is noise the user can't act on.
    let entries: Vec<String> = t
        .tool_calls
        .iter()
        .filter_map(|r| {
            let call = episodes.tools.get(&r.name)?.calls.get(r.index)?;
            let dur = human_short(call.span.duration());
            Some(format!("{name}({dur})", name = r.name))
        })
        .collect();

    if entries.is_empty() {
        let base = format!("{prefix}(no tool calls)");
        let with_mark = append_thinking_marker(base, budget);
        return append_detail_hint(with_mark, budget);
    }

    let body = fit_entries(&prefix, &entries, budget);
    append_detail_hint(body, budget)
}

/// Append the `" · Enter for detail"` discoverability hint to `line`,
/// or drop it entirely if it does not fully fit within `budget` chars.
///
/// The hint advertises the [`turn_detail`](crate::views::turn_detail)
/// `Enter` affordance. All-or-nothing semantics avoid ambiguous mid-hint
/// truncations (e.g. a lone `· Enter` reading like another keybinding
/// label). This matches the surrounding [`fit_entries`] convention of
/// dropping rather than truncating overflowing items — users can still
/// discover the key via the `?` help overlay.
fn append_detail_hint(line: String, budget: usize) -> String {
    const HINT: &str = " · Enter for detail";
    if budget == 0 || line.chars().count() + HINT.chars().count() > budget {
        return line;
    }
    format!("{line}{HINT}")
}

/// Append `" · thinking only"` to `line` if it fully fits within `budget`
/// characters; otherwise return `line` unchanged. Matches the all-or-
/// nothing semantics of [`append_detail_hint`] so narrow terminals show
/// either the complete marker or nothing — never a half-truncated
/// `· think` that reads like a stray keybinding.
fn append_thinking_marker(line: String, budget: usize) -> String {
    const MARK: &str = " · thinking only";
    if budget == 0 || line.chars().count() + MARK.chars().count() > budget {
        return line;
    }
    format!("{line}{MARK}")
}

/// Build the compact meta line shown directly below the bordered
/// Flamegraph block (F1.9 — replaces the old 3-row bordered " Detail "
/// block).
///
/// Single-row summary of the selected turn that surfaces information not
/// available elsewhere on the same screen: relative start time (from
/// session start), uncompacted duration, tool call count, model, mode.
/// Fields visible in the flame row itself (turn label, duration, output
/// tokens) are intentionally not duplicated here when possible; `dur` is
/// the one exception because the flame row's `Duration` column is fixed
/// at 10 chars and uses [`human_short`] truncation, while this meta line
/// can display the precise value when there is room.
///
/// Format:
///
/// ```text
/// T3 · +1.4m in · 5.0s · 3 calls · model=gpt-5-mini · mode=Interactive
/// ```
///
/// Fields are listed in priority order (drop-from-right on narrow
/// terminals, see [`fit_priority_segments`]):
///
/// | Priority | Field | Source |
/// |---|---|---|
/// | 1 (highest) | `Tn` | `state.flame_selected + 1` (1-based) |
/// | 2 | `+rel in` | `row.started_at - report.meta.started_at` |
/// | 3 | `dur` | `row.duration` (or `—` for open turns) |
/// | 4 | `N calls` | `row.tool_call_count` |
/// | 5 | `model=...` | `row.model` (or `—`) |
/// | 6 (lowest) | `mode=...` | `row.mode` (or `—`) |
///
/// When `state.flame_selected` is out of range, returns
/// `"(no turn selected)"` (parity with [`selected_turn_footer_line`]'s
/// fallback).
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use agentprof_tui::app::state::AppState;
/// use agentprof_tui::views::flamegraph::format_meta_line;
/// use chrono::Utc;
///
/// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
/// let report = AnalysisReport::new(meta);
/// let episodes = Episodes::new();
/// let state = AppState::new(&report, &episodes);
/// // Empty report → out-of-range selection → fixed placeholder.
/// assert_eq!(format_meta_line(&state, 80), "(no turn selected)");
/// ```
#[must_use]
pub fn format_meta_line(state: &AppState<'_>, max_width: u16) -> String {
    let turns = &state.report.turn_summary;
    let Some(row) = turns.get(state.flame_selected) else {
        return "(no turn selected)".to_string();
    };

    let session_start = state.report.meta.started_at;
    let rel_in = human_short(row.started_at - session_start);
    let dur_str = row.duration.map_or_else(|| "—".to_string(), human_short);
    let model_name = row.model.as_deref().unwrap_or("—").to_string();
    let mode_label = row
        .mode
        .as_ref()
        .map_or_else(|| "—".to_string(), |m| format!("{m:?}"));

    // Segments in HIGH-priority → LOW-priority order;
    // [`fit_priority_segments`] drops from the back when overflow.
    let segments = [
        format!("T{}", state.flame_selected + 1),
        format!("+{rel_in} in"),
        dur_str,
        format!("{} calls", row.tool_call_count),
        format!("model={model_name}"),
        format!("mode={mode_label}"),
    ];

    fit_priority_segments(&segments, " · ", max_width as usize)
}

/// Pack as many `segments` (joined by `sep`) into `budget` chars as
/// possible, dropping from the back (low-priority end). Companion to
/// [`format_meta_line`].
///
/// Differs from `fit_entries` (the footer's `+K more` packer) in two
/// ways:
///
/// 1. No `" +K more"` suffix — dropped segments simply disappear. The
///    meta line is a summary, not a list, so the user is not looking
///    for "how many fields are hidden"; they're looking for "the
///    important fields are visible".
/// 2. Drops from the **right** in fixed priority order (caller controls
///    priority by segment list order). `fit_entries` also drops from
///    the right but its semantics are "how many entries fit", not
///    "which high-priority entries to preserve".
///
/// When even the single highest-priority segment overflows, falls back
/// to char-level truncation from the right (acceptable degradation for
/// width-0 / width-1 terminals).
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::flamegraph::fit_priority_segments;
/// let segs = ["T3".to_string(), "+10s in".to_string(), "5.0s".to_string()];
/// // Wide budget → all 3 segments joined.
/// assert_eq!(fit_priority_segments(&segs, " · ", 80), "T3 · +10s in · 5.0s");
/// // Tight budget — drop the lowest-priority last segment.
/// assert_eq!(fit_priority_segments(&segs, " · ", 14), "T3 · +10s in");
/// // Extremely narrow — keep only the first segment.
/// assert_eq!(fit_priority_segments(&segs, " · ", 5), "T3");
/// ```
#[must_use]
pub fn fit_priority_segments(segments: &[String], sep: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    let mut keep = segments.len();
    while keep > 0 {
        let body = segments[..keep].join(sep);
        if body.chars().count() <= budget {
            return body;
        }
        keep -= 1;
    }
    // Even the single highest-priority segment overflows: char-truncate
    // from the right rather than rendering empty.
    segments
        .first()
        .map_or_else(String::new, |s| s.chars().take(budget).collect())
}

/// Pack as many `entries` (joined by a single space) into the budget as
/// possible after `prefix`, appending `" +K more"` when entries had to be
/// dropped. Truncates from the right (drops the *last* entries first).
fn fit_entries(prefix: &str, entries: &[String], budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    let mut keep = entries.len();
    loop {
        let body = entries[..keep].join(" ");
        let dropped = entries.len() - keep;
        let suffix = if dropped == 0 {
            String::new()
        } else {
            format!(" +{dropped} more")
        };
        let total_len = prefix.len() + body.len() + suffix.len();
        if total_len <= budget || keep == 0 {
            // When keep == 0 and even just `prefix + " +K more"` exceeds
            // budget, truncate by char count from the right of the line.
            let mut line = format!("{prefix}{body}{suffix}");
            if line.chars().count() > budget {
                line = line.chars().take(budget).collect();
            }
            return line;
        }
        keep -= 1;
    }
}

/// Build the per-cell gantt character buffer for one turn row.
///
/// The buffer has length `gantt_w` and contains exactly three character
/// kinds, in this fixed precedence (later overrides earlier):
///
/// 1. `░` (U+2591 LIGHT SHADE) — initial fill. Cells that survive both
///    later passes represent **LLM thinking time** (in-turn slices with
///    no tool / hook / skill running).
/// 2. `█` (U+2588 FULL BLOCK) — overlaid for every cell inside any segment
///    in `segs` (`(cell_start, cell_len, _)` as produced by
///    [`segment_layout`]). Represents tool / hook / skill execution.
/// 3. `·` (U+00B7 MIDDLE DOT) — overlaid for every cell past `scaled`,
///    i.e. cells representing wall-time the turn did not occupy because
///    this row is shorter than the longest non-blocking turn. NOT part
///    of the turn — pure visual padding.
///
/// `scaled` is clipped to `gantt_w` (the trailing-padding loop is a no-op
/// when `scaled >= gantt_w`). Segments past `scaled` are overwritten by
/// the padding pass — by design defensive against bad inputs.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::flamegraph::build_gantt_cells;
/// // 10-cell row, scaled=6 (so cells 6..10 will be padding),
/// // one tool segment at cells 2..4.
/// let buf = build_gantt_cells(6, 10, &[(2, 2, 0)]);
/// assert_eq!(
///     buf.iter().collect::<String>(),
///     "░░██░░····",
/// );
/// ```
#[must_use]
pub fn build_gantt_cells(scaled: u16, gantt_w: u16, segs: &[(u16, u16, usize)]) -> Vec<char> {
    let width = gantt_w as usize;
    let mut buf: Vec<char> = vec!['░'; width];
    for (cs, cl, _) in segs {
        let start = *cs as usize;
        let end = (start + *cl as usize).min(width);
        for cell in &mut buf[start.min(width)..end] {
            *cell = '█';
        }
    }
    for cell in buf.iter_mut().skip(scaled as usize) {
        *cell = '·';
    }
    buf
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn segment_layout_for_3_tools_in_one_turn() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t10 = t0 + Duration::seconds(10);
        let calls = [
            (t0 + Duration::seconds(0), t0 + Duration::seconds(2), 0),
            (t0 + Duration::seconds(3), t0 + Duration::seconds(5), 1),
            (t0 + Duration::seconds(8), t0 + Duration::seconds(10), 2),
        ];
        let segs = segment_layout(t0, t10, &calls, 20);
        assert_eq!(segs.len(), 3);
        // First call: 0..2s of 10s in 20 cells = cells 0..4
        assert_eq!(segs[0], (0, 4, 0));
        // Second: 3..5s = cells 6..10
        assert_eq!(segs[1], (6, 4, 1));
        // Third: 8..10s = cells 16..20
        assert_eq!(segs[2], (16, 4, 2));
    }

    #[test]
    fn segment_layout_zero_duration_turn_returns_empty() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let segs = segment_layout(t0, t0, &[(t0, t0, 0)], 20);
        assert!(segs.is_empty());
    }

    #[test]
    fn segment_layout_zero_width_returns_empty() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t10 = t0 + Duration::seconds(10);
        let segs = segment_layout(t0, t10, &[(t0, t10, 0)], 0);
        assert!(segs.is_empty());
    }

    /// Build a `Turn` for tests with `started_at`/`ended_at` set and one
    /// tool call by name.
    fn turn_with_tool(id: &str, started_s: i64, ended_s: i64, tool: &str) -> Turn {
        use agentprof_core::episode::CallRef;
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new(id.into(), base + Duration::seconds(started_s));
        turn.ended_at = Some(base + Duration::seconds(ended_s));
        turn.status = TurnStatus::Completed;
        turn.tool_calls.push(CallRef::new(tool.into(), 0));
        turn
    }

    /// Replicates the `max_dur` (p95-based) scaling calculation from
    /// `render()` so we can unit-test it without instantiating a full
    /// ratatui Frame + `AppState`.
    fn compute_max_dur(turns: &[Turn]) -> i64 {
        let turn_by_id: std::collections::HashMap<&str, &Turn> =
            turns.iter().map(|t| (t.id.as_str(), t)).collect();

        let mut non_blocking_durs_ms: Vec<i64> = turns
            .iter()
            .filter_map(|t| {
                let turn = turn_by_id.get(t.id.as_str())?;
                if turn.is_user_blocking() {
                    None
                } else {
                    t.ended_at.map(|e| (e - t.started_at).num_milliseconds())
                }
            })
            .collect();
        non_blocking_durs_ms.sort_unstable();

        if non_blocking_durs_ms.is_empty() {
            turns
                .iter()
                .filter_map(|t| t.ended_at.map(|e| (e - t.started_at).num_milliseconds()))
                .max()
                .unwrap_or(1)
                .max(1)
        } else {
            let n = non_blocking_durs_ms.len();
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let p95_idx_raw = ((n as f64) * 0.95).ceil() as usize;
            let p95_idx = p95_idx_raw.saturating_sub(1).min(n - 1);
            non_blocking_durs_ms[p95_idx].max(1)
        }
    }

    #[test]
    fn p95_dur_excludes_user_blocking_turns() {
        // 3 turns:
        // - T1: 5s normal turn (`bash`)
        // - T2: 600s (10min) user-blocking turn (`ask_user`)
        // - T3: 5s normal turn (`edit`)
        //
        // After filtering out the user-blocking turn, the remaining
        // non-blocking durations are [5000, 5000] ms. With n=2,
        // p95_idx = ceil(2*0.95)-1 = ceil(1.9)-1 = 2-1 = 1 → sorted[1]
        // = 5000 ms. Verifies that user-blocking filter still applies on
        // top of p95 scaling.
        let turns = vec![
            turn_with_tool("t1", 0, 5, "bash"),
            turn_with_tool("t2", 10, 610, "ask_user"),
            turn_with_tool("t3", 700, 705, "edit"),
        ];
        assert_eq!(compute_max_dur(&turns), 5_000);
    }

    #[test]
    fn p95_dur_falls_back_when_all_turns_are_user_blocking() {
        // Degenerate case: only user-blocking turns exist. Filter would
        // produce an empty vec — must fall back to max-across-all so we
        // don't divide by 1 ms.
        let turns = vec![turn_with_tool("t1", 0, 300, "ask_user")];
        assert_eq!(
            compute_max_dur(&turns),
            300_000,
            "fallback must use all-turns max when filter empties"
        );
    }

    #[test]
    fn p95_scaling_resists_single_long_agent_task_outlier() {
        // The motivating bug-fix case: 20 short turns (5s each) + 1
        // long agent-task turn (50min). All are non-user-blocking (no
        // `ask_user` involved — `task` is agent-self-driven).
        //
        // With max-based scaling: 50min = 3_000_000ms would set the
        // scale and 5s turns would render to <1% of gantt_w (squashed
        // to 1 cell). That is exactly what b5c1429 *didn't* fix.
        //
        // With p95-based scaling: sorted durations are
        // [5000 × 20, 3_000_000]. n = 21, p95_idx =
        // ceil(21 * 0.95) - 1 = ceil(19.95) - 1 = 20 - 1 = 19, so
        // sorted[19] = 5000 ms — the outlier at sorted[20] is NOT
        // picked. Normal turns render at full gantt width.
        let mut turns: Vec<Turn> = (0..20)
            .map(|i| turn_with_tool(&format!("short{i}"), i * 10, i * 10 + 5, "bash"))
            .collect();
        // 50-minute agent-task outlier (3000s = 50min). `task` is not
        // in USER_BLOCKING_TOOLS, so it survives the is_user_blocking
        // filter.
        turns.push(turn_with_tool("long", 10_000, 10_000 + 3_000, "task"));

        assert_eq!(
            compute_max_dur(&turns),
            5_000,
            "p95 of 21 sorted durations [5000×20, 3_000_000] is sorted[19] = 5000ms, \
             not the outlier at sorted[20] = 3_000_000ms"
        );
    }

    #[test]
    fn p95_scaling_with_single_turn_uses_that_turn() {
        // n = 1 → p95_idx = ceil(0.95) - 1 = 1 - 1 = 0 → sorted[0].
        // Single turn is its own p95.
        let turns = vec![turn_with_tool("only", 0, 12, "bash")];
        assert_eq!(compute_max_dur(&turns), 12_000);
    }

    #[test]
    fn p95_scaling_with_many_uniform_turns_picks_around_max() {
        // n = 100, all 5000 ms. p95_idx = ceil(100*0.95)-1 = 94.
        // sorted[94] = 5000 ms. Confirms p95 ≈ max in the no-outlier
        // case (so the existing snapshot fixtures with few turns and
        // uniform durations are unaffected).
        let turns: Vec<Turn> = (0..100)
            .map(|i| turn_with_tool(&format!("t{i}"), i * 10, i * 10 + 5, "bash"))
            .collect();
        assert_eq!(compute_max_dur(&turns), 5_000);
    }

    #[test]
    fn render_cell_chars_distinguish_thinking_tool_padding() {
        // Two synthetic turns rendered to the same 40-cell gantt area.
        //
        // Both have one tool call covering cells 8..16 (8 cells of █).
        //
        // T1 (= longest non-blocking turn): scaled == gantt_w == 40, so
        // the row has NO padding — cells outside the tool segment are
        // pure thinking time.
        //   - cells  0.. 8  → ░ (thinking before tool)
        //   - cells  8..16  → █ (tool execution)
        //   - cells 16..40  → ░ (thinking after tool)
        let t1 = build_gantt_cells(40, 40, &[(8, 8, 0)]);
        let t1_str: String = t1.iter().collect();
        assert_eq!(t1.len(), 40);
        assert_eq!(t1_str.chars().filter(|&c| c == '░').count(), 32);
        assert_eq!(t1_str.chars().filter(|&c| c == '█').count(), 8);
        assert_eq!(
            t1_str.chars().filter(|&c| c == '·').count(),
            0,
            "longest turn must have zero padding cells"
        );
        // Spot-check the boundary cells.
        assert_eq!(t1[7], '░');
        assert_eq!(t1[8], '█');
        assert_eq!(t1[15], '█');
        assert_eq!(t1[16], '░');

        // T2 (shorter, e.g. wall-time = 40% of T1): scaled = 16, so cells
        // 16..40 are padding past the turn's actual end.
        //   - cells  0.. 8  → ░ (thinking before tool)
        //   - cells  8..16  → █ (tool execution)
        //   - cells 16..40  → · (padding)
        let t2 = build_gantt_cells(16, 40, &[(8, 8, 0)]);
        let t2_str: String = t2.iter().collect();
        assert_eq!(t2.len(), 40);
        assert_eq!(t2_str.chars().filter(|&c| c == '░').count(), 8);
        assert_eq!(t2_str.chars().filter(|&c| c == '█').count(), 8);
        assert_eq!(t2_str.chars().filter(|&c| c == '·').count(), 24);
        assert_eq!(t2[7], '░');
        assert_eq!(t2[8], '█');
        assert_eq!(t2[15], '█');
        assert_eq!(t2[16], '·');
        assert_eq!(t2[39], '·');
    }

    #[test]
    fn build_gantt_cells_pure_thinking_no_tools_no_padding() {
        // No tool calls, no padding (scaled == gantt_w): every cell is
        // thinking. Pre-fix this would have been all-spaces — visually
        // indistinguishable from padding.
        let buf = build_gantt_cells(10, 10, &[]);
        assert_eq!(buf.iter().collect::<String>(), "░░░░░░░░░░");
    }

    #[test]
    fn build_gantt_cells_segments_past_scaled_are_overwritten_by_padding() {
        // Defensive: if `segment_layout` ever produced a segment past
        // `scaled` (it shouldn't, since segments are clipped to the
        // `scaled` row width), the trailing padding pass must still win
        // — otherwise the rendered row would be inconsistent with the
        // turn's actual wall-time.
        let buf = build_gantt_cells(4, 8, &[(6, 2, 0)]);
        assert_eq!(buf.iter().collect::<String>(), "░░░░····");
    }

    #[test]
    fn build_styled_cells_colors_tool_blocks_by_source() {
        // Layout: ░ █(Builtin) █(Builtin) █(MCP) █(Skill) ░ ·
        //         thinking, two-cell Builtin run, one MCP, one Skill,
        //         then thinking + padding.
        let buf = ['░', '█', '█', '█', '█', '░', '·'];
        let sources = [
            None,
            Some(ToolSource::Builtin),
            Some(ToolSource::Builtin),
            Some(ToolSource::Mcp {
                server: "github".into(),
            }),
            Some(ToolSource::Skill {
                name: "lint".into(),
            }),
            None,
            None,
        ];

        let spans = build_styled_cells_with_source(&buf, &sources);
        // Expected spans (run-length compressed by style):
        //   "░"  dim
        //   "██" cyan (Builtin)
        //   "█"  magenta (Mcp)
        //   "█"  yellow (Skill)
        //   "░"  dim
        //   "·"  dark gray (no DIM — invisible on black terminals)
        assert_eq!(spans.len(), 6, "spans: {spans:?}");
        assert_eq!(spans[0].content, "░");
        assert!(
            spans[0].style.add_modifier.contains(Modifier::DIM),
            "thinking cell should be DIM"
        );
        assert_eq!(spans[1].content, "██");
        assert_eq!(spans[1].style.fg, Some(Color::Cyan), "Builtin → cyan");
        assert_eq!(spans[2].content, "█");
        assert_eq!(spans[2].style.fg, Some(Color::Magenta), "MCP → magenta");
        assert_eq!(spans[3].content, "█");
        assert_eq!(spans[3].style.fg, Some(Color::Yellow), "Skill → yellow");
        assert_eq!(spans[4].content, "░");
        assert_eq!(spans[5].content, "·");
        assert_eq!(spans[5].style.fg, Some(Color::DarkGray));
        assert!(
            !spans[5].style.add_modifier.contains(Modifier::DIM),
            "padding · must NOT use DIM — DarkGray + DIM is invisible on \
             dark terminal themes (see regression: 2026-06-03 user report)"
        );
    }

    #[test]
    fn build_styled_cells_handles_no_sources_thinking_only() {
        // All `None` sources, mix of ░ and · only (no █). Verify no fg
        // color is set for thinking cells (they should rely on DIM only,
        // not a foreground color override).
        let buf = ['░', '░', '░', '·', '·'];
        let sources = [None, None, None, None, None];
        let spans = build_styled_cells_with_source(&buf, &sources);
        // Expect 2 spans: "░░░" (dim, no fg) + "··" (DarkGray, no DIM).
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "░░░");
        assert_eq!(spans[0].style.fg, None, "thinking has no fg override");
        assert!(spans[0].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(spans[1].content, "··");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert!(
            !spans[1].style.add_modifier.contains(Modifier::DIM),
            "padding · must NOT use DIM (see test \
             build_styled_cells_handles_all_cell_types)"
        );
    }

    #[test]
    fn build_styled_cells_falls_back_to_cyan_when_source_missing_for_block() {
        // Defensive path: a `█` cell with no source entry (length
        // mismatch, or `None` slot) should still render — fall back to
        // cyan so it stays visible.
        let buf = ['█', '█'];
        let sources: [Option<ToolSource>; 0] = [];
        let spans = build_styled_cells_with_source(&buf, &sources);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "██");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn selected_turn_footer_line_lists_calls_with_durations() {
        use agentprof_core::episode::{CallRef, Span as EpisodeSpan, ToolCall, ToolEpisode};
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        // Build a Turn with 2 tool calls: bash + edit.
        let mut t = Turn::new("t1".into(), base);
        t.ended_at = Some(base + Duration::seconds(2));
        t.status = TurnStatus::Completed;
        t.tool_calls.push(CallRef::new("bash".into(), 0));
        t.tool_calls.push(CallRef::new("edit".into(), 0));
        // Episodes carry the per-call timing.
        let mut episodes = Episodes::default();
        let mut bash_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        bash_ep.calls.push(ToolCall::new(EpisodeSpan::new(
            base,
            base + Duration::milliseconds(120),
        )));
        episodes.tools.insert("bash".into(), bash_ep);
        let mut edit_ep = ToolEpisode::new("edit".into(), ToolSource::Builtin);
        edit_ep.calls.push(ToolCall::new(EpisodeSpan::new(
            base + Duration::milliseconds(500),
            base + Duration::milliseconds(720),
        )));
        episodes.tools.insert("edit".into(), edit_ep);

        let line = selected_turn_footer_line(2, Some(&t), &episodes, 200);
        assert!(
            line.starts_with("T3 selected:  "),
            "footer should start with 1-based label: {line:?}",
        );
        assert!(line.contains("bash(120ms)"), "got: {line:?}");
        assert!(line.contains("edit(220ms)"), "got: {line:?}");
        assert!(!line.contains('+'), "no truncation expected: {line:?}");
        assert!(
            line.ends_with(" · Enter for detail"),
            "wide footer should advertise the detail-view hint: {line:?}",
        );
    }

    #[test]
    fn selected_turn_footer_line_empty_tool_calls() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("t1".into(), base);
        t.ended_at = Some(base + Duration::seconds(1));
        let line = selected_turn_footer_line(0, Some(&t), &Episodes::default(), 80);
        assert_eq!(
            line,
            "T1 selected:  (no tool calls) · thinking only · Enter for detail"
        );
    }

    #[test]
    fn selected_turn_footer_line_none_turn() {
        let line = selected_turn_footer_line(5, None, &Episodes::default(), 80);
        assert_eq!(line, "(no turn selected)");
    }

    #[test]
    fn selected_turn_footer_line_truncates_with_more_suffix() {
        use agentprof_core::episode::{CallRef, Span as EpisodeSpan, ToolCall, ToolEpisode};
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("t1".into(), base);
        t.ended_at = Some(base + Duration::seconds(10));
        let mut episodes = Episodes::default();
        // 5 calls of the same tool; force truncation by giving budget
        // that only fits 2 entries + the " +3 more" suffix.
        let mut ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        for i in 0..5_i64 {
            t.tool_calls
                .push(CallRef::new("bash".into(), usize::try_from(i).unwrap()));
            ep.calls.push(ToolCall::new(EpisodeSpan::new(
                base + Duration::milliseconds(i * 100),
                base + Duration::milliseconds(i * 100 + 80),
            )));
        }
        episodes.tools.insert("bash".into(), ep);

        // Narrow budget: "T1 selected:  bash(80ms) bash(80ms) +3 more" = 43.
        // The " · Enter for detail" hint (19 chars) doesn't fit within
        // the 44-char budget, so it is dropped entirely (all-or-nothing
        // semantics) and the line ends cleanly on " +3 more".
        let line = selected_turn_footer_line(0, Some(&t), &episodes, 44);
        assert!(line.starts_with("T1 selected:  "), "got: {line:?}");
        assert!(line.ends_with(" +3 more"), "got: {line:?}");
        assert!(line.len() <= 44, "got: {line:?} (len {})", line.len());
        assert!(
            !line.contains("Enter for detail"),
            "hint should be fully dropped when there's no room: {line:?}",
        );
    }

    #[test]
    fn selected_turn_footer_line_hint_dropped_when_partial_fit() {
        use agentprof_core::episode::{CallRef, Span as EpisodeSpan, ToolCall, ToolEpisode};
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("t1".into(), base);
        t.ended_at = Some(base + Duration::seconds(10));
        let mut episodes = Episodes::default();
        let mut ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        for i in 0..5_i64 {
            t.tool_calls
                .push(CallRef::new("bash".into(), usize::try_from(i).unwrap()));
            ep.calls.push(ToolCall::new(EpisodeSpan::new(
                base + Duration::milliseconds(i * 100),
                base + Duration::milliseconds(i * 100 + 80),
            )));
        }
        episodes.tools.insert("bash".into(), ep);

        // At a budget where the body fits with " +K more" but the full
        // " · Enter for detail" hint (19 chars) does NOT fit, the hint
        // is dropped entirely (all-or-nothing semantics).
        // body=43 chars + hint=19 = 62 chars. With budget=50, hint
        // doesn't fit → dropped.
        let line = selected_turn_footer_line(0, Some(&t), &episodes, 50);
        assert!(
            !line.contains("Enter"),
            "hint must be dropped entirely when it doesn't fully fit: {line}"
        );
        assert!(
            line.ends_with("+3 more") || line.contains("+3 more"),
            "body content preserved: {line}"
        );
    }

    #[test]
    fn build_row_thinking_only_turn_has_blue_prefix() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("T1".into(), base);
        turn.ended_at = Some(base + Duration::seconds(2));
        // tool_calls intentionally left empty → thinking-only.
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let first = &line.spans[0];
        assert_eq!(
            first.style.fg,
            Some(Color::Blue),
            "thinking-only turn prefix must be Blue"
        );
        assert!(
            !first.style.add_modifier.contains(Modifier::REVERSED),
            "unselected row must not be REVERSED"
        );
    }

    #[test]
    fn build_row_thinking_only_selected_composes_blue_with_reversed() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("T1".into(), base);
        turn.ended_at = Some(base + Duration::seconds(2));
        let episodes = Episodes::default();
        let line = build_row(
            0,
            true, // selected
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let first = &line.spans[0];
        assert_eq!(first.style.fg, Some(Color::Blue));
        assert!(first.style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn build_row_thinking_only_aborted_t_id_is_red_not_blue() {
        // F1.10 precedence rule: Aborted > Open > thinking-only.
        // An aborted-thinking-only turn shows Red T-id (Aborted wins),
        // NOT Blue (was the pre-F1.10 behavior).
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("T1".into(), base);
        turn.ended_at = Some(base + Duration::seconds(2));
        turn.status =
            TurnStatus::Aborted(agentprof_core::episode::AbortInfo::new("test".into(), base));
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let tid = &line.spans[0];
        assert_eq!(
            tid.style.fg,
            Some(Color::Red),
            "aborted-thinking-only turn T-id must be Red (Aborted > Blue per F1.10 precedence)"
        );
        assert!(
            tid.style.add_modifier.contains(Modifier::UNDERLINED),
            "aborted T-id must still carry UNDERLINED as a color-blind backup signal"
        );
    }

    #[test]
    fn build_row_turn_with_tool_calls_has_no_blue_prefix() {
        use agentprof_core::episode::{Span as EpisodeSpan, ToolCall, ToolEpisode};
        let turn = turn_with_tool("T1", 0, 2, "bash");
        let mut episodes = Episodes::default();
        let mut ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        let ended = turn.ended_at.unwrap_or(turn.started_at);
        ep.calls
            .push(ToolCall::new(EpisodeSpan::new(turn.started_at, ended)));
        episodes.tools.insert("bash".into(), ep);
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let first = &line.spans[0];
        assert_ne!(
            first.style.fg,
            Some(Color::Blue),
            "tool-bearing turn prefix must NOT be Blue"
        );
    }

    #[test]
    fn selected_turn_footer_line_thinking_only_says_thinking_only() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("t1".into(), base);
        t.ended_at = Some(base + Duration::seconds(1));
        let line = selected_turn_footer_line(0, Some(&t), &Episodes::default(), 80);
        assert!(
            line.contains("thinking only"),
            "footer must surface thinking-only marker: {line}"
        );
        assert!(line.contains("(no tool calls)"), "got: {line}");
    }

    #[test]
    fn build_row_open_turn_with_no_tool_calls_is_not_blue() {
        // An in-flight turn (ended_at=None) with empty tool_calls must NOT
        // be marked Blue. Otherwise watch mode would flash Blue briefly on
        // every new turn before tools arrive. See review I-1.
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let turn = Turn::new("T1".into(), base);
        // tool_calls is empty by default; ended_at is None (open).
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            None, // duration unknown for open turn
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let first = &line.spans[0];
        assert_ne!(
            first.style.fg,
            Some(Color::Blue),
            "open turn (ended_at=None) must not get Blue prefix even with empty tool_calls"
        );
    }

    #[test]
    fn footer_thinking_marker_dropped_when_budget_too_tight() {
        // Thinking-only turn (no tool_calls, ended). Mirrors the
        // `selected_turn_footer_line_thinking_only_says_thinking_only`
        // fixture but uses a tight budget.
        //
        // Body: "T1 selected:  (no tool calls)" = 29 chars.
        // " · thinking only" adds 16 → 45 total, far exceeding budget=30.
        // All-or-nothing: marker (and hint) must be dropped, body kept.
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("t1".into(), base);
        t.ended_at = Some(base + Duration::seconds(1));
        let line = selected_turn_footer_line(0, Some(&t), &Episodes::default(), 30);
        assert!(
            !line.contains("thinking"),
            "thinking marker must be dropped when budget too tight: {line}"
        );
        assert!(
            line.contains("(no tool calls)"),
            "body must be preserved: {line}"
        );
    }

    #[test]
    fn build_row_selected_aborted_t_id_red_with_reversed_and_underlined() {
        // F1.10: triple composition on the T-id span — selected
        // (REVERSED) + aborted (UNDERLINED) + Aborted-precedence color
        // (Red, NOT Blue even though thinking-only also true). All three
        // signals must coexist on the same span — none clobber the
        // others.
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("T1".into(), base);
        turn.ended_at = Some(base + Duration::seconds(2));
        turn.status =
            TurnStatus::Aborted(agentprof_core::episode::AbortInfo::new("test".into(), base));
        // tool_calls remains empty → would be thinking-only, but
        // Aborted takes precedence per F1.10 §3.5.
        let episodes = Episodes::default();
        let line = build_row(
            0,
            true, // selected
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let tid = &line.spans[0];
        assert_eq!(
            tid.style.fg,
            Some(Color::Red),
            "Aborted Red must survive REVERSED + UNDERLINED (and override Blue)"
        );
        assert!(
            tid.style.add_modifier.contains(Modifier::REVERSED),
            "REVERSED must survive Red + UNDERLINED"
        );
        assert!(
            tid.style.add_modifier.contains(Modifier::UNDERLINED),
            "UNDERLINED must survive Red + REVERSED"
        );
    }

    #[test]
    fn build_row_includes_tokens_column_when_some() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("T1".into(), base);
        turn.ended_at = Some(base + Duration::seconds(2));
        turn.output_tokens = Some(1234);
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        // F1.10: prefix is split into 2 spans (T-id + rest). Token column
        // lives in spans[1] (the "rest" span).
        let prefix_rest = line.spans[1].content.as_ref();
        assert!(
            prefix_rest.contains("1.2k"),
            "prefix rest must contain abbreviated token count, got {prefix_rest:?}"
        );
    }

    #[test]
    fn build_row_tokens_column_dash_when_none() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("T1".into(), base);
        turn.ended_at = Some(base + Duration::seconds(2));
        // turn.output_tokens defaults to None on Turn::new.
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        // F1.10: token column lives in spans[1] (post-split).
        let prefix_rest = line.spans[1].content.as_ref();
        // Centered dash from format_tokens_short(None) == "  -  ".
        assert!(
            prefix_rest.contains("  -  "),
            "prefix rest must contain centered dash for None tokens, got {prefix_rest:?}"
        );
    }

    #[test]
    fn build_row_prefix_width_matches_layout_constant() {
        // Critical invariant: the prefix produced by build_row MUST be
        // exactly PREFIX_WIDTH chars wide, else render() over-allocates
        // gantt cells and ratatui silently clips the rightmost edge.
        // See F1.6 critical code-review fix.
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(1)),
            1000,
            50,   // gantt_width
            None, // turn (None → no gantt cells rendered, but prefix still produced)
            &episodes,
            chrono::Utc::now(),
        );
        // F1.10: prefix is split into 2 spans. Sum span[0] + span[1]
        // must equal PREFIX_WIDTH (5 + 19 = 24).
        let prefix_chars =
            line.spans[0].content.chars().count() + line.spans[1].content.chars().count();
        assert_eq!(
            prefix_chars, PREFIX_WIDTH as usize,
            "PREFIX_WIDTH constant must match actual build_row prefix width \
             (sum of T-id span + rest span) — if you change the format!() \
             in build_row, update PREFIX_WIDTH too"
        );
        // Defense: span split must be exactly 5 + 19 to keep T-id labels
        // aligned with the header's T-id column.
        assert_eq!(
            line.spans[0].content.chars().count(),
            5,
            "T-id span must be exactly 5 chars wide (matches the right-aligned 5-width template)"
        );
        assert_eq!(
            line.spans[1].content.chars().count(),
            19,
            "Rest span must be exactly 19 chars (PREFIX_WIDTH - T-id width)"
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // Sticky header (F1.8) tests
    // ──────────────────────────────────────────────────────────────────

    /// Flatten all spans in a [`Line`] to a plain String for assertions.
    fn line_to_string(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn header_line_prefix_matches_build_row_format() {
        // The first PREFIX_WIDTH chars of header_line MUST be the exact
        // same template as build_row's prefix so column labels sit
        // directly above the data values. If this fails, the layout
        // constant changed in only one of the two functions.
        let text = line_to_string(&header_line());
        let prefix: String = text.chars().take(PREFIX_WIDTH as usize).collect();
        assert_eq!(
            prefix.chars().count(),
            PREFIX_WIDTH as usize,
            "header prefix must be exactly PREFIX_WIDTH chars"
        );
        assert_eq!(
            prefix,
            format!("{:>5} {:>10} {:>5}  ", "Turn", "Duration", "OutTK"),
            "header prefix must match build_row's right-aligned 5/10/5 column template"
        );
    }

    #[test]
    fn header_line_contains_three_legend_symbols() {
        // The user-facing purpose of the header is to teach the meaning
        // of the gantt cell characters. All three symbols MUST appear
        // in the legend, in any order.
        let text = line_to_string(&header_line());
        assert!(
            text.contains('█'),
            "header_line missing █ (tool block) legend: {text:?}"
        );
        assert!(
            text.contains('░'),
            "header_line missing ░ (thinking) legend: {text:?}"
        );
        assert!(
            text.contains('·'),
            "header_line missing · (padding) legend: {text:?}"
        );
    }

    #[test]
    fn header_line_legend_labels_present() {
        // Each symbol must be paired with its word so a user looking at
        // the header can map character → meaning without external docs.
        let text = line_to_string(&header_line());
        for word in ["tool", "thinking", "padding"] {
            assert!(
                text.contains(word),
                "header_line missing legend word {word:?}: {text:?}"
            );
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // T-id status color (F1.10) tests
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn t_id_status_color_returns_none_for_no_turn() {
        assert_eq!(
            t_id_status_color(None, &Episodes::default(), chrono::Utc::now()),
            None
        );
    }

    #[test]
    fn t_id_status_color_aborted_returns_red() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("T1".into(), base);
        t.ended_at = Some(base + Duration::seconds(2));
        t.status =
            TurnStatus::Aborted(agentprof_core::episode::AbortInfo::new("test".into(), base));
        assert_eq!(
            t_id_status_color(Some(&t), &Episodes::default(), chrono::Utc::now()),
            Some(Color::Red)
        );
    }

    #[test]
    fn t_id_status_color_open_returns_darkgray() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let t = Turn::new("T1".into(), base);
        // ended_at is None by default → Open / in-flight.
        assert_eq!(
            t_id_status_color(Some(&t), &Episodes::default(), chrono::Utc::now()),
            Some(Color::DarkGray)
        );
    }

    #[test]
    fn t_id_status_color_thinking_only_returns_blue() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("T1".into(), base);
        t.ended_at = Some(base + Duration::seconds(2));
        // tool_calls empty + closed + not aborted → thinking-only.
        assert_eq!(
            t_id_status_color(Some(&t), &Episodes::default(), chrono::Utc::now()),
            Some(Color::Blue)
        );
    }

    #[test]
    fn t_id_status_color_completed_with_tools_returns_none() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("T1".into(), base);
        t.ended_at = Some(base + Duration::seconds(2));
        t.tool_calls
            .push(agentprof_core::episode::CallRef::new("bash".into(), 0));
        assert_eq!(
            t_id_status_color(Some(&t), &Episodes::default(), chrono::Utc::now()),
            None
        );
    }

    #[test]
    fn t_id_status_color_aborted_open_red_wins_over_darkgray() {
        // Precedence: Aborted > Open. A turn whose status is Aborted
        // AND has no ended_at (defensive: should not happen in real
        // data but logic must be robust) returns Red.
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("T1".into(), base);
        // ended_at remains None → would normally be Open / DarkGray.
        t.status =
            TurnStatus::Aborted(agentprof_core::episode::AbortInfo::new("test".into(), base));
        assert_eq!(
            t_id_status_color(Some(&t), &Episodes::default(), chrono::Utc::now()),
            Some(Color::Red)
        );
    }

    #[test]
    fn build_row_open_turn_t_id_is_darkgray() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let turn = Turn::new("T1".into(), base);
        // ended_at = None → Open / in-flight.
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            None, // duration unknown (open)
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let tid = &line.spans[0];
        assert_eq!(
            tid.style.fg,
            Some(Color::DarkGray),
            "open turn T-id must be DarkGray"
        );
    }

    #[test]
    fn build_row_aborted_turn_t_id_is_red_and_underlined() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = turn_with_tool("T1", 0, 2, "bash"); // has tool calls (not thinking-only)
        turn.status =
            TurnStatus::Aborted(agentprof_core::episode::AbortInfo::new("test".into(), base));
        let mut episodes = Episodes::default();
        let mut ep = agentprof_core::episode::ToolEpisode::new("bash".into(), ToolSource::Builtin);
        ep.calls.push(agentprof_core::episode::ToolCall::new(
            agentprof_core::episode::Span::new(turn.started_at, turn.ended_at.unwrap()),
        ));
        episodes.tools.insert("bash".into(), ep);
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let tid = &line.spans[0];
        assert_eq!(tid.style.fg, Some(Color::Red), "aborted T-id must be Red");
        assert!(
            tid.style.add_modifier.contains(Modifier::UNDERLINED),
            "aborted T-id must carry UNDERLINED as color-blind backup"
        );
    }

    #[test]
    fn build_row_thinking_only_blue_only_on_t_id_span() {
        // F1.10 tightening: F1.5's Blue marker now confined to the
        // 5-char T-id span. The duration/OutTK columns (spans[1]) must
        // NOT carry the Blue fg, so they remain visually consistent
        // across turn statuses.
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut turn = Turn::new("T1".into(), base);
        turn.ended_at = Some(base + Duration::seconds(2));
        // tool_calls empty → thinking-only.
        let episodes = Episodes::default();
        let line = build_row(
            0,
            false,
            Some(Duration::seconds(2)),
            2000,
            40,
            Some(&turn),
            &episodes,
            chrono::Utc::now(),
        );
        let tid = &line.spans[0];
        let rest = &line.spans[1];
        assert_eq!(
            tid.style.fg,
            Some(Color::Blue),
            "thinking-only T-id span must be Blue"
        );
        assert_eq!(
            rest.style.fg, None,
            "thinking-only rest span (duration / OutTK columns) must NOT be Blue"
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // Meta line (F1.9) tests
    // ──────────────────────────────────────────────────────────────────

    use crate::app::state::AppState;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::{AnalysisReport, TurnSummaryRow};
    use agentprof_core::episode::{Mode, TurnStatus};
    use agentprof_core::model::SessionMeta;

    /// Build an [`AppState`] for meta-line tests. The session starts at
    /// 2026-06-04T10:00:00Z; the single turn starts +1.4m (= 84s) into
    /// the session, has 5.0s duration, 3 tool calls, model = "gpt-5-mini",
    /// and mode = `Interactive`.
    fn meta_line_state<'a>(report: &'a AnalysisReport, episodes: &'a Episodes) -> AppState<'a> {
        let mut s = AppState::new(report, episodes);
        s.flame_selected = 0;
        s
    }

    fn meta_line_report_full() -> AnalysisReport {
        let session_start = Utc.with_ymd_and_hms(2026, 6, 4, 10, 0, 0).unwrap();
        let turn_start = session_start + Duration::seconds(84); // human_short → "1.4m"
        let row = TurnSummaryRow::new(
            "t1".to_string(),
            turn_start,
            Some(Duration::seconds(5)),
            TurnStatus::Completed,
            Some("gpt-5-mini".to_string()),
            Some(Mode::Interactive),
            Some(123),
            3,
            0,
            0,
        );
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, session_start, false);
        let mut r = AnalysisReport::new(meta);
        r.turn_summary.push(row);
        r
    }

    #[test]
    fn meta_line_includes_all_fields_when_full_width() {
        let report = meta_line_report_full();
        let episodes = Episodes::new();
        let state = meta_line_state(&report, &episodes);
        let line = format_meta_line(&state, 120);
        for needle in [
            "T1",
            "+1.4m in",
            "5.0s",
            "3 calls",
            "model=gpt-5-mini",
            "mode=Interactive",
        ] {
            assert!(
                line.contains(needle),
                "wide-width meta line missing {needle:?}: {line:?}"
            );
        }
    }

    #[test]
    fn meta_line_drops_mode_first_on_narrow_width() {
        // Full line is ~70 chars. Budget 50 forces a drop, and `mode=...`
        // (lowest priority) must go first while `model=...` stays.
        let report = meta_line_report_full();
        let episodes = Episodes::new();
        let state = meta_line_state(&report, &episodes);
        let line = format_meta_line(&state, 50);
        assert!(
            !line.contains("mode="),
            "narrow-width meta line should drop mode= first: {line:?}"
        );
        assert!(
            line.contains("model=gpt-5-mini"),
            "narrow-width meta line should keep model=: {line:?}"
        );
    }

    #[test]
    fn meta_line_keeps_t_id_relative_dur_when_extremely_narrow() {
        // Full line is "T1 · +1.4m in · 5.0s · 3 calls · model=gpt-5-mini · mode=Interactive".
        // Budget 20 fits "T1 · +1.4m in · 5.0s" (20 chars). Budget 19
        // forces another drop to "T1 · +1.4m in" (13 chars).
        let report = meta_line_report_full();
        let episodes = Episodes::new();
        let state = meta_line_state(&report, &episodes);
        let line = format_meta_line(&state, 19);
        assert!(
            line.contains("T1"),
            "extreme-narrow meta line must keep T-id: {line:?}"
        );
        assert!(
            line.contains("+1.4m"),
            "extreme-narrow meta line must keep relative start: {line:?}"
        );
        assert!(
            !line.contains("model="),
            "extreme-narrow meta line must drop model: {line:?}"
        );
        assert!(
            !line.contains("calls"),
            "extreme-narrow meta line must drop calls: {line:?}"
        );
    }

    #[test]
    fn meta_line_open_turn_renders_dash_for_duration() {
        let session_start = Utc.with_ymd_and_hms(2026, 6, 4, 10, 0, 0).unwrap();
        let row = TurnSummaryRow::new(
            "t1".to_string(),
            session_start,
            None, // open turn — no duration yet
            TurnStatus::Open,
            None,
            None,
            None,
            0,
            0,
            0,
        );
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, session_start, false);
        let mut r = AnalysisReport::new(meta);
        r.turn_summary.push(row);
        let episodes = Episodes::new();
        let state = meta_line_state(&r, &episodes);
        let line = format_meta_line(&state, 120);
        assert!(
            line.contains(" · — · "),
            "open turn should render `—` for duration: {line:?}"
        );
    }

    #[test]
    fn meta_line_no_model_renders_dash_for_model() {
        let session_start = Utc.with_ymd_and_hms(2026, 6, 4, 10, 0, 0).unwrap();
        let row = TurnSummaryRow::new(
            "t1".to_string(),
            session_start,
            Some(Duration::seconds(1)),
            TurnStatus::Completed,
            None, // no model
            None, // no mode
            None,
            0,
            0,
            0,
        );
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, session_start, false);
        let mut r = AnalysisReport::new(meta);
        r.turn_summary.push(row);
        let episodes = Episodes::new();
        let state = meta_line_state(&r, &episodes);
        let line = format_meta_line(&state, 120);
        assert!(
            line.contains("model=—"),
            "missing model should render `model=—`: {line:?}"
        );
        assert!(
            line.contains("mode=—"),
            "missing mode should render `mode=—`: {line:?}"
        );
    }

    #[test]
    fn meta_line_fallback_for_out_of_range_selection() {
        let meta = SessionMeta::new(
            "s".into(),
            AgentKind::Copilot,
            Utc.with_ymd_and_hms(2026, 6, 4, 10, 0, 0).unwrap(),
            false,
        );
        let report = AnalysisReport::new(meta);
        let episodes = Episodes::new();
        let mut state = AppState::new(&report, &episodes);
        state.flame_selected = 99; // out of range (turn_summary empty)
        let line = format_meta_line(&state, 120);
        assert_eq!(line, "(no turn selected)");
    }

    // ──────────────────────────────────────────────────────────────────
    // fit_priority_segments helper tests
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn fit_priority_segments_drops_from_back_in_priority_order() {
        let segs = [
            "T1".to_string(),
            "+10s in".to_string(),
            "5.0s".to_string(),
            "3 calls".to_string(),
        ];
        // Wide: all preserved.
        assert_eq!(
            fit_priority_segments(&segs, " · ", 80),
            "T1 · +10s in · 5.0s · 3 calls"
        );
        // Tight: drop only the lowest-priority last segment.
        assert_eq!(
            fit_priority_segments(&segs, " · ", 25),
            "T1 · +10s in · 5.0s"
        );
        // Even tighter: drop two.
        assert_eq!(fit_priority_segments(&segs, " · ", 14), "T1 · +10s in");
        // Extreme: only top.
        assert_eq!(fit_priority_segments(&segs, " · ", 5), "T1");
    }

    #[test]
    fn fit_priority_segments_budget_zero_returns_empty() {
        let segs = ["T1".to_string()];
        assert_eq!(fit_priority_segments(&segs, " · ", 0), "");
    }

    #[test]
    fn fit_priority_segments_char_truncates_when_top_overflows() {
        // When even the highest-priority segment alone doesn't fit,
        // truncate from the right rather than rendering empty.
        let segs = ["T123456789".to_string()];
        assert_eq!(fit_priority_segments(&segs, " · ", 4), "T123");
    }

    // ──────────────────────────────────────────────────────────────────
    // F2.2 — Pending Yellow T-id color + is_turn_pending helper
    // ──────────────────────────────────────────────────────────────────

    /// Build an Episodes with a single tool episode containing one
    /// `OpenAtEndOfSession` call started at `started_at`, and a Turn
    /// referencing that call.
    fn episodes_with_open_call(
        tool_name: &str,
        started_at: chrono::DateTime<Utc>,
    ) -> (Episodes, Turn) {
        use agentprof_core::episode::tool::{ToolCallStatus, ToolEpisode};
        use agentprof_core::episode::turn::Span;
        use agentprof_core::episode::{CallRef, ToolCall};
        use agentprof_core::model::ToolSource;

        let mut ep = ToolEpisode::new(tool_name.into(), ToolSource::Builtin);
        let mut call = ToolCall::new(Span::new(started_at, started_at));
        call.status = ToolCallStatus::OpenAtEndOfSession;
        ep.calls.push(call);

        let mut episodes = Episodes::default();
        episodes.tools.insert(tool_name.into(), ep);

        let mut turn = Turn::new("t1".into(), started_at);
        turn.tool_calls.push(CallRef::new(tool_name.into(), 0));
        // turn.ended_at intentionally None → open turn (typical for
        // a turn with a pending ask_user mid-flight).
        (episodes, turn)
    }

    #[test]
    fn is_turn_pending_empty_turn_returns_false() {
        let turn = Turn::new(
            "t1".into(),
            Utc.with_ymd_and_hms(2026, 6, 5, 0, 0, 0).unwrap(),
        );
        assert!(!is_turn_pending(&turn, &Episodes::default(), Utc::now()));
    }

    #[test]
    fn is_turn_pending_ask_user_above_threshold_is_true() {
        let started = Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap();
        let (episodes, turn) = episodes_with_open_call("ask_user", started);
        // 60s elapsed > 30s threshold.
        let now = started + Duration::seconds(60);
        assert!(is_turn_pending(&turn, &episodes, now));
    }

    #[test]
    fn is_turn_pending_ask_user_below_threshold_is_false() {
        let started = Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap();
        let (episodes, turn) = episodes_with_open_call("ask_user", started);
        // 10s elapsed < 30s threshold.
        let now = started + Duration::seconds(10);
        assert!(!is_turn_pending(&turn, &episodes, now));
    }

    #[test]
    fn t_id_status_color_pending_turn_is_yellow() {
        let started = Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap();
        let (episodes, turn) = episodes_with_open_call("ask_user", started);
        let now = started + Duration::seconds(60);
        assert_eq!(
            t_id_status_color(Some(&turn), &episodes, now),
            Some(Color::Yellow),
            "pending turn must be Yellow"
        );
    }

    #[test]
    fn t_id_status_color_aborted_pending_red_wins_over_yellow() {
        // F2.2 precedence: Aborted Red > Pending Yellow.
        let started = Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap();
        let (episodes, mut turn) = episodes_with_open_call("ask_user", started);
        turn.status = TurnStatus::Aborted(agentprof_core::episode::AbortInfo::new(
            "test".into(),
            started,
        ));
        let now = started + Duration::seconds(60);
        assert_eq!(
            t_id_status_color(Some(&turn), &episodes, now),
            Some(Color::Red),
            "Aborted Red wins over Pending Yellow per F2.2 precedence"
        );
    }

    #[test]
    fn t_id_status_color_pending_wins_over_open_darkgray() {
        // F2.2 precedence: Pending Yellow > Open DarkGray.
        // An open turn with a pending ask_user must be Yellow, not
        // DarkGray (DarkGray would mean "running normally").
        let started = Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap();
        let (episodes, turn) = episodes_with_open_call("ask_user", started);
        // Turn's ended_at is None (open) AND the call is pending.
        assert!(turn.ended_at.is_none(), "test fixture must be open");
        let now = started + Duration::seconds(60);
        assert_eq!(
            t_id_status_color(Some(&turn), &episodes, now),
            Some(Color::Yellow),
            "Pending wins over Open per F2.2 precedence"
        );
    }

    #[test]
    fn t_id_status_color_open_non_pending_still_darkgray() {
        // Regression guard: open turn with NO pending call (either no
        // tool_calls, or tool_calls are within threshold) → DarkGray.
        let started = Utc.with_ymd_and_hms(2026, 6, 5, 10, 0, 0).unwrap();
        let (episodes, turn) = episodes_with_open_call("ask_user", started);
        // Just 5s elapsed → not pending.
        let now = started + Duration::seconds(5);
        assert_eq!(
            t_id_status_color(Some(&turn), &episodes, now),
            Some(Color::DarkGray),
            "open turn with non-pending call stays DarkGray"
        );
    }
}
