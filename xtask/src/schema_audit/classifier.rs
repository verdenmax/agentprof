//! Classify a [`Vec<SessionAudit>`] into the four report sections.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use agentprof_adapters::copilot::CopilotEvent;
use agentprof_core::error::ParseWarning;

use crate::schema_audit::scanner::{raw_type, RawLine, SessionAudit};

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
        // P2 backlog `classifier-zip-fix`: skip raw lines whose typed
        // parse produced a `ParseWarning::Json`/`Io` (i.e. the raw line
        // parsed as JSON but failed CopilotEvent deserialization, or
        // an I/O glitch dropped the read). Those lines appear in
        // `audit.raw_lines` but not in `audit.typed.events`; a naive
        // `.zip()` would shift the alignment for every event after.
        let aligned = aligned_raw_lines(&audit.raw_lines, &audit.typed.parse_warnings);
        for (raw, typed) in aligned.iter().zip(audit.typed.events.iter()) {
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

/// Return the subset of `raw_lines` whose `line_no` does NOT correspond
/// to a typed-pass `ParseWarning::Json` or `ParseWarning::Io`.
///
/// Both passes (raw → `serde_json::Value`, typed → [`CopilotEvent`])
/// independently skip empty lines, and `read_raw_lines` skips lines that
/// fail at the `Value` level too. But a line CAN parse as a generic
/// `Value` and then fail as a typed `CopilotEvent` (wrong field types,
/// missing required fields, etc.) — the typed pass records that as
/// `ParseWarning::Json { line_no, .. }` while `read_raw_lines` still
/// pushes the line into `raw_lines`. The two vectors are therefore
/// off-by-N from that line onward; a naive `raw_lines.iter().zip(...)`
/// mis-attributes every subsequent Unknown's wire `type` field.
///
/// `RawLine.line_no` is 1-based (per `scanner.rs::read_raw_lines`);
/// `ParseWarning.line_no` is 0-based (per `parser.rs::parse_events_jsonl`).
/// We bridge with `.saturating_sub(1)` on the raw side.
fn aligned_raw_lines<'a>(
    raw_lines: &'a [RawLine],
    parse_warnings: &[ParseWarning],
) -> Vec<&'a RawLine> {
    let bad: HashSet<usize> = parse_warnings
        .iter()
        .filter_map(|w| match w {
            ParseWarning::Json { line_no, .. } | ParseWarning::Io { line_no, .. } => Some(*line_no),
            _ => None,
        })
        .collect();
    raw_lines
        .iter()
        .filter(|r| !bad.contains(&r.line_no.saturating_sub(1)))
        .collect()
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

    #[test]
    fn aligned_raw_lines_realigns_around_typed_parse_warnings() {
        // P2 backlog `classifier-zip-fix`: 3 raw lines, line 2 (1-based)
        // produced a `ParseWarning::Json` (0-based line_no = 1), so the
        // aligned view must contain only lines 1 and 3 — preserving
        // positional sync with `typed.events` which has 2 entries.
        let raw_lines = vec![
            RawLine {
                line_no: 1,
                value: serde_json::json!({"type": "session.start"}),
            },
            RawLine {
                line_no: 2,
                value: serde_json::json!({"type": "tool.execution_start"}),
            },
            RawLine {
                line_no: 3,
                value: serde_json::json!({"type": "some.unknown"}),
            },
        ];
        let warnings = vec![ParseWarning::Json {
            line_no: 1, // 0-based ⇒ corresponds to RawLine.line_no = 2
            error: "missing field `toolCallId`".into(),
        }];
        let aligned = aligned_raw_lines(&raw_lines, &warnings);
        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[0].line_no, 1);
        assert_eq!(
            aligned[1].line_no, 3,
            "line 3 must follow line 1 directly, not the dropped line 2"
        );
    }

    #[test]
    fn aligned_raw_lines_handles_io_warning_same_as_json() {
        // `ParseWarning::Io` shares the same line_no field shape; both
        // branches must be skipped.
        let raw_lines = vec![
            RawLine {
                line_no: 1,
                value: serde_json::json!({}),
            },
            RawLine {
                line_no: 2,
                value: serde_json::json!({}),
            },
        ];
        let warnings = vec![ParseWarning::Io {
            line_no: 0, // 0-based ⇒ RawLine.line_no = 1
            error: "EOF mid-line".into(),
        }];
        let aligned = aligned_raw_lines(&raw_lines, &warnings);
        assert_eq!(aligned.len(), 1);
        assert_eq!(aligned[0].line_no, 2);
    }

    #[test]
    fn aligned_raw_lines_unaffected_by_non_line_warnings() {
        // `ParseWarning::OutOfOrder`, `UnclosedTurn`, etc. carry no
        // line_no and must not affect alignment.
        let raw_lines = vec![
            RawLine {
                line_no: 1,
                value: serde_json::json!({}),
            },
            RawLine {
                line_no: 2,
                value: serde_json::json!({}),
            },
        ];
        let warnings = vec![
            ParseWarning::OutOfOrder,
            ParseWarning::UnclosedTurn {
                turn_id: "t-1".into(),
            },
        ];
        let aligned = aligned_raw_lines(&raw_lines, &warnings);
        assert_eq!(aligned.len(), 2);
    }
}
