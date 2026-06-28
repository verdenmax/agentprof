//! Opt-in report redaction (`--privacy`).
//!
//! Strips 🔴 HIGH PII at the report layer so every export format inherits
//! it. See `docs/superpowers/specs/2026-06-28-privacy-redaction-design.md`
//! and [ADR-0026](../../../docs/internals/adr-0026-report-redaction.md).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
