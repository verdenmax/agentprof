//! Classify a [`Vec<SessionAudit>`] into the four report sections.

use std::collections::BTreeMap;

use serde_json::Value;

use agentprof_adapters::copilot::CopilotEvent;
use agentprof_core::error::ParseWarning;

use crate::schema_audit::scanner::{raw_type, SessionAudit};

/// Aggregated audit findings across all scanned sessions.
#[derive(Debug, Default)]
pub struct Classification {
    /// Number of sessions scanned.
    pub session_count: usize,
    /// Total typed events across all sessions.
    pub event_count: usize,
    /// `agent_version` string → number of sessions reporting it.
    pub agent_version_counts: BTreeMap<String, usize>,
    /// Wire `type` field value → group of `CopilotEvent::Unknown` occurrences.
    pub unknown_by_type: BTreeMap<String, UnknownGroup>,
    /// [`ParseWarning`] variant name → aggregated group.
    pub warning_counts: BTreeMap<String, WarningGroup>,
    /// Start/end pair balance per category (turn, tool, hook).
    pub balance: Vec<BalanceRow>,
    /// [`agentprof_core::adapter::EventKind`] (Debug-formatted) → count.
    pub event_kind_counts: BTreeMap<String, usize>,
}

/// One row of "this `type` field showed up as `CopilotEvent::Unknown`".
#[derive(Debug, Default)]
pub struct UnknownGroup {
    /// Number of lines whose wire `type` matched this group's key.
    pub count: usize,
    /// Up to [`SAMPLES_PER_UNKNOWN`] redacted JSON samples.
    pub samples: Vec<String>,
    /// Session id of the first occurrence, for human follow-up.
    pub example_session: Option<String>,
}

/// One row of "this [`ParseWarning`] variant was emitted N times".
#[derive(Debug, Default)]
pub struct WarningGroup {
    /// Total occurrences across all sessions.
    pub count: usize,
    /// Up to [`EXAMPLE_LOCATIONS_PER_WARNING`] `"session_id:..."` locators.
    pub example_locations: Vec<String>,
}

/// A start/end pair balance check (turn, tool, hook).
#[derive(Debug)]
pub struct BalanceRow {
    /// Human-readable label, e.g. `"Turn(start, end)"`.
    pub label: String,
    /// Count of `*Start`-flavored events.
    pub start_count: usize,
    /// Count of `*End`/`*Complete`-flavored events.
    pub end_count: usize,
    /// `start_count - end_count`.
    pub delta: i64,
    /// Categorical assessment of `delta` magnitude.
    pub severity: BalanceSeverity,
}

/// Severity classification for a [`BalanceRow`].
#[derive(Debug)]
pub enum BalanceSeverity {
    /// Within 5% (or both sides zero).
    Ok,
    /// Between 5% and 20% off.
    Minor,
    /// At least 20% off.
    Severe,
}

/// Maximum redacted samples to keep per Unknown type.
const SAMPLES_PER_UNKNOWN: usize = 2;
/// Maximum example locations to keep per [`ParseWarning`] variant.
const EXAMPLE_LOCATIONS_PER_WARNING: usize = 3;
/// String values longer than this are truncated in samples.
const SAMPLE_STRING_TRUNCATE: usize = 60;

/// Walk the scanned [`SessionAudit`]s and produce a [`Classification`].
#[must_use]
pub fn classify(audits: &[SessionAudit]) -> Classification {
    let mut c = Classification {
        session_count: audits.len(),
        ..Classification::default()
    };

    for audit in audits {
        if let Some(ver) = &audit.agent_version {
            *c.agent_version_counts.entry(ver.clone()).or_insert(0) += 1;
        }

        c.event_count += audit.typed.events.len();
        for (raw, typed) in audit.raw_lines.iter().zip(audit.typed.events.iter()) {
            let kind = typed.kind();
            *c.event_kind_counts.entry(format!("{kind:?}")).or_insert(0) += 1;
            if matches!(typed, CopilotEvent::Unknown) {
                let type_str = raw_type(&raw.value).unwrap_or("(no type field)").to_owned();
                let group = c.unknown_by_type.entry(type_str).or_default();
                group.count += 1;
                if group.example_session.is_none() {
                    group.example_session = Some(audit.sref.id.clone());
                }
                if group.samples.len() < SAMPLES_PER_UNKNOWN {
                    group.samples.push(redact(&raw.value));
                }
            }
        }

        for w in &audit.typed.parse_warnings {
            let key = warning_key(w);
            let loc = warning_location(&audit.sref.id, w);
            let group = c.warning_counts.entry(key).or_default();
            group.count += 1;
            if group.example_locations.len() < EXAMPLE_LOCATIONS_PER_WARNING {
                group.example_locations.push(loc);
            }
        }
    }

    c.balance = compute_balance(&c.event_kind_counts);
    c
}

fn warning_key(w: &ParseWarning) -> String {
    match w {
        ParseWarning::Json { .. } => "Json".into(),
        ParseWarning::Io { .. } => "Io".into(),
        ParseWarning::OutOfOrder => "OutOfOrder".into(),
        ParseWarning::UnclosedTurn { .. } => "UnclosedTurn".into(),
        ParseWarning::UnclosedToolCall { .. } => "UnclosedToolCall".into(),
        ParseWarning::UnclosedHook { .. } => "UnclosedHook".into(),
        ParseWarning::UnknownToolSourcePrefix { .. } => "UnknownToolSourcePrefix".into(),
        // ParseWarning is `#[non_exhaustive]`; surface future variants generically.
        _ => "Other".into(),
    }
}

fn warning_location(session_id: &str, w: &ParseWarning) -> String {
    match w {
        ParseWarning::Json { line_no, .. } | ParseWarning::Io { line_no, .. } => {
            format!("{session_id}:line {line_no}")
        }
        ParseWarning::OutOfOrder => format!("{session_id}:(file order)"),
        ParseWarning::UnclosedTurn { turn_id } => format!("{session_id}:turn {turn_id}"),
        ParseWarning::UnclosedToolCall { call_id } => format!("{session_id}:tool {call_id}"),
        ParseWarning::UnclosedHook { name } => format!("{session_id}:hook {name}"),
        ParseWarning::UnknownToolSourcePrefix { tool_name } => {
            format!("{session_id}:tool-prefix {tool_name}")
        }
        _ => session_id.to_owned(),
    }
}

fn redact(v: &Value) -> String {
    let mut clone = v.clone();
    truncate_strings(&mut clone, SAMPLE_STRING_TRUNCATE);
    serde_json::to_string(&clone).unwrap_or_else(|_| "<unserializable>".into())
}

fn truncate_strings(v: &mut Value, max: usize) {
    match v {
        Value::String(s) if s.chars().count() > max => {
            let truncated: String = s.chars().take(max).collect();
            *s = format!("{truncated}...");
        }
        Value::Array(arr) => {
            for item in arr {
                truncate_strings(item, max);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                truncate_strings(item, max);
            }
        }
        _ => {}
    }
}

fn compute_balance(kinds: &BTreeMap<String, usize>) -> Vec<BalanceRow> {
    let pairs = [
        ("TurnStart", "TurnEnd", "Turn(start, end)"),
        (
            "ToolExecStart",
            "ToolExecComplete",
            "ToolExec(start, complete)",
        ),
        ("HookStart", "HookEnd", "Hook(start, end)"),
    ];
    pairs
        .into_iter()
        .map(|(s, e, label)| {
            let sc = kinds.get(s).copied().unwrap_or(0);
            let ec = kinds.get(e).copied().unwrap_or(0);
            let delta =
                i64::try_from(sc).unwrap_or(i64::MAX) - i64::try_from(ec).unwrap_or(i64::MAX);
            let total = sc.max(ec);
            let off = usize::try_from(delta.unsigned_abs()).unwrap_or(usize::MAX);
            #[allow(clippy::cast_precision_loss)]
            let severity = if total == 0 {
                BalanceSeverity::Ok
            } else {
                let ratio = (off as f64) / (total as f64);
                if ratio < 0.05 {
                    BalanceSeverity::Ok
                } else if ratio < 0.20 {
                    BalanceSeverity::Minor
                } else {
                    BalanceSeverity::Severe
                }
            };
            BalanceRow {
                label: label.into(),
                start_count: sc,
                end_count: ec,
                delta,
                severity,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_strings_clips_long_values() {
        let mut v: Value = serde_json::json!({
            "a": "short",
            "b": "x".repeat(200),
            "c": ["y".repeat(200)],
        });
        truncate_strings(&mut v, 10);
        assert_eq!(v["a"], "short");
        assert!(v["b"].as_str().unwrap().ends_with("..."));
        assert!(v["b"].as_str().unwrap().len() <= 13);
        assert!(v["c"][0].as_str().unwrap().ends_with("..."));
    }

    #[test]
    fn balance_severity_thresholds() {
        let mut kinds = BTreeMap::new();
        kinds.insert("TurnStart".into(), 100);
        kinds.insert("TurnEnd".into(), 99); // 1% off → Ok
        kinds.insert("ToolExecStart".into(), 100);
        kinds.insert("ToolExecComplete".into(), 85); // 15% off → Minor
        kinds.insert("HookStart".into(), 1);
        kinds.insert("HookEnd".into(), 100); // 99% off → Severe
        let rows = compute_balance(&kinds);
        assert!(matches!(rows[0].severity, BalanceSeverity::Ok));
        assert!(matches!(rows[1].severity, BalanceSeverity::Minor));
        assert!(matches!(rows[2].severity, BalanceSeverity::Severe));
    }
}
