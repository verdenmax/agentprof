//! `agentprof mcp-waste` — cross-session report of MCP tools loaded but never
//! called. See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md`
//! §7.3 for the user-facing contract.
//!
//! As of T4.2 the [`run`] entry point wires the full pipeline: discover
//! sessions (Copilot adapter) → filter by `--since` mtime → load
//! `mcp.json` once → per-session
//! `load_session → derive_episodes → analyze → extract_loaded_set →
//! compute_waste` → `aggregate_waste` → render → stdout/file.
//! T4.3 fills in the three renderers (`render_md`, `render_json`,
//! `render_html`) — the HTML output is a self-contained document built
//! from `templates/mcp_waste_full.html.jinja`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

use clap::ValueEnum;

use crate::cmd::analyze::ExitKind;
use crate::cmd::since::parse_since;
use crate::cmd::{LogConfig, TracingHandle};

/// Arguments for `agentprof mcp-waste`.
///
/// Mirrors the spec §7.3 CLI surface: time-window filter, optional adapter
/// root override, `mcp.json` path override, and an export format that
/// deliberately excludes TUI (see [`McpWasteExport`] for rationale).
#[derive(Debug, clap::Args)]
#[non_exhaustive]
pub struct McpWasteArgs {
    /// Adapter session-state root (default: per-adapter standard path).
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Time-window filter (e.g. `7d`, `30d`, `all`). Default: `7d`.
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Limit "Always unused" table to top N entries. Default 20.
    #[arg(long, default_value_t = 20)]
    pub top: usize,

    /// Override `mcp.json` path (default: `~/.copilot/mcp.json`).
    /// Relative paths resolve against CWD; `~`-prefixed paths expand
    /// via `directories::BaseDirs::home_dir()` to match `--root`
    /// convention.
    #[arg(long)]
    pub mcp_config: Option<PathBuf>,

    /// Output format. TUI is deferred to a future milestone — see
    /// spec §10 for the rationale.
    #[arg(long, value_enum, default_value = "md")]
    pub export: McpWasteExport,

    /// Output file (default: stdout).
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Output formats supported by `agentprof mcp-waste`.
///
/// TUI is intentionally **not** included; per spec §7.3 / §10 the live
/// dashboard for waste lives in `agentprof watch` and the dedicated TUI
/// view is deferred to a future milestone.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
#[non_exhaustive]
pub enum McpWasteExport {
    /// Markdown report (default; renders the same template family as
    /// `analyze --export md`).
    Md,
    /// JSON document suitable for machine consumption / CI gating.
    Json,
    /// Self-contained HTML report (askama template).
    Html,
}

/// Resolve `~/.copilot/mcp.json` (or the override) into an absolute
/// [`PathBuf`].
///
/// This helper is shared with `cmd::analyze`'s `--section mcp-waste`
/// dispatch (T3.1) so both code paths read the same configuration with
/// identical `~`-expansion semantics.
///
/// # Examples
///
/// ```ignore
/// # use std::path::Path;
/// # use agentprof_cli::cmd::mcp_waste::resolve_mcp_config_path;
/// // Override wins over the default home-based lookup.
/// let p = resolve_mcp_config_path(Some(Path::new("./mcp.json")));
/// assert!(p.ends_with("mcp.json"));
/// ```
///
/// # Errors
///
/// Returns an `anyhow::Error` when `override_path` is `None` and
/// `directories::BaseDirs::new()` fails to determine the user's home
/// directory (e.g. on a minimal CI container without `HOME` set and no
/// `/etc/passwd` entry for the current uid).
pub fn resolve_mcp_config_path(override_path: Option<&std::path::Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(expand_tilde(p));
    }
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("could not resolve user home directory"))?;
    Ok(base.home_dir().join(".copilot").join("mcp.json"))
}

/// Expand a leading `~/` against the current user's home directory.
///
/// Falls back to the original path when no home directory can be
/// determined (preserves the user's literal input for the caller's
/// downstream error message).
fn expand_tilde(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(base) = directories::BaseDirs::new() {
            return base.home_dir().join(rest);
        }
    }
    p.to_path_buf()
}

/// Subcommand entry point.
///
/// Implements the 7-stage `mcp-waste` pipeline:
///
/// 1. Discover sessions via [`agentprof_adapters::copilot::CopilotAdapter`]
///    under `--root` (defaults to `~/.copilot/session-state`).
/// 2. Filter by `--since` window using `SessionRef::modified_at`
///    (mirrors `aggregate`'s mtime-based filter).
/// 3. Load `mcp.json` once (resolved via [`resolve_mcp_config_path`]);
///    parse failures degrade silently to `None` so the CLI stays usable
///    on hosts without a Copilot MCP config.
/// 4. For each surviving session, run
///    `load_session → derive_episodes → analyze → extract_loaded_set →
///    compute_waste`. Per-session failures are warned to stderr (via
///    `tracing::warn!`) and skipped, matching `aggregate`'s policy.
/// 5. Cross-session aggregation via
///    [`agentprof_core::analyzer::aggregate_waste`].
/// 6. Render (md / json / html — implemented in T4.3).
/// 7. Write to `--output` or stdout.
///
/// `_cfg` and `_tracing_handle` are accepted for signature uniformity
/// with the other `cmd::*::run` entry points and are unused here today.
///
/// # Examples
///
/// ```ignore
/// # use agentprof_cli::cmd::mcp_waste::{run, McpWasteArgs, McpWasteExport};
/// # let args = McpWasteArgs {
/// #     root: Some("crates/agentprof-adapters/tests/fixtures/copilot".into()),
/// #     since: "all".into(), top: 20, mcp_config: None,
/// #     export: McpWasteExport::Md, output: None,
/// # };
/// // run(args, &cfg, &handle)?;
/// ```
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is [`ExitKind`]:
///
/// - [`ExitKind::UserError`] — invalid `--since`, or `mcp.json` override
///   path that cannot be resolved.
/// - [`ExitKind::DataError`] — session discovery failed, no sessions
///   matched `--since`, or every discovered session failed to parse.
/// - [`ExitKind::OutputError`] — writing to `--output` failed.
#[allow(clippy::too_many_lines)] // single linear pipeline: discover → load → compute → render
pub fn run(
    args: McpWasteArgs,
    _cfg: &LogConfig,
    _tracing_handle: &TracingHandle,
) -> anyhow::Result<()> {
    use agentprof_adapters::copilot::{
        extract_loaded_set_from_session, load_mcp_config, CopilotAdapter,
    };
    use agentprof_core::adapter::Adapter;
    use agentprof_core::analyzer::{aggregate_waste, analyze, compute_waste};
    use agentprof_core::episode::derive_episodes;

    // 1. Discover sessions (Copilot is the only adapter with MCP wire
    //    data today; see spec §7.3).
    let adapter = CopilotAdapter;
    let root = args.root.unwrap_or_else(|| {
        directories::BaseDirs::new().map_or_else(
            || PathBuf::from(".copilot/session-state"),
            |b| b.home_dir().join(".copilot").join("session-state"),
        )
    });
    let sessions = adapter.discover_sessions(&root).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!(
            "discovering sessions under {}: {e}",
            root.display()
        ))
    })?;

    // 2. Filter by --since mtime window (mirrors aggregate.rs).
    let since_dur = parse_since(&args.since)
        .map_err(|msg| ExitKind::UserError.into_anyhow(format!("invalid --since: {msg}")))?;
    let now = SystemTime::now();
    let filtered: Vec<_> = sessions
        .into_iter()
        .filter(|s| {
            now.duration_since(s.modified_at)
                .map_or(true, |elapsed| elapsed <= since_dur)
        })
        .collect();

    if filtered.is_empty() {
        return Err(ExitKind::DataError.into_anyhow(format!(
            "no sessions matched --since {} under {}",
            args.since,
            root.display()
        )));
    }

    // 3. Load mcp.json once. Parse failures degrade to None so the CLI
    //    remains usable on hosts without a Copilot MCP config.
    let mcp_config_path = resolve_mcp_config_path(args.mcp_config.as_deref())?;
    let parsed_cfg = load_mcp_config(&mcp_config_path);
    let cfg_map: Option<BTreeMap<String, Vec<String>>> = parsed_cfg.as_ref().map(|c| {
        c.servers
            .iter()
            .filter_map(|(name, info)| info.tools.as_ref().map(|t| (name.clone(), t.clone())))
            .collect()
    });

    // 4. Per session: load, derive episodes, analyze, compute_waste.
    let mut per_session: Vec<(
        agentprof_core::adapter::SessionRef,
        agentprof_core::model::WasteReport,
    )> = Vec::with_capacity(filtered.len());
    let mut failed: usize = 0;
    for sref in &filtered {
        let result: Result<agentprof_core::model::WasteReport, anyhow::Error> = (|| {
            let raw = adapter.load_session(sref)?;
            let episodes = derive_episodes(&raw.events, &raw.meta);
            let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);
            let wire = extract_loaded_set_from_session(&raw.events);
            // M1.6.6 T1.4: assemble the WasteComputeContext (tokenizer
            // inferred from the session's first observed model; sidecar
            // wiring lands in M1.6.6 T3.1).
            let model_hint: Option<String> = report
                .model_metrics
                .as_ref()
                .and_then(|m| m.keys().next().cloned());
            let tokenizer = agentprof_core::analyzer::waste::infer_tokenizer(model_hint.as_deref());
            let mut waste_ctx = agentprof_core::analyzer::waste::WasteComputeContext::new(&wire)
                .with_tokenizer(tokenizer);
            if let Some(c) = cfg_map.as_ref() {
                waste_ctx = waste_ctx.with_config(c);
            }
            Ok(compute_waste(&report, &waste_ctx))
        })();
        match result {
            Ok(waste) => per_session.push((sref.clone(), waste)),
            Err(e) => {
                tracing::warn!(
                    session = %agentprof_core::observability::pii::hash_short(&sref.id),
                    error = %format_args!("{e:#}"),
                    "failed to analyze session"
                );
                failed += 1;
            }
        }
    }
    if per_session.is_empty() {
        return Err(
            ExitKind::DataError.into_anyhow(format!("all {failed} session(s) failed to parse"))
        );
    }
    if failed > 0 {
        tracing::warn!(
            sessions_ok = per_session.len(),
            sessions_failed = failed,
            sessions_total = filtered.len(),
            "partial parse failures; report covers successful sessions only"
        );
    }

    // 5. Cross-session aggregation.
    let agg = aggregate_waste(&per_session);

    // 6. Render.
    let rendered = match args.export {
        McpWasteExport::Md => render_md(&agg, args.top),
        McpWasteExport::Json => render_json(&agg)?,
        McpWasteExport::Html => render_html(&agg, args.top)?,
    };

    // 7. Output.
    if let Some(path) = &args.output {
        std::fs::write(path, &rendered).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!("writing to {}: {e}", path.display()))
        })?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

/// Render the aggregate waste report as Markdown.
///
/// Produces three sections — a generation-stamped header, an
/// "Always unused" table truncated to `top` rows (sessions-loaded
/// counts looked up from `per_server.tool_usage`), and a per-server
/// cross-session table summing `total_call_count` across tools.
/// Mirrors the layout of `aggregate --by mcp-server --export md` so
/// the two outputs are comparable side-by-side.
fn render_md(a: &agentprof_core::model::AggregateWasteReport, top: usize) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let now = chrono::Utc::now().format("%Y-%m-%d");
    let _ = writeln!(
        out,
        "# MCP Waste Report ({} sessions, generated {})\n",
        a.sessions, now
    );
    let _ = writeln!(out, "## Summary\n");
    let _ = writeln!(out, "- {} sessions analyzed", a.sessions);
    let _ = writeln!(out, "- {} MCP servers in scope", a.per_server.len());
    let _ = writeln!(
        out,
        "- {} tools NEVER called across ANY session\n",
        a.never_called_tools.len()
    );

    if !a.never_called_tools.is_empty() {
        let _ = writeln!(
            out,
            "## \"Always unused\" — {} tools (top {})\n",
            a.never_called_tools.len(),
            top.min(a.never_called_tools.len())
        );
        let _ = writeln!(out, "| Tool | Sessions loaded |");
        let _ = writeln!(out, "|------|----------------:|");
        let mut lookup: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for sw in &a.per_server {
            for t in &sw.tool_usage {
                lookup.insert(t.tool_name.as_str(), t.sessions_loaded);
            }
        }
        for tname in a.never_called_tools.iter().take(top) {
            let n = lookup.get(tname.as_str()).copied().unwrap_or(0);
            let _ = writeln!(out, "| {tname} | {n} |");
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Per-server cross-session\n");
    let _ = writeln!(
        out,
        "| Server | Sessions loaded | Sessions w/0 calls | Calls (sum) |"
    );
    let _ = writeln!(
        out,
        "|--------|----------------:|-------------------:|------------:|"
    );
    for sw in &a.per_server {
        let total_calls: usize = sw.tool_usage.iter().map(|t| t.total_call_count).sum();
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            sw.server, sw.sessions_loaded, sw.sessions_with_zero_calls, total_calls
        );
    }
    out
}

/// Render the aggregate waste report as pretty-printed JSON.
///
/// Appends a trailing newline to match the CLI's existing
/// `--export json` convention (so `agentprof ... | jq` and POSIX
/// `read` loops behave predictably).
///
/// # Errors
///
/// Returns `serde_json::Error` (boxed into `anyhow::Error`) if any
/// field in the report fails to serialise — should not happen in
/// practice because every field in `AggregateWasteReport` is
/// `Serialize`.
fn render_json(a: &agentprof_core::model::AggregateWasteReport) -> anyhow::Result<String> {
    let mut s = serde_json::to_string_pretty(a)?;
    s.push('\n');
    Ok(s)
}

/// Render the aggregate waste report as a self-contained HTML document.
///
/// Uses askama (`templates/mcp_waste_full.html.jinja`) — unlike
/// `templates/mcp_waste_section.html.jinja` (which is a fragment
/// injected into `report.html`), this template is a complete
/// `<!doctype html>` document because `mcp-waste` produces standalone
/// output (no enclosing report to embed in).
///
/// # Errors
///
/// Returns `askama::Error` (boxed into `anyhow::Error`) if template
/// rendering fails — typically only a programming error (mismatched
/// field name vs. template variable).
fn render_html(
    a: &agentprof_core::model::AggregateWasteReport,
    top: usize,
) -> anyhow::Result<String> {
    use askama::Template;

    #[derive(Template)]
    #[template(path = "mcp_waste_full.html.jinja", escape = "html")]
    struct McpWasteFullTpl<'a> {
        sessions: usize,
        servers_count: usize,
        never_called_count: usize,
        top: usize,
        never_called_top: Vec<NeverCalledRow<'a>>,
        per_server: Vec<PerServerRow<'a>>,
    }
    struct NeverCalledRow<'a> {
        tool: &'a str,
        sessions_loaded: usize,
    }
    struct PerServerRow<'a> {
        server: &'a str,
        sessions_loaded: usize,
        sessions_with_zero_calls: usize,
        total_calls: usize,
    }

    let mut lookup: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for sw in &a.per_server {
        for t in &sw.tool_usage {
            lookup.insert(t.tool_name.as_str(), t.sessions_loaded);
        }
    }
    let never_called_top: Vec<NeverCalledRow> = a
        .never_called_tools
        .iter()
        .take(top)
        .map(|tname| NeverCalledRow {
            tool: tname.as_str(),
            sessions_loaded: lookup.get(tname.as_str()).copied().unwrap_or(0),
        })
        .collect();
    let per_server: Vec<PerServerRow> = a
        .per_server
        .iter()
        .map(|sw| PerServerRow {
            server: sw.server.as_str(),
            sessions_loaded: sw.sessions_loaded,
            sessions_with_zero_calls: sw.sessions_with_zero_calls,
            total_calls: sw.tool_usage.iter().map(|t| t.total_call_count).sum(),
        })
        .collect();

    let tpl = McpWasteFullTpl {
        sessions: a.sessions,
        servers_count: a.per_server.len(),
        never_called_count: a.never_called_tools.len(),
        top,
        never_called_top,
        per_server,
    };
    Ok(tpl.render()?)
}
