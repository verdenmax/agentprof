//! Cross-session aggregation reports (M1.6.2).
//!
//! Roll up N [`crate::analyzer::AnalysisReport`]s (one per session) into
//! a single [`AggregateReport`] keyed by one of [`AggregateKey`]'s four
//! group-by axes (tool / mcp-server / day / model).
//!
//! See `docs/internals/adr-0008-aggregate-report-and-utilization.md`
//! (added in T3) for the data model + utilization metric semantics.
//!
//! # Module map
//!
//! - [`bucket`] — 4 bucket types (one per [`AggregateKey`])
//! - [`group_by_tool`] / [`group_by_mcp`] / [`group_by_day`] /
//!   [`group_by_model`] — pure aggregator functions
//! - [`AggregateReport`] — generic data type; [`AnyAggregateReport`] —
//!   serde-tagged outer enum at the CLI/storage boundary

use chrono::Duration;
use serde::{Deserialize, Serialize};

pub mod bucket;
pub mod group_by_day;
pub mod group_by_mcp;
pub mod group_by_model;
pub mod group_by_tool;

pub use bucket::{DayBucket, McpServerBucket, ModelBucket, ToolBucket};

/// Shared session-wall helper used by every aggregator.
mod wall {
    use chrono::{DateTime, Duration, Utc};

    use crate::episode::Episodes;

    /// Wall duration of a single session = `max(last_event_ts, session_start) - session_start`.
    ///
    /// Walks `Turn.ended_at` + every `ToolCall.span.ended_at`. Clamped
    /// to non-negative.
    pub fn compute_wall(episodes: &Episodes, session_start: DateTime<Utc>) -> Duration {
        let mut latest = session_start;
        for turn in &episodes.turns {
            if let Some(end) = turn.ended_at {
                if end > latest {
                    latest = end;
                }
            }
        }
        for tool in episodes.tools.values() {
            for call in &tool.calls {
                if call.span.ended_at > latest {
                    latest = call.span.ended_at;
                }
            }
        }
        let d = latest - session_start;
        if d < Duration::zero() {
            Duration::zero()
        } else {
            d
        }
    }
}

/// Which key an [`AggregateReport`] was grouped by.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::AggregateKey;
/// assert_eq!(AggregateKey::Tool, AggregateKey::Tool);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AggregateKey {
    /// Group by tool name (`tool_rank.name`).
    Tool,
    /// Group by MCP server (the `<server>` segment of `mcp__<server>__<tool>`).
    McpServer,
    /// Group by UTC calendar date (D-9 in the M1.6.2 design).
    Day,
    /// Group by model id (D-12: first-turn model; sessions with `None`
    /// first-turn model are excluded).
    Model,
}

/// Generic cross-session aggregation report, parameterised by the
/// per-row bucket type.
///
/// `#[non_exhaustive]` — construct via [`AggregateReport::new`] from
/// outside the crate (e.g. tests).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::{AggregateKey, AggregateReport, ToolBucket};
/// use chrono::Duration;
///
/// let r: AggregateReport<ToolBucket> = AggregateReport::new(
///     AggregateKey::Tool,
///     Duration::days(30),
///     0,
///     0,
///     Duration::zero(),
///     Vec::new(),
/// );
/// assert_eq!(r.session_count, 0);
/// assert!(r.buckets.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AggregateReport<B> {
    /// Which key the rollup was grouped by.
    pub by: AggregateKey,
    /// Time window the input sessions were filtered to (informational —
    /// the aggregator itself does not filter; the CLI passes it in).
    #[serde(with = "duration_seconds")]
    pub since: Duration,
    /// Number of input [`crate::analyzer::AnalysisReport`]s.
    pub session_count: usize,
    /// Number of input sessions that failed to load or parse (reserved
    /// for T2; aggregators here always set it to `0`).
    pub failure_count: usize,
    /// Sum of per-session wall durations.
    #[serde(with = "duration_seconds")]
    pub total_wall_duration: Duration,
    /// Per-key rows. Sort order is documented on each aggregator.
    pub buckets: Vec<B>,
}

impl<B> AggregateReport<B> {
    /// Construct an [`AggregateReport`].
    ///
    /// Provided because [`AggregateReport`] is `#[non_exhaustive]`,
    /// which forbids struct-literal construction from outside the crate
    /// (notably integration tests in `tests/`).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::{AggregateKey, AggregateReport, ToolBucket};
    /// use chrono::Duration;
    ///
    /// let _r: AggregateReport<ToolBucket> = AggregateReport::new(
    ///     AggregateKey::Tool, Duration::zero(), 0, 0, Duration::zero(), Vec::new(),
    /// );
    /// ```
    #[must_use]
    pub const fn new(
        by: AggregateKey,
        since: Duration,
        session_count: usize,
        failure_count: usize,
        total_wall_duration: Duration,
        buckets: Vec<B>,
    ) -> Self {
        Self {
            by,
            since,
            session_count,
            failure_count,
            total_wall_duration,
            buckets,
        }
    }
}

/// Type-erased wrapper around the four concrete [`AggregateReport`]
/// instantiations, used at the CLI / storage / serde boundary.
///
/// Serialised with `#[serde(tag = "by", content = "data")]` so a JSON
/// payload looks like `{"by":"tool","data":{...}}`.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::{
///     AggregateKey, AggregateReport, AnyAggregateReport, ToolBucket,
/// };
/// use chrono::Duration;
///
/// let inner: AggregateReport<ToolBucket> = AggregateReport::new(
///     AggregateKey::Tool, Duration::zero(), 0, 0, Duration::zero(), Vec::new(),
/// );
/// let _any = AnyAggregateReport::Tool(inner);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "by", content = "data", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AnyAggregateReport {
    /// Grouped by tool name.
    Tool(AggregateReport<ToolBucket>),
    /// Grouped by MCP server.
    McpServer(AggregateReport<McpServerBucket>),
    /// Grouped by UTC calendar date.
    Day(AggregateReport<DayBucket>),
    /// Grouped by model id.
    Model(AggregateReport<ModelBucket>),
}

/// Serde helper: serialise [`chrono::Duration`] as integer seconds.
mod duration_seconds {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        d.num_seconds().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let s = i64::deserialize(d)?;
        Ok(Duration::seconds(s))
    }
}
