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

pub mod hook_rank;
pub mod tool_rank;
pub mod turn_summary;

pub use hook_rank::{hook_rank, HookRankRow};
pub use tool_rank::{tool_rank, ToolRankRow};
pub use turn_summary::{turn_summary, TurnSummaryRow};

use serde::{Deserialize, Serialize};

use crate::episode::{DeriveWarning, Episodes};
use crate::error::ParseWarning;
use crate::model::SessionMeta;

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
        }
    }
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
pub fn analyze(
    episodes: &Episodes,
    meta: &SessionMeta,
    parse_warnings: &[ParseWarning],
) -> AnalysisReport {
    AnalysisReport {
        meta: meta.clone(),
        turn_summary: turn_summary(episodes),
        tool_rank: tool_rank(episodes),
        hook_rank: hook_rank(episodes),
        warnings: episodes.warnings.clone(),
        parse_warnings: parse_warnings.to_vec(),
    }
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
