//! `FlamegraphView` — per-turn horizontal gantt of tool calls.
//!
//! Each row = one Turn (chronological order). The longest turn's duration
//! fills the inner content area; other rows scale proportionally. Within a
//! row, each [`agentprof_core::episode::ToolCall`] is drawn as a segment at
//! `(call.span.start - turn.start) / turn.duration` of row width, colored by
//! [`agentprof_core::model::ToolSource`].
//!
//! ## Rendered output
//!
//! Each turn row mixes three character types in its gantt area:
//!
//! - `█` (U+2588 FULL BLOCK) — a tool / hook / skill is executing during this time slice.
//! - `░` (U+2591 LIGHT SHADE) — the turn is in-flight but no tool is running (LLM thinking time:
//!   reasoning, generating output tokens, interpreting tool output). This time IS part of the
//!   turn's wall-time and is real cost.
//! - `·` (U+00B7 MIDDLE DOT) — padding: the turn ended before this position. The character only
//!   exists because this row is shorter than the longest non-blocking turn (which sets the
//!   horizontal scale). This time is NOT part of the turn.
//!
//! User-blocking turns (e.g. containing an `ask_user` call where the user spent minutes on
//! the keyboard or AFK) are excluded from the "longest turn" calculation per
//! [`Turn::is_user_blocking`]; otherwise such a turn's wall-time would dwarf others and
//! squash every normal turn's gantt-bar to ≤ 1 cell. See ADR-0005 §6 for the same split
//! applied to the Tool Rank table.
//!
//! See `docs/superpowers/specs/2026-05-30-m1.5-tui-design.md` §6.1.

use agentprof_core::episode::{Episodes, Turn, TurnStatus};
use chrono::Duration;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span as TextSpan};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::state::AppState;
use crate::views::format::human_short;

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
/// Layout: vertical list of rows; each row has an 18-cell prefix
/// (5-char turn label like `T1234` + 1 space + 10-char duration like
/// `12.4h` + 2 trailing spaces) followed by the gantt strip. The selected
/// turn (`state.flame_selected`) is highlighted with reverse-video.
///
/// **Viewport (edge-triggered)**: `state.flame_viewport_top` is persisted
/// across frames via [`std::cell::Cell`]. Render only adjusts it when
/// `flame_selected` leaves the visible window — pressing `↓` past the
/// bottom shifts viewport down by 1; pressing `↑` past the top shifts it
/// up by 1; movement within the visible window does not scroll. This is
/// the canonical scroll-to-follow pattern.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Flamegraph (1/3) ");
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    let prefix_width: u16 = 18;
    let gantt_width = inner.width.saturating_sub(prefix_width);

    let turns = &state.report.turn_summary;

    // Polish #1: build turn-by-id lookup once per render (was O(N×M) per-
    // row iter().find(); now O(N) build + O(M) lookups).
    let turn_by_id: std::collections::HashMap<&str, &agentprof_core::episode::Turn> = state
        .episodes
        .turns
        .iter()
        .map(|t| (t.id.as_str(), t))
        .collect();

    // Compute max_dur EXCLUDING user-blocking turns (whose wall-time is
    // dominated by human-thinking-time, not agent work — e.g. an `ask_user`
    // turn where the user spent 10 minutes deciding). Without this filter,
    // a single such outlier squashes every other turn's gantt-bar width to
    // near-zero, making the visualization effectively useless.
    //
    // Fallback: if EVERY turn is user-blocking (rare degenerate case),
    // fall back to the original "max across all turns" so we don't divide
    // by 1 ms.
    //
    // Mirrors ADR-0005 §6 + the user-blocking split in the Tool Rank table
    // (see [`agentprof_core::analyzer::tool_rank`]).
    let max_dur_excl_blocking = turns
        .iter()
        .filter_map(|row| {
            let turn = turn_by_id.get(row.turn_id.as_str())?;
            if turn.is_user_blocking() {
                None
            } else {
                row.duration.map(|d| d.num_milliseconds())
            }
        })
        .max();

    let max_dur = max_dur_excl_blocking
        .or_else(|| {
            turns
                .iter()
                .filter_map(|t| t.duration.map(|d| d.num_milliseconds()))
                .max()
        })
        .unwrap_or(1)
        .max(1);

    // Edge-triggered viewport: only scroll when selected leaves the window.
    // Persisted via Cell on AppState so the cursor can move freely within
    // the viewport (instead of being glued to the bottom edge every frame).
    let visible_rows = (inner.height as usize).max(1);
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
        );
        lines.push(line);
    }

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);

    // Detail strip.
    let detail_block = Block::default().borders(Borders::ALL).title(" Detail ");
    let detail_inner = detail_block.inner(chunks[1]);
    frame.render_widget(detail_block, chunks[1]);
    let detail_text = turns.get(state.flame_selected).map_or_else(
        || "(no turn selected)".to_string(),
        |row| {
            format!(
                "Turn {} | model={} mode={} out_tokens={} tools={}",
                row.turn_id,
                row.model.as_deref().unwrap_or("-"),
                row.mode
                    .as_ref()
                    .map_or_else(|| "-".to_string(), |m| format!("{m:?}")),
                row.output_tokens
                    .map_or_else(|| "-".to_string(), |n| n.to_string()),
                row.tool_call_count,
            )
        },
    );
    frame.render_widget(Paragraph::new(detail_text), detail_inner);
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
) -> Line<'a> {
    let dur_str = duration.map_or_else(|| "—".to_string(), human_short);
    let prefix = format!("{:>5} {:>10}  ", format!("T{}", idx + 1), dur_str);
    let mut spans: Vec<TextSpan<'_>> = Vec::new();
    let prefix_style = if selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    spans.push(TextSpan::styled(prefix, prefix_style));

    let gantt_w = gantt_width;
    let mut cells: Vec<TextSpan<'_>> = vec![TextSpan::raw(" ".repeat(gantt_w as usize))];
    if let Some(t) = turn {
        if let Some(ended) = t.ended_at {
            let started = t.started_at;
            // Scale this turn's gantt width relative to the longest turn.
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
            // Build a String of gantt_w cells; overlay segments.
            let buf = build_gantt_cells(scaled, gantt_w, &segs);
            cells = build_styled_cells(&buf);
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

fn build_styled_cells<'a>(buf: &[char]) -> Vec<TextSpan<'a>> {
    // For M1.5 we do not color-per-segment in the buffer (would require
    // per-cell styling and a bigger refactor). Display whole gantt row in one
    // span, neutral cyan — color-by-source is reserved for a polish pass
    // in M1.6. (Documented as Open Question §14 in the spec.)
    vec![TextSpan::styled(
        buf.iter().collect::<String>(),
        Style::default().fg(Color::Cyan),
    )]
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

    /// Replicates the `max_dur` calculation from `render()` so we can
    /// unit-test it without instantiating a full ratatui Frame + `AppState`.
    fn compute_max_dur(turns: &[Turn]) -> i64 {
        let turn_by_id: std::collections::HashMap<&str, &Turn> =
            turns.iter().map(|t| (t.id.as_str(), t)).collect();

        let max_excl = turns
            .iter()
            .filter_map(|t| {
                let turn = turn_by_id.get(t.id.as_str())?;
                if turn.is_user_blocking() {
                    None
                } else {
                    t.ended_at.map(|e| (e - t.started_at).num_milliseconds())
                }
            })
            .max();

        max_excl
            .or_else(|| {
                turns
                    .iter()
                    .filter_map(|t| t.ended_at.map(|e| (e - t.started_at).num_milliseconds()))
                    .max()
            })
            .unwrap_or(1)
            .max(1)
    }

    #[test]
    fn max_dur_excludes_user_blocking_turns() {
        // 3 turns:
        // - T1: 5s normal turn (`bash`)
        // - T2: 600s (10min) user-blocking turn (`ask_user`)
        // - T3: 5s normal turn (`edit`)
        //
        // Expected: max_dur for scaling = 5000 ms (the non-blocking max),
        //           NOT 600000 ms (which would squash T1+T3 to ~0 cells).
        let turns = vec![
            turn_with_tool("t1", 0, 5, "bash"),
            turn_with_tool("t2", 10, 610, "ask_user"),
            turn_with_tool("t3", 700, 705, "edit"),
        ];
        assert_eq!(compute_max_dur(&turns), 5_000);
    }

    #[test]
    fn max_dur_falls_back_when_all_turns_are_user_blocking() {
        // Degenerate case: only user-blocking turns exist. Filter would
        // produce None — must fall back to original max-across-all so we
        // don't divide by 1 ms.
        let turns = vec![turn_with_tool("t1", 0, 300, "ask_user")];
        assert_eq!(
            compute_max_dur(&turns),
            300_000,
            "fallback must use all-turns max when filter empties"
        );
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
}
