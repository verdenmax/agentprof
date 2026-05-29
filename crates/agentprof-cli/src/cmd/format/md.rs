//! Markdown renderer for [`AnalysisReport`].
//!
//! Produces a tabular, human-readable report with a fixed structure:
//! Session header → Turn Summary → Tool Rank → Hook Rank → Warnings.
//! Sections can be filtered via the `--section` CLI flag (Session and
//! Warnings are always included).

use std::borrow::Cow;
use std::fmt::Write as _;

use chrono::Duration;

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::{DeriveWarning, Mode, TurnStatus};

use crate::cmd::analyze::AnalysisSection;

/// Render `report` to markdown.
///
/// `sections` controls which mid-report tables are emitted; the Session
/// header and Warnings tail are always included.
///
/// # Examples
///
/// ```ignore
/// // agentprof-cli is a bin-only crate, so this doctest is not executed
/// // (no library target to import from). The shape below mirrors how
/// // `cmd::analyze::render_report` invokes this function.
/// use agentprof_cli::cmd::analyze::AnalysisSection;
/// use agentprof_cli::cmd::format::md::render;
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::model::SessionMeta;
/// use chrono::Utc;
///
/// let report = AnalysisReport::new(SessionMeta::new(
///     "s".into(), AgentKind::Copilot, Utc::now(), false,
/// ));
/// let md = render(&report, &AnalysisSection::all_vec());
/// assert!(md.starts_with("# agentprof analyze"));
/// ```
#[must_use]
pub fn render(report: &AnalysisReport, sections: &[AnalysisSection]) -> String {
    let mut out = String::with_capacity(8 * 1024);

    write_header(&mut out, report);

    if sections.contains(&AnalysisSection::TurnSummary) {
        write_turn_summary(&mut out, report);
    }
    if sections.contains(&AnalysisSection::ToolRank) {
        write_tool_rank(&mut out, report);
    }
    if sections.contains(&AnalysisSection::HookRank) {
        write_hook_rank(&mut out, report);
    }

    write_warnings(&mut out, report);

    out
}

fn write_header(out: &mut String, report: &AnalysisReport) {
    let _ = writeln!(out, "# agentprof analyze — {}", report.meta.id);
    let _ = writeln!(out);
    let _ = writeln!(out, "## Session");

    let version_suffix = report
        .meta
        .agent_version
        .as_ref()
        .map(|v| format!(" (v{v})"))
        .unwrap_or_default();
    let _ = writeln!(out, "- Agent: {:?}{version_suffix}", report.meta.agent);

    let _ = writeln!(
        out,
        "- Started: {}",
        report.meta.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(cwd) = &report.meta.cwd {
        let _ = writeln!(out, "- CWD: {}", md_cell_escape(cwd));
    }
    if let Some(branch) = &report.meta.branch {
        let _ = writeln!(out, "- Branch: {}", md_cell_escape(branch));
    }
    let _ = writeln!(
        out,
        "- Live: {}",
        if report.meta.is_live { "yes" } else { "no" }
    );
    let _ = writeln!(out, "- Turns: {}", report.turn_summary.len());
    let _ = writeln!(out, "- Tools tracked: {}", report.tool_rank.len());
    let _ = writeln!(out, "- Hooks tracked: {}", report.hook_rank.len());
    let _ = writeln!(out, "- Derive warnings: {}", report.warnings.len());
    let _ = writeln!(out);
}

fn write_turn_summary(out: &mut String, report: &AnalysisReport) {
    let _ = writeln!(out, "## Turn Summary");
    let _ = writeln!(
        out,
        "| # | Turn ID | Status | Duration | Model | Mode | Tools | Hooks | Skills | Out-Tokens |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|");
    for (i, row) in report.turn_summary.iter().enumerate() {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            i + 1,
            md_cell_escape(&row.turn_id),
            fmt_status(&row.status),
            row.duration.map_or_else(|| "—".into(), fmt_duration),
            row.model
                .as_deref()
                .map_or_else(|| Cow::Borrowed("—"), md_cell_escape),
            row.mode.as_ref().map_or_else(|| "—".into(), fmt_mode),
            row.tool_call_count,
            row.hook_call_count,
            row.skill_call_count,
            row.output_tokens
                .map_or_else(|| "—".into(), |n| n.to_string()),
        );
    }
    let _ = writeln!(out);
}

fn write_tool_rank(out: &mut String, report: &AnalysisReport) {
    let _ = writeln!(out, "## Tool Rank (by total duration)");
    let _ = writeln!(
        out,
        "| Tool | Source | Calls | OK | Fail | Orphan | User-req | Total | p50 | p95 | Max |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|");
    for row in &report.tool_rank {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            md_cell_escape(&row.name),
            md_cell_escape(&format!("{:?}", row.source)),
            row.call_count,
            row.success_count,
            row.failure_count,
            row.orphan_count,
            row.user_requested_count,
            fmt_duration(row.total_duration),
            fmt_duration(row.p50_duration),
            fmt_duration(row.p95_duration),
            fmt_duration(row.max_duration),
        );
    }
    let _ = writeln!(out);
}

fn write_hook_rank(out: &mut String, report: &AnalysisReport) {
    let _ = writeln!(out, "## Hook Rank (by total duration)");
    let _ = writeln!(
        out,
        "| Hook | Calls | OK | Fail | Synth | Total | p50 | p95 |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
    for row in &report.hook_rank {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            md_cell_escape(&row.name),
            row.call_count,
            row.success_count,
            row.failure_count,
            row.synthesized_start_count,
            fmt_duration(row.total_duration),
            fmt_duration(row.p50_duration),
            fmt_duration(row.p95_duration),
        );
    }
    let _ = writeln!(out);
}

fn write_warnings(out: &mut String, report: &AnalysisReport) {
    let _ = writeln!(out, "## Warnings");
    if report.warnings.is_empty() {
        let _ = writeln!(out, "(none)");
        return;
    }
    let _ = writeln!(out, "Derive-stage warnings: {}", report.warnings.len());
    let mut synthesized = 0_usize;
    let mut open_at_end = 0_usize;
    let mut abort = 0_usize;
    let mut non_monotonic = 0_usize;
    let mut payload_name_missing = 0_usize;
    for w in &report.warnings {
        match w {
            DeriveWarning::SynthesizedStart { .. } => synthesized += 1,
            DeriveWarning::OpenAtEndOfSession { .. } => open_at_end += 1,
            DeriveWarning::AbortWithoutOpenElement { .. } => abort += 1,
            DeriveWarning::NonMonotonicTimestamp { .. } => non_monotonic += 1,
            DeriveWarning::PayloadNameMissing { .. } => payload_name_missing += 1,
            // Future DeriveWarning variants: surface in 'other' bucket
            // rather than miscounting an existing category.
            _ => {}
        }
    }
    let _ = writeln!(out, "- SynthesizedStart: {synthesized}");
    let _ = writeln!(out, "- OpenAtEndOfSession: {open_at_end}");
    let _ = writeln!(out, "- AbortWithoutOpenElement: {abort}");
    let _ = writeln!(out, "- NonMonotonicTimestamp: {non_monotonic}");
    let _ = writeln!(out, "- PayloadNameMissing: {payload_name_missing}");
}

// ---------- formatting helpers ----------

/// Escape user-controlled string for safe insertion into a GFM table cell.
///
/// Replaces:
/// - `\|` for cell-delimiter pipes (would split the row otherwise)
/// - `<br>` for newlines (would terminate the cell + the table early)
/// - `\\` literal stays as `\\` (Rust escape; GFM doesn't process backslashes
///   outside `\|`)
///
/// Returns `Cow::Borrowed` when no escaping is needed (common case for
/// well-formed tool names like `bash` or `mcp__github__search_issues`).
///
/// Note: this function does NOT escape backticks, asterisks, underscores, or
/// other markdown formatting characters. Cell content is interpreted as
/// markdown by renderers, which is the intended behavior — a tool named
/// `*bold*` will be rendered as bold. Only the structural characters (`|`,
/// `\n`) need defensive escaping because they break the **table layout**,
/// not just the cell rendering.
fn md_cell_escape(s: &str) -> Cow<'_, str> {
    if !s.contains('|') && !s.contains('\n') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '|' => out.push_str("\\|"),
            '\n' => out.push_str("<br>"),
            other => out.push(other),
        }
    }
    Cow::Owned(out)
}

fn fmt_status(status: &TurnStatus) -> String {
    match status {
        TurnStatus::Open => "Open".into(),
        TurnStatus::Completed => "Completed".into(),
        TurnStatus::Aborted(info) => format!("Aborted({})", md_cell_escape(&info.reason)),
        // `TurnStatus` is `#[non_exhaustive]`; surface unknown variants
        // visibly so a future enum addition doesn't silently format to
        // an empty cell.
        _ => "?".into(),
    }
}

fn fmt_mode(mode: &Mode) -> String {
    match mode {
        Mode::Ask => "ask".into(),
        Mode::Auto => "auto".into(),
        Mode::Expert => "expert".into(),
        Mode::Unknown(s) => md_cell_escape(s).into_owned(),
        // `Mode` is `#[non_exhaustive]`; same rationale as fmt_status.
        _ => "?".into(),
    }
}

/// Format a [`Duration`] as a human-readable string with auto-picked units.
///
/// Boundaries:
/// - `< 1s` → `"<n>ms"` (integer)
/// - `< 1m` → `"<n.nn>s"`
/// - `< 1h` → `"<n.n>m"`
/// - `≥ 1h` → `"<n.nn>h"`
///
/// Negative durations (from non-monotonic timestamps) format as raw `ms`.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // ms → f64 is bounded to display-precision use
fn fmt_duration(d: Duration) -> String {
    let ms = d.num_milliseconds();
    if ms < 0 {
        return format!("{ms}ms");
    }
    if ms < 1_000 {
        return format!("{ms}ms");
    }
    if ms < 60_000 {
        let secs = ms as f64 / 1000.0;
        return format!("{secs:.2}s");
    }
    if ms < 3_600_000 {
        let mins = ms as f64 / 60_000.0;
        return format!("{mins:.1}m");
    }
    let hours = ms as f64 / 3_600_000.0;
    format!("{hours:.2}h")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::model::SessionMeta;
    use chrono::Utc;

    fn empty_report() -> AnalysisReport {
        let meta = SessionMeta::new("test-session".into(), AgentKind::Copilot, Utc::now(), false);
        AnalysisReport::new(meta)
    }

    #[test]
    fn empty_report_no_sections_emits_only_header_and_warnings() {
        let report = empty_report();
        let md = render(&report, &[]);
        assert!(md.contains("# agentprof analyze — test-session"));
        assert!(md.contains("## Session"));
        assert!(md.contains("## Warnings"));
        assert!(md.contains("(none)"));
        assert!(!md.contains("## Turn Summary"));
        assert!(!md.contains("## Tool Rank"));
        assert!(!md.contains("## Hook Rank"));
    }

    #[test]
    fn empty_report_all_sections_emits_all_tables() {
        let report = empty_report();
        let md = render(&report, &AnalysisSection::all_vec());
        assert!(md.contains("## Turn Summary"));
        assert!(md.contains("## Tool Rank"));
        assert!(md.contains("## Hook Rank"));
        // Tables still present even when empty (header row + separator).
        assert!(md.contains("| # | Turn ID |"));
    }

    #[test]
    fn fmt_duration_picks_units_at_boundaries() {
        assert_eq!(fmt_duration(Duration::milliseconds(500)), "500ms");
        assert_eq!(fmt_duration(Duration::milliseconds(999)), "999ms");
        assert_eq!(fmt_duration(Duration::milliseconds(1_000)), "1.00s");
        assert_eq!(fmt_duration(Duration::milliseconds(2_500)), "2.50s");
        assert_eq!(fmt_duration(Duration::seconds(59)), "59.00s");
        assert_eq!(fmt_duration(Duration::seconds(60)), "1.0m");
        assert_eq!(fmt_duration(Duration::seconds(120)), "2.0m");
        assert_eq!(fmt_duration(Duration::seconds(3_599)), "60.0m");
        assert_eq!(fmt_duration(Duration::seconds(3_600)), "1.00h");
        assert_eq!(fmt_duration(Duration::seconds(7_200)), "2.00h");
        assert_eq!(fmt_duration(Duration::milliseconds(-50)), "-50ms");
    }

    #[test]
    fn fmt_status_handles_each_variant() {
        use agentprof_core::episode::AbortInfo;
        assert_eq!(fmt_status(&TurnStatus::Open), "Open");
        assert_eq!(fmt_status(&TurnStatus::Completed), "Completed");
        let info = AbortInfo::new("user_cancel".into(), Utc::now());
        assert_eq!(
            fmt_status(&TurnStatus::Aborted(info)),
            "Aborted(user_cancel)"
        );
    }

    #[test]
    fn fmt_mode_handles_each_variant() {
        assert_eq!(fmt_mode(&Mode::Ask), "ask");
        assert_eq!(fmt_mode(&Mode::Auto), "auto");
        assert_eq!(fmt_mode(&Mode::Expert), "expert");
        assert_eq!(fmt_mode(&Mode::Unknown("yolo".into())), "yolo");
    }

    #[test]
    fn md_cell_escape_passes_through_safe_strings() {
        let safe = "bash";
        let escaped = md_cell_escape(safe);
        assert_eq!(escaped, "bash");
        // The Cow should remain Borrowed (no allocation) for safe strings.
        assert!(matches!(escaped, Cow::Borrowed(_)));
    }

    #[test]
    fn md_cell_escape_escapes_pipes() {
        let dangerous = "a|b|c";
        assert_eq!(md_cell_escape(dangerous), "a\\|b\\|c");
    }

    #[test]
    fn md_cell_escape_replaces_newlines_with_br() {
        let multiline = "line1\nline2";
        assert_eq!(md_cell_escape(multiline), "line1<br>line2");
    }

    #[test]
    fn md_cell_escape_handles_mixed_dangerous_chars() {
        let nasty = "tool|name\nwith pipes";
        assert_eq!(md_cell_escape(nasty), "tool\\|name<br>with pipes");
    }

    #[test]
    fn fmt_status_escapes_abort_reason_with_pipe() {
        use agentprof_core::episode::AbortInfo;
        let info = AbortInfo::new("user|cancel".into(), Utc::now());
        // Pipe in reason must NOT split the markdown cell.
        assert_eq!(
            fmt_status(&TurnStatus::Aborted(info)),
            "Aborted(user\\|cancel)"
        );
    }

    #[test]
    fn fmt_mode_escapes_unknown_with_pipe() {
        assert_eq!(
            fmt_mode(&Mode::Unknown("pipe|in|mode".into())),
            "pipe\\|in\\|mode"
        );
    }
}
