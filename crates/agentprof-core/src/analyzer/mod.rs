//! # Analyzer rollups
//!
//! Pure rollup functions that consume `&Episodes` and produce per-row
//! analytics. [`analyze`] bundles all three rollups + meta + warnings
//! into a single [`AnalysisReport`] for downstream rendering.
//!
//! All rollup functions are **agent-agnostic** — they operate on the
//! shared [`Episodes`] shape from [`crate::episode`], so Claude
//! (Phase 2) and Codex (Phase 3) adapters automatically benefit.
//!
//! See `docs/internals/adr-0005-analyzer-and-payload-name.md` for the
//! decision to place this layer in `agentprof-core` (rather than
//! `agentprof-cli`) so future TUI and storage consumers can reuse it.
//!
//! ## Module layout
//!
//! | Module | Purpose |
//! |---|---|
//! | [`mod@turn_summary`] | Per-Turn row: status, duration, model, mode, tool/hook/skill counts, output_tokens |
//! | [`mod@tool_rank`] | Per-tool-name row: call counts (success/failure/orphan/user-requested), p50/p95/max duration |
//! | [`mod@hook_rank`] | Per-hook-name row: call counts, p50/p95 duration, success/failure |

pub mod aggregate;
pub mod hook_rank;
pub mod pending;
pub mod stats;
pub mod tool_rank;
pub mod turn_summary;
pub mod waste;

pub use aggregate::{AggregateKey, AggregateReport, AnyAggregateReport};
pub use hook_rank::{hook_rank, HookRankRow};
pub use tool_rank::{tool_rank, ToolRankRow};
pub use turn_summary::{turn_summary, TurnSummaryRow};
pub use waste::{aggregate_waste, compute_waste};

use serde::{Deserialize, Serialize};

use crate::episode::{DeriveWarning, Episodes};
use crate::error::ParseWarning;
use crate::model::SessionMeta;

/// Per-model token-usage rollup, sourced from session-level events
/// (e.g. Copilot CLI's `session.shutdown.modelMetrics`).
///
/// All four fields default to `0` when the wire format omits them.
/// Cardinality discrimination ("known-zero vs unreported") is intentionally
/// elided in v1 — Copilot CLI consistently reports all four fields when
/// `usage` is present. See ADR-0012 D-15 for rationale.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::ModelUsage;
/// let mut u = ModelUsage::new();
/// u.input_tokens = 100;
/// u.output_tokens = 50;
/// assert_eq!(u.total(), 150);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModelUsage {
    /// Input (request prompt) tokens.
    pub input_tokens: u64,
    /// Output (response generation) tokens.
    pub output_tokens: u64,
    /// Cache-read tokens (re-used context from prompt cache).
    pub cache_read_tokens: u64,
    /// Cache-write tokens (new context entered into the prompt cache).
    pub cache_write_tokens: u64,
}

impl Default for ModelUsage {
    /// Equivalent to [`ModelUsage::new`] — zero-initialized.
    fn default() -> Self {
        Self::new()
    }
}

impl ModelUsage {
    /// Construct a zero-initialized rollup.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::ModelUsage;
    /// let u = ModelUsage::new();
    /// assert_eq!(u.input_tokens, 0);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        }
    }

    /// Total of all four token categories. Saturating add (returns
    /// `u64::MAX` on overflow rather than panicking).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::ModelUsage;
    /// let mut u = ModelUsage::new();
    /// u.input_tokens = 10;
    /// u.output_tokens = 20;
    /// u.cache_read_tokens = 30;
    /// u.cache_write_tokens = 40;
    /// assert_eq!(u.total(), 100);
    /// ```
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }
}

/// Bundled analyzer output for a single session.
///
/// Constructed by [`analyze`]. Snapshot-stable: every contained `Vec` is
/// in deterministic order; all `Duration` fields serialize to integer
/// milliseconds (see [`duration_ms`] helper module).
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::{analyze, AnalysisReport};
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use chrono::Utc;
///
/// let episodes = Episodes::new();
/// let meta = SessionMeta::new("s1".into(), AgentKind::Copilot, Utc::now(), false);
/// let report: AnalysisReport = analyze(&episodes, &meta, &[]);
/// assert!(report.turn_summary.is_empty());
/// assert!(report.parse_warnings.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AnalysisReport {
    /// Source session metadata (cloned from input).
    pub meta: SessionMeta,
    /// Per-Turn rows in chronological order.
    pub turn_summary: Vec<TurnSummaryRow>,
    /// Per-tool rows in `total_duration` descending order.
    pub tool_rank: Vec<ToolRankRow>,
    /// Per-hook rows in `total_duration` descending order.
    pub hook_rank: Vec<HookRankRow>,
    /// `DeriveWarning`s carried forward from `Episodes`.
    pub warnings: Vec<DeriveWarning>,
    /// `ParseWarning`s carried forward from the raw session loader.
    ///
    /// Pre-fix, parse warnings were collected by `parse_events_jsonl` but
    /// never surfaced in the report — users couldn't see that (e.g.) 17 %
    /// of their session was silently dropping due to schema mismatches.
    /// Carrying them through `AnalysisReport` lets every renderer (md / json
    /// / future TUI) display a "Parse warnings: N" line and a per-error
    /// breakdown in the Warnings section.
    ///
    /// Distinct from `warnings` ([`DeriveWarning`]) which are analyzer-time
    /// data anomalies (e.g. orphan hook.end), not parser-time format errors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parse_warnings: Vec<ParseWarning>,
    /// Per-model token-usage rollup, cloned from
    /// [`crate::episode::Episodes::model_metrics`] by [`analyze`].
    /// `None` when the session had no event reporting model metrics
    /// (e.g. Copilot session without `session.shutdown`).
    ///
    /// Map key is the model identifier as reported by the adapter.
    /// Skipped in JSON output when `None` for archive cleanliness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_metrics: Option<std::collections::BTreeMap<String, ModelUsage>>,
    /// Set of MCP tool names that were loaded into the session's tool
    /// catalog (regardless of whether they were ever invoked).
    ///
    /// Populated by the analyzer during [`analyze`] from
    /// [`crate::episode::Episodes::loaded_mcp_tools`]. Empty for sessions
    /// pre-dating the M2.1 capture or for non-Copilot agents that don't
    /// expose tool-loading events. `#[serde(default)]` keeps backward
    /// compatibility with pre-M2.1 stored `AnalysisReport` JSON blobs
    /// in `agentprof-storage`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub loaded_mcp_tools: std::collections::BTreeSet<String>,
}

impl AnalysisReport {
    /// Construct an empty `AnalysisReport` with the given meta.
    ///
    /// All rollups start as empty `Vec`s. Used as a baseline when no
    /// `Episodes` are available (e.g. error paths).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s1".into(), AgentKind::Copilot, Utc::now(), false);
    /// let r = AnalysisReport::new(meta);
    /// assert!(r.turn_summary.is_empty());
    /// assert!(r.parse_warnings.is_empty());
    /// ```
    #[must_use]
    pub const fn new(meta: SessionMeta) -> Self {
        Self {
            meta,
            turn_summary: Vec::new(),
            tool_rank: Vec::new(),
            hook_rank: Vec::new(),
            warnings: Vec::new(),
            parse_warnings: Vec::new(),
            model_metrics: None,
            loaded_mcp_tools: std::collections::BTreeSet::new(),
        }
    }

    /// Sum of `input_tokens` across every entry in
    /// [`Self::model_metrics`], saturating at [`u64::MAX`] and clamped to
    /// [`i64::MAX`] for `SQLite` storage. Returns `None` when
    /// `model_metrics` is `None` or empty.
    ///
    /// Used by [`agentprof-storage`] when populating
    /// `sessions.total_input_tokens` so the column reflects the same
    /// session-level rollup that the markdown / TUI renderers display.
    ///
    /// [`agentprof-storage`]: https://docs.rs/agentprof-storage
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::{AnalysisReport, ModelUsage};
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    /// use std::collections::BTreeMap;
    ///
    /// let mut report = AnalysisReport::new(
    ///     SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false),
    /// );
    /// let mut m = BTreeMap::new();
    /// let mut u = ModelUsage::new();
    /// u.input_tokens = 1_000;
    /// m.insert("gpt-5".into(), u);
    /// report.model_metrics = Some(m);
    /// assert_eq!(report.total_input_tokens(), Some(1_000));
    /// ```
    #[must_use]
    pub fn total_input_tokens(&self) -> Option<i64> {
        sum_model_tokens(self, |u| u.input_tokens)
    }

    /// Sum of `output_tokens` across every entry in
    /// [`Self::model_metrics`]. Same shape and rationale as
    /// [`Self::total_input_tokens`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let report = AnalysisReport::new(
    ///     SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false),
    /// );
    /// assert_eq!(report.total_output_tokens(), None);
    /// ```
    #[must_use]
    pub fn total_output_tokens(&self) -> Option<i64> {
        sum_model_tokens(self, |u| u.output_tokens)
    }

    /// Sum of `cache_read_tokens` across every entry in
    /// [`Self::model_metrics`]. Same shape and rationale as
    /// [`Self::total_input_tokens`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let report = AnalysisReport::new(
    ///     SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false),
    /// );
    /// assert_eq!(report.total_cache_read(), None);
    /// ```
    #[must_use]
    pub fn total_cache_read(&self) -> Option<i64> {
        sum_model_tokens(self, |u| u.cache_read_tokens)
    }

    /// Sum of `cache_write_tokens` across every entry in
    /// [`Self::model_metrics`] (named `total_cache_creation` to mirror
    /// the `SQLite` column / Anthropic API terminology — cache *creation*
    /// is the write side). Same shape and rationale as
    /// [`Self::total_input_tokens`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let report = AnalysisReport::new(
    ///     SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false),
    /// );
    /// assert_eq!(report.total_cache_creation(), None);
    /// ```
    #[must_use]
    pub fn total_cache_creation(&self) -> Option<i64> {
        sum_model_tokens(self, |u| u.cache_write_tokens)
    }
}

#[allow(clippy::cast_possible_wrap)]
fn sum_model_tokens<F: Fn(&ModelUsage) -> u64>(report: &AnalysisReport, pick: F) -> Option<i64> {
    let m = report.model_metrics.as_ref()?;
    if m.is_empty() {
        return None;
    }
    let total_u64 = m.values().map(&pick).fold(0_u64, u64::saturating_add);
    // Clamp at i64::MAX for SQLite (it's a signed 64-bit integer column).
    let clamped = total_u64.min(i64::MAX as u64);
    Some(clamped as i64)
}

/// Compute all 3 analyzer rollups for a session.
///
/// Pure function: same `(&Episodes, &SessionMeta, &[ParseWarning])` →
/// byte-identical `AnalysisReport`. No I/O, no clock access.
///
/// `parse_warnings` is typically `raw.parse_warnings` from `parse_events_jsonl`;
/// callers that have no parse warnings (e.g. unit tests building `Episodes`
/// in memory) can pass `&[]`.
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::analyze;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::model::SessionMeta;
/// use chrono::Utc;
///
/// let episodes = Episodes::new();
/// let meta = SessionMeta::new("s1".into(), AgentKind::Copilot, Utc::now(), false);
/// let report = analyze(&episodes, &meta, &[]);
/// assert_eq!(report.turn_summary.len(), 0);
/// assert_eq!(report.tool_rank.len(), 0);
/// assert_eq!(report.hook_rank.len(), 0);
/// assert_eq!(report.parse_warnings.len(), 0);
/// ```
#[must_use]
#[tracing::instrument(name = "analyzer.analyze", skip_all, fields(turns = episodes.turns.len()))]
pub fn analyze(
    episodes: &Episodes,
    meta: &SessionMeta,
    parse_warnings: &[ParseWarning],
) -> AnalysisReport {
    let report = AnalysisReport {
        meta: meta.clone(),
        turn_summary: turn_summary(episodes),
        tool_rank: tool_rank(episodes),
        hook_rank: hook_rank(episodes),
        warnings: episodes.warnings.clone(),
        parse_warnings: parse_warnings.to_vec(),
        model_metrics: episodes.model_metrics.clone(),
        loaded_mcp_tools: episodes.loaded_mcp_tools.clone(),
    };
    tracing::debug!(
        tool_count = report.tool_rank.len(),
        hook_count = report.hook_rank.len(),
        "produced analysis report"
    );
    report
}

// ---------- Duration <-> milliseconds serde helper ----------

/// Serde helpers for [`chrono::Duration`] ↔ integer-milliseconds JSON.
///
/// Use as `#[serde(with = "duration_ms")]` on `Duration` fields.
/// Matches the ADR-0004 IMP-007 snapshot-stability convention.
pub mod duration_ms {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a [`Duration`] as integer milliseconds.
    ///
    /// # Errors
    ///
    /// Propagates serializer errors. Cannot fail intrinsically — `i64`
    /// always serializes.
    #[allow(clippy::trivially_copy_pass_by_ref)] // serde calling convention
    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(d.num_milliseconds())
    }

    /// Deserialize integer milliseconds into a [`Duration`].
    ///
    /// # Errors
    ///
    /// Propagates deserializer errors (e.g. non-integer JSON).
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let ms = i64::deserialize(d)?;
        Ok(Duration::milliseconds(ms))
    }
}

/// `Option<Duration>` variant of [`duration_ms`].
pub mod duration_ms_opt {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize an `Option<Duration>` as either integer milliseconds or `null`.
    ///
    /// # Errors
    ///
    /// Propagates serializer errors. Cannot fail intrinsically.
    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.serialize_i64(d.num_milliseconds()),
            None => s.serialize_none(),
        }
    }

    /// Deserialize `null | int_ms` into `Option<Duration>`.
    ///
    /// # Errors
    ///
    /// Propagates deserializer errors (e.g. non-integer non-null JSON).
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let opt = Option::<i64>::deserialize(d)?;
        Ok(opt.map(Duration::milliseconds))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::adapter::AgentKind;
    use chrono::Utc;

    fn meta() -> SessionMeta {
        SessionMeta::new("s1".into(), AgentKind::Copilot, Utc::now(), false)
    }

    #[test]
    fn analyze_empty_episodes_produces_empty_report_with_meta() {
        let report = analyze(&Episodes::new(), &meta(), &[]);
        assert!(report.turn_summary.is_empty());
        assert!(report.tool_rank.is_empty());
        assert!(report.hook_rank.is_empty());
        assert!(report.warnings.is_empty());
        assert!(report.parse_warnings.is_empty());
        assert_eq!(report.meta.id, "s1");
    }

    #[test]
    fn analyze_clones_warnings_from_episodes() {
        let mut ep = Episodes::new();
        ep.warnings.push(DeriveWarning::AbortWithoutOpenElement {
            reason: "test".into(),
            at: Utc::now(),
        });
        let report = analyze(&ep, &meta(), &[]);
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn analyze_carries_parse_warnings_through() {
        // Lock the contract that ParseWarnings emitted by the loader
        // surface in the rendered report (md/json) so users can see
        // silent event drops. Regression guard for post-output-audit P0 #2.
        let pw = vec![
            ParseWarning::Json {
                line_no: 5,
                error: "missing field `source`".into(),
            },
            ParseWarning::Json {
                line_no: 934,
                error: "missing field `turnId`".into(),
            },
            ParseWarning::OutOfOrder,
        ];
        let report = analyze(&Episodes::new(), &meta(), &pw);
        assert_eq!(report.parse_warnings.len(), 3);
        // Vec content preserved verbatim — same enum variants, same line_no.
        assert_eq!(report.parse_warnings, pw);
    }

    #[test]
    fn report_new_starts_empty() {
        let r = AnalysisReport::new(meta());
        assert!(r.turn_summary.is_empty());
        assert!(r.tool_rank.is_empty());
        assert!(r.hook_rank.is_empty());
        assert!(r.warnings.is_empty());
        assert!(r.parse_warnings.is_empty());
        assert!(
            r.loaded_mcp_tools.is_empty(),
            "loaded_mcp_tools defaults to empty on AnalysisReport::new"
        );
    }

    #[test]
    fn analyze_populates_loaded_mcp_tools_from_episodes() {
        // Episodes carries the per-event-accumulated set (populated by
        // derive_episodes via Event::payload_loaded_mcp_tools); analyze()
        // must clone it into the report verbatim so downstream renderers
        // and storage see exactly what the analyzer pipeline observed.
        let mut ep = Episodes::new();
        ep.loaded_mcp_tools.insert("mcp__github__search".into());
        ep.loaded_mcp_tools.insert("mcp__github__create".into());
        let report = analyze(&ep, &meta(), &[]);
        assert_eq!(report.loaded_mcp_tools.len(), 2);
        assert!(report.loaded_mcp_tools.contains("mcp__github__search"));
        assert!(report.loaded_mcp_tools.contains("mcp__github__create"));
    }

    #[test]
    fn analyze_empty_when_no_mcp_load_events() {
        // Sessions without any tool-loading events (or adapters that
        // don't expose them) must yield an empty loaded_mcp_tools set
        // — not an Option::None — to keep downstream waste analysis
        // and storage uniformly indexable.
        let report = analyze(&Episodes::new(), &meta(), &[]);
        assert!(
            report.loaded_mcp_tools.is_empty(),
            "no tool-load events → empty loaded_mcp_tools"
        );
    }

    #[test]
    fn analysis_report_json_round_trip_is_lossless() {
        // Build a non-trivial report exercising all rollup vec types +
        // warnings + parse_warnings + a non-empty meta. Round-trip through
        // JSON and assert equality. Locks the wire-format contract for
        // downstream consumers (SQLite storage, future HTTP API, …).
        let mut ep = Episodes::new();
        ep.warnings.push(DeriveWarning::AbortWithoutOpenElement {
            reason: "round-trip user_cancel".into(),
            at: chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 5, 29, 12, 0, 0)
                .single()
                .unwrap(),
        });
        let pw = vec![ParseWarning::Json {
            line_no: 5,
            error: "round-trip parse failure".into(),
        }];
        let original = analyze(&ep, &meta(), &pw);

        // Serialize → Deserialize → equal.
        let json = serde_json::to_string(&original).expect("serialize");
        let recovered: AnalysisReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, recovered, "JSON round-trip must preserve report");

        // Pretty form must also round-trip (used by --export json).
        let pretty = serde_json::to_string_pretty(&original).expect("serialize pretty");
        let recovered_pretty: AnalysisReport =
            serde_json::from_str(&pretty).expect("deserialize pretty");
        assert_eq!(
            original, recovered_pretty,
            "pretty JSON round-trip must preserve report"
        );
    }
}

#[cfg(test)]
mod model_usage_tests {
    use super::*;

    #[test]
    fn model_usage_new_zero_initialized() {
        let u = ModelUsage::new();
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.output_tokens, 0);
        assert_eq!(u.cache_read_tokens, 0);
        assert_eq!(u.cache_write_tokens, 0);
    }

    #[test]
    fn model_usage_total_sums_all_four() {
        let u = ModelUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 1000,
            cache_write_tokens: 25,
        };
        assert_eq!(u.total(), 1175);
    }

    #[test]
    fn model_usage_total_saturates_at_u64_max() {
        let u = ModelUsage {
            input_tokens: u64::MAX,
            output_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };
        assert_eq!(u.total(), u64::MAX, "saturating_add prevents overflow");
    }

    #[test]
    fn model_usage_total_saturates_mid_chain() {
        // Sum exceeds u64::MAX only after the 3rd addend — exercises the
        // saturating semantics in the middle of the chain, not just at
        // the trivial MAX + 1 case. Documents intent for future refactors.
        let u = ModelUsage {
            input_tokens: u64::MAX / 2,
            output_tokens: u64::MAX / 2,
            cache_read_tokens: 2, // (MAX/2)+(MAX/2)=MAX-1; +2 saturates
            cache_write_tokens: 100,
        };
        assert_eq!(u.total(), u64::MAX);
    }

    #[test]
    fn model_usage_serde_roundtrip() {
        let u = ModelUsage {
            input_tokens: 781_437,
            output_tokens: 17_664,
            cache_read_tokens: 499_072,
            cache_write_tokens: 0,
        };
        let json = serde_json::to_string(&u).unwrap();
        let back: ModelUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, u);
    }

    #[test]
    fn model_usage_default_equals_new() {
        // Exercise the Default impl (delegates to new() — see impl Default
        // for ModelUsage). PartialEq is derived so this is a direct compare.
        assert_eq!(ModelUsage::default(), ModelUsage::new());
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod analyze_model_metrics_tests {
    use super::*;
    use crate::adapter::AgentKind;
    use crate::episode::Episodes;
    use crate::model::SessionMeta;
    use chrono::Utc;
    use std::collections::BTreeMap;

    fn fixture_meta() -> SessionMeta {
        SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false)
    }

    #[test]
    fn analyze_passes_none_when_episodes_have_no_metrics() {
        let episodes = Episodes::default();
        let report = analyze(&episodes, &fixture_meta(), &[]);
        assert!(report.model_metrics.is_none());
    }

    /// Build a report with the supplied per-model usage rollups.
    fn report_with_metrics(metrics: BTreeMap<String, ModelUsage>) -> AnalysisReport {
        let mut r = AnalysisReport::new(fixture_meta());
        r.model_metrics = Some(metrics);
        r
    }

    #[test]
    fn total_input_tokens_returns_none_when_model_metrics_absent() {
        let r = AnalysisReport::new(fixture_meta());
        assert!(r.model_metrics.is_none());
        assert_eq!(r.total_input_tokens(), None);
        assert_eq!(r.total_output_tokens(), None);
        assert_eq!(r.total_cache_read(), None);
        assert_eq!(r.total_cache_creation(), None);
    }

    #[test]
    fn total_input_tokens_returns_none_when_model_metrics_empty() {
        let r = report_with_metrics(BTreeMap::new());
        assert_eq!(
            r.total_input_tokens(),
            None,
            "empty BTreeMap is treated like no metrics — None, not Some(0)"
        );
        assert_eq!(r.total_output_tokens(), None);
        assert_eq!(r.total_cache_read(), None);
        assert_eq!(r.total_cache_creation(), None);
    }

    #[test]
    fn total_input_tokens_sums_multi_model() {
        let mut m = BTreeMap::new();
        m.insert(
            "claude-haiku-4.5".into(),
            ModelUsage {
                input_tokens: 100,
                output_tokens: 11,
                cache_read_tokens: 1,
                cache_write_tokens: 2,
            },
        );
        m.insert(
            "gpt-5-mini".into(),
            ModelUsage {
                input_tokens: 200,
                output_tokens: 22,
                cache_read_tokens: 3,
                cache_write_tokens: 4,
            },
        );
        let r = report_with_metrics(m);
        assert_eq!(r.total_input_tokens(), Some(300));
        assert_eq!(r.total_output_tokens(), Some(33));
        assert_eq!(r.total_cache_read(), Some(4));
        assert_eq!(r.total_cache_creation(), Some(6));
    }

    #[test]
    fn total_input_tokens_saturating_add_doesnt_panic_on_u64_max() {
        // Two u64::MAX values — naive `+` panics in debug, our impl
        // uses saturating_add and then clamps the u64 result to i64::MAX
        // for storage. Lock the no-panic contract.
        let mut m = BTreeMap::new();
        m.insert(
            "m-a".into(),
            ModelUsage {
                input_tokens: u64::MAX,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            },
        );
        m.insert(
            "m-b".into(),
            ModelUsage {
                input_tokens: u64::MAX,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            },
        );
        let r = report_with_metrics(m);
        assert_eq!(
            r.total_input_tokens(),
            Some(i64::MAX),
            "u64 saturates at MAX, then clamps to i64::MAX for SQLite"
        );
    }

    #[test]
    fn total_input_tokens_clamps_to_i64_max() {
        // Sum exceeds i64::MAX but fits in u64 — the clamp step must
        // bring it down to i64::MAX so the SQLite write doesn't wrap.
        // i64::MAX = 9_223_372_036_854_775_807; pick two values whose
        // sum exceeds that but is well below u64::MAX.
        const I64_MAX_U64: u64 = i64::MAX as u64;
        let a = I64_MAX_U64 / 2 + 1;
        let b = I64_MAX_U64 / 2 + 1; // a + b = i64::MAX + 2 (still in u64)
        let mut m = BTreeMap::new();
        m.insert(
            "m-a".into(),
            ModelUsage {
                input_tokens: a,
                ..ModelUsage::new()
            },
        );
        m.insert(
            "m-b".into(),
            ModelUsage {
                input_tokens: b,
                ..ModelUsage::new()
            },
        );
        let r = report_with_metrics(m);
        assert_eq!(
            r.total_input_tokens(),
            Some(i64::MAX),
            "values above i64::MAX must clamp at i64::MAX"
        );
    }

    #[test]
    fn analyze_clones_episodes_model_metrics_into_report() {
        let mut m = BTreeMap::new();
        let mut usage = ModelUsage::new();
        usage.input_tokens = 98_327;
        usage.output_tokens = 47_523;
        usage.cache_read_tokens = 3_444_639;
        usage.cache_write_tokens = 721_860;
        m.insert("claude-opus-4.7-1m-internal".into(), usage);

        let episodes = Episodes {
            model_metrics: Some(m.clone()),
            ..Episodes::default()
        };
        let report = analyze(&episodes, &fixture_meta(), &[]);
        let rm = report.model_metrics.expect("clone");
        assert_eq!(
            rm.get("claude-opus-4.7-1m-internal").unwrap().input_tokens,
            98_327
        );
        assert_eq!(rm.len(), 1);
    }
}
