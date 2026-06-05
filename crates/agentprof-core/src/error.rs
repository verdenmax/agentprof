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
    ///
    /// Carries a `path` field per full-review CORE #3
    /// (`json-error-path`) — pre-fix this variant was
    /// `Json(#[from] serde_json::Error)` with no path context, so a
    /// parse failure on one of N session files would surface as a
    /// bare `"JSON error: ..."` message with no indication WHICH file
    /// caused it. Now mirrors the [`CoreError::Io`] shape.
    ///
    /// The `#[from] serde_json::Error` impl was dropped along with
    /// the variant change because there were no live call sites
    /// (audit at the same commit); future producers must construct
    /// the variant explicitly so the path is never lost.
    #[error("JSON error reading {path}: {source}")]
    Json {
        /// Offending path.
        path: PathBuf,
        /// Underlying `serde_json` error.
        #[source]
        source: serde_json::Error,
    },

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

impl std::fmt::Display for ParseWarning {
    /// Human-readable rendering for report surfaces (markdown / HTML).
    ///
    /// Mirrors the warning's structure in a single line; reports embed
    /// this directly rather than the enum's `Debug` representation so the
    /// rendered text is stable across future variant refactors.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::error::ParseWarning;
    /// let w = ParseWarning::Json { line_no: 7, error: "expected `}`".into() };
    /// assert_eq!(w.to_string(), "line 7: JSON parse error: expected `}`");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json { line_no, error } => {
                write!(f, "line {line_no}: JSON parse error: {error}")
            }
            Self::Io { line_no, error } => {
                write!(f, "line {line_no}: I/O error: {error}")
            }
            Self::OutOfOrder => f.write_str("events have non-monotonic timestamps in file order"),
            Self::UnclosedTurn { turn_id } => {
                write!(f, "turn {turn_id}: turn_start without matching turn_end")
            }
            Self::UnclosedToolCall { call_id } => {
                write!(
                    f,
                    "tool call {call_id}: execution_start without matching execution_complete"
                )
            }
            Self::UnclosedHook { name } => {
                write!(f, "hook {name}: hook.start without matching hook.end")
            }
            Self::UnknownToolSourcePrefix { tool_name } => {
                write!(
                    f,
                    "tool {tool_name}: unrecognized `__`-separated source prefix"
                )
            }
        }
    }
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

    #[test]
    fn core_error_json_carries_path_context() {
        // CORE #3 regression guard: variant must carry `path` for
        // failure-attribution when parsing one of N session files.
        // Construct a real serde_json::Error to capture both halves of
        // the Display output without faking the inner error type.
        let inner: serde_json::Error = serde_json::from_str::<u32>("not a number").unwrap_err();
        let err = CoreError::Json {
            path: std::path::PathBuf::from("/tmp/session-abc.jsonl"),
            source: inner,
        };
        let display = format!("{err}");
        assert!(
            display.contains("/tmp/session-abc.jsonl"),
            "JSON error must include the path: {display}"
        );
        assert!(
            display.contains("JSON error reading"),
            "JSON error prefix should be stable: {display}"
        );
    }

    #[test]
    fn parse_warning_display_is_human_readable() {
        let cases: &[(ParseWarning, &str)] = &[
            (
                ParseWarning::Json {
                    line_no: 7,
                    error: "expected `}`".into(),
                },
                "line 7",
            ),
            (
                ParseWarning::Io {
                    line_no: 3,
                    error: "EOF".into(),
                },
                "I/O error",
            ),
            (ParseWarning::OutOfOrder, "non-monotonic"),
            (
                ParseWarning::UnclosedTurn {
                    turn_id: "t-1".into(),
                },
                "t-1",
            ),
            (
                ParseWarning::UnclosedToolCall {
                    call_id: "c-1".into(),
                },
                "c-1",
            ),
            (ParseWarning::UnclosedHook { name: "h-1".into() }, "h-1"),
            (
                ParseWarning::UnknownToolSourcePrefix {
                    tool_name: "weird__name".into(),
                },
                "weird__name",
            ),
        ];
        for (w, needle) in cases {
            let rendered = w.to_string();
            assert!(
                rendered.contains(needle),
                "Display of {w:?} = {rendered:?} missing {needle:?}"
            );
            // No Debug syntax leaking through.
            assert!(
                !rendered.contains(" { "),
                "Display of {w:?} = {rendered:?} looks like Debug output"
            );
        }
    }
}
