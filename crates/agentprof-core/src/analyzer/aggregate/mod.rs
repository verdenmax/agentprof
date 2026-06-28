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

// Wave D1: re-export the per-key aggregator functions at the module
// root so callers can write `aggregate::aggregate_by_tool(...)` instead
// of `aggregate::group_by_tool::aggregate_by_tool(...)` (closes
// `m1.6.2-followup-m2-pub-use-aggregators`). The `group_by_*` modules
// stay `pub` for users who want to reach into their type definitions
// (e.g. `TempToolAcc` is private, but the module path still resolves
// for rustdoc cross-links).
pub use group_by_day::aggregate_by_day;
pub use group_by_mcp::aggregate_by_mcp_server;
pub use group_by_model::aggregate_by_model;
pub use group_by_tool::aggregate_by_tool;

/// Shared session-wall helper used by every aggregator.
///
/// Wave D1 / `m1.6.2-followup-compute-wall-shared`: keeping this as a
/// private sibling module rather than hoisting to `impl Episodes` is
/// a deliberate YAGNI call — only the 4 cross-session aggregators
/// consume it, no other call site needs `compute_wall` semantics, and
/// the function is read-only over `Episodes` (no `&mut self` benefit).
/// If a 5th consumer outside `aggregate::` ever needs the same
/// "latest endpoint across all episode endpoints" walk, hoist this
/// fn to `impl Episodes` then. See Wave C item 2 (the sum-invariant
/// tests in `tests/aggregate.rs`) for indirect public-API coverage
/// of `compute_wall`'s behaviour.
mod wall {
    use chrono::{DateTime, Duration, Utc};

    use crate::episode::Episodes;

    /// Wall duration of a single session = `max(last_event_ts, session_start) - session_start`.
    ///
    /// Walks the maximum end timestamp across:
    /// - `Turn.ended_at`
    /// - every `ToolCall.span.ended_at`
    /// - every `HookCall.span.ended_at`
    /// - every `SkillInvocation.at` (skills are instants — M1.6.4 T1)
    /// - every `ModeSegment.ended_at` when `Some`
    ///
    /// Clamped to non-negative. Sessions whose last observable event is
    /// a hook / skill / mode change (no later turn or tool end) would
    /// otherwise be under-counted, which over-reports the
    /// `utilization_pct` headline metric in the `--by day` rollup.
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
        for hook in episodes.hooks.values() {
            for call in &hook.calls {
                if call.span.ended_at > latest {
                    latest = call.span.ended_at;
                }
            }
        }
        for skill in episodes.skills.values() {
            for inv in &skill.invocations {
                if inv.at > latest {
                    latest = inv.at;
                }
            }
        }
        for seg in &episodes.mode_segments {
            if let Some(end) = seg.ended_at {
                if end > latest {
                    latest = end;
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

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::wall::compute_wall;
    use crate::episode::hook::{HookCall, HookEpisode};
    use crate::episode::skill::{SkillEpisode, SkillInvocation};
    use crate::episode::turn::Span;
    use crate::episode::Episodes;

    #[test]
    fn compute_wall_includes_hook_end_when_no_tool_or_turn_end() {
        let session_start = Utc.with_ymd_and_hms(2026, 6, 1, 10, 0, 0).unwrap();
        let hook_end = Utc.with_ymd_and_hms(2026, 6, 1, 10, 0, 5).unwrap();

        let mut episodes = Episodes::new();
        let mut hook = HookEpisode::new("synthetic_hook".to_string());
        hook.calls.push(HookCall::new(Span {
            started_at: session_start,
            ended_at: hook_end,
        }));
        episodes.hooks.insert("synthetic_hook".to_string(), hook);

        let wall = compute_wall(&episodes, session_start);
        assert_eq!(wall.num_seconds(), 5);
    }

    #[test]
    fn compute_wall_includes_skill_at_when_no_tool_or_hook_end() {
        let session_start = Utc.with_ymd_and_hms(2026, 6, 1, 10, 0, 0).unwrap();
        let skill_at = Utc.with_ymd_and_hms(2026, 6, 1, 10, 0, 7).unwrap();

        let mut episodes = Episodes::new();
        let mut skill = SkillEpisode::new("brainstorming".to_string());
        skill.invocations.push(SkillInvocation::new(skill_at));
        episodes.skills.insert("brainstorming".to_string(), skill);

        let wall = compute_wall(&episodes, session_start);
        assert_eq!(wall.num_seconds(), 7);
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
///     Some(Duration::days(30)),
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
    ///
    /// `None` means "no lower bound" — the CLI maps the `--since all`
    /// argument to `None` here (Wave C item 1 — `json-since-sentinel`).
    /// Pre-Wave-C this was a bare `Duration` and the CLI passed
    /// `Duration::MAX` as an in-band sentinel, which then serialised
    /// to JSON as the raw integer `9223372036854775807` ms — visibly
    /// ugly and arithmetically dangerous for any consumer summing
    /// windows.
    ///
    /// **Wire format: optional integer milliseconds** (CORE #2 +
    /// Wave C). JSON renders as `null` when `None`; rendered as the
    /// raw ms integer otherwise. Consumers divide by 1000 if they
    /// want seconds. The field is paired with
    /// `skip_serializing_if = "Option::is_none"` so legacy consumers
    /// expecting an integer-or-absent field continue to work without
    /// observing `null`.
    #[serde(
        with = "bucket::ms_duration_opt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub since: Option<Duration>,
    /// Number of input [`crate::analyzer::AnalysisReport`]s.
    pub session_count: usize,
    /// Number of input sessions that failed to load or parse (reserved
    /// for T2; aggregators here always set it to `0`).
    pub failure_count: usize,
    /// Sum of per-session wall durations.
    ///
    /// **Wire format: integer milliseconds** (CORE #2). See
    /// [`Self::since`] for the unit normalization story.
    #[serde(with = "bucket::ms_duration")]
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
    /// `since` accepts `Option<Duration>` — pass `None` to model "no
    /// lower time bound" (matches the CLI's `--since all` argument);
    /// pass `Some(d)` for a finite window.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::{AggregateKey, AggregateReport, ToolBucket};
    /// use chrono::Duration;
    ///
    /// // Finite window:
    /// let _r: AggregateReport<ToolBucket> = AggregateReport::new(
    ///     AggregateKey::Tool, Some(Duration::days(7)), 0, 0, Duration::zero(), Vec::new(),
    /// );
    ///
    /// // "All time" window:
    /// let _r2: AggregateReport<ToolBucket> = AggregateReport::new(
    ///     AggregateKey::Tool, None, 0, 0, Duration::zero(), Vec::new(),
    /// );
    /// ```
    #[must_use]
    pub const fn new(
        by: AggregateKey,
        since: Option<Duration>,
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

/// M2.5 — sealed-ish contract for per-bucket cache attribution.
///
/// Implemented for [`AggregateReport`] bucket types that carry
/// per-bucket cache token sums (input / cache-read / cache-creation),
/// enabling [`crate::analyzer::cache::CacheMetrics`] computation per
/// bucket.
///
/// **Implemented for `ModelBucket` + `DayBucket` only.** `ToolBucket`
/// and `McpServerBucket` deliberately do **not** implement this trait:
/// cache tokens are prompt-level (per-turn, per-model), and the
/// per-tool / per-server attribution is undefined — splitting cache
/// reads across the N tools called within a single turn would be
/// arbitrary fiction. See ADR-0023 D-3.
///
/// The trait is the **type-level filter** that prevents the generic
/// [`AggregateReport::cache_metrics_per_bucket`] accessor from ever
/// being called against a `ToolBucket` / `McpServerBucket` report
/// (it simply doesn't exist for those instantiations). The runtime
/// [`supports_cache_attribution`] helper mirrors this filter for
/// render-layer / `AnyAggregateReport` dispatch where the bucket type
/// is erased to [`AggregateKey`].
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::{
///     CacheAttributable, DayBucket, ModelBucket,
/// };
/// use chrono::{Duration, NaiveDate};
///
/// let m = ModelBucket::new("claude-sonnet-4.5".into(), 1, 0, 0, Duration::zero())
///     .with_cache_metrics(10_000, 8_000, 2_000);
/// assert_eq!(m.bucket_key(), "claude-sonnet-4.5");
/// assert_eq!(m.bucket_input(), 10_000);
/// assert_eq!(m.bucket_cache_read(), 8_000);
/// assert_eq!(m.bucket_cache_creation(), 2_000);
///
/// let d = DayBucket::new(
///     NaiveDate::from_ymd_opt(2026, 5, 30).unwrap(),
///     1, Duration::zero(), Duration::zero(), 0, 0.0, false,
/// )
/// .with_cache_metrics(10_000, 8_000, 2_000);
/// assert_eq!(d.bucket_key(), "2026-05-30");
/// ```
pub trait CacheAttributable {
    /// Stable string identifying this bucket (model id for
    /// `ModelBucket`, ISO-8601 calendar date for `DayBucket`). Used
    /// as the key in the [`AggregateReport::cache_metrics_per_bucket`]
    /// return map.
    fn bucket_key(&self) -> String;
    /// Sum of `input_tokens` across the sessions in this bucket
    /// (M2.5: pulled from `model_metrics[*].input_tokens`). Feeds
    /// `CacheMetrics::from_raw`'s `input` parameter.
    fn bucket_input(&self) -> u64;
    /// Sum of `cache_read_tokens` across the sessions in this bucket.
    fn bucket_cache_read(&self) -> u64;
    /// Sum of `cache_write_tokens` (Anthropic `cache_creation`) across
    /// the sessions in this bucket.
    fn bucket_cache_creation(&self) -> u64;
}

impl CacheAttributable for ModelBucket {
    fn bucket_key(&self) -> String {
        self.model.clone()
    }
    fn bucket_input(&self) -> u64 {
        self.total_input_tokens
    }
    fn bucket_cache_read(&self) -> u64 {
        self.total_cache_read
    }
    fn bucket_cache_creation(&self) -> u64 {
        self.total_cache_creation
    }
}

impl CacheAttributable for DayBucket {
    fn bucket_key(&self) -> String {
        self.date.to_string()
    }
    fn bucket_input(&self) -> u64 {
        self.total_input_tokens
    }
    fn bucket_cache_read(&self) -> u64 {
        self.total_cache_read
    }
    fn bucket_cache_creation(&self) -> u64 {
        self.total_cache_creation
    }
}

impl<B> AggregateReport<B>
where
    B: CacheAttributable,
{
    /// Per-bucket [`crate::analyzer::cache::CacheMetrics`], keyed by
    /// the bucket's stable identifier (model id for `--by model`,
    /// ISO-8601 date for `--by day`). M2.5 / ADR-0023.
    ///
    /// Returns `None` when **either**:
    ///
    /// 1. The report's [`AggregateReport::by`] is not [`AggregateKey::Model`]
    ///    or [`AggregateKey::Day`] — a defense-in-depth check; the
    ///    [`CacheAttributable`] trait bound is the type-level filter
    ///    that should already prevent reaching here for the other two
    ///    aggregate keys, but this catches malformed reports (e.g.
    ///    deserialized JSON where `by` was hand-edited).
    /// 2. No bucket had any cache activity — i.e. every bucket's
    ///    `cache_read == 0 && cache_creation == 0`, which makes
    ///    [`crate::analyzer::cache::CacheMetrics::from_raw`] return
    ///    `None` for that bucket. Same "skip empty rows" semantics as
    ///    [`crate::analyzer::AnalysisReport::cache_metrics`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::{
    ///     AggregateKey, AggregateReport, ModelBucket,
    /// };
    /// use chrono::Duration;
    ///
    /// let r: AggregateReport<ModelBucket> = AggregateReport::new(
    ///     AggregateKey::Model, None, 0, 0, Duration::zero(), Vec::new(),
    /// );
    /// assert!(r.cache_metrics_per_bucket().is_none(), "empty report");
    /// ```
    #[must_use]
    pub fn cache_metrics_per_bucket(
        &self,
    ) -> Option<std::collections::HashMap<String, crate::analyzer::cache::CacheMetrics>> {
        if !supports_cache_attribution(self.by) {
            return None;
        }
        let mut out = std::collections::HashMap::new();
        for b in &self.buckets {
            if let Some(m) = crate::analyzer::cache::CacheMetrics::from_raw(
                b.bucket_cache_creation(),
                b.bucket_cache_read(),
                b.bucket_input(),
            ) {
                out.insert(b.bucket_key(), m);
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

/// True when `key` supports per-bucket cache attribution.
///
/// Returns `true` for [`AggregateKey::Model`] and [`AggregateKey::Day`],
/// `false` otherwise — mirrors the [`CacheAttributable`] trait bound
/// used by [`AggregateReport::cache_metrics_per_bucket`].
///
/// Use this at render-layer / [`AnyAggregateReport`] dispatch points
/// where the concrete bucket type has been erased to [`AggregateKey`]
/// and a static trait bound would no longer apply. See ADR-0023 D-3
/// for why tool / mcp-server buckets are excluded.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate::{
///     supports_cache_attribution, AggregateKey,
/// };
/// assert!(supports_cache_attribution(AggregateKey::Model));
/// assert!(supports_cache_attribution(AggregateKey::Day));
/// assert!(!supports_cache_attribution(AggregateKey::Tool));
/// assert!(!supports_cache_attribution(AggregateKey::McpServer));
/// ```
#[must_use]
pub const fn supports_cache_attribution(key: AggregateKey) -> bool {
    matches!(key, AggregateKey::Model | AggregateKey::Day)
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
///     AggregateKey::Tool, None, 0, 0, Duration::zero(), Vec::new(),
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

// CORE #2 (`wire-format-units`) — the private `duration_seconds`
// helper that lived here was removed; [`AggregateReport`] now serializes
// `since` + `total_wall_duration` as integer milliseconds via the
// `bucket::ms_duration` helper (the same helper used by every bucket
// field). Mixed units in a single JSON object are now structurally
// impossible. See the field docs for the rationale + consumer impact.
