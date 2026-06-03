//! HTML report renderer for [`AnalysisReport`].
//!
//! Self-contained (no JS, no external assets); the flamegraph is embedded
//! as a build-time-rendered SVG (see
//! [`agentprof_core::export::svg_flamegraph`]). Tables mirror the markdown
//! report; CSS is inlined via `include_str!` for portability.

use askama::Template;
use chrono::Utc;

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::Episodes;
use agentprof_core::export::svg_flamegraph::SvgFlamegraph;
use agentprof_core::model::SessionMeta;

use crate::cmd::analyze::AnalysisSection;

const EMBEDDED_CSS: &str = include_str!("../../../templates/styles.css");

/// Render `report` + `episodes` to a single self-contained HTML document.
///
/// `sections` controls which mid-report tables (and the flamegraph) are
/// emitted; the session header and warnings tail are always included.
///
/// # Examples
///
/// ```ignore
/// // agentprof-cli is bin-only; this doctest is illustrative only.
/// let html = format::html::render(&report, &episodes, &meta, &sections, "0.1.0");
/// assert!(html.contains("<html"));
/// ```
#[must_use]
pub fn render(
    report: &AnalysisReport,
    episodes: &Episodes,
    meta: &SessionMeta,
    sections: &[AnalysisSection],
    agentprof_version: &str,
) -> String {
    let svg = SvgFlamegraph::from_episodes(episodes, meta).into_svg_string();

    let turn_rows: Vec<TurnRow> = report
        .turn_summary
        .iter()
        .map(|r| TurnRow {
            turn_id: r.turn_id.clone(),
            started_at: r.started_at.to_rfc3339(),
            duration: r.duration.map_or_else(
                || "-".to_string(),
                |d| format!("{} ms", d.num_milliseconds()),
            ),
            status: format!("{:?}", r.status),
            model: r.model.clone().unwrap_or_else(|| "-".to_string()),
            mode: r
                .mode
                .as_ref()
                .map_or_else(|| "-".to_string(), |m| format!("{m:?}")),
            output_tokens: r
                .output_tokens
                .map_or_else(|| "-".to_string(), |t| t.to_string()),
        })
        .collect();

    let tool_rows: Vec<ToolRow> = report
        .tool_rank
        .iter()
        .map(|r| ToolRow {
            name: r.name.clone(),
            source_label: r.source.to_string(),
            call_count: r.call_count.to_string(),
            fail_count: r.failure_count.to_string(),
            total_duration: format!("{} ms", r.total_duration.num_milliseconds()),
            p50: format!("{} ms", r.p50_duration.num_milliseconds()),
            p95: format!("{} ms", r.p95_duration.num_milliseconds()),
        })
        .collect();

    let hook_rows: Vec<HookRow> = report
        .hook_rank
        .iter()
        .map(|r| HookRow {
            name: r.name.clone(),
            call_count: r.call_count.to_string(),
            fail_count: r.failure_count.to_string(),
            total_duration: format!("{} ms", r.total_duration.num_milliseconds()),
        })
        .collect();

    let warnings: Vec<String> = report
        .parse_warnings
        .iter()
        .map(ToString::to_string)
        .chain(report.warnings.iter().map(ToString::to_string))
        .collect();

    let total_output_tokens: Option<u32> = report
        .turn_summary
        .iter()
        .filter_map(|r| r.output_tokens)
        .reduce(u32::saturating_add);

    let session_short_id: String = meta.id.chars().take(8).collect();
    let duration_human = format_duration_short(report);

    let template = ReportTemplate {
        session_short_id,
        agent: meta.agent.to_string(),
        model: report.turn_summary.first().and_then(|r| r.model.clone()),
        started_at_utc: meta.started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        duration_human,
        turn_count: report.turn_summary.len(),
        tool_count: report.tool_rank.len(),
        hook_count: report.hook_rank.len(),
        total_output_tokens,
        show_flamegraph: sections.contains(&AnalysisSection::TurnSummary)
            || sections.contains(&AnalysisSection::ToolRank)
            || sections.contains(&AnalysisSection::HookRank),
        show_turns: sections.contains(&AnalysisSection::TurnSummary),
        show_tools: sections.contains(&AnalysisSection::ToolRank),
        show_hooks: sections.contains(&AnalysisSection::HookRank),
        svg_flamegraph: svg,
        turn_rows,
        tool_rows,
        hook_rows,
        warnings,
        embedded_css: EMBEDDED_CSS.to_string(),
        exporter: format!("agentprof v{agentprof_version}"),
        generated_at: Utc::now().to_rfc3339(),
    };

    template.render().unwrap_or_else(|e| {
        format!(
            "<html><body><h1>HTML render error</h1><pre>{}</pre></body></html>",
            html_escape(&e.to_string())
        )
    })
}

/// Minimal HTML escape for the 5 metacharacters relevant inside element
/// text and double-quoted attributes. Defensive: current callers carry no
/// user-controlled content, but escaping here prevents future error
/// variants from becoming an XSS vector.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[derive(Template)]
#[template(path = "report.html")]
#[allow(clippy::struct_excessive_bools)] // template-control flags map 1:1 to template `{% if %}` arms
struct ReportTemplate {
    session_short_id: String,
    agent: String,
    model: Option<String>,
    started_at_utc: String,
    duration_human: String,
    turn_count: usize,
    tool_count: usize,
    hook_count: usize,
    total_output_tokens: Option<u32>,
    show_flamegraph: bool,
    show_turns: bool,
    show_tools: bool,
    show_hooks: bool,
    svg_flamegraph: String,
    turn_rows: Vec<TurnRow>,
    tool_rows: Vec<ToolRow>,
    hook_rows: Vec<HookRow>,
    warnings: Vec<String>,
    embedded_css: String,
    exporter: String,
    generated_at: String,
}

struct TurnRow {
    turn_id: String,
    started_at: String,
    duration: String,
    status: String,
    model: String,
    mode: String,
    output_tokens: String,
}

struct ToolRow {
    name: String,
    source_label: String,
    call_count: String,
    fail_count: String,
    total_duration: String,
    p50: String,
    p95: String,
}

struct HookRow {
    name: String,
    call_count: String,
    fail_count: String,
    total_duration: String,
}

fn format_duration_short(report: &AnalysisReport) -> String {
    let total: chrono::Duration = report
        .turn_summary
        .iter()
        .filter_map(|r| r.duration)
        .fold(chrono::Duration::zero(), |a, b| a + b);
    let ms = total.num_milliseconds();
    if ms == 0 {
        "0 ms".to_string()
    } else if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        #[allow(clippy::cast_precision_loss)]
        let s = ms as f64 / 1000.0;
        format!("{s:.1} s")
    } else {
        #[allow(clippy::cast_precision_loss)]
        let m = ms as f64 / 60_000.0;
        format!("{m:.1} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_handles_all_five_metacharacters() {
        let input = r#"<script src="x" attr='y'>&"#;
        let escaped = html_escape(input);
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(!escaped.contains('"'));
        assert!(!escaped.contains('\''));
        assert_eq!(
            escaped,
            "&lt;script src=&quot;x&quot; attr=&#39;y&#39;&gt;&amp;"
        );
    }

    #[test]
    fn html_escape_preserves_safe_text() {
        assert_eq!(html_escape("plain text 123"), "plain text 123");
    }

    #[test]
    fn html_escape_escapes_ampersand_first() {
        // Ensure `&lt;` doesn't get double-escaped to `&amp;lt;`.
        assert_eq!(html_escape("&<"), "&amp;&lt;");
    }
}
