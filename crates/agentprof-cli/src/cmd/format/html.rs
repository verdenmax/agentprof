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
use agentprof_core::model::{SessionMeta, WasteDataSource, WasteReport};

use crate::cmd::analyze::AnalysisSection;

const EMBEDDED_CSS: &str = include_str!("../../../templates/styles.css");

/// Render `report` + `episodes` to a single self-contained HTML document.
///
/// `sections` controls which mid-report tables (and the flamegraph) are
/// emitted; the session header and warnings tail are always included.
/// `mcp_waste` is rendered when [`AnalysisSection::McpWaste`] is in
/// `sections` AND `Some(_)` is provided (caller passes `None` when the
/// section was not requested).
///
/// # Examples
///
/// ```ignore
/// // agentprof-cli is bin-only; this doctest is illustrative only.
/// let html = format::html::render(&report, &episodes, &meta, &sections, None, "0.1.0");
/// assert!(html.contains("<html"));
/// ```
#[must_use]
pub fn render(
    report: &AnalysisReport,
    episodes: &Episodes,
    meta: &SessionMeta,
    sections: &[AnalysisSection],
    mcp_waste: Option<&WasteReport>,
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

    let mcp_waste_html = render_mcp_waste_section(sections, mcp_waste);
    let show_mcp_waste = !mcp_waste_html.is_empty();

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
        show_mcp_waste,
        svg_flamegraph: svg,
        turn_rows,
        tool_rows,
        hook_rows,
        warnings,
        mcp_waste_html,
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

/// Render the optional MCP-waste section to HTML.
///
/// Returns an empty string when [`AnalysisSection::McpWaste`] is not in
/// `sections` or `mcp_waste` is `None`. The sub-template lives in
/// `templates/mcp_waste_section.html.jinja` so the future `mcp-waste`
/// subcommand can reuse it without dragging session-level context in.
fn render_mcp_waste_section(
    sections: &[AnalysisSection],
    mcp_waste: Option<&WasteReport>,
) -> String {
    let Some(w) = mcp_waste else {
        return String::new();
    };
    if !sections.contains(&AnalysisSection::McpWaste) {
        return String::new();
    }
    let fully_unused_count = w.server_waste.iter().filter(|s| s.is_fully_unused).count();
    let data_source = match w.data_source {
        WasteDataSource::None => "neither wire notices nor mcp.json found",
        WasteDataSource::Wire => "wire notices",
        WasteDataSource::Config => "~/.copilot/mcp.json",
        WasteDataSource::Both => "wire notices + ~/.copilot/mcp.json",
        _ => "unknown",
    }
    .to_string();
    let server_waste: Vec<McpServerRow> = w
        .server_waste
        .iter()
        .map(|sw| McpServerRow {
            server: sw.server.clone(),
            loaded_count: sw.loaded_count,
            called_count: sw.called_count,
            unused_count: sw.unused_count,
            is_fully_unused: sw.is_fully_unused,
        })
        .collect();
    let sub = McpWasteSectionTemplate {
        data_source,
        total_loaded: w.total_loaded_tool_count,
        server_count: w.server_waste.len(),
        total_unused: w.total_unused_tool_count,
        fully_unused_count,
        server_waste,
    };
    sub.render().unwrap_or_else(|e| {
        format!(
            "<section id=\"mcp-waste\"><h2>MCP Server Waste</h2><pre>{}</pre></section>",
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
    show_mcp_waste: bool,
    svg_flamegraph: String,
    turn_rows: Vec<TurnRow>,
    tool_rows: Vec<ToolRow>,
    hook_rows: Vec<HookRow>,
    warnings: Vec<String>,
    mcp_waste_html: String,
    embedded_css: String,
    exporter: String,
    generated_at: String,
}

/// Askama sub-template for the optional MCP-waste section.
///
/// Lives in `templates/mcp_waste_section.html.jinja` so it can be reused
/// by the future `mcp-waste` subcommand (single-session HTML output).
/// Rendered out-of-band and injected into the main report template as
/// pre-escaped HTML via `{{ mcp_waste_html|safe }}`.
#[derive(Template)]
#[template(path = "mcp_waste_section.html.jinja", escape = "html")]
struct McpWasteSectionTemplate {
    data_source: String,
    total_loaded: usize,
    server_count: usize,
    total_unused: usize,
    fully_unused_count: usize,
    server_waste: Vec<McpServerRow>,
}

struct McpServerRow {
    server: String,
    loaded_count: usize,
    called_count: usize,
    unused_count: usize,
    is_fully_unused: bool,
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
