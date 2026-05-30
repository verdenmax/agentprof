//! Workspace-level error types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Errors that can occur in `agentprof-core` itself.
///
/// Adapter-side errors live in [`crate::adapter::AdapterError`].
///
/// # Examples
///
/// ```
/// use agentprof_core::error::CoreError;
/// let err = CoreError::Io {
///     path: std::path::PathBuf::from("/tmp/x"),
///     source: std::io::Error::other("boom"),
/// };
/// assert!(format!("{err}").contains("/tmp/x"));
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError {
    /// Generic I/O failure inside core.
    #[error("I/O error reading {path}: {source}")]
    Io {
        /// Offending path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Malformed JSON inside an export or fixture.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// An analyzer pre-condition was violated.
    #[error("invariant violation: {0}")]
    Invariant(String),
}

/// Non-fatal warnings collected during parsing.
///
/// Surfaced via [`crate::model::session::RawSession::parse_warnings`].
///
/// # Examples
///
/// ```
/// use agentprof_core::error::ParseWarning;
/// let w = ParseWarning::Json { line_no: 7, error: "expected `}`".into() };
/// match w {
///     ParseWarning::Json { line_no, .. } => assert_eq!(line_no, 7),
///     _ => unreachable!(),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ParseWarning {
    /// One JSONL line failed to parse as the adapter's event type.
    Json {
        /// 0-based line number.
        line_no: usize,
        /// `serde_json` error message.
        error: String,
    },
    /// One JSONL line failed to read.
    Io {
        /// 0-based line number.
        line_no: usize,
        /// I/O error message.
        error: String,
    },
    /// Events have non-monotonic timestamps in file order.
    OutOfOrder,
    /// A turn `assistant.turn_start` was not followed by a matching `turn_end`.
    UnclosedTurn {
        /// Turn ID.
        turn_id: String,
    },
    /// A `tool.execution_start` was not followed by `tool.execution_complete`.
    UnclosedToolCall {
        /// Tool call ID.
        call_id: String,
    },
    /// A `hook.start` was not followed by `hook.end`.
    UnclosedHook {
        /// Hook name.
        name: String,
    },
    /// A tool name had a `__`-separated prefix the parser doesn't recognize.
    UnknownToolSourcePrefix {
        /// Tool name whose prefix is unknown.
        tool_name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_warning_round_trip_json() {
        let w = ParseWarning::Json {
            line_no: 42,
            error: "boom".into(),
        };
        let s = serde_json::to_string(&w).unwrap();
        let back: ParseWarning = serde_json::from_str(&s).unwrap();
        match back {
            ParseWarning::Json { line_no, error } => {
                assert_eq!(line_no, 42);
                assert_eq!(error, "boom");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn core_error_display_is_informative() {
        let err = CoreError::Io {
            path: std::path::PathBuf::from("/tmp/x.jsonl"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        };
        let display = format!("{err}");
        assert!(display.contains("/tmp/x.jsonl"));
        assert!(display.contains("no such file"));
    }
}
