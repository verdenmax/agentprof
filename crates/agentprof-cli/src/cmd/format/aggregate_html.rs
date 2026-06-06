//! HTML renderer for [`AnyAggregateReport`] (askama 0.16).
//!
//! Produces a self-contained static HTML document — no JavaScript,
//! inline CSS from `templates/styles.css`. Day rows with
//! `is_low_utilization = true` get the `warn-row` class.

use std::fmt::Write as _;

use askama::Template;
use chrono::Utc;

use agentprof_core::analyzer::aggregate::{
    AggregateReport, AnyAggregateReport, DayBucket, McpServerBucket, ModelBucket, ToolBucket,
};

const EMBEDDED_CSS: &str = include_str!("../../../templates/styles.css");

/// Render `report` to a self-contained HTML document.
///
/// `low_threshold` is only used to populate the metadata header for
/// `--by day` reports; the actual `is_low_utilization` flag is already
/// set inside each [`DayBucket`] by the aggregator.
///
/// `agentprof_version` should be `env!("CARGO_PKG_VERSION")` of the
/// CLI binary; it's stamped into the footer.
///
/// # Examples
///
/// ```ignore
/// let html = aggregate_html::render(&report, 20.0, env!("CARGO_PKG_VERSION"));
/// assert!(html.contains("<table"));
/// ```
#[must_use]
pub fn render(report: &AnyAggregateReport, low_threshold: f32, agentprof_version: &str) -> String {
    let (by_label, session_count, failure_count, since, wall) = meta(report);
    let buckets_html = render_buckets(report);

    let template = AggregateTemplate {
        by_label: by_label.to_string(),
        session_count,
        failure_count,
        // Wave C: `since` is now Option<Duration>; fold None →
        // Duration::MAX so `human_duration` renders the existing
        // "all" branch (>= 100 years).
        since_human: human_duration(since.unwrap_or(chrono::Duration::MAX)),
        wall_human: human_duration(wall),
        low_threshold,
        low_threshold_visible: matches!(report, AnyAggregateReport::Day(_)),
        buckets_html,
        embedded_css: EMBEDDED_CSS.to_string(),
        exporter: format!("agentprof v{agentprof_version}"),
        generated_at: Utc::now().to_rfc3339(),
    };

    template.render().unwrap_or_else(|e| {
        format!(
            "<html><body><h1>render error</h1><pre>{}</pre></body></html>",
            html_escape(&format!("{e}"))
        )
    })
}

#[derive(Template)]
#[template(path = "aggregate.html")]
struct AggregateTemplate {
    by_label: String,
    session_count: usize,
    failure_count: usize,
    since_human: String,
    wall_human: String,
    low_threshold: f32,
    low_threshold_visible: bool,
    buckets_html: String,
    embedded_css: String,
    exporter: String,
    generated_at: String,
}

fn meta(
    r: &AnyAggregateReport,
) -> (
    &'static str,
    usize,
    usize,
    Option<chrono::Duration>,
    chrono::Duration,
) {
    match r {
        AnyAggregateReport::Tool(x) => (
            "tool",
            x.session_count,
            x.failure_count,
            x.since,
            x.total_wall_duration,
        ),
        AnyAggregateReport::McpServer(x) => (
            "mcp-server",
            x.session_count,
            x.failure_count,
            x.since,
            x.total_wall_duration,
        ),
        AnyAggregateReport::Day(x) => (
            "day",
            x.session_count,
            x.failure_count,
            x.since,
            x.total_wall_duration,
        ),
        AnyAggregateReport::Model(x) => (
            "model",
            x.session_count,
            x.failure_count,
            x.since,
            x.total_wall_duration,
        ),
        _ => unreachable!(
            "new AnyAggregateReport variant; add an explicit arm in aggregate_html::meta"
        ),
    }
}

fn render_buckets(r: &AnyAggregateReport) -> String {
    match r {
        AnyAggregateReport::Tool(x) => render_tool(x),
        AnyAggregateReport::McpServer(x) => render_mcp(x),
        AnyAggregateReport::Day(x) => render_day(x),
        AnyAggregateReport::Model(x) => render_model(x),
        _ => "<section><p>(unsupported aggregate variant)</p></section>".to_string(),
    }
}

fn render_tool(r: &AggregateReport<ToolBucket>) -> String {
    let mut s = String::new();
    s.push_str("<section id=\"buckets\"><table><thead><tr>");
    s.push_str("<th>Tool</th><th>Source</th><th class=\"num\">Calls</th>");
    s.push_str("<th class=\"num\">Success</th><th class=\"num\">Fail</th>");
    s.push_str("<th class=\"num\">Total</th><th class=\"num\">p50</th><th class=\"num\">p95</th>");
    s.push_str("<th class=\"num\">Sessions</th></tr></thead><tbody>");
    for b in &r.buckets {
        let _ = write!(
            s,
            "<tr><td><code>{}</code></td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            html_escape(&b.name),
            html_escape(&source_label(&b.source)),
            b.call_count,
            b.success_count,
            b.failure_count,
            human_duration(b.total_duration),
            human_duration(b.p50_duration),
            human_duration(b.p95_duration),
            b.session_count,
        );
    }
    s.push_str("</tbody></table></section>");
    s
}

fn render_mcp(r: &AggregateReport<McpServerBucket>) -> String {
    let mut s = String::new();
    s.push_str("<section id=\"buckets\"><table><thead><tr>");
    s.push_str("<th>Server</th><th class=\"num\">Tools</th><th class=\"num\">Calls</th>");
    s.push_str("<th class=\"num\">Failures</th><th class=\"num\">Total</th><th class=\"num\">Sessions</th></tr></thead><tbody>");
    for b in &r.buckets {
        let _ = write!(
            s,
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            html_escape(&b.server),
            b.tool_count,
            b.call_count,
            b.failure_count,
            human_duration(b.total_duration),
            b.session_count,
        );
    }
    s.push_str("</tbody></table></section>");
    s
}

fn render_day(r: &AggregateReport<DayBucket>) -> String {
    let mut s = String::new();
    s.push_str("<section id=\"buckets\"><table><thead><tr>");
    s.push_str("<th>Date</th><th class=\"num\">Sessions</th><th class=\"num\">Wall</th>");
    s.push_str("<th class=\"num\">Tool time</th><th class=\"num\">Out tokens</th>");
    s.push_str("<th class=\"num\">Utilization</th></tr></thead><tbody>");
    for b in &r.buckets {
        let cls = if b.is_low_utilization {
            " class=\"warn-row\""
        } else {
            ""
        };
        let _ = write!(
            s,
            "<tr{cls}><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>",
            b.date,
            b.session_count,
            human_duration(b.total_wall_duration),
            human_duration(b.total_tool_duration),
            b.total_output_tokens,
            b.utilization_pct,
        );
    }
    s.push_str("</tbody></table></section>");
    s
}

fn render_model(r: &AggregateReport<ModelBucket>) -> String {
    let mut s = String::new();
    s.push_str("<section id=\"buckets\"><table><thead><tr>");
    s.push_str("<th>Model</th><th class=\"num\">Sessions</th><th class=\"num\">Turns</th>");
    s.push_str(
        "<th class=\"num\">Out tokens</th><th class=\"num\">Total wall</th></tr></thead><tbody>",
    );
    for b in &r.buckets {
        let _ = write!(
            s,
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            html_escape(&b.model),
            b.session_count,
            b.turn_count,
            b.total_output_tokens,
            human_duration(b.total_duration),
        );
    }
    s.push_str("</tbody></table></section>");
    s
}

fn source_label(s: &agentprof_core::model::ToolSource) -> String {
    use agentprof_core::model::ToolSource;
    match s {
        ToolSource::Builtin => "builtin".to_string(),
        ToolSource::Mcp { server } => format!("mcp:{server}"),
        ToolSource::Skill { name } => format!("skill:{name}"),
        _ => "unknown".to_string(),
    }
}

#[allow(clippy::cast_precision_loss)]
fn human_duration(d: chrono::Duration) -> String {
    let ms = d.num_milliseconds();
    if ms == 0 {
        return "0 ms".to_string();
    }
    if d >= chrono::Duration::days(365 * 100) {
        return "all".to_string();
    }
    if ms < 1000 {
        return format!("{ms} ms");
    }
    if ms < 60_000 {
        return format!("{:.1} s", ms as f64 / 1000.0);
    }
    if ms < 3_600_000 {
        return format!("{:.1} min", ms as f64 / 60_000.0);
    }
    format!("{:.1} h", ms as f64 / 3_600_000.0)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
