//! Bucket types — one per [`crate::analyzer::aggregate::AggregateKey`].

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::model::ToolSource;

/// Per-tool aggregated row for `--by tool` reports.
///
/// `p50_duration` / `p95_duration` are **re-computed** from the pooled
/// per-call durations across all input sessions (NOT averaged from
/// per-session percentiles — that would be statistically wrong).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::ToolBucket;
/// use agentprof_core::model::ToolSource;
/// use chrono::Duration;
///
/// let b = ToolBucket::new(
///     "bash".to_string(),
///     ToolSource::Builtin,
///     0, 0, 0,
///     Duration::zero(), Duration::zero(), Duration::zero(),
///     0,
/// );
/// assert_eq!(b.call_count, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolBucket {
    /// Tool name (e.g. `"bash"`, `"mcp__github__list_pulls"`).
    pub name: String,
    /// Provenance: built-in / MCP / skill.
    pub source: ToolSource,
    /// Sum of `tool_rank.call_count` across input sessions.
    pub call_count: usize,
    /// Sum of `tool_rank.success_count`.
    pub success_count: usize,
    /// Sum of `tool_rank.failure_count`.
    pub failure_count: usize,
    /// Sum of `tool_rank.total_duration`.
    #[serde(with = "ms_duration")]
    pub total_duration: Duration,
    /// 50th percentile, re-computed from pooled per-call durations.
    #[serde(with = "ms_duration")]
    pub p50_duration: Duration,
    /// 95th percentile, re-computed from pooled per-call durations.
    #[serde(with = "ms_duration")]
    pub p95_duration: Duration,
    /// Number of input sessions that used this tool at least once.
    pub session_count: usize,
}

impl ToolBucket {
    /// Construct a [`ToolBucket`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::ToolBucket;
    /// use agentprof_core::model::ToolSource;
    /// use chrono::Duration;
    /// let _b = ToolBucket::new(
    ///     "bash".into(), ToolSource::Builtin,
    ///     0, 0, 0,
    ///     Duration::zero(), Duration::zero(), Duration::zero(),
    ///     0,
    /// );
    /// ```
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        name: String,
        source: ToolSource,
        call_count: usize,
        success_count: usize,
        failure_count: usize,
        total_duration: Duration,
        p50_duration: Duration,
        p95_duration: Duration,
        session_count: usize,
    ) -> Self {
        Self {
            name,
            source,
            call_count,
            success_count,
            failure_count,
            total_duration,
            p50_duration,
            p95_duration,
            session_count,
        }
    }
}

/// Per-MCP-server aggregated row for `--by mcp-server` reports.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::McpServerBucket;
/// use chrono::Duration;
///
/// let b = McpServerBucket::new("github".to_string(), 0, 0, 0, Duration::zero(), 0);
/// assert_eq!(b.tool_count, 0);
/// assert_eq!(b.unused_tool_count, 0);
/// assert_eq!(b.fully_unused_session_count, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct McpServerBucket {
    /// MCP server name.
    pub server: String,
    /// Number of distinct tool names served by this server.
    pub tool_count: usize,
    /// Sum of `tool_rank.call_count` across this server's tools.
    pub call_count: usize,
    /// Sum of `tool_rank.failure_count` across this server's tools.
    pub failure_count: usize,
    /// Sum of `tool_rank.total_duration` across this server's tools.
    #[serde(with = "ms_duration")]
    pub total_duration: Duration,
    /// Number of input sessions that used this server at least once.
    pub session_count: usize,
    /// M1.6.5 — sum of per-session "unused tool" counts (tools loaded
    /// in ≥ 1 session under this server but never called in that
    /// session). The strongest "remove from `mcp.json`" signal at
    /// the per-server granularity. `#[serde(default)]` keeps
    /// pre-M1.6.5 cached JSON deserializable.
    #[serde(default)]
    pub unused_tool_count: usize,
    /// M1.6.5 — count of sessions in which this server was loaded
    /// (≥ 1 tool present in wire / `mcp.json`) but received zero
    /// tool calls. Server-level waste signal.
    /// `#[serde(default)]` keeps pre-M1.6.5 cached JSON
    /// deserializable.
    #[serde(default)]
    pub fully_unused_session_count: usize,
}

impl McpServerBucket {
    /// Construct a [`McpServerBucket`].
    ///
    /// The two M1.6.5 waste fields (`unused_tool_count`,
    /// `fully_unused_session_count`) are initialised to `0`; the
    /// aggregator constructs via struct literal when waste data is
    /// available (see [`crate::analyzer::aggregate::group_by_mcp::aggregate_by_mcp_server`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::McpServerBucket;
    /// use chrono::Duration;
    /// let _b = McpServerBucket::new("github".into(), 0, 0, 0, Duration::zero(), 0);
    /// ```
    #[must_use]
    pub const fn new(
        server: String,
        tool_count: usize,
        call_count: usize,
        failure_count: usize,
        total_duration: Duration,
        session_count: usize,
    ) -> Self {
        Self {
            server,
            tool_count,
            call_count,
            failure_count,
            total_duration,
            session_count,
            unused_tool_count: 0,
            fully_unused_session_count: 0,
        }
    }
}

/// Per-day aggregated row, with the utilization metric (see ADR-0008).
///
/// `utilization_pct` = `total_tool_duration / total_wall_duration × 100`,
/// clamped to `[0, 100]`. `is_low_utilization` is `true` iff
/// `utilization_pct < threshold` (threshold supplied by the caller).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::DayBucket;
/// use chrono::{Duration, NaiveDate};
///
/// let b = DayBucket::new(
///     NaiveDate::from_ymd_opt(2026, 5, 30).unwrap(),
///     0, Duration::zero(), Duration::zero(), 0, 0.0, false,
/// );
/// assert_eq!(b.session_count, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DayBucket {
    /// UTC calendar date.
    pub date: NaiveDate,
    /// Number of input sessions that started on this UTC date.
    pub session_count: usize,
    /// Sum of per-session wall durations on this day.
    #[serde(with = "ms_duration")]
    pub total_wall_duration: Duration,
    /// Sum of per-tool-call durations on this day.
    #[serde(with = "ms_duration")]
    pub total_tool_duration: Duration,
    /// Sum of `turn_summary.output_tokens` on this day.
    pub total_output_tokens: u64,
    /// `tool / wall × 100`, clamped to `[0, 100]`.
    ///
    /// Stored as `f32` (Wave D1 / `m1.6.2-followup-m5-utilization-precision`):
    /// the value is a percentage display number (`0.0`–`100.0`),
    /// rounded for human consumption; sub-1% precision is not
    /// meaningful for the "low utilization day" UX it drives. `f32`
    /// is exactly representable for all integer percentages 0..=100
    /// and survives JSON round-trip losslessly within that range.
    /// Callers needing higher precision should reconstruct from
    /// `total_tool_duration` + `total_wall_duration` (both `Duration`,
    /// integer milliseconds on the wire) rather than relying on this
    /// field.
    pub utilization_pct: f32,
    /// `utilization_pct < threshold` (threshold = caller-supplied).
    pub is_low_utilization: bool,
}

impl DayBucket {
    /// Construct a [`DayBucket`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::DayBucket;
    /// use chrono::{Duration, NaiveDate};
    /// let _b = DayBucket::new(
    ///     NaiveDate::from_ymd_opt(2026, 5, 30).unwrap(),
    ///     0, Duration::zero(), Duration::zero(), 0, 0.0, false,
    /// );
    /// ```
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        date: NaiveDate,
        session_count: usize,
        total_wall_duration: Duration,
        total_tool_duration: Duration,
        total_output_tokens: u64,
        utilization_pct: f32,
        is_low_utilization: bool,
    ) -> Self {
        Self {
            date,
            session_count,
            total_wall_duration,
            total_tool_duration,
            total_output_tokens,
            utilization_pct,
            is_low_utilization,
        }
    }
}

/// Per-model aggregated row for `--by model` reports.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::ModelBucket;
/// use chrono::Duration;
///
/// let b = ModelBucket::new("gpt-5".to_string(), 0, 0, 0, Duration::zero());
/// assert_eq!(b.session_count, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModelBucket {
    /// Model id (first-turn model — see D-12).
    pub model: String,
    /// Number of input sessions whose first turn used this model.
    pub session_count: usize,
    /// Sum of `turn_summary.len()` across those sessions.
    pub turn_count: usize,
    /// Sum of `turn_summary.output_tokens`.
    pub total_output_tokens: u64,
    /// Sum of per-session wall durations.
    #[serde(with = "ms_duration")]
    pub total_duration: Duration,
}

impl ModelBucket {
    /// Construct a [`ModelBucket`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::ModelBucket;
    /// use chrono::Duration;
    /// let _b = ModelBucket::new("gpt-5".into(), 0, 0, 0, Duration::zero());
    /// ```
    #[must_use]
    pub const fn new(
        model: String,
        session_count: usize,
        turn_count: usize,
        total_output_tokens: u64,
        total_duration: Duration,
    ) -> Self {
        Self {
            model,
            session_count,
            turn_count,
            total_output_tokens,
            total_duration,
        }
    }
}

/// Serde helper: serialise [`chrono::Duration`] as integer milliseconds.
///
/// `pub(super)` since CORE #2 (`wire-format-units`) so the sibling
/// [`super::AggregateReport`] can reference it via
/// `#[serde(with = "bucket::ms_duration")]` — pre-CORE-#2 the
/// aggregate report had its own `duration_seconds` helper, mixing
/// units silently in the same JSON object. The unification dropped
/// `duration_seconds` and routes everything through this module.
pub(super) mod ms_duration {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        d.num_milliseconds().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let ms = i64::deserialize(d)?;
        Ok(Duration::milliseconds(ms))
    }
}

/// Sibling serde helper for `Option<chrono::Duration>` rendered as
/// optional integer milliseconds. Used by
/// [`super::AggregateReport::since`] (Wave C item 1 —
/// `json-since-sentinel`) where `None` semantically means "no lower
/// bound on the time window" (the CLI's `--since all` argument).
///
/// Pre-Wave-C `since` was a bare `Duration` and `--since all` flowed
/// the in-band sentinel `Duration::MAX` (≈ 292 million years) all
/// the way into JSON output as the raw integer `9223372036854775807`
/// ms — visibly ugly and arithmetically dangerous for any downstream
/// JSON consumer summing windows. With `Option<Duration>`, `--since
/// all` becomes `None` at the CLI boundary and serializes as JSON
/// `null` (or omits the field entirely when paired with
/// `skip_serializing_if`).
pub(super) mod ms_duration_opt {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    // serde `with = "..."` adapters require `&Option<T>` signature
    // (NOT `Option<&T>`). The clippy::ref_option lint suggests the
    // borrowed form, but it would break the serde contract.
    #[allow(clippy::ref_option)]
    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        d.map(|d| d.num_milliseconds()).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let ms: Option<i64> = Option::deserialize(d)?;
        Ok(ms.map(Duration::milliseconds))
    }
}
