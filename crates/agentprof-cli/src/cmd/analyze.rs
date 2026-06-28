//! `agentprof analyze` subcommand.
//!
//! Resolves a session per the [`SessionSelector`], loads its events via
//! the adapter, derives Episodes, runs the 3 analyzers, then renders to
//! md or json.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use anyhow::{Context as _, Result};
use clap::{Args, ValueEnum};

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_adapters::registry;
use agentprof_core::adapter::{Adapter, AgentKind, SessionRef};
use agentprof_core::analyzer::{analyze, AnalysisReport};
use agentprof_core::episode::{derive_episodes, Episodes};
use agentprof_core::model::SessionMeta;

use crate::cmd::format;
use crate::cmd::mcp_waste::resolve_mcp_config_path;

/// Arguments for `agentprof analyze`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct AnalyzeCmd {
    /// Agent whose session to analyze. M1.4 supports only `copilot`;
    /// `claude` / `codex` are reserved for Phase 2 / 3.
    #[arg(long, value_enum, default_value_t = AgentKind::Copilot)]
    pub agent: AgentKind,

    /// Which session to load. Use `latest`, `previous`, a UUID, or an
    /// explicit path to an `events.jsonl` file (or its parent directory).
    #[arg(long, default_value = "latest")]
    pub session: SessionSelector,

    /// Custom session-state root directory. Defaults to the adapter's
    /// own convention (e.g. `~/.copilot/session-state/` for Copilot).
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = ExportFormat::Md)]
    pub export: ExportFormat,

    /// Write to file instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Which sections of the report to include in `--export md`.
    /// `--export json` always includes every section regardless of this flag.
    #[arg(long, value_enum, value_delimiter = ',', default_values_t = AnalysisSection::all_vec())]
    pub section: Vec<AnalysisSection>,

    /// Heuristic token cost per MCP tool when no sidecar is provided.
    /// Default 200 (≈ description + small inputSchema). M1.6.6.
    #[arg(long, default_value_t = agentprof_core::analyzer::waste::DEFAULT_HEURISTIC_TOKENS)]
    pub tokens_per_tool: u64,

    /// Optional sidecar path with per-tool descriptions for exact token counts.
    /// Path → file: global JSON `{"<server>": [<ToolEntry>, ...]}`.
    /// Path → dir: per-server `<server>.json` files (MCP `tools/list` shape).
    /// Absent → heuristic-only mode. M1.6.6.
    #[arg(long, value_name = "PATH")]
    pub tool_descriptions: Option<PathBuf>,

    /// Redact PII from the report before rendering. `none` (default) =
    /// no change. `redact` strips 🔴 HIGH fields; `anonymize` also writes
    /// an `agentprof-redaction-map.json` sidecar. See docs/features/privacy.md.
    #[arg(long, value_enum, default_value_t = agentprof_core::analyzer::redact::PrivacyLevel::None)]
    pub privacy: agentprof_core::analyzer::redact::PrivacyLevel,
}

/// How the CLI selects which session to analyze.
#[derive(Debug, Clone)]
pub enum SessionSelector {
    /// Most-recently-modified session.
    Latest,
    /// Second-most-recently-modified session.
    Previous,
    /// Explicit session UUID; looked up via `Adapter::discover_sessions`.
    Uuid(String),
    /// Explicit path to an `events.jsonl` file (or its parent directory).
    Path(PathBuf),
}

impl FromStr for SessionSelector {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "latest" => Ok(Self::Latest),
            "previous" => Ok(Self::Previous),
            _ if s.contains('/') || s.contains('\\') => Ok(Self::Path(PathBuf::from(s))),
            _ if looks_like_uuid(s) => Ok(Self::Uuid(s.to_string())),
            other => Err(format!(
                "unrecognized session selector: {other:?} \
                 (use 'latest', 'previous', a UUID like \
                 '00000000-0000-0000-0000-000000000001', or a filesystem path)"
            )),
        }
    }
}

fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Output serialization format.
#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
pub enum ExportFormat {
    /// Human-readable markdown with tables.
    Md,
    /// Machine-readable JSON (matches `AnalysisReport` serde shape).
    Json,
    /// Interactive ratatui TUI (3 views: Flamegraph / Roi / Aggregate).
    /// Requires a TTY on stdout; otherwise exits with `OutputError` (3).
    /// `--output` and `--section` are ignored when `--export tui`.
    Tui,
    /// Speedscope evented JSON profile (M1.6.4). Upload to
    /// <https://speedscope.app> for interactive exploration. `--section`
    /// is ignored (Speedscope is a single surface).
    Speedscope,
    /// Self-contained static HTML report (M1.6.4) with embedded SVG
    /// flamegraph; no JS. Best with `--output report.html`.
    Html,
}

/// Sections of the report that can be enabled/disabled.
#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
pub enum AnalysisSection {
    /// Per-Turn summary table.
    TurnSummary,
    /// Per-tool ranking table.
    ToolRank,
    /// Per-hook ranking table.
    HookRank,
    /// MCP server waste — loaded-but-uncalled tools and fully-unused
    /// servers (M1.6.5). Opt-in only via `--section mcp-waste`; md /
    /// json / html renderers wired in T3.2. Intentionally excluded
    /// from [`Self::all_vec`] so the default `analyze` output stays
    /// byte-identical to the pre-M1.6.5 baseline.
    McpWaste,
}

impl AnalysisSection {
    /// All defined sections (default for `--section`).
    ///
    /// Note: [`Self::McpWaste`] is intentionally excluded so default
    /// `analyze` output stays byte-identical to the pre-M1.6.5
    /// baseline; users opt in via `--section mcp-waste`.
    #[must_use]
    pub fn all_vec() -> Vec<Self> {
        vec![Self::TurnSummary, Self::ToolRank, Self::HookRank]
    }
}

/// Exit-code hint surfaced to `main()` via `anyhow` downcast.
///
/// Process exit-code taxonomy.
///
/// **Re-exported from [`crate::cmd::exit`]** — moved out of this
/// module per full-review CLI #10 (`exitkind-location`). Kept here as
/// a `pub use` to avoid breaking external callers that imported it
/// from `cmd::analyze`. New code should import via
/// `crate::cmd::ExitKind` or `crate::cmd::exit::ExitKind`.
pub use crate::cmd::exit::ExitKind;

/// Wire the analyze pipeline.
///
/// After the in-memory `AnalysisReport` is produced, the report is
/// write-through-cached into the `SQLite` store via
/// [`agentprof_storage::upsert::upsert_report`] unless `no_cache` is set
/// (M2.1 T5.3). Persistence failures are logged at `warn` and do **not**
/// affect the command's exit status — stdout output stays unchanged.
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is `ExitKind`,
/// signaling which process exit code `main()` should use.
#[allow(clippy::needless_pass_by_value)] // main() owns the parsed Cli enum and moves the variant payload in.
pub fn run(
    cmd: AnalyzeCmd,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
    no_cache: bool,
    storage_path: Option<PathBuf>,
) -> Result<()> {
    // Compute a redacted span field for the session selector. The `Path`
    // variant otherwise Display-formats the raw filesystem path, which
    // is PII (rubber-duck Critical #2). We redact it via
    // [`agentprof_core::observability::pii::hash_path`], which itself
    // honours `AGENTPROF_LOG_FULL_PATHS=1` at every emission layer
    // (M1.6.4 final-review follow-up — see CHANGELOG entry
    // `m1.6.4-final-followup-full-paths-l2-l3-gap`).
    let session_field = match &cmd.session {
        SessionSelector::Path(p) => agentprof_core::observability::pii::hash_path(p),
        SessionSelector::Latest => "latest".to_string(),
        SessionSelector::Previous => "previous".to_string(),
        SessionSelector::Uuid(u) => u.clone(),
    };
    // Manual `info_span!` (NOT `#[tracing::instrument]`) so the runtime
    // `cfg`-aware redaction above can populate `session`.
    let _cmd_span = tracing::info_span!(
        "cmd.analyze",
        agent = "copilot",
        export = ?cmd.export,
        session = %session_field,
    )
    .entered();

    let adapter = registry::adapter_for(cmd.agent).ok_or_else(|| {
        ExitKind::UserError.into_anyhow(format!(
            "{:?} adapter not yet implemented (M1.4 ships copilot only; \
             claude and codex are on the M1.5+ roadmap — see docs/plan.md)",
            cmd.agent
        ))
    })?;

    let sref = resolve_session(&adapter, cmd.root.clone(), &cmd.session)?;

    let raw = adapter.load_session(&sref).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("loading session {}: {e}", sref.path.display()))
    })?;

    let episodes = derive_episodes(&raw.events, &raw.meta);
    let report = analyze(&episodes, &raw.meta, &raw.parse_warnings);

    // M2.1 T5.3 — write-through the freshly-computed `AnalysisReport`
    // into the SQLite cache. Side-effect only: failures are downgraded
    // to `warn` so a broken cache never blocks the user-facing render
    // and exit code stays driven by the analyzer pipeline.
    if !no_cache {
        write_through_report(&report, &episodes, &sref.path, storage_path);
    }

    // M1.6.5 T3.2 + T5.3 — compute MCP waste when:
    //   - explicitly opted in via `--section mcp-waste` (md / json / html
    //     renderers surface it), OR
    //   - `--export tui` is selected (the McpWaste view at key `5` needs
    //     it; computed BEFORE entering the alt-screen so there's no IO
    //     inside the TUI loop, matching the M1.4 "compute then display"
    //     shape).
    // Skipped entirely otherwise so the default analyze path remains
    // byte-identical to the pre-M1.6.5 baseline.
    let waste = if cmd.section.contains(&AnalysisSection::McpWaste)
        || cmd.export == ExportFormat::Tui
    {
        // M2.1 T5.2.5: the ever-loaded MCP tool set is now carried by
        // the analyzer pipeline inside report.loaded_mcp_tools, so we
        // no longer need to walk raw.events here. Borrowing the field
        // keeps WasteComputeContext's &BTreeSet contract intact and
        // avoids a redundant per-event re-scan.
        let wire_loaded = &report.loaded_mcp_tools;
        let mcp_config_path = resolve_mcp_config_path(None)?;
        let parsed_cfg = agentprof_adapters::copilot::load_mcp_config(&mcp_config_path);
        let config_loaded = parsed_cfg.as_ref().map(|c| {
            c.servers
                .iter()
                .filter_map(|(name, info)| info.tools.as_ref().map(|t| (name.clone(), t.clone())))
                .collect::<std::collections::BTreeMap<_, _>>()
        });
        // Build the WasteComputeContext (M1.6.6 T1.4 + T3.1 + audit B1):
        // pick the *dominant* model (largest token total) — not the
        // BTreeMap's alphabetically-smallest key — to drive tokenizer
        // selection. Mixed-model sessions otherwise mis-route to the
        // wrong encoder; see `cmd::model_hint::dominant_model`.
        let model_hint = crate::cmd::model_hint::dominant_model(&report);
        let tokenizer = agentprof_core::analyzer::waste::infer_tokenizer(model_hint.as_deref());

        let sidecar = if let Some(path) = cmd.tool_descriptions.as_deref() {
            Some(
                agentprof_adapters::copilot::tool_sidecar::load_sidecar(path).map_err(|e| {
                    ExitKind::UserError
                        .into_anyhow(format!("sidecar load failed for {}: {e}", path.display()))
                })?,
            )
        } else {
            None
        };

        // M1.6.6 audit A1: build the tokenizer once per command, share via
        // Arc so per-session contexts reuse the same encoder instead of
        // re-parsing the embedded merge table each call.
        let bpe = agentprof_core::analyzer::waste::build_bpe(tokenizer).map(std::sync::Arc::new);

        let mut waste_ctx = agentprof_core::analyzer::waste::WasteComputeContext::new(wire_loaded)
            .with_tokenizer(tokenizer)
            .with_heuristic(cmd.tokens_per_tool);
        if let Some(b) = bpe.as_ref() {
            waste_ctx = waste_ctx.with_bpe(b.clone());
        }
        if let Some(c) = config_loaded.as_ref() {
            waste_ctx = waste_ctx.with_config(c);
        }
        if let Some(s) = sidecar.as_ref() {
            waste_ctx = waste_ctx.with_sidecar(s);
        }
        Some(agentprof_core::analyzer::compute_waste(&report, &waste_ctx))
    } else {
        None
    };

    if cmd.export == ExportFormat::Tui {
        // Polish #4: warn (don't error) if user passed flags that the TUI
        // dispatch ignores. Documented in ExportFormat::Tui doc-comment
        // but worth surfacing at runtime so silent ignore doesn't confuse.
        if cmd.output.is_some() {
            tracing::warn!(flag = "--output", with = "--export tui", "flag ignored");
        }
        if cmd.section != AnalysisSection::all_vec() {
            tracing::warn!(flag = "--section", with = "--export tui", "flag ignored");
        }
        if cmd.privacy != agentprof_core::analyzer::redact::PrivacyLevel::None {
            tracing::warn!(flag = "--privacy", with = "--export tui", "flag ignored");
        }
        return run_tui(&report, &episodes, waste.as_ref(), cfg, tracing_handle);
    }

    if cmd.export == ExportFormat::Speedscope && cmd.section != AnalysisSection::all_vec() {
        tracing::warn!(
            flag = "--section",
            with = "--export speedscope",
            "flag ignored"
        );
    }
    if cmd.export == ExportFormat::Html && cmd.output.is_none() {
        tracing::warn!(
            export = "html",
            "writing HTML to stdout; pass --output report.html for a saved file"
        );
    }

    // Redact AFTER the cache write-through above so the cache always stores
    // the original (real) data, never the redacted copy. The shadowed
    // `report` below is only used for rendering + the sidecar.
    let (report, redaction_map) = report.redact(cmd.privacy);

    // Part B (deferred-with-guard): warn that flamegraph frames still leak
    // original turn-ids even after the meta is redacted above. See the helper.
    warn_unredacted_flamegraph(&cmd);

    // Pass the REDACTED report's meta (not `raw.meta`) so html/speedscope render
    // the redacted session id + started_at. At `PrivacyLevel::None` it is
    // byte-identical to `raw.meta`, so non-privacy behavior is unchanged.
    let rendered = render_report(&report, &episodes, &report.meta, &cmd, waste.as_ref())?;
    write_output(&rendered, cmd.output.as_deref())?;
    crate::cmd::privacy::emit_redaction_sidecar(&redaction_map, cmd.privacy, cmd.output.as_deref())
}

/// Warn that html/speedscope flamegraph frames keep original turn-ids.
///
/// Part A redacts [`AnalysisReport::meta`], but the html/speedscope flamegraph
/// frames are built from `episodes` (un-redacted) — there is no
/// `Episodes::redact` yet — so original turn-ids and MCP server names still leak
/// into the SVG/frames. This warns (without blocking) when a privacy level is
/// active and the export is `Html` or `Speedscope`, steering fully-redacted
/// sharing to `md`/`json`.
fn warn_unredacted_flamegraph(cmd: &AnalyzeCmd) {
    if cmd.privacy != agentprof_core::analyzer::redact::PrivacyLevel::None
        && matches!(cmd.export, ExportFormat::Html | ExportFormat::Speedscope)
    {
        tracing::warn!(
            export = ?cmd.export,
            "flamegraph frames retain original turn-ids and MCP server names; episodes are not yet redacted — use --export md|json for fully-redacted sharing"
        );
    }
}

/// Write the freshly-computed `AnalysisReport` and `Episodes` into the
/// `SQLite` cache.
///
/// Pure side effect: every error path is downgraded to a `tracing::warn!`
/// so a broken cache never blocks the user-facing render. Called only
/// when `--no-cache` is unset (gate lives in the caller).
///
/// `upsert_report` runs first; on success `upsert_episodes` is called
/// to populate the M2.1.1 `episodes_json` column. If `upsert_report`
/// fails the session row never exists, so `upsert_episodes` would
/// silently no-op — skipped via the `else if` chain.
fn write_through_report(
    report: &AnalysisReport,
    episodes: &Episodes,
    raw_path: &Path,
    storage_path: Option<PathBuf>,
) {
    let storage_cfg = match agentprof_cli::config::resolve_storage_config(
        agentprof_storage::config::PartialStorageConfig::default(),
        storage_path,
    ) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!(error = %e, "storage config resolution failed; skipping write-through");
            return;
        }
    };
    match agentprof_storage::Db::open_and_migrate(&storage_cfg.path) {
        Ok(mut db) => {
            let now_secs = chrono::Utc::now().timestamp();
            if let Err(e) =
                agentprof_storage::upsert::upsert_report(&mut db, report, raw_path, now_secs)
            {
                tracing::warn!(error = %e, "write-through upsert failed (non-fatal)");
            } else if let Err(e) = agentprof_storage::upsert::upsert_episodes(
                &mut db,
                &report.meta.id,
                episodes,
                now_secs,
            ) {
                tracing::warn!(error = %e, "write-through upsert_episodes failed (non-fatal)");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "storage open failed; skipping write-through");
        }
    }
}

fn run_tui(
    report: &AnalysisReport,
    episodes: &Episodes,
    waste: Option<&agentprof_core::model::WasteReport>,
    cfg: &crate::cmd::LogConfig,
    tracing_handle: &crate::cmd::TracingHandle,
) -> Result<()> {
    use std::io::IsTerminal as _;

    // Swap the tracing writer to a rolling file BEFORE entering the
    // alt-screen so subsequent emissions don't visually corrupt the TUI.
    // The named binding holds the guard for the whole TUI session; on
    // Drop (after `terminal::leave` below) it prints the log path.
    let _log_guard = crate::observability::enter_tui_log_guard(cfg, tracing_handle);

    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err(ExitKind::OutputError.into_anyhow(
            "--export tui requires both stdin and stdout to be TTYs; pipe md or json for \
             headless use (e.g. `agentprof analyze --export md > report.md`)"
                .to_string(),
        ));
    }
    agentprof_tui::app::terminal::install_panic_hook();
    let mut term = agentprof_tui::app::terminal::enter()
        .map_err(|e| ExitKind::OutputError.into_anyhow(format!("entering tui: {e}")))?;
    // M1.6.5 T5.3: prefer the waste-aware constructor when a `WasteReport`
    // was pre-computed; fall back to the legacy `new` for non-waste
    // callers (kept for `cmd::watch` and back-compat).
    let mut runner = waste.map_or_else(
        || agentprof_tui::AppRunner::new(report, episodes),
        |w| agentprof_tui::AppRunner::new_with_waste(report, episodes, w),
    );
    let res = runner.run(&mut term);
    let _ = agentprof_tui::app::terminal::leave(&mut term);
    res.map_err(|e| ExitKind::OutputError.into_anyhow(format!("tui runtime: {e}")))
}

// `resolve_mcp_config_path` was moved to `crate::cmd::mcp_waste` in
// M1.6.5 T4.1 so the dedicated `agentprof mcp-waste` subcommand and
// `analyze --section mcp-waste` share a single implementation.

#[allow(clippy::trivially_copy_pass_by_ref)] // CopilotAdapter is a unit struct today but the Adapter trait API takes &self.
pub fn resolve_session(
    adapter: &CopilotAdapter,
    root: Option<PathBuf>,
    sel: &SessionSelector,
) -> Result<SessionRef> {
    match sel {
        SessionSelector::Path(p) => {
            // CLI #4: --root is silently ignored when --session names a
            // concrete path (the path is the session, root would only
            // matter for discovery). Mirror the existing
            // "--output ignored with --export tui" warning pattern so
            // users with a typo'd combo aren't left guessing why
            // --root has no effect.
            if root.is_some() {
                tracing::warn!(
                    flag = "--root",
                    with = "--session <PATH>",
                    "flag ignored (session path bypasses root discovery)"
                );
            }
            resolve_session_by_path(adapter, p)
        }
        SessionSelector::Latest | SessionSelector::Previous | SessionSelector::Uuid(_) => {
            resolve_session_by_discovery(adapter, root, sel)
        }
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // CopilotAdapter is a unit struct today but the Adapter trait API takes &self.
fn resolve_session_by_path(adapter: &CopilotAdapter, p: &Path) -> Result<SessionRef> {
    // Resolve the user-supplied path to a concrete events.jsonl file:
    //   - existing file → use as-is
    //   - existing directory → join "events.jsonl"
    //   - non-existent path ending in ".jsonl" → treat as the (missing) file
    //     itself; otherwise treat as a (missing) session directory and look
    //     for a child events.jsonl. This avoids the misleading double-tail
    //     error "events.jsonl not found at /x/events.jsonl/events.jsonl".
    let path = if p.is_file() {
        p.to_path_buf()
    } else if p.is_dir() {
        p.join("events.jsonl")
    } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
        p.to_path_buf()
    } else {
        p.join("events.jsonl")
    };
    if !path.is_file() {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "session events.jsonl not found at {}",
            path.display()
        )));
    }
    let meta = std::fs::metadata(&path).ok();
    if meta.is_none() {
        // CLI #8: previously fell back to UNIX_EPOCH / size 0 silently.
        // Surface as warn so users know the displayed sort-by-mtime and
        // size_bytes values are placeholders, not the real file's stats.
        tracing::warn!(
            path = %agentprof_core::observability::pii::hash_path(&path),
            "fs metadata unavailable for resolved session path; \
             modified_at defaults to UNIX_EPOCH and size_bytes to 0"
        );
    }
    let modified_at = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let size_bytes = meta.map_or(0, |m| m.len());
    let id = agentprof_adapters::copilot::paths::extract_session_id_from_first_event(&path)
        .unwrap_or_else(|| {
            path.parent().and_then(|d| d.file_name()).map_or_else(
                || "unknown".to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        });
    Ok(SessionRef::new(
        id,
        adapter.agent_kind(),
        path,
        modified_at,
        size_bytes,
        false,
    ))
}

#[allow(clippy::trivially_copy_pass_by_ref)] // CopilotAdapter is a unit struct today but the Adapter trait API takes &self.
fn resolve_session_by_discovery(
    adapter: &CopilotAdapter,
    root: Option<PathBuf>,
    sel: &SessionSelector,
) -> Result<SessionRef> {
    let actual_root = root
        .or_else(|| adapter.default_session_root())
        .ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow("could not determine session root (set --root or HOME)".to_string())
        })?;
    if !actual_root.is_dir() {
        return Err(ExitKind::UserError
            .into_anyhow(format!("session root not found: {}", actual_root.display())));
    }
    let sessions = adapter
        .discover_sessions(&actual_root)
        .with_context(|| format!("scanning {}", actual_root.display()))?;

    match sel {
        SessionSelector::Latest => sessions.into_iter().next().ok_or_else(|| {
            ExitKind::UserError
                .into_anyhow(format!("no sessions found under {}", actual_root.display()))
        }),
        SessionSelector::Previous => {
            let mut iter = sessions.into_iter();
            let _latest = iter.next().ok_or_else(|| {
                ExitKind::UserError
                    .into_anyhow(format!("no sessions found under {}", actual_root.display()))
            })?;
            iter.next().ok_or_else(|| {
                ExitKind::UserError
                    .into_anyhow("no previous session: only 1 session present".to_string())
            })
        }
        SessionSelector::Uuid(u) => {
            let target = u.clone();
            if let Some(sref) = sessions.iter().find(|s| s.id == target).cloned() {
                return Ok(sref);
            }
            let head: Vec<String> = sessions.iter().take(5).map(|s| s.id.clone()).collect();
            Err(ExitKind::UserError.into_anyhow(format!(
                "session UUID {target} not found under {}; first 5 available: {}",
                actual_root.display(),
                head.join(", ")
            )))
        }
        SessionSelector::Path(_) => Err(ExitKind::UserError
            .into_anyhow("internal: Path selector reached discovery path".to_string())),
    }
}

fn render_report(
    report: &AnalysisReport,
    episodes: &Episodes,
    meta: &SessionMeta,
    cmd: &AnalyzeCmd,
    mcp_waste: Option<&agentprof_core::model::WasteReport>,
) -> Result<String> {
    match cmd.export {
        ExportFormat::Md => Ok(format::md::render(report, &cmd.section, mcp_waste)),
        ExportFormat::Json => {
            // serde_json::to_string_pretty omits the trailing newline;
            // append one so the shell prompt doesn't stick to `}` on
            // stdout and so file output has a POSIX-compliant final line.
            let mut s = format::json::render(report, mcp_waste)
                .map_err(|e| ExitKind::OutputError.into_anyhow(format!("json render: {e}")))?;
            s.push('\n');
            Ok(s)
        }
        ExportFormat::Tui => Err(ExitKind::DataError
            .into_anyhow("internal: render_report called with Tui export".to_string())),
        ExportFormat::Speedscope => {
            let (mut json, warnings) =
                format::speedscope::render(episodes, meta, env!("CARGO_PKG_VERSION"));
            for w in &warnings {
                tracing::warn!(category = "speedscope", warning = %w, "render warning");
            }
            // POSIX-final-newline + visual separation, mirroring json branch.
            json.push('\n');
            Ok(json)
        }
        ExportFormat::Html => Ok(format::html::render(
            report,
            episodes,
            meta,
            &cmd.section,
            mcp_waste,
            env!("CARGO_PKG_VERSION"),
        )),
    }
}

fn write_output(content: &str, path: Option<&Path>) -> Result<()> {
    match path {
        None => {
            print!("{content}");
            Ok(())
        }
        Some(p) => {
            std::fs::write(p, content).map_err(|e| {
                ExitKind::OutputError.into_anyhow(format!("writing {}: {e}", p.display()))
            })?;
            tracing::info!(
                bytes = content.len(),
                path = %agentprof_core::observability::pii::hash_path(p),
                "wrote output file"
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_selector_parses_latest_and_previous() {
        assert!(matches!(
            SessionSelector::from_str("latest").unwrap(),
            SessionSelector::Latest
        ));
        assert!(matches!(
            SessionSelector::from_str("previous").unwrap(),
            SessionSelector::Previous
        ));
    }

    #[test]
    fn session_selector_parses_uuid() {
        let s = SessionSelector::from_str("00000000-0000-0000-0000-000000000001").unwrap();
        match s {
            SessionSelector::Uuid(u) => assert_eq!(u, "00000000-0000-0000-0000-000000000001"),
            other => panic!("expected Uuid, got {other:?}"),
        }
    }

    #[test]
    fn session_selector_parses_path_with_slash() {
        // P4 trivia `t9-tmp-path-rationale`: this test uses `/tmp/sess/...`
        // because ADR-0003 §3 (line 63) explicitly mandates `/tmp/agentprof-fixture/*`
        // for ephemeral fixture paths, and `/tmp/sess/...` is consistent with
        // that convention. During T9 / T10 review, a subagent confabulated
        // a "hard runtime rule forbidding /tmp references" — no such rule
        // exists. The path here is a purely-illustrative string for the
        // `FromStr::from_str` parse — the file does not need to exist.
        let s = SessionSelector::from_str("/tmp/sess/events.jsonl").unwrap();
        match s {
            SessionSelector::Path(p) => assert_eq!(p, PathBuf::from("/tmp/sess/events.jsonl")),
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn session_selector_rejects_garbage() {
        let err = SessionSelector::from_str("garbage").unwrap_err();
        assert!(err.contains("unrecognized session selector"));
        assert!(err.contains("latest"));
    }

    #[test]
    fn all_vec_contains_all_three_sections() {
        let v = AnalysisSection::all_vec();
        assert_eq!(v.len(), 3);
        assert!(v.contains(&AnalysisSection::TurnSummary));
        assert!(v.contains(&AnalysisSection::ToolRank));
        assert!(v.contains(&AnalysisSection::HookRank));
    }

    #[test]
    fn exit_kind_into_anyhow_carries_downcast_target() {
        let e = ExitKind::DataError.into_anyhow("test message".to_string());
        let displayed = format!("{e:#}");
        assert!(displayed.contains("test message"));
        assert!(e.downcast_ref::<ExitKind>().is_some());
        let kind = e.downcast_ref::<ExitKind>().copied().unwrap();
        assert!(matches!(kind, ExitKind::DataError));
    }

    #[test]
    fn exit_kind_downcast_survives_extra_context_layers() {
        // anyhow walks the entire context chain when downcasting, so adding
        // extra .context(...) on top of an ExitKind-tagged error must NOT
        // hide the ExitKind from `classify_error` in main.rs. This is the
        // defense against the M1.4 audit's "ExitKind downcast is a future
        // footgun" warning (audit D5): if someone later writes
        //   `cmd::analyze::run(cmd).context("processing analyze command")`
        // in main.rs's dispatcher, exit codes must still resolve correctly.
        //
        // Note: anyhow::Error has an inherent `.context()` method, so no
        // trait import is needed even though the Context trait exists.
        let e = ExitKind::OutputError
            .into_anyhow("disk full".to_string())
            .context("writing report.md")
            .context("processing analyze command");

        // Full message chain should expose all layers.
        let displayed = format!("{e:#}");
        assert!(
            displayed.contains("disk full"),
            "lost innermost message: {displayed}"
        );
        assert!(
            displayed.contains("writing report.md"),
            "lost middle context: {displayed}"
        );
        assert!(
            displayed.contains("processing analyze command"),
            "lost outermost context: {displayed}"
        );

        // Critical assertion: ExitKind is still findable via downcast.
        assert!(e.downcast_ref::<ExitKind>().is_some());
        let kind = e.downcast_ref::<ExitKind>().copied().unwrap();
        assert!(matches!(kind, ExitKind::OutputError));
    }

    #[test]
    fn looks_like_uuid_accepts_canonical_form() {
        assert!(looks_like_uuid("00000000-0000-0000-0000-000000000001"));
        assert!(looks_like_uuid("abcdef01-2345-6789-abcd-ef0123456789"));
        assert!(looks_like_uuid("ABCDEF01-2345-6789-ABCD-EF0123456789"));
    }

    #[test]
    fn looks_like_uuid_rejects_wrong_length() {
        assert!(!looks_like_uuid(""));
        assert!(!looks_like_uuid("00000000-0000-0000-0000-00000000000"));
        assert!(!looks_like_uuid("00000000-0000-0000-0000-0000000000010"));
    }

    #[test]
    fn looks_like_uuid_rejects_dashes_in_wrong_positions() {
        // 36 chars + 4 dashes but dashes mis-placed.
        assert!(!looks_like_uuid("0-000000-0000-0000-0000-00000000000-"));
    }

    #[test]
    fn looks_like_uuid_rejects_non_hex_with_correct_shape() {
        // 36 chars, 4 dashes in right slots, but contains 'g' (not hex).
        assert!(!looks_like_uuid("0000000g-0000-0000-0000-000000000001"));
        // Trailing typo.
        assert!(!looks_like_uuid("00000000-0000-0000-0000-00000000000g"));
        assert!(!looks_like_uuid("00000000-0000-0000-0000-00000000000Z"));
    }

    #[test]
    fn session_selector_rejects_uuid_shaped_non_hex() {
        // Without hex validation this would parse as Uuid and leak through
        // to discover_sessions, dumping real session IDs in the error.
        let err = SessionSelector::from_str("00000000-0000-0000-0000-00000000000g").unwrap_err();
        assert!(err.contains("unrecognized session selector"));
    }
}
