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
//! `T3 selected:  bash(120ms) read_file(85ms) +2 more`. Truncates from the right with
//! `+K more` when the line exceeds the footer width.
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
#[allow(clippy::too_many_lines)]
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

    // Reserve 1 line at the bottom of `inner` for the selected-turn footer
    // (e.g. "T3 selected:  bash(120ms) read_file(85ms) +2 more"). The
    // footer lives INSIDE the bordered Flamegraph block so the existing
    // Detail strip (`chunks[1]`) is unchanged.
    let (rows_area, footer_area) = if inner.height >= 2 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);
        (split[0], Some(split[1]))
    } else {
        (inner, None)
    };

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
/// Format: `"T{n} selected:  bash(120ms) read_file(85ms) +K more"` where
/// `n` is the 1-based turn index, durations come from
/// [`agentprof_core::episode::Span::duration`], and `+K more` appears if
/// truncation from the right was required to fit `max_width` columns. When
/// the selected turn has no tool calls, the line reads
/// `"T{n} selected:  (no tool calls)"`. When `turn` is `None` (e.g. an
/// out-of-range selection index), the line reads `"(no turn selected)"`.
///
/// Hooks and Skill invocations are intentionally omitted — only entries in
/// [`Turn::tool_calls`] are listed.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::Episodes;
/// use agentprof_tui::views::flamegraph::selected_turn_footer_line;
/// // No turn → fixed placeholder.
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
    if t.tool_calls.is_empty() {
        return format!("{prefix}(no tool calls)");
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
        return format!("{prefix}(no tool calls)");
    }

    let budget = usize::from(max_width);
    fit_entries(&prefix, &entries, budget)
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
    }

    #[test]
    fn selected_turn_footer_line_empty_tool_calls() {
        let base = Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap();
        let mut t = Turn::new("t1".into(), base);
        t.ended_at = Some(base + Duration::seconds(1));
        let line = selected_turn_footer_line(0, Some(&t), &Episodes::default(), 80);
        assert_eq!(line, "T1 selected:  (no tool calls)");
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

        // Narrow budget: "T1 selected:  bash(80ms) bash(80ms) +3 more" = 44.
        let line = selected_turn_footer_line(0, Some(&t), &episodes, 44);
        assert!(line.starts_with("T1 selected:  "), "got: {line:?}");
        assert!(line.ends_with(" +3 more"), "got: {line:?}");
        assert!(line.len() <= 44, "got: {line:?} (len {})", line.len());
    }
}
