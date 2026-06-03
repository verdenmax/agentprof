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

    /// A turn was still open at the end of the session and its synthetic
    /// Close had to be clamped to the start of the following turn so that
    /// Speedscope's per-stack at-monotonicity invariant holds.
    ///
    /// Without clamping the synthetic Close would land at `total_ms`
    /// (session end) and overshoot the following turn's Open, producing
    /// a profile that fails strict-nesting validation.
    #[error(
        "speedscope open-turn truncated: turn={turn_id} \
         original_at_ms={original_at} clamped_at_ms={clamped_at} \
         (next turn started before session end)"
    )]
    OpenTurnTruncated {
        /// Identifier of the open turn whose synthetic Close was clamped.
        turn_id: String,
        /// Original synthetic close timestamp (ms from session start),
        /// i.e. `total_ms`.
        original_at: i64,
        /// Clamped synthetic close timestamp (ms from session start),
        /// i.e. the next turn's start.
        clamped_at: i64,
    },

    /// The first event in the trailing `turn-orphan` section started
    /// before the last in-turn event ended; the orphan section's open
    /// timestamp was shifted forward to preserve Speedscope's per-stack
    /// at-monotonicity invariant across the boundary.
    #[error(
        "speedscope orphan time shifted: kind={orphan_kind} \
         original_at_ms={original_at} shifted_to_ms={shifted_to} \
         (orphan began before last in-turn event ended)"
    )]
    OrphanTimeShifted {
        /// Human-readable description of the first orphan (frame name).
        orphan_kind: String,
        /// Originally computed orphan start (ms from session start).
        original_at: i64,
        /// Shifted orphan start (ms from session start).
        shifted_to: i64,
    },

    /// A span's `ended_at` was earlier than its `started_at`. The
    /// duration is clamped to 0 ms for output correctness, but the
    /// warning informs the caller that a real timestamp inversion
    /// exists in the source session data (clock skew, parser bug, or
    /// out-of-order events upstream).
    #[error(
        "speedscope negative duration clamped to 0 ms: name={name} \
         started_at={started_at} ended_at={ended_at} \
         (ended_at < started_at in source data)"
    )]
    NegativeDurationClamped {
        /// Human-readable label identifying the affected span (e.g.
        /// turn id, tool frame name).
        name: String,
        /// Source `started_at` timestamp.
        started_at: DateTime<Utc>,
        /// Source `ended_at` timestamp (earlier than `started_at`).
        ended_at: DateTime<Utc>,
    },
}
