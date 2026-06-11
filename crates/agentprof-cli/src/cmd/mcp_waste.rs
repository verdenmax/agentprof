//! `agentprof mcp-waste` — cross-session report of MCP tools loaded but never
//! called. See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md`
//! §7.3 for the user-facing contract.
//!
//! As of T4.2 the [`run`] entry point wires the full pipeline: discover
//! sessions (Copilot adapter) → filter by `--since` mtime → load
//! `mcp.json` once → per-session
//! `load_session → derive_episodes → analyze → compute_waste`
//! (M2.1 T5.2.5: ever-loaded MCP tools now hang off `analyze`'s
//! `AnalysisReport.loaded_mcp_tools`, so the previous explicit
//! `extract_loaded_set_from_session` step is folded into `analyze`;
//! M2.1 T5.2.6: discovery + per-session analyze now flow through
//! [`build_data_source`] so the `SQLite` cache short-circuits
//! re-parsing when available — dual-path divergence warnings are
//! drained to stderr after the loop unless `--quiet` is set)
//! → `aggregate_waste` → render → stdout/file.
//!
//! [`build_data_source`]: agentprof_cli::data_source_factory::build_data_source
//! T4.3 fills in the three renderers (`render_md`, `render_json`,
//! `render_html`) — the HTML output is a self-contained document built
//! from `templates/mcp_waste_full.html.jinja`.
//!
//! M1.6.6 T4.1: also accepts `--tokens-per-tool` and
//! `--tool-descriptions` (same shape as `analyze` / `aggregate`); the
//! sidecar is loaded ONCE outside the per-session loop and layered
//! onto the per-session `WasteComputeContext`. Renderers surface the
//! resulting token costs as a Summary `≈X wasted tokens` line, a
//! `Largest waste` line, a "Tokens (per session)" column on the
//! always-unused table, and a `Wasted tokens` column on the per-server
//! table. See `docs/superpowers/plans/2026-06-08-m1.6.6-token-cost.md`
//! §T4.1.

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

    /// Heuristic token cost per MCP tool when no sidecar covers a tool.
    /// Default 200 (≈ description + small inputSchema). M1.6.6 T4.1.
    #[arg(long, default_value_t = agentprof_core::analyzer::waste::DEFAULT_HEURISTIC_TOKENS)]
    pub tokens_per_tool: u64,

    /// Optional sidecar path with per-tool descriptions for exact token counts.
    /// Path → file: global JSON `{"<server>": [<ToolEntry>, ...]}`.
    /// Path → dir: per-server `<server>.json` files (MCP `tools/list` shape).
    /// Absent → heuristic-only mode. M1.6.6 T4.1.
    #[arg(long, value_name = "PATH")]
    pub tool_descriptions: Option<PathBuf>,
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
/// 1. Build a [`SessionDataSource`] via
///    [`agentprof_cli::data_source_factory::build_data_source`]; on `--no-cache`
///    or storage-open failure this degrades to an adapter-only source.
///    Discover sessions under `--root` (defaults to
///    `~/.copilot/session-state`) and filter by the `--since` window
///    via [`SessionDataSource::discover`].
/// 2. Load `mcp.json` once (resolved via [`resolve_mcp_config_path`]);
///    parse failures degrade silently to `None` so the CLI stays usable
///    on hosts without a Copilot MCP config.
/// 3. For each surviving session, ask the data source for an
///    [`agentprof_core::analyzer::AnalysisReport`] (the source either
///    re-parses the live file or returns a cached row) then run
///    `compute_waste` against it. The ever-loaded MCP tool set comes
///    from `AnalysisReport.loaded_mcp_tools` (M2.1 T5.2.5), so no
///    separate `Episodes`/raw-event pass is needed here. Per-session
///    failures are warned to stderr (via `tracing::warn!`) and skipped,
///    matching `aggregate`'s policy.
/// 4. Cross-session aggregation via
///    [`agentprof_core::analyzer::aggregate_waste`].
/// 5. Render (md / json / html).
/// 6. Write to `--output` or stdout.
/// 7. Drain accumulated dual-path divergence warnings to stderr (one
///    line per affected session) unless `--quiet` is set — same
///    contract as `cmd::list` (M2.1 T5.2).
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
/// #     tokens_per_tool: 200, tool_descriptions: None,
/// # };
/// // run(args, &cfg, &handle, /* no_cache */ false, /* storage_path */ None, /* quiet */ false)?;
/// ```
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is [`ExitKind`]:
///
/// - [`ExitKind::UserError`] — invalid `--since`, `mcp.json` override
///   path that cannot be resolved, or `build_data_source` rejected the
///   agent name / storage config.
/// - [`ExitKind::DataError`] — session discovery failed, no sessions
///   matched `--since`, or every discovered session failed to parse.
/// - [`ExitKind::OutputError`] — writing to `--output` failed.
///
/// [`SessionDataSource`]: agentprof_core::datasource::SessionDataSource
/// [`SessionDataSource::discover`]: agentprof_core::datasource::SessionDataSource::discover
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)] // single linear pipeline: discover → load → compute → render; sig mirrors `cmd::list::run`
pub fn run(
    args: McpWasteArgs,
    _cfg: &LogConfig,
    _tracing_handle: &TracingHandle,
    no_cache: bool,
    storage_path: Option<PathBuf>,
    quiet: bool,
) -> anyhow::Result<()> {
    use agentprof_adapters::copilot::{load_mcp_config, CopilotAdapter};
    use agentprof_core::adapter::Adapter as _;
    use agentprof_core::analyzer::{aggregate_waste, compute_waste};

    // 1. Discover sessions through the dual-path factory (M2.1 T5.2.6).
    //    Copilot is still the only supported agent (spec §7.3); resolve
    //    the default session-state root via `CopilotAdapter` so the
    //    `~/.copilot/...` convention is owned by exactly one crate.
    let root = args.root.clone().unwrap_or_else(|| {
        CopilotAdapter
            .default_session_root()
            .unwrap_or_else(|| PathBuf::from(".copilot/session-state"))
    });
    let storage_cfg = agentprof_cli::config::resolve_storage_config(
        agentprof_storage::config::PartialStorageConfig::default(),
        storage_path,
    )
    .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let (ds, warnings_handle) = agentprof_cli::data_source_factory::build_data_source(
        "copilot",
        &root,
        &storage_cfg,
        no_cache,
    )
    .map_err(|e| ExitKind::UserError.into_anyhow(format!("{e}")))?;

    // 2. Filter by --since via the data source (the underlying
    //    AdapterDataSource still uses mtime; SqliteDataSource uses the
    //    persisted `started_at`).
    let since_dur = parse_since(&args.since)
        .map_err(|msg| ExitKind::UserError.into_anyhow(format!("invalid --since: {msg}")))?;
    let filtered: Vec<agentprof_core::datasource::SessionRef> =
        ds.discover(since_dur).map_err(|e| {
            ExitKind::DataError.into_anyhow(format!(
                "discovering sessions under {}: {e}",
                root.display()
            ))
        })?;

    if filtered.is_empty() {
        drain_and_emit_warnings(&warnings_handle, quiet);
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

    // M1.6.6 T4.1 — load the optional `--tool-descriptions` sidecar
    // ONCE outside the per-session loop (mirrors `cmd::analyze` /
    // `cmd::aggregate`). Heuristic-only mode when `None`.
    let sidecar = if let Some(path) = args.tool_descriptions.as_deref() {
        Some(
            agentprof_adapters::copilot::tool_sidecar::load_sidecar(path).map_err(|e| {
                ExitKind::UserError
                    .into_anyhow(format!("sidecar load failed for {}: {e}", path.display()))
            })?,
        )
    } else {
        None
    };

    // M1.6.6 audit A1: build each BPE encoder at most once per command
    // (only two variants) and share via Arc — re-parsing the embedded
    // merge table for every session in a 100-session run was ~5 s + GBs
    // of allocate-drop.
    let bpe_cl100k = agentprof_core::analyzer::waste::build_bpe(
        agentprof_core::model::TokenizerKind::Cl100kBase,
    )
    .map(std::sync::Arc::new);
    let bpe_o200k =
        agentprof_core::analyzer::waste::build_bpe(agentprof_core::model::TokenizerKind::O200kBase)
            .map(std::sync::Arc::new);

    // 4. Per session: load (via data source — adapter or SQLite cache),
    //    compute_waste. Episodes are NOT needed downstream here because
    //    `compute_waste` only consumes `AnalysisReport`; the per-session
    //    `loaded_mcp_tools` came along on the report itself (T5.2.5).
    let mut per_session: Vec<(
        agentprof_core::datasource::SessionRef,
        agentprof_core::model::WasteReport,
    )> = Vec::with_capacity(filtered.len());
    let mut failed: usize = 0;
    for sref in &filtered {
        let result: Result<agentprof_core::model::WasteReport, anyhow::Error> = (|| {
            let report = ds.load_session(&sref.id)?;
            // M2.1 T5.2.5: ever-loaded MCP tool set now hangs off the
            // analyzer report. Borrowing keeps WasteComputeContext's
            // &BTreeSet contract intact.
            let wire = &report.loaded_mcp_tools;
            // M1.6.6 T1.4 + T4.1 + audit B1: assemble the
            // WasteComputeContext — tokenizer inferred from the
            // session's *dominant* model (largest token total) via
            // `cmd::model_hint::dominant_model`, then layer
            // `--tokens-per-tool` (heuristic) and `--tool-descriptions`
            // (sidecar) per ADR-0016 D-2.
            let model_hint = crate::cmd::model_hint::dominant_model(&report);
            let tokenizer = agentprof_core::analyzer::waste::infer_tokenizer(model_hint.as_deref());
            let mut waste_ctx = agentprof_core::analyzer::waste::WasteComputeContext::new(wire)
                .with_tokenizer(tokenizer)
                .with_heuristic(args.tokens_per_tool);
            let shared_bpe = match tokenizer {
                agentprof_core::model::TokenizerKind::Cl100kBase => bpe_cl100k.as_ref(),
                agentprof_core::model::TokenizerKind::O200kBase => bpe_o200k.as_ref(),
                _ => None,
            };
            if let Some(b) = shared_bpe {
                waste_ctx = waste_ctx.with_bpe(b.clone());
            }
            if let Some(c) = cfg_map.as_ref() {
                waste_ctx = waste_ctx.with_config(c);
            }
            if let Some(s) = sidecar.as_ref() {
                waste_ctx = waste_ctx.with_sidecar(s);
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
        drain_and_emit_warnings(&warnings_handle, quiet);
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
    //
    //    `aggregate_waste` is keyed on the per-session `WasteReport`
    //    alone; the `SessionRef` companion is only used for tagging
    //    per-session provenance. Both the legacy
    //    `agentprof_core::adapter::SessionRef` and the dual-path
    //    `agentprof_core::datasource::SessionRef` expose a stable
    //    `id` field, which is all `aggregate_waste` reads.
    let agg = {
        // Build the legacy `(adapter::SessionRef, WasteReport)` pairs
        // that `aggregate_waste` expects. The synthesised ref carries
        // only the id + a placeholder path; `aggregate_waste` does
        // not read `path`/`modified_at`.
        let pairs: Vec<(
            agentprof_core::adapter::SessionRef,
            agentprof_core::model::WasteReport,
        )> = per_session
            .iter()
            .map(|(sref, w)| {
                let legacy_ref = agentprof_core::adapter::SessionRef::new(
                    sref.id.clone(),
                    sref.agent,
                    sref.raw_path.clone().unwrap_or_default(),
                    SystemTime::UNIX_EPOCH,
                    0,
                    false,
                );
                (legacy_ref, w.clone())
            })
            .collect();
        aggregate_waste(&pairs)
    };

    // M1.6.6 T4.1 — build a tool_name → description_tokens lookup from
    // per-session WasteReports so the rendered "Always unused" table
    // can show a per-session token count. Token cost for a given tool
    // is a function of its description+schema, so values should be
    // identical across sessions; we take the max defensively in case
    // a sidecar update changes the cost mid-window.
    let mut tool_tokens: BTreeMap<String, u64> = BTreeMap::new();
    for (_sref, wreport) in &per_session {
        for sw in &wreport.server_waste {
            for t in &sw.tools {
                let entry = tool_tokens.entry(t.tool_name.clone()).or_insert(0);
                if t.description_tokens > *entry {
                    *entry = t.description_tokens;
                }
            }
        }
    }

    // 6. Render.
    let rendered = match args.export {
        McpWasteExport::Md => render_md(&agg, args.top, &tool_tokens),
        McpWasteExport::Json => render_json(&agg)?,
        McpWasteExport::Html => render_html(&agg, args.top, &tool_tokens)?,
    };

    // 7. Output.
    if let Some(path) = &args.output {
        std::fs::write(path, &rendered).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!("writing to {}: {e}", path.display()))
        })?;
    } else {
        print!("{rendered}");
    }

    // 8. Drain accumulated dual-path warnings (T5.2 contract).
    drain_and_emit_warnings(&warnings_handle, quiet);

    Ok(())
}

/// Drain accumulated dual-path warnings from the shared handle and
/// emit each one as a single `agentprof: warn: …` line on stderr.
///
/// Mirrors [`crate::cmd::list`]'s implementation so both commands
/// surface divergences in the same format (M2.1 spec §7.3). A no-op
/// when `quiet` is `true` or no divergences were collected (the typical
/// `--no-cache` case).
fn drain_and_emit_warnings(
    handle: &agentprof_cli::data_source_factory::WarningsHandle,
    quiet: bool,
) {
    let warnings = {
        let mut guard = handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *guard)
    };
    if quiet || warnings.is_empty() {
        return;
    }
    for w in &warnings {
        eprintln!(
            "agentprof: warn: session {}: {} fields differ ({}); using adapter; will re-upsert",
            w.session_id,
            w.differing_fields.len(),
            w.differing_fields.join(", "),
        );
    }
}

/// Render the aggregate waste report as Markdown.
///
/// Produces a generation-stamped header followed by:
///
/// - **Summary** — sessions / servers / never-called counts, the
///   M1.6.6 `≈X wasted tokens` total, and (when at least one server
///   shows non-zero token waste) a `Largest waste: <server>, ≈X tokens
///   across N sessions` line.
/// - **Always unused** table — truncated to `top` rows; columns
///   `Tool | Server | Sessions loaded | Tokens (per session)`. The
///   server name is recovered from the same `per_server` walk that
///   feeds `sessions_loaded`; the token count comes from `tool_tokens`
///   built across the per-session reports (see [`run`]).
/// - **Per-server cross-session** table — one row per server with
///   `Sessions loaded | Sessions w/0 calls | Calls (sum) |
///   Wasted tokens`. Wasted tokens is the sum of
///   `McpServerWaste.unused_tokens` rolled up by `aggregate_waste`.
///
/// Token figures are prefixed with `≈` because v0.1.x aggregates a
/// mix of heuristic and sidecar-derived per-session sums into a single
/// scalar; per-session provenance is preserved in `--export json`.
fn render_md(
    a: &agentprof_core::model::AggregateWasteReport,
    top: usize,
    tool_tokens: &BTreeMap<String, u64>,
) -> String {
    use crate::cmd::format::md::format_int;
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
        "- {} tools NEVER called across ANY session",
        a.never_called_tools.len()
    );
    // M1.6.6 T4.1 — total wasted tokens (≈ because heuristic / sidecar
    // mix is folded; per-session token_provenance is in --export json).
    let _ = writeln!(
        out,
        "- ≈{} wasted tokens (heuristic / sidecar / mixed — see `--export json` for per-session provenance)",
        format_int(a.total_unused_tokens),
    );
    if let Some(top_server) = a
        .per_server
        .iter()
        .filter(|s| s.total_unused_tokens > 0)
        .max_by_key(|s| s.total_unused_tokens)
    {
        let _ = writeln!(
            out,
            "- Largest waste: `{}` server, ≈{} tokens across {} sessions",
            top_server.server,
            format_int(top_server.total_unused_tokens),
            top_server.sessions_loaded,
        );
    }
    let _ = writeln!(out);

    if !a.never_called_tools.is_empty() {
        let _ = writeln!(
            out,
            "## \"Always unused\" — {} tools (top {})\n",
            a.never_called_tools.len(),
            top.min(a.never_called_tools.len())
        );
        let _ = writeln!(
            out,
            "| Tool | Server | Sessions loaded | Tokens (per session) |"
        );
        let _ = writeln!(
            out,
            "|------|--------|----------------:|---------------------:|"
        );
        // tool_name → (server, sessions_loaded)
        let mut lookup: std::collections::HashMap<&str, (&str, usize)> =
            std::collections::HashMap::new();
        for sw in &a.per_server {
            for t in &sw.tool_usage {
                lookup.insert(
                    t.tool_name.as_str(),
                    (sw.server.as_str(), t.sessions_loaded),
                );
            }
        }
        for tname in a.never_called_tools.iter().take(top) {
            let (server, n) = lookup
                .get(tname.as_str())
                .copied()
                .unwrap_or(("(unknown)", 0));
            let tokens = tool_tokens.get(tname).copied().unwrap_or(0);
            let _ = writeln!(
                out,
                "| {tname} | {server} | {n} | ≈{} |",
                format_int(tokens)
            );
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Per-server cross-session\n");
    let _ = writeln!(
        out,
        "| Server | Sessions loaded | Sessions w/0 calls | Calls (sum) | Wasted tokens |"
    );
    let _ = writeln!(
        out,
        "|--------|----------------:|-------------------:|------------:|--------------:|"
    );
    for sw in &a.per_server {
        let total_calls: usize = sw.tool_usage.iter().map(|t| t.total_call_count).sum();
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | ≈{} |",
            sw.server,
            sw.sessions_loaded,
            sw.sessions_with_zero_calls,
            total_calls,
            format_int(sw.total_unused_tokens),
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
/// M1.6.6 T4.1: extended with the cross-session token totals
/// (`total_unused_tokens` + per-server `wasted_tokens`) and a
/// per-tool token column in the "Always unused" table.
///
/// # Errors
///
/// Returns `askama::Error` (boxed into `anyhow::Error`) if template
/// rendering fails — typically only a programming error (mismatched
/// field name vs. template variable).
fn render_html(
    a: &agentprof_core::model::AggregateWasteReport,
    top: usize,
    tool_tokens: &BTreeMap<String, u64>,
) -> anyhow::Result<String> {
    use askama::Template;

    #[derive(Template)]
    #[template(path = "mcp_waste_full.html.jinja", escape = "html")]
    struct McpWasteFullTpl<'a> {
        sessions: usize,
        servers_count: usize,
        never_called_count: usize,
        top: usize,
        total_unused_tokens: u64,
        largest_waste: Option<LargestWasteRow<'a>>,
        never_called_top: Vec<NeverCalledRow<'a>>,
        per_server: Vec<PerServerRow<'a>>,
    }
    struct NeverCalledRow<'a> {
        tool: &'a str,
        server: &'a str,
        sessions_loaded: usize,
        tokens: u64,
    }
    struct PerServerRow<'a> {
        server: &'a str,
        sessions_loaded: usize,
        sessions_with_zero_calls: usize,
        total_calls: usize,
        wasted_tokens: u64,
    }
    struct LargestWasteRow<'a> {
        server: &'a str,
        wasted_tokens: u64,
        sessions_loaded: usize,
    }

    // tool_name → (server, sessions_loaded)
    let mut lookup: std::collections::HashMap<&str, (&str, usize)> =
        std::collections::HashMap::new();
    for sw in &a.per_server {
        for t in &sw.tool_usage {
            lookup.insert(
                t.tool_name.as_str(),
                (sw.server.as_str(), t.sessions_loaded),
            );
        }
    }
    let never_called_top: Vec<NeverCalledRow> = a
        .never_called_tools
        .iter()
        .take(top)
        .map(|tname| {
            let (server, sessions_loaded) = lookup
                .get(tname.as_str())
                .copied()
                .unwrap_or(("(unknown)", 0));
            NeverCalledRow {
                tool: tname.as_str(),
                server,
                sessions_loaded,
                tokens: tool_tokens.get(tname).copied().unwrap_or(0),
            }
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
            wasted_tokens: sw.total_unused_tokens,
        })
        .collect();
    let largest_waste = a
        .per_server
        .iter()
        .filter(|s| s.total_unused_tokens > 0)
        .max_by_key(|s| s.total_unused_tokens)
        .map(|s| LargestWasteRow {
            server: s.server.as_str(),
            wasted_tokens: s.total_unused_tokens,
            sessions_loaded: s.sessions_loaded,
        });

    let tpl = McpWasteFullTpl {
        sessions: a.sessions,
        servers_count: a.per_server.len(),
        never_called_count: a.never_called_tools.len(),
        top,
        total_unused_tokens: a.total_unused_tokens,
        largest_waste,
        never_called_top,
        per_server,
    };
    Ok(tpl.render()?)
}

/// Store-mode aggregate waste over all sessions in a `since` window.
///
/// Heuristic-only — no sidecar, no MCP config. Token counts use the
/// per-session tokenizer inferred from the dominant model and the
/// default `tokens_per_tool` heuristic
/// ([`agentprof_core::analyzer::waste::DEFAULT_HEURISTIC_TOKENS`]).
/// The dashboard surfaces a banner clarifying this; the full CLI
/// command [`run`] retains the sidecar + `mcp.json` plumbing required
/// for exact counts.
///
/// Per-session `load_session` failures are logged and skipped; only
/// when *every* session fails to load do we surface an error.
///
/// A single shared [`agentprof_core::analyzer::waste::build_bpe`]
/// encoder per [`agentprof_core::model::TokenizerKind`] is built once
/// and reused across all sessions to avoid the per-call BPE setup
/// cost.
///
/// # Errors
///
/// - [`ExitKind::DataError`] when the `query_sessions_since` query
///   fails, or when every session in the window failed to load.
///
/// # Examples
///
/// ```ignore
/// # use std::time::Duration;
/// # fn demo(db: &agentprof_storage::Db) -> anyhow::Result<()> {
/// // cli is bin-only; this helper is consumed by the dashboard
/// // handlers in `cmd::serve::handlers`.
/// let agg = agentprof_cli::cmd::mcp_waste::compute_aggregate_waste_from_store(
///     db,
///     Duration::from_secs(7 * 86_400),
/// )?;
/// assert!(agg.sessions <= usize::MAX);
/// # Ok(()) }
/// ```
pub fn compute_aggregate_waste_from_store(
    db: &agentprof_storage::Db,
    since: std::time::Duration,
) -> anyhow::Result<agentprof_core::model::AggregateWasteReport> {
    use std::sync::Arc;

    use agentprof_core::analyzer::waste::{build_bpe, infer_tokenizer, WasteComputeContext};
    use agentprof_core::analyzer::{aggregate_waste, compute_waste};
    use agentprof_core::model::{AggregateWasteReport, TokenizerKind, WasteReport};

    let now_ms = chrono::Utc::now().timestamp_millis();
    let refs = agentprof_storage::query::query_sessions_since(db, since, now_ms)
        .map_err(|e| ExitKind::DataError.into_anyhow(format!("query_sessions_since: {e}")))?;

    // Build the two possible BPE encoders once; reuse across all sessions.
    let bpe_cl100k = build_bpe(TokenizerKind::Cl100kBase).map(Arc::new);
    let bpe_o200k = build_bpe(TokenizerKind::O200kBase).map(Arc::new);

    // `aggregate_waste` takes `(adapter::SessionRef, WasteReport)`
    // pairs; the storage layer yields `datasource::SessionRef`, so we
    // build the legacy adapter ref here. The synthesised ref carries
    // only the id + a placeholder path; `aggregate_waste` reads
    // neither `path` nor `modified_at` (see the equivalent shim in
    // [`run`]'s store-mode branch).
    let mut per_session: Vec<(agentprof_core::adapter::SessionRef, WasteReport)> =
        Vec::with_capacity(refs.len());
    let mut failed: usize = 0;
    for sref in &refs {
        let report = match agentprof_storage::query::load_session(db, &sref.id) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    session_id = %sref.id,
                    error = %e,
                    "load_session failed during mcp-waste; skipping"
                );
                failed += 1;
                continue;
            }
        };
        let model_hint = crate::cmd::model_hint::dominant_model(&report);
        let tokenizer = infer_tokenizer(model_hint.as_deref());
        let mut ctx = WasteComputeContext::new(&report.loaded_mcp_tools).with_tokenizer(tokenizer);
        let shared = match tokenizer {
            TokenizerKind::Cl100kBase => bpe_cl100k.as_ref(),
            TokenizerKind::O200kBase => bpe_o200k.as_ref(),
            // `TokenizerKind` is `#[non_exhaustive]`; future variants
            // fall back to no shared encoder (per-call construction).
            _ => None,
        };
        if let Some(b) = shared {
            ctx = ctx.with_bpe(Arc::clone(b));
        }
        // No config, no sidecar — heuristic-only mode for the dashboard.
        let waste = compute_waste(&report, &ctx);
        let legacy_ref = agentprof_core::adapter::SessionRef::new(
            sref.id.clone(),
            sref.agent,
            sref.raw_path.clone().unwrap_or_default(),
            SystemTime::UNIX_EPOCH,
            0,
            false,
        );
        per_session.push((legacy_ref, waste));
    }

    if per_session.is_empty() && failed > 0 {
        return Err(ExitKind::DataError
            .into_anyhow(format!("all {failed} session(s) failed to load from store")));
    }
    let agg: AggregateWasteReport = aggregate_waste(&per_session);
    Ok(agg)
}
