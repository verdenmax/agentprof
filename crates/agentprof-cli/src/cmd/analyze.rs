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
use agentprof_core::episode::derive_episodes;

use crate::cmd::format;

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
    s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
}

/// Output serialization format.
#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
pub enum ExportFormat {
    /// Human-readable markdown with tables.
    Md,
    /// Machine-readable JSON (matches `AnalysisReport` serde shape).
    Json,
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
}

impl AnalysisSection {
    /// All defined sections (default for `--section`).
    #[must_use]
    pub fn all_vec() -> Vec<Self> {
        vec![Self::TurnSummary, Self::ToolRank, Self::HookRank]
    }
}

/// Exit-code hint surfaced to `main()` via `anyhow` downcast.
///
/// Mapped to process exit codes per `docs/architecture.md`:
/// - `UserError = 1` — invalid args, session not found.
/// - `DataError = 2` — adapter could not parse the session.
/// - `OutputError = 3` — failed to write to `--output` path.
#[allow(clippy::enum_variant_names)] // names spec'd in docs/architecture.md
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum ExitKind {
    /// User error: invalid args, session not found.
    #[error("user error")]
    UserError = 1,
    /// Data error: session file could not be parsed by the adapter.
    #[error("data error")]
    DataError = 2,
    /// I/O error during output write.
    #[error("output error")]
    OutputError = 3,
}

impl ExitKind {
    /// Wrap a user-facing message into an `anyhow::Error` whose downcast
    /// target is `ExitKind`. `main()`'s `classify_error` extracts this to
    /// pick the process exit code.
    pub(crate) fn into_anyhow(self, msg: String) -> anyhow::Error {
        anyhow::Error::msg(msg).context(self)
    }
}

/// Wire the analyze pipeline.
///
/// # Errors
///
/// Returns an `anyhow::Error` whose downcast target is `ExitKind`,
/// signaling which process exit code `main()` should use.
#[allow(clippy::needless_pass_by_value)] // main() owns the parsed Cli enum and moves the variant payload in.
pub fn run(cmd: AnalyzeCmd) -> Result<()> {
    let adapter = registry::adapter_for(cmd.agent).ok_or_else(|| {
        ExitKind::UserError.into_anyhow(format!("no adapter wired for agent {:?}", cmd.agent))
    })?;

    let sref = resolve_session(&adapter, cmd.root.clone(), &cmd.session)?;

    let raw = adapter.load_session(&sref).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("loading session {}: {e}", sref.path.display()))
    })?;

    let episodes = derive_episodes(&raw.events, &raw.meta);
    let report = analyze(&episodes, &raw.meta);

    let rendered = render_report(&report, &cmd)?;
    write_output(&rendered, cmd.output.as_deref())?;
    Ok(())
}

#[allow(clippy::trivially_copy_pass_by_ref)] // CopilotAdapter is a unit struct today but the Adapter trait API takes &self.
fn resolve_session(
    adapter: &CopilotAdapter,
    root: Option<PathBuf>,
    sel: &SessionSelector,
) -> Result<SessionRef> {
    match sel {
        SessionSelector::Path(p) => resolve_session_by_path(adapter, p),
        SessionSelector::Latest | SessionSelector::Previous | SessionSelector::Uuid(_) => {
            resolve_session_by_discovery(adapter, root, sel)
        }
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // CopilotAdapter is a unit struct today but the Adapter trait API takes &self.
fn resolve_session_by_path(adapter: &CopilotAdapter, p: &Path) -> Result<SessionRef> {
    let path = if p.is_file() {
        p.to_path_buf()
    } else {
        p.join("events.jsonl")
    };
    if !path.is_file() {
        return Err(ExitKind::UserError.into_anyhow(format!(
            "session events.jsonl not found at {} (and {} is not a file)",
            path.display(),
            p.display()
        )));
    }
    let meta = std::fs::metadata(&path).ok();
    let modified_at = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let size_bytes = meta.map_or(0, |m| m.len());
    let id = path.parent().and_then(|d| d.file_name()).map_or_else(
        || "unknown".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
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

fn render_report(report: &AnalysisReport, cmd: &AnalyzeCmd) -> Result<String> {
    match cmd.export {
        ExportFormat::Md => Ok(format::md::render(report, &cmd.section)),
        ExportFormat::Json => format::json::render(report)
            .map_err(|e| ExitKind::OutputError.into_anyhow(format!("json render: {e}"))),
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
            eprintln!("wrote {} bytes to {}", content.len(), p.display());
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
}
