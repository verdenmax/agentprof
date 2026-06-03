//! Session-level per-model token-usage view (key `4`, F1.7).
//!
//! Sources from [`agentprof_core::analyzer::AnalysisReport::model_metrics`].
//! When the session has emitted a `session.shutdown` event, shows a
//! sortable table (sorted by input desc); otherwise shows a centered
//! "no model usage data yet" placeholder explaining why.
//!
//! See `docs/superpowers/specs/2026-06-03-f1.7-models-view-design.md`
//! and ADR-0012 D-9 / D-12 for the design rationale.

use std::collections::BTreeMap;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span as TextSpan};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use agentprof_core::analyzer::ModelUsage;

/// Render the Models view body.
///
/// Two branches:
/// - `app_state.report.model_metrics` is `Some(non_empty)` → table sorted
///   by input desc + totals footer row.
/// - Otherwise → centered placeholder + multi-line "no shutdown event
///   yet" explanation.
///
/// # Examples
///
/// ```no_run
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use agentprof_tui::app::state::AppState;
/// use agentprof_tui::views::models::render;
/// use chrono::Utc;
/// use ratatui::backend::TestBackend;
/// use ratatui::Terminal;
///
/// let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
/// let report = AnalysisReport::new(meta);
/// let episodes = Episodes::default();
/// let state = AppState::new(&report, &episodes);
///
/// let backend = TestBackend::new(100, 20);
/// let mut terminal = Terminal::new(backend).expect("backend");
/// terminal
///     .draw(|f| render(f, f.area(), &state))
///     .expect("draw");
/// ```
pub fn render(f: &mut Frame<'_>, area: Rect, app_state: &crate::app::state::AppState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Models — session token usage ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split inner: body + 1-line footer (footer doesn't scroll).
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let body_area = chunks[0];
    let footer_area = chunks[1];

    match app_state.report.model_metrics.as_ref() {
        Some(metrics) if !metrics.is_empty() => {
            render_with_data(
                f,
                body_area,
                footer_area,
                metrics,
                app_state.models_selected,
            );
        }
        _ => {
            render_empty_state(f, body_area, footer_area);
        }
    }
}

/// Render the with-data branch — table + totals footer.
fn render_with_data(
    f: &mut Frame<'_>,
    body_area: Rect,
    footer_area: Rect,
    metrics: &BTreeMap<String, ModelUsage>,
    selected: usize,
) {
    // Sort by input_tokens desc (D-11).
    let mut rows: Vec<(&String, &ModelUsage)> = metrics.iter().collect();
    rows.sort_by(|a, b| b.1.input_tokens.cmp(&a.1.input_tokens));

    // Defensive: clamp `selected` to actual row count. WatchRunner (T10)
    // may reload a report with fewer models than the prior render, leaving
    // state.models_selected potentially out of bounds. Without this clamp
    // no row would highlight (silent UX glitch).
    let selected = selected.min(rows.len().saturating_sub(1));

    // Compute totals (saturating to avoid panic on extreme inputs).
    let totals = rows.iter().fold(ModelUsage::new(), |mut acc, (_, u)| {
        acc.input_tokens = acc.input_tokens.saturating_add(u.input_tokens);
        acc.output_tokens = acc.output_tokens.saturating_add(u.output_tokens);
        acc.cache_read_tokens = acc.cache_read_tokens.saturating_add(u.cache_read_tokens);
        acc.cache_write_tokens = acc.cache_write_tokens.saturating_add(u.cache_write_tokens);
        acc
    });

    // Header row + data rows + totals. Use `Table::header()` so the header
    // is pinned outside the scrollable body (future-proof for T10 watch mode
    // + small terminals where the body may scroll).
    let header = Row::new(vec!["Model", "Input", "Output", "Cache R", "Cache W"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let totals_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let data_rows: Vec<Row<'_>> = rows
        .iter()
        .enumerate()
        .map(|(idx, (model, u))| {
            let is_selected = idx == selected;
            let style = if is_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                (*model).clone(),
                format_token_u64_short(u.input_tokens),
                format_token_u64_short(u.output_tokens),
                format_token_u64_short(u.cache_read_tokens),
                format_token_u64_short(u.cache_write_tokens),
            ])
            .style(style)
        })
        .chain(std::iter::once(
            Row::new(vec![
                "Total".to_string(),
                format_token_u64_short(totals.input_tokens),
                format_token_u64_short(totals.output_tokens),
                format_token_u64_short(totals.cache_read_tokens),
                format_token_u64_short(totals.cache_write_tokens),
            ])
            .style(totals_style),
        ))
        .collect();

    let widths = [
        Constraint::Min(40),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
    ];
    let table = Table::new(data_rows, widths)
        .header(header)
        .column_spacing(1);
    f.render_widget(table, body_area);

    // Footer hint.
    let model_count = metrics.len();
    let plural = if model_count == 1 { "" } else { "s" };
    let footer_text = format!(
        "session has {model_count} model{plural} · sorted by input desc · \
         j/k navigate · Esc / 1 switches to view 1"
    );
    let footer = Paragraph::new(footer_text).style(Style::default().add_modifier(Modifier::DIM));
    f.render_widget(footer, footer_area);
}

/// Render the empty-state branch — centered placeholder.
fn render_empty_state(f: &mut Frame<'_>, body_area: Rect, footer_area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(TextSpan::styled(
            "(no model usage data — session has not emitted shutdown event yet)",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("Token totals are reported on `session.shutdown`. If you're"),
        Line::from("in live watch mode, wait for the agent to exit (or hit Ctrl-D)."),
    ];
    let p = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
    f.render_widget(p, body_area);

    let footer = Paragraph::new("Esc / 1 switches to view 1")
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
    f.render_widget(footer, footer_area);
}

/// Format a `u64` token count as a 5-char wide right-padded string,
/// suitable for table cells.
///
/// Uses k/M/G abbreviations for large counts. Sibling helper to
/// [`crate::views::format::format_tokens_short`] (which takes
/// `Option<u32>`); this one handles `u64` for session-level totals
/// which can exceed `u32::MAX`.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::models::format_token_u64_short;
/// assert_eq!(format_token_u64_short(42), "   42");
/// assert_eq!(format_token_u64_short(1234), " 1.2k");
/// assert_eq!(format_token_u64_short(123_456), " 123k");
/// assert_eq!(format_token_u64_short(1_500_000), " 1.5M");
/// assert_eq!(format_token_u64_short(15_000_000), "  15M");
/// assert_eq!(format_token_u64_short(2_500_000_000), " 2.5G");
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_token_u64_short(n: u64) -> String {
    if n < 1_000 {
        format!("{n:>5}")
    } else if n < 100_000 {
        // e.g. 1_234 → " 1.2k"; 99_999 → "99.9k".
        let tenths = n / 100;
        let display = tenths as f64 / 10.0;
        format!("{display:>4.1}k")
    } else if n < 1_000_000 {
        format!("{:>4}k", n / 1_000)
    } else if n < 10_000_000 {
        let tenths_m = n / 100_000;
        let display = tenths_m as f64 / 10.0;
        format!("{display:>4.1}M")
    } else if n < 1_000_000_000 {
        format!("{:>4}M", n / 1_000_000)
    } else if n < 10_000_000_000 {
        let tenths_g = n / 100_000_000;
        let display = tenths_g as f64 / 10.0;
        format!("{display:>4.1}G")
    } else if n < 1_000_000_000_000 {
        format!("{:>4}G", n / 1_000_000_000)
    } else if n < 10_000_000_000_000 {
        let tenths_t = n / 100_000_000_000;
        let display = tenths_t as f64 / 10.0;
        format!("{display:>4.1}T")
    } else if n < 1_000_000_000_000_000 {
        format!("{:>4}T", n / 1_000_000_000_000)
    } else if n < 10_000_000_000_000_000 {
        let tenths_p = n / 100_000_000_000_000;
        let display = tenths_p as f64 / 10.0;
        format!("{display:>4.1}P")
    } else {
        // u64::MAX ≈ 18.4 EB → /1e15 = 18_446; clamp to 4 digits so the
        // 5-char cell width holds even at the upper bound.
        let p = (n / 1_000_000_000_000_000).min(9_999);
        format!("{p:>4}P")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::{AnalysisReport, ModelUsage};
    use agentprof_core::episode::Episodes;
    use agentprof_core::model::SessionMeta;
    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn fixture_meta() -> SessionMeta {
        SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false)
    }

    fn fixture_with_metrics() -> AnalysisReport {
        let mut report = AnalysisReport::new(fixture_meta());
        let mut m = BTreeMap::new();
        let mut claude = ModelUsage::new();
        claude.input_tokens = 98_327;
        claude.output_tokens = 47_523;
        claude.cache_read_tokens = 3_444_639;
        claude.cache_write_tokens = 721_860;
        m.insert("claude-opus-4.7-1m-internal".into(), claude);
        let mut gpt = ModelUsage::new();
        gpt.input_tokens = 12_500;
        gpt.output_tokens = 3_400;
        gpt.cache_read_tokens = 8_200;
        gpt.cache_write_tokens = 0;
        m.insert("gpt-5-mini".into(), gpt);
        report.model_metrics = Some(m);
        report
    }

    fn buffer_to_symbol_grid(buffer: &ratatui::buffer::Buffer) -> String {
        let cells_per_row = buffer.area.width as usize;
        let mut text = String::with_capacity(buffer.content.len() + buffer.area.height as usize);
        for (i, cell) in buffer.content.iter().enumerate() {
            if i > 0 && i % cells_per_row == 0 {
                text.push('\n');
            }
            text.push_str(cell.symbol());
        }
        text
    }

    #[test]
    fn render_with_data_shows_model_names() {
        let report = fixture_with_metrics();
        let episodes = Episodes::default();
        let state = AppState::new(&report, &episodes);

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, f.area(), &state)).unwrap();
        let grid = buffer_to_symbol_grid(terminal.backend().buffer());

        assert!(grid.contains("claude-opus"), "missing model name: {grid}");
        assert!(grid.contains("gpt-5-mini"), "missing gpt model: {grid}");
        assert!(grid.contains("Total"), "missing total row: {grid}");
    }

    #[test]
    fn render_empty_state_shows_explanation() {
        let report = AnalysisReport::new(fixture_meta()); // model_metrics = None
        let episodes = Episodes::default();
        let state = AppState::new(&report, &episodes);

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, f.area(), &state)).unwrap();
        let grid = buffer_to_symbol_grid(terminal.backend().buffer());

        assert!(
            grid.contains("no model usage data"),
            "missing empty-state placeholder: {grid}"
        );
        assert!(
            grid.contains("shutdown"),
            "missing 'shutdown' explanation: {grid}"
        );
    }

    #[test]
    fn render_sorted_by_input_desc() {
        let report = fixture_with_metrics();
        let episodes = Episodes::default();
        let state = AppState::new(&report, &episodes);

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, f.area(), &state)).unwrap();
        let grid = buffer_to_symbol_grid(terminal.backend().buffer());

        let claude_pos = grid.find("claude-opus").expect("claude in grid");
        let gpt_pos = grid.find("gpt-5-mini").expect("gpt in grid");
        assert!(
            claude_pos < gpt_pos,
            "claude (higher input) must appear before gpt-5-mini"
        );
    }

    #[test]
    fn render_empty_btreemap_uses_empty_state() {
        let mut report = AnalysisReport::new(fixture_meta());
        report.model_metrics = Some(BTreeMap::new()); // explicitly empty
        let episodes = Episodes::default();
        let state = AppState::new(&report, &episodes);

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, f.area(), &state)).unwrap();
        let grid = buffer_to_symbol_grid(terminal.backend().buffer());

        // Empty BTreeMap should trigger empty-state branch (not table).
        assert!(grid.contains("no model usage data"));
    }

    #[test]
    fn format_token_u64_short_handles_large_values() {
        assert_eq!(format_token_u64_short(0), "    0");
        assert_eq!(format_token_u64_short(999), "  999");
        assert_eq!(format_token_u64_short(1_234), " 1.2k");
        assert_eq!(format_token_u64_short(100_000), " 100k");
        assert_eq!(format_token_u64_short(999_999), " 999k");
        assert_eq!(format_token_u64_short(1_500_000), " 1.5M");
        assert_eq!(format_token_u64_short(123_456_789), " 123M");
        assert_eq!(format_token_u64_short(1_234_567_890), " 1.2G");
        // T (terabyte/trillion) + P (petabyte/quadrillion) branches.
        assert_eq!(format_token_u64_short(2_500_000_000_000), " 2.5T");
        assert_eq!(format_token_u64_short(123_000_000_000_000), " 123T");
        assert_eq!(format_token_u64_short(2_500_000_000_000_000), " 2.5P");
        // u64::MAX (~18.4 EB) clamps at "9999P" per implementation —
        // beyond ~10^16 tokens we lose the actual magnitude. Acceptable
        // since no real session will exceed this.
        let s = format_token_u64_short(u64::MAX);
        assert!(
            s.chars().count() <= 5,
            "5-char cap held even at u64::MAX: {s}"
        );
        assert_eq!(s.trim(), "9999P", "u64::MAX clamps at 9999P: {s}");
    }
}
