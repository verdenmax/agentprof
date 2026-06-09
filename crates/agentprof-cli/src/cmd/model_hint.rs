//! Pick the *dominant* model for a session report when inferring tokenizer.
//!
//! `AnalysisReport::model_metrics` is a [`std::collections::BTreeMap`], so
//! `keys().next()` returns the alphabetically smallest key — which is
//! **not** the dominant model in mixed sessions. A Copilot CLI session
//! that mostly used `gpt-5-mini` but logged a single `claude-haiku-4.5`
//! call would be incorrectly classified as Anthropic and routed to
//! `cl100k_base`, undercounting tokens for the bulk of the work.
//!
//! [`dominant_model`] picks the model with the largest `ModelUsage::total()`
//! (input + output + cache-read + cache-write). Ties break on the model
//! name (ascending) for determinism. Returns `None` when `model_metrics`
//! is `None` or empty.
//!
//! Shared by `analyze`, `aggregate`, and `mcp-waste` — keeping the rule
//! in one place avoids the three sites silently disagreeing.

use agentprof_core::analyzer::AnalysisReport;

/// Return the dominant model name in `report.model_metrics`, or `None`
/// when no model usage is recorded.
///
/// "Dominant" = largest [`agentprof_core::analyzer::ModelUsage::total`]
/// (sum of input, output, cache-read and cache-write tokens). On ties,
/// the alphabetically earliest model name wins so callers get a
/// deterministic answer across runs.
///
/// # Examples
///
/// ```ignore
/// // Wired through `cmd::model_hint::dominant_model(&report)` in
/// // analyze / aggregate / mcp-waste — see those subcommands for use.
/// ```
#[must_use]
pub fn dominant_model(report: &AnalysisReport) -> Option<String> {
    report.model_metrics.as_ref().and_then(|m| {
        m.iter()
            .max_by(|a, b| a.1.total().cmp(&b.1.total()).then_with(|| b.0.cmp(a.0)))
            .map(|(name, _)| name.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::{AnalysisReport, ModelUsage};
    use agentprof_core::model::SessionMeta;
    use chrono::{TimeZone, Utc};
    use std::collections::BTreeMap;

    fn empty_report() -> AnalysisReport {
        AnalysisReport::new(SessionMeta::new(
            "s1".into(),
            AgentKind::Copilot,
            Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap(),
            false,
        ))
    }

    fn usage(input: u64, output: u64) -> ModelUsage {
        let mut u = ModelUsage::new();
        u.input_tokens = input;
        u.output_tokens = output;
        u
    }

    #[test]
    fn dominant_model_none_when_no_metrics() {
        let report = empty_report();
        assert_eq!(dominant_model(&report), None);
    }

    #[test]
    fn dominant_model_none_when_metrics_empty() {
        let mut report = empty_report();
        report.model_metrics = Some(BTreeMap::new());
        assert_eq!(dominant_model(&report), None);
    }

    #[test]
    fn dominant_model_picks_largest_total_not_alphabetical() {
        // Regression for audit B1: BTreeMap::keys().next() would return
        // "claude-haiku-4.5" (alphabetically smallest); the correct
        // dominant model is "gpt-5-mini" by token total. This forced
        // mixed-model sessions onto cl100k_base, mis-pricing the
        // gpt-5-mini bulk of the work.
        let mut metrics = BTreeMap::new();
        metrics.insert("claude-haiku-4.5".to_string(), usage(100, 0));
        metrics.insert("gpt-5-mini".to_string(), usage(8000, 2000));
        let mut report = empty_report();
        report.model_metrics = Some(metrics);

        assert_eq!(dominant_model(&report).as_deref(), Some("gpt-5-mini"));
    }

    #[test]
    fn dominant_model_single_entry_returned() {
        let mut metrics = BTreeMap::new();
        metrics.insert("gpt-4o".to_string(), usage(10, 5));
        let mut report = empty_report();
        report.model_metrics = Some(metrics);

        assert_eq!(dominant_model(&report).as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn dominant_model_ties_break_alphabetically_ascending() {
        // Two models with identical totals should produce a stable
        // answer; the smaller name wins so successive runs agree.
        let mut metrics = BTreeMap::new();
        metrics.insert("zeta".to_string(), usage(100, 0));
        metrics.insert("alpha".to_string(), usage(100, 0));
        let mut report = empty_report();
        report.model_metrics = Some(metrics);

        assert_eq!(dominant_model(&report).as_deref(), Some("alpha"));
    }
}
