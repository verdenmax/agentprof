//! `agentprof analyze` subcommand.
//!
//! Resolves a session per the [`SessionSelector`], loads its events via
//! the adapter, derives Episodes, runs the 3 analyzers, then renders to
//! md or json. Task 9 lands the arg parsing + session-selector parsing;
//! Task 10 wires the full run flow.

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;
use clap::{Args, ValueEnum};

use agentprof_core::adapter::AgentKind;

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
#[allow(dead_code)] // payloads consumed by Task 10
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
/// - `UserError = 1` — invalid args, session not found
/// - `DataError = 2` — adapter could not parse the session
/// - `OutputError = 3` — failed to write to `--output` path
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[allow(dead_code)] // constructed by Task 10's real run() flow
#[allow(clippy::enum_variant_names)] // intentional `*Error` naming per CLI spec
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

/// Stub `run()` — wired in Task 10. Returns `Ok(())` after printing a
/// reminder to stderr so the CLI is invocable end-to-end.
///
/// # Errors
///
/// Currently never errors. Task 10 will wire the real flow which may
/// return `ExitKind`-tagged errors via `anyhow`.
#[allow(clippy::unnecessary_wraps)] // stable signature; Task 10 returns real errors
pub fn run(_cmd: AnalyzeCmd) -> Result<()> {
    eprintln!("agentprof analyze: Task 10 will wire the main flow");
    Ok(())
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
        let s = SessionSelector::from_str("/var/sess/events.jsonl").unwrap();
        match s {
            SessionSelector::Path(p) => assert_eq!(p, PathBuf::from("/var/sess/events.jsonl")),
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
}
