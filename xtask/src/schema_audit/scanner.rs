//! Session scanner: iterate sessions under a root and dual-parse each line
//! as both raw JSON and typed [`CopilotEvent`].
//!
//! Reading the raw JSON in parallel with the typed enum lets `classifier`
//! reach into a [`CopilotEvent::Unknown`] value and recover its original
//! `type` field for grouping.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use agentprof_adapters::copilot::{CopilotAdapter, CopilotEvent};
use agentprof_core::adapter::{Adapter, SessionRef};
use agentprof_core::model::RawSession;

/// Per-session audit data — both typed + raw views of every line.
#[derive(Debug)]
pub struct SessionAudit {
    /// Discovered session reference (id, path, mtime, ...).
    pub sref: SessionRef,
    /// Agent version extracted from `session.start` (if any).
    pub agent_version: Option<String>,
    /// Raw JSON view of every non-empty, JSON-parseable line.
    pub raw_lines: Vec<RawLine>,
    /// Typed [`CopilotEvent`] view of the same session.
    pub typed: RawSession<CopilotEvent>,
}

/// A single line as raw JSON, paired with its line number for diagnostics.
#[derive(Debug)]
pub struct RawLine {
    /// 1-based line number within `events.jsonl`.
    #[allow(dead_code)]
    pub line_no: usize,
    /// Parsed raw JSON value.
    pub value: Value,
}

/// Discover and scan sessions.
///
/// - `root`: directory containing `<uuid>/events.jsonl`.
/// - `sample_limit`: cap on most-recent sessions; `None` = scan all.
/// - `session_filter`: if non-empty, only sessions whose `id` is in this set.
///
/// # Errors
///
/// Returns an error if `root` cannot be enumerated by the adapter or if any
/// matched session's file cannot be read.
pub fn scan(
    root: &Path,
    sample_limit: Option<usize>,
    session_filter: &[String],
) -> Result<Vec<SessionAudit>> {
    let adapter = CopilotAdapter;
    let mut sessions = adapter
        .discover_sessions(root)
        .with_context(|| format!("discovering sessions under {}", root.display()))?;
    if !session_filter.is_empty() {
        let allowed: std::collections::HashSet<&str> =
            session_filter.iter().map(String::as_str).collect();
        sessions.retain(|s| allowed.contains(s.id.as_str()));
    }
    if let Some(limit) = sample_limit {
        sessions.truncate(limit);
    }

    let mut out = Vec::with_capacity(sessions.len());
    for sref in sessions {
        let raw_lines = read_raw_lines(&sref.path)
            .with_context(|| format!("reading raw lines from {}", sref.path.display()))?;
        let typed = adapter
            .load_session(&sref)
            .with_context(|| format!("typed-loading session {}", sref.path.display()))?;
        let agent_version = typed.meta.agent_version.clone();
        out.push(SessionAudit {
            sref,
            agent_version,
            raw_lines,
            typed,
        });
    }
    Ok(out)
}

fn read_raw_lines(path: &Path) -> Result<Vec<RawLine>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            out.push(RawLine {
                line_no: idx + 1,
                value: v,
            });
        }
        // Un-parsable lines are recorded as ParseWarning::Json in the typed pass.
    }
    Ok(out)
}

/// Pull the wire-format `type` field from a raw line, if present.
#[must_use]
pub fn raw_type(v: &Value) -> Option<&str> {
    v.get("type").and_then(Value::as_str)
}

/// Extract an ISO-8601 timestamp from a raw line, if present.
///
/// Currently unused by `classifier` / `report`; kept public for future
/// time-series analyses (e.g. inter-event gap detection).
#[must_use]
#[allow(dead_code)]
pub fn raw_timestamp(v: &Value) -> Option<DateTime<Utc>> {
    v.get("timestamp")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn read_raw_lines_skips_empty_and_invalid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("events.jsonl");
        std::fs::write(
            &path,
            "{\"type\":\"session.start\",\"id\":\"a\"}\n\n{ not json\n{\"type\":\"shutdown\",\"id\":\"b\"}\n",
        )
        .unwrap();
        let lines = read_raw_lines(&path).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(raw_type(&lines[0].value), Some("session.start"));
        assert_eq!(raw_type(&lines[1].value), Some("shutdown"));
    }
}
