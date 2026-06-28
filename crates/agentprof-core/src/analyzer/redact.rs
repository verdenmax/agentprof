//! Opt-in report redaction (`--privacy`).
//!
//! Strips 🔴 HIGH PII at the report layer so every export format inherits
//! it. See `docs/superpowers/specs/2026-06-28-privacy-redaction-design.md`
//! and [ADR-0026](../../../docs/internals/adr-0026-report-redaction.md).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::analyzer::aggregate::bucket::{DayBucket, McpServerBucket, ModelBucket, ToolBucket};
use crate::analyzer::aggregate::AggregateReport;

/// Redaction strength selected by the `--privacy` flag.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::redact::PrivacyLevel;
/// assert_eq!(PrivacyLevel::default(), PrivacyLevel::None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "clap-derive", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PrivacyLevel {
    /// No redaction (default; zero behavior change).
    #[default]
    None,
    /// Strip 🔴 HIGH fields (cwd/branch/repository/UUIDs, model→family).
    Redact,
    /// `Redact` + `version`/`started_at` + MCP-name hashing + sidecar map.
    Anonymize,
}

/// Collapse a model identifier to its family (first two `-`-segments).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::redact::model_family;
/// assert_eq!(model_family("claude-opus-4.7-1m-internal"), "claude-opus");
/// ```
#[must_use]
pub fn model_family(model: &str) -> String {
    model.split('-').take(2).collect::<Vec<_>>().join("-")
}

/// Hash the server segment of an `mcp__server__tool` name, keeping the tool.
/// Non-MCP names (and malformed ones) are returned unchanged.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::redact::hash_mcp_tool_name;
/// let h = hash_mcp_tool_name("mcp__github__search_issues");
/// assert!(h.ends_with("__search_issues"));
/// assert_eq!(hash_mcp_tool_name("bash"), "bash");
/// ```
#[must_use]
pub fn hash_mcp_tool_name(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("mcp__") {
        if let Some((server, tool)) = rest.split_once("__") {
            return format!(
                "mcp__{}__{}",
                crate::observability::pii::hash_short(server),
                tool
            );
        }
    }
    name.to_string()
}

/// Allocates stable `<uuid-N>` replacements in first-seen order.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::redact::UuidRedactor;
/// let mut r = UuidRedactor::default();
/// assert_eq!(r.redact("session-aaa"), "<uuid-0>");
/// assert_eq!(r.redact("session-aaa"), "<uuid-0>"); // stable
/// ```
#[derive(Debug, Default)]
pub struct UuidRedactor {
    counter: usize,
    map: BTreeMap<String, String>, // original → <uuid-N>
}

impl UuidRedactor {
    /// Return the stable replacement for `original`, allocating on first sight.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::redact::UuidRedactor;
    /// let mut r = UuidRedactor::default();
    /// assert_eq!(r.redact("a"), "<uuid-0>");
    /// assert_eq!(r.redact("b"), "<uuid-1>");
    /// ```
    pub fn redact(&mut self, original: &str) -> String {
        if let Some(r) = self.map.get(original) {
            return r.clone();
        }
        let r = format!("<uuid-{}>", self.counter);
        self.counter += 1;
        self.map.insert(original.to_string(), r.clone());
        r
    }

    /// Invert to `<uuid-N> → original` for the exported [`RedactionMap`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::redact::UuidRedactor;
    /// let mut r = UuidRedactor::default();
    /// r.redact("aaa");
    /// let inv = r.into_inverse();
    /// assert_eq!(inv.get("<uuid-0>").map(String::as_str), Some("aaa"));
    /// ```
    #[must_use]
    pub fn into_inverse(self) -> BTreeMap<String, String> {
        self.map.into_iter().map(|(o, r)| (r, o)).collect()
    }
}

/// Replacement→original maps so a holder of a redacted report can un-redact.
/// Empty for `Redact`; filled for `Anonymize`.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::redact::RedactionMap;
/// assert!(RedactionMap::default().is_empty());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RedactionMap {
    /// `<uuid-N>` → original UUID.
    pub uuids: BTreeMap<String, String>,
    /// family → original model identifier.
    pub models: BTreeMap<String, String>,
    /// `<hash8>` → original MCP server.
    pub mcp_servers: BTreeMap<String, String>,
}

impl RedactionMap {
    /// `true` when nothing was recorded (the `Redact`-level common case).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::redact::RedactionMap;
    /// assert!(RedactionMap::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.uuids.is_empty() && self.models.is_empty() && self.mcp_servers.is_empty()
    }
}

impl crate::analyzer::AnalysisReport {
    /// Return a redacted copy plus a [`RedactionMap`] (non-empty only for
    /// [`PrivacyLevel::Anonymize`]). Pure: never fails, never panics.
    ///
    /// At [`PrivacyLevel::None`] this is the identity (clone + empty map).
    /// At [`PrivacyLevel::Redact`] 🔴 HIGH fields (`cwd` / `branch` /
    /// `repository`) become `<redacted>`, UUIDs become stable `<uuid-N>`
    /// placeholders and models collapse to their family; the returned map
    /// is empty. [`PrivacyLevel::Anonymize`] additionally strips
    /// `agent_version` / `producer`, zeroes `started_at`, hashes MCP server
    /// names, and fills the [`RedactionMap`] so a trusted holder can invert.
    ///
    /// Diagnostic `warnings` / `parse_warnings` are cleared under redaction
    /// because their payloads embed raw event UUIDs and timestamps; they are
    /// local diagnostics, not part of the shareable ROI signal.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::analyzer::redact::PrivacyLevel;
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let r = AnalysisReport::new(SessionMeta::new("id".into(), AgentKind::Copilot, Utc::now(), false));
    /// let (red, map) = r.redact(PrivacyLevel::Redact);
    /// assert!(map.is_empty());
    /// ```
    #[must_use]
    pub fn redact(&self, level: PrivacyLevel) -> (Self, RedactionMap) {
        if level == PrivacyLevel::None {
            return (self.clone(), RedactionMap::default());
        }
        let anon = level == PrivacyLevel::Anonymize;
        let mut out = self.clone();
        let mut uuids = UuidRedactor::default();
        let mut models: BTreeMap<String, String> = BTreeMap::new();
        let mut servers: BTreeMap<String, String> = BTreeMap::new();

        // meta — 🔴 HIGH (both levels)
        redact_opt(&mut out.meta.cwd);
        redact_opt(&mut out.meta.branch);
        redact_opt(&mut out.meta.repository);
        out.meta.id = uuids.redact(&out.meta.id);

        // meta — anonymize-only
        if anon {
            redact_opt(&mut out.meta.agent_version);
            redact_opt(&mut out.meta.producer);
            // 1970 = sentinel (started_at can't be "<redacted>")
            out.meta.started_at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
        }

        // turn rows — stable UUIDs (cross-site stable with meta.id) + model→family
        for row in &mut out.turn_summary {
            row.turn_id = uuids.redact(&row.turn_id);
            if let Some(m) = row.model.take() {
                let fam = model_family(&m);
                models.entry(fam.clone()).or_insert(m);
                row.model = Some(fam);
            }
        }

        // model_metrics — collapse to family, MERGING counters on collision
        if let Some(mm) = out.model_metrics.take() {
            let mut new_mm: BTreeMap<String, crate::analyzer::ModelUsage> = BTreeMap::new();
            for (model, usage) in mm {
                let fam = model_family(&model);
                models.entry(fam.clone()).or_insert(model);
                let slot = new_mm.entry(fam).or_default();
                slot.merge(&usage);
            }
            out.model_metrics = Some(new_mm);
        }

        // tool names — anonymize-only: hash MCP server segment
        if anon {
            for row in &mut out.tool_rank {
                record_mcp_server(&row.name, &mut servers);
                row.name = hash_mcp_tool_name(&row.name);
            }
            out.loaded_mcp_tools = out
                .loaded_mcp_tools
                .iter()
                .map(|t| {
                    record_mcp_server(t, &mut servers);
                    hash_mcp_tool_name(t)
                })
                .collect();
        }

        // diagnostics cleared — embed raw ids/timestamps
        out.warnings = Vec::new();
        out.parse_warnings = Vec::new();

        let map = if anon {
            RedactionMap {
                uuids: uuids.into_inverse(),
                models,
                mcp_servers: servers,
            }
        } else {
            RedactionMap::default()
        };
        (out, map)
    }
}

/// Replace a present `Option<String>` with `<redacted>` (keeps `None`).
fn redact_opt(field: &mut Option<String>) {
    if field.is_some() {
        *field = Some("<redacted>".to_string());
    }
}

/// If `name` is `mcp__server__tool`, record `hash8 → server` for the map.
fn record_mcp_server(name: &str, out: &mut BTreeMap<String, String>) {
    if let Some(rest) = name.strip_prefix("mcp__") {
        if let Some((server, _tool)) = rest.split_once("__") {
            out.entry(crate::observability::pii::hash_short(server))
                .or_insert_with(|| server.to_string());
        }
    }
}

/// Redact a single [`AggregateReport`] bucket's identifying key in place.
///
/// One impl per bucket type encodes the spec §7 per-key policy:
/// model → family (both levels), MCP server / tool → hashed
/// (`Anonymize` only), day → never (D-7, it is the aggregation
/// dimension). The two `BTreeMap` accumulators collect reversible
/// `replacement → original` entries for the exported [`RedactionMap`].
pub trait RedactBucket {
    /// Redact this bucket's key field, recording reversible mappings into
    /// `models` (family → original) and `servers` (`hash8` → original).
    ///
    /// No-op at [`PrivacyLevel::None`]; impls that only act at
    /// [`PrivacyLevel::Anonymize`] additionally no-op at
    /// [`PrivacyLevel::Redact`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use agentprof_core::analyzer::aggregate::ModelBucket;
    /// use agentprof_core::analyzer::redact::{PrivacyLevel, RedactBucket};
    /// use chrono::Duration;
    ///
    /// let mut b = ModelBucket::new("claude-opus-4.7-1m".into(), 0, 0, 0, Duration::zero());
    /// let (mut models, mut servers) = (BTreeMap::new(), BTreeMap::new());
    /// b.redact_key(PrivacyLevel::Redact, &mut models, &mut servers);
    /// assert_eq!(b.model, "claude-opus");
    /// ```
    fn redact_key(
        &mut self,
        level: PrivacyLevel,
        models: &mut BTreeMap<String, String>,
        servers: &mut BTreeMap<String, String>,
    );
}

impl RedactBucket for ModelBucket {
    fn redact_key(
        &mut self,
        level: PrivacyLevel,
        models: &mut BTreeMap<String, String>,
        _servers: &mut BTreeMap<String, String>,
    ) {
        if level == PrivacyLevel::None {
            return;
        }
        // model → family at BOTH Redact and Anonymize (spec §7)
        let fam = model_family(&self.model);
        models
            .entry(fam.clone())
            .or_insert_with(|| self.model.clone());
        self.model = fam;
    }
}

impl RedactBucket for McpServerBucket {
    fn redact_key(
        &mut self,
        level: PrivacyLevel,
        _models: &mut BTreeMap<String, String>,
        servers: &mut BTreeMap<String, String>,
    ) {
        // MCP names are 🟡 MEDIUM: hash only at Anonymize.
        if level != PrivacyLevel::Anonymize {
            return;
        }
        let h = crate::observability::pii::hash_short(&self.server);
        servers
            .entry(h.clone())
            .or_insert_with(|| self.server.clone());
        self.server = h;
    }
}

impl RedactBucket for ToolBucket {
    fn redact_key(
        &mut self,
        level: PrivacyLevel,
        _models: &mut BTreeMap<String, String>,
        servers: &mut BTreeMap<String, String>,
    ) {
        // Hash the MCP server segment only at Anonymize; non-MCP names
        // and the `source` field are left untouched.
        if level != PrivacyLevel::Anonymize {
            return;
        }
        record_mcp_server(&self.name, servers);
        self.name = hash_mcp_tool_name(&self.name);
    }
}

impl RedactBucket for DayBucket {
    fn redact_key(
        &mut self,
        _level: PrivacyLevel,
        _models: &mut BTreeMap<String, String>,
        _servers: &mut BTreeMap<String, String>,
    ) {
        // D-7: the day is the aggregation dimension; never redacted.
    }
}

impl<B: RedactBucket + Clone> AggregateReport<B> {
    /// Return a redacted copy of this report plus a [`RedactionMap`]
    /// (non-empty only for [`PrivacyLevel::Anonymize`]). Pure: never
    /// fails, never panics.
    ///
    /// Only each bucket's identifying key is rewritten, per [`RedactBucket`]:
    /// model → family (both levels), MCP server / tool → hashed
    /// (`Anonymize` only), day → never. Summary fields (`by` / `since` /
    /// counts / `total_wall_duration`) and non-key bucket metrics carry no
    /// PII and are preserved verbatim. At [`PrivacyLevel::None`] this is the
    /// identity (clone + empty map).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::{AggregateKey, AggregateReport, ModelBucket};
    /// use agentprof_core::analyzer::redact::PrivacyLevel;
    /// use chrono::Duration;
    ///
    /// let report: AggregateReport<ModelBucket> = AggregateReport::new(
    ///     AggregateKey::Model, None, 0, 0, Duration::zero(),
    ///     vec![ModelBucket::new("claude-opus-4.7-1m".into(), 0, 0, 0, Duration::zero())],
    /// );
    /// let (out, map) = report.redact(PrivacyLevel::Redact);
    /// assert_eq!(out.buckets[0].model, "claude-opus");
    /// assert!(map.is_empty()); // map filled only at Anonymize
    /// ```
    #[must_use]
    pub fn redact(&self, level: PrivacyLevel) -> (Self, RedactionMap) {
        if level == PrivacyLevel::None {
            return (self.clone(), RedactionMap::default());
        }
        let anon = level == PrivacyLevel::Anonymize;
        let mut out = self.clone();
        let mut models: BTreeMap<String, String> = BTreeMap::new();
        let mut servers: BTreeMap<String, String> = BTreeMap::new();
        for b in &mut out.buckets {
            b.redact_key(level, &mut models, &mut servers);
        }
        let map = if anon {
            RedactionMap {
                uuids: BTreeMap::new(),
                models,
                mcp_servers: servers,
            }
        } else {
            RedactionMap::default()
        };
        (out, map)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn model_family_takes_first_two_segments() {
        assert_eq!(model_family("claude-opus-4.7-1m-internal"), "claude-opus");
        assert_eq!(model_family("gpt-5-mini"), "gpt-5");
        assert_eq!(model_family("o1"), "o1"); // fewer than 2 segments kept
        assert_eq!(model_family(""), "");
    }

    #[test]
    fn hash_mcp_tool_keeps_tool_segment() {
        let h = hash_mcp_tool_name("mcp__github__search_issues");
        assert!(h.starts_with("mcp__"), "got {h}");
        assert!(h.ends_with("__search_issues"), "got {h}");
        assert_ne!(h, "mcp__github__search_issues"); // server hashed
        assert_eq!(hash_mcp_tool_name("bash"), "bash"); // non-MCP unchanged
        assert_eq!(hash_mcp_tool_name("mcp__only_server"), "mcp__only_server"); // no tool sep
    }

    #[test]
    fn uuid_redactor_assigns_stable_replacements() {
        let mut r = UuidRedactor::default();
        assert_eq!(r.redact("aaa"), "<uuid-0>");
        assert_eq!(r.redact("bbb"), "<uuid-1>");
        assert_eq!(r.redact("aaa"), "<uuid-0>"); // cached → stable
        let inv = r.into_inverse();
        assert_eq!(inv.get("<uuid-0>").map(String::as_str), Some("aaa"));
    }

    #[test]
    fn privacy_level_default_is_none() {
        assert_eq!(PrivacyLevel::default(), PrivacyLevel::None);
    }
}
