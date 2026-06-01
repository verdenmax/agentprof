//! Warnings emitted by the `agentprof-core::export` module.
//!
//! Unlike [`crate::error::ParseWarning`] (parse layer) and
//! [`crate::episode::DeriveWarning`] (derive layer), `ExportWarning` is
//! produced by the export pipelines that re-shape `Episodes` into
//! external formats (e.g. Speedscope's strict-nesting requirement).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A non-fatal observation surfaced during export.
///
/// `agentprof-cli` prints these to stderr (one line each) and does not
/// change the process exit code on their account.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::ExportWarning;
/// use chrono::Utc;
///
/// let w = ExportWarning::SpanAdjustedForSpeedscope {
///     tool_name: "bash".to_string(),
///     original_start: Utc::now(),
///     adjusted_start: Utc::now(),
/// };
/// // Display impl is provided by `thiserror::Error`.
/// let msg = format!("{w}");
/// assert!(msg.contains("bash"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExportWarning {
    /// A tool span overlapped a sibling span in the same turn; Speedscope
    /// requires strict nesting, so the later span was shifted by 1 ms.
    ///
    /// Reported timing for the affected call is approximated; the warning
    /// lets the user know.
    #[error(
        "span adjusted for speedscope: tool={tool_name} original_start={original_start} \
         adjusted_start={adjusted_start}"
    )]
    SpanAdjustedForSpeedscope {
        /// Name of the tool whose span was shifted.
        tool_name: String,
        /// Originally reported `started_at`.
        original_start: DateTime<Utc>,
        /// Adjusted `started_at` actually emitted to the profile.
        adjusted_start: DateTime<Utc>,
    },
}
