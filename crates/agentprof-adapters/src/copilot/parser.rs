//! Parser: `events.jsonl` → `RawSession<CopilotEvent>`.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use agentprof_core::adapter::{AdapterError, AgentKind};
use agentprof_core::error::ParseWarning;
use agentprof_core::model::meta::SessionMeta;
use agentprof_core::model::session::RawSession;

use crate::copilot::event::CopilotEvent;

/// Parse a Copilot `events.jsonl` file into a [`RawSession`].
///
/// Parameters:
/// - `path`: filesystem path to the `events.jsonl` file
/// - `is_live`: `true` if an `inuse.<pid>.lock` was observed for this session
///   (suppresses warnings about the incomplete trailing line)
///
/// # Errors
///
/// - [`AdapterError::Io`] if the file cannot be opened or read at the byte level
/// - [`AdapterError::MissingSessionStart`] if no `session.start` event is found
///
/// Per-line JSON parse failures are NOT errors — they accumulate in
/// [`RawSession::parse_warnings`].
///
/// # Examples
///
/// ```no_run
/// use agentprof_adapters::copilot::parser::parse_events_jsonl;
///
/// fn read_events() -> Result<usize, Box<dyn std::error::Error>> {
///     let raw = parse_events_jsonl(std::path::Path::new("/tmp/events.jsonl"), false)?;
///     Ok(raw.events.len())
/// }
/// ```
#[tracing::instrument(
    name = "adapter.parse",
    skip_all,
    fields(path = %agentprof_core::observability::pii::hash_path(path))
)]
pub fn parse_events_jsonl(
    path: &Path,
    is_live: bool,
) -> Result<RawSession<CopilotEvent>, AdapterError> {
    let file = File::open(path).map_err(|source| AdapterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);

    let mut events: Vec<CopilotEvent> = Vec::with_capacity(256);
    let mut warnings: Vec<ParseWarning> = Vec::new();
    let mut meta_builder = MetaBuilder::default();

    let lines: Vec<(usize, std::io::Result<String>)> = reader.lines().enumerate().collect();
    let last_idx = lines.len().saturating_sub(1);

    for (line_no, line_result) in lines {
        match line_result {
            Err(io_err) => warnings.push(ParseWarning::Io {
                line_no,
                error: io_err.to_string(),
            }),
            Ok(line) if line.trim().is_empty() => {}
            Ok(line) => match serde_json::from_str::<CopilotEvent>(&line) {
                Ok(event) => {
                    meta_builder.absorb(&event);
                    events.push(event);
                }
                Err(parse_err) => {
                    if line_no == last_idx && is_live && looks_like_incomplete_json(&line) {
                        continue;
                    }
                    warnings.push(ParseWarning::Json {
                        line_no,
                        error: parse_err.to_string(),
                    });
                }
            },
        }
    }

    if !is_monotonic(&events) {
        warnings.push(ParseWarning::OutOfOrder);
    }

    let meta =
        meta_builder
            .build(is_live, path)
            .ok_or_else(|| AdapterError::MissingSessionStart {
                path: path.to_path_buf(),
            })?;

    tracing::debug!(
        events = events.len(),
        warnings = warnings.len(),
        "parsed events.jsonl"
    );
    Ok(RawSession::new(meta, events, warnings))
}

/// Heuristic: did parsing fail because the line was being written when we read it?
///
/// Returns `true` when the brace count is imbalanced (more `{` than `}`),
/// or when a string literal is still open at end of line. String content
/// (including escaped quotes) is excluded from brace counting so that
/// braces appearing inside JSON string values do not skew the result.
///
/// Visibility: `pub(crate)` per full-review CLI #5 — only used by the
/// sibling [`parse_events_jsonl`] for the last-line tail-truncation
/// detection during live reads. Was historically `pub` by accident.
#[must_use]
pub(crate) fn looks_like_incomplete_json(line: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for ch in line.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => depth -= 1,
            _ => {}
        }
    }
    depth > 0 || in_string
}

fn is_monotonic(events: &[CopilotEvent]) -> bool {
    let mut prev: Option<chrono::DateTime<chrono::Utc>> = None;
    for ev in events {
        let ts = ev.timestamp();
        if let Some(p) = prev {
            if ts < p {
                return false;
            }
        }
        prev = Some(ts);
    }
    true
}

/// Builds [`SessionMeta`] incrementally from observed events.
#[derive(Default)]
pub(crate) struct MetaBuilder {
    session_id: Option<String>,
    producer: Option<String>,
    agent_version: Option<String>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    cwd: Option<String>,
    repository: Option<String>,
    branch: Option<String>,
}

impl MetaBuilder {
    pub(crate) fn absorb(&mut self, event: &CopilotEvent) {
        if let CopilotEvent::SessionStart(env) = event {
            self.session_id = Some(env.data.session_id.clone());
            self.producer = Some(env.data.producer.clone());
            self.agent_version = Some(env.data.copilot_version.clone());
            self.started_at = Some(env.data.start_time);
            self.cwd = Some(env.data.context.cwd.clone());
            self.repository.clone_from(&env.data.context.repository);
            self.branch.clone_from(&env.data.context.branch);
        }
    }

    pub(crate) fn build(self, is_live: bool, path: &Path) -> Option<SessionMeta> {
        let started_at = self.started_at?;
        let id = self.session_id.unwrap_or_else(|| {
            path.parent().and_then(|p| p.file_name()).map_or_else(
                || "unknown".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            )
        });
        let mut meta = SessionMeta::new(id, AgentKind::Copilot, started_at, is_live);
        meta.producer = self.producer;
        meta.agent_version = self.agent_version;
        meta.cwd = self.cwd;
        meta.repository = self.repository;
        meta.branch = self.branch;
        Some(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_incomplete_json_detects_unclosed_brace() {
        assert!(looks_like_incomplete_json(r#"{"type":"abc","data":{"x":1"#));
        assert!(!looks_like_incomplete_json(r#"{"type":"abc","data":{}}"#));
    }

    #[test]
    fn looks_like_incomplete_json_ignores_unbalanced_braces_inside_strings() {
        assert!(!looks_like_incomplete_json(
            r#"{"k":"{value with brace inside}"}"#
        ));
    }
}
