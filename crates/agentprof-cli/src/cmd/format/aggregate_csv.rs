//! CSV renderer for [`AnyAggregateReport`].

use anyhow::{Context as _, Result};

use agentprof_core::analyzer::aggregate::{
    AggregateReport, AnyAggregateReport, DayBucket, McpServerBucket, ModelBucket, ToolBucket,
};

/// Render `report` to a CSV document.
///
/// The header row and column order depend on which [`AnyAggregateReport`]
/// variant is passed; see the per-variant writers below.
///
/// # Examples
///
/// ```ignore
/// let csv = aggregate_csv::render(&report)?;
/// assert!(csv.lines().next().unwrap().contains(','));
/// ```
///
/// # Errors
///
/// Propagates I/O errors from [`csv::Writer`] (rare for in-memory
/// buffers) and rejects non-UTF-8 output.
pub fn render(report: &AnyAggregateReport) -> Result<String> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = csv::Writer::from_writer(&mut buf);
        match report {
            AnyAggregateReport::Tool(r) => write_tool(&mut w, r)?,
            AnyAggregateReport::McpServer(r) => write_mcp(&mut w, r)?,
            AnyAggregateReport::Day(r) => write_day(&mut w, r)?,
            AnyAggregateReport::Model(r) => write_model(&mut w, r)?,
            _ => anyhow::bail!("unsupported AnyAggregateReport variant"),
        }
        w.flush().context("flush CSV writer")?;
    }
    String::from_utf8(buf).context("CSV writer produced non-UTF-8 output")
}

fn write_tool<W: std::io::Write>(
    w: &mut csv::Writer<W>,
    r: &AggregateReport<ToolBucket>,
) -> Result<()> {
    w.write_record([
        "name",
        "source",
        "call_count",
        "success_count",
        "failure_count",
        "total_duration_ms",
        "p50_ms",
        "p95_ms",
        "session_count",
    ])
    .context("write tool header")?;
    for b in &r.buckets {
        w.write_record([
            b.name.as_str(),
            source_label(&b.source).as_str(),
            &b.call_count.to_string(),
            &b.success_count.to_string(),
            &b.failure_count.to_string(),
            &b.total_duration.num_milliseconds().to_string(),
            &b.p50_duration.num_milliseconds().to_string(),
            &b.p95_duration.num_milliseconds().to_string(),
            &b.session_count.to_string(),
        ])
        .context("write tool row")?;
    }
    Ok(())
}

fn write_mcp<W: std::io::Write>(
    w: &mut csv::Writer<W>,
    r: &AggregateReport<McpServerBucket>,
) -> Result<()> {
    w.write_record([
        "server",
        "tool_count",
        "call_count",
        "failure_count",
        "total_duration_ms",
        "session_count",
        "unused_tool_count",
        "fully_unused_session_count",
    ])
    .context("write mcp header")?;
    for b in &r.buckets {
        w.write_record([
            b.server.as_str(),
            &b.tool_count.to_string(),
            &b.call_count.to_string(),
            &b.failure_count.to_string(),
            &b.total_duration.num_milliseconds().to_string(),
            &b.session_count.to_string(),
            &b.unused_tool_count.to_string(),
            &b.fully_unused_session_count.to_string(),
        ])
        .context("write mcp row")?;
    }
    Ok(())
}

fn write_day<W: std::io::Write>(
    w: &mut csv::Writer<W>,
    r: &AggregateReport<DayBucket>,
) -> Result<()> {
    w.write_record([
        "date",
        "session_count",
        "wall_ms",
        "tool_ms",
        "output_tokens",
        "utilization_pct",
        "is_low_utilization",
    ])
    .context("write day header")?;
    for b in &r.buckets {
        w.write_record([
            &b.date.to_string(),
            &b.session_count.to_string(),
            &b.total_wall_duration.num_milliseconds().to_string(),
            &b.total_tool_duration.num_milliseconds().to_string(),
            &b.total_output_tokens.to_string(),
            &format!("{:.2}", b.utilization_pct),
            &b.is_low_utilization.to_string(),
        ])
        .context("write day row")?;
    }
    Ok(())
}

fn write_model<W: std::io::Write>(
    w: &mut csv::Writer<W>,
    r: &AggregateReport<ModelBucket>,
) -> Result<()> {
    w.write_record([
        "model",
        "session_count",
        "turn_count",
        "output_tokens",
        "total_duration_ms",
    ])
    .context("write model header")?;
    for b in &r.buckets {
        w.write_record([
            b.model.as_str(),
            &b.session_count.to_string(),
            &b.turn_count.to_string(),
            &b.total_output_tokens.to_string(),
            &b.total_duration.num_milliseconds().to_string(),
        ])
        .context("write model row")?;
    }
    Ok(())
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
