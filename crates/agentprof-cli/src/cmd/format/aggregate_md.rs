//! Markdown renderer for [`AnyAggregateReport`].

use std::fmt::Write as _;

use chrono::Duration;

use agentprof_core::analyzer::aggregate::{
    AggregateReport, AnyAggregateReport, DayBucket, McpServerBucket, ModelBucket, ToolBucket,
};

/// Render `report` to a Markdown document.
///
/// The output starts with `# agentprof aggregate`, then a metadata
/// block (`By: ...`, `Window: ...`, `Sessions: ...`, optional failed
/// count, total wall-clock), then a per-key table.
///
/// # Examples
///
/// ```ignore
/// // Run inside the bin crate; pulled in via `use crate::cmd::format::aggregate_md`.
/// let md = aggregate_md::render(&report);
/// assert!(md.starts_with("# agentprof aggregate"));
/// ```
#[must_use]
pub fn render(report: &AnyAggregateReport) -> String {
    let mut out = String::new();
    let (by_label, session_count, failure_count, since, wall) = meta(report);
    let _ = writeln!(out, "# agentprof aggregate\n");
    let _ = writeln!(out, "- By: {by_label}");
    // Wave C: `since` is now Option<Duration> (None = "all time").
    // `human_duration` already renders `>= 100 years` as "all", so
    // we fold None → Duration::MAX to reuse the existing branch.
    let _ = writeln!(
        out,
        "- Window: {}",
        human_duration(since.unwrap_or(Duration::MAX))
    );
    let _ = writeln!(out, "- Sessions: {session_count}");
    if failure_count > 0 {
        let _ = writeln!(out, "- Failed (see stderr): {failure_count}");
    }
    let _ = writeln!(out, "- Total wall-clock time: {}", human_duration(wall));
    out.push('\n');

    match report {
        AnyAggregateReport::Tool(r) => render_tool(&mut out, r),
        AnyAggregateReport::McpServer(r) => render_mcp(&mut out, r),
        AnyAggregateReport::Day(r) => render_day(&mut out, r),
        AnyAggregateReport::Model(r) => render_model(&mut out, r),
        _ => {
            let _ = writeln!(out, "(unsupported aggregate variant)");
        }
    }
    out
}

fn meta(r: &AnyAggregateReport) -> (&'static str, usize, usize, Option<Duration>, Duration) {
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
            "new AnyAggregateReport variant; add an explicit arm in aggregate_md::meta"
        ),
    }
}

fn render_tool(out: &mut String, r: &AggregateReport<ToolBucket>) {
    let _ = writeln!(out, "## By tool\n");
    if r.buckets.is_empty() {
        let _ = writeln!(out, "(no buckets)");
        return;
    }
    let _ = writeln!(
        out,
        "| Tool | Source | Calls | Success | Fail | Total | p50 | p95 | Sessions |"
    );
    let _ = writeln!(out, "|---|---|---:|---:|---:|---:|---:|---:|---:|");
    for b in &r.buckets {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |",
            md_escape(&b.name),
            source_label(&b.source),
            b.call_count,
            b.success_count,
            b.failure_count,
            human_duration(b.total_duration),
            human_duration(b.p50_duration),
            human_duration(b.p95_duration),
            b.session_count,
        );
    }
}

fn render_mcp(out: &mut String, r: &AggregateReport<McpServerBucket>) {
    let _ = writeln!(out, "## By MCP server\n");
    if r.buckets.is_empty() {
        let _ = writeln!(out, "(no buckets)");
        return;
    }
    let _ = writeln!(
        out,
        "| Server | Tools | Calls | Failures | Total | Sessions | **Unused tools** | **Sessions w/0 calls** |"
    );
    let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|---:|");
    for b in &r.buckets {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            md_escape(&b.server),
            b.tool_count,
            b.call_count,
            b.failure_count,
            human_duration(b.total_duration),
            b.session_count,
            b.unused_tool_count,
            b.fully_unused_session_count,
        );
    }
}

fn render_day(out: &mut String, r: &AggregateReport<DayBucket>) {
    let _ = writeln!(out, "## By day\n");
    if r.buckets.is_empty() {
        let _ = writeln!(out, "(no buckets)");
        return;
    }
    let _ = writeln!(
        out,
        "| Date | Sessions | Wall | Tool time | Out tokens | Utilization |"
    );
    let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|");
    for b in &r.buckets {
        let warn = if b.is_low_utilization { "⚠ " } else { "" };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {}{:.1}% |",
            b.date,
            b.session_count,
            human_duration(b.total_wall_duration),
            human_duration(b.total_tool_duration),
            b.total_output_tokens,
            warn,
            b.utilization_pct,
        );
    }
}

fn render_model(out: &mut String, r: &AggregateReport<ModelBucket>) {
    let _ = writeln!(out, "## By model\n");
    if r.buckets.is_empty() {
        let _ = writeln!(out, "(no buckets)");
        return;
    }
    let _ = writeln!(
        out,
        "| Model | Sessions | Turns | Out tokens | Total wall |"
    );
    let _ = writeln!(out, "|---|---:|---:|---:|---:|");
    for b in &r.buckets {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            md_escape(&b.model),
            b.session_count,
            b.turn_count,
            b.total_output_tokens,
            human_duration(b.total_duration),
        );
    }
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

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[allow(clippy::cast_precision_loss)]
fn human_duration(d: Duration) -> String {
    let ms = d.num_milliseconds();
    if ms == 0 {
        return "0 ms".to_string();
    }
    if d >= Duration::days(365 * 100) {
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
