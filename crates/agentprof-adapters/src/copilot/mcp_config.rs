//! Best-effort `~/.copilot/mcp.json` parser.
//!
//! Returns `None` on missing / unreadable / unparseable. Recognizes two
//! common schemas; degrades to empty `ParsedMcpConfig` on unknown shapes
//! (always non-fatal).
//!
//! See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md` §6.2.

use std::collections::BTreeMap;
use std::path::Path;

/// Outcome of best-effort `mcp.json` parsing. `tools = None` when the
/// schema does not enumerate per-server tool lists (most common `VSCode`
/// shape); `tools = Some(_)` when present (self-describing shape).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ParsedMcpConfig {
    /// Server name → metadata. Empty when no recognized top-level key
    /// was present (unknown schema).
    pub servers: BTreeMap<String, ServerInfo>,
}

/// Per-server metadata extracted from `mcp.json`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ServerInfo {
    /// `Some(list)` when the schema enumerates per-server tools
    /// (self-describing); `None` when only the server registration is
    /// known (`VSCode` shape — tools are discovered at runtime).
    pub tools: Option<Vec<String>>,
}

/// Try-load `path` as mcp.json. Returns `None` if the file doesn't exist
/// or any error occurs; the caller treats `None` as "config baseline
/// unavailable" and relies on the wire source.
///
/// # Examples
///
/// ```
/// use agentprof_adapters::copilot::mcp_config::load_mcp_config;
/// let r = load_mcp_config(std::path::Path::new("/nonexistent"));
/// assert!(r.is_none());
/// ```
#[must_use]
#[tracing::instrument(name = "adapter.mcp_config", skip_all, fields(path = %path.display()))]
pub fn load_mcp_config(path: &Path) -> Option<ParsedMcpConfig> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, "mcp.json read failed");
            }
            return None;
        }
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "mcp.json JSON parse failed");
            return None;
        }
    };
    Some(parse_value(&value))
}

fn parse_value(v: &serde_json::Value) -> ParsedMcpConfig {
    if let Some(map) = v.get("mcpServers").and_then(|x| x.as_object()) {
        return ParsedMcpConfig {
            servers: map
                .keys()
                .map(|k| (k.clone(), ServerInfo { tools: None }))
                .collect(),
        };
    }
    if let Some(map) = v.get("servers").and_then(|x| x.as_object()) {
        return ParsedMcpConfig {
            servers: map
                .iter()
                .map(|(k, val)| {
                    let tools = val.get("tools").and_then(|t| t.as_array()).map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    });
                    (k.clone(), ServerInfo { tools })
                })
                .collect(),
        };
    }
    tracing::warn!("mcp.json schema not recognized (expected `mcpServers` or `servers`)");
    ParsedMcpConfig::default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_missing_file_returns_none() {
        let r = load_mcp_config(std::path::Path::new("/this/path/does/not/exist/mcp.json"));
        assert!(r.is_none());
    }

    #[test]
    fn load_invalid_json_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mcp.json");
        std::fs::write(&p, "{ not valid json").unwrap();
        let r = load_mcp_config(&p);
        assert!(r.is_none());
    }

    #[test]
    fn load_vscode_schema_extracts_server_names_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mcp.json");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(
            f,
            r#"{{
  "mcpServers": {{
    "github": {{ "command": "npx", "args": ["-y", "@github/mcp"] }},
    "filesystem": {{ "command": "fs-server" }}
  }}
}}"#,
        )
        .unwrap();
        let cfg = load_mcp_config(&p).expect("parse");
        assert_eq!(cfg.servers.len(), 2);
        assert!(
            cfg.servers["github"].tools.is_none(),
            "VSCode schema does not list tools"
        );
        assert!(cfg.servers.contains_key("filesystem"));
    }

    #[test]
    fn load_self_describing_schema_extracts_tool_lists() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mcp.json");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(
            f,
            r#"{{
  "servers": {{
    "github": {{ "tools": ["search", "create", "delete"] }}
  }}
}}"#,
        )
        .unwrap();
        let cfg = load_mcp_config(&p).expect("parse");
        let tools = cfg.servers["github"].tools.as_ref().expect("tools listed");
        assert_eq!(tools.len(), 3);
        assert!(tools.contains(&"search".to_string()));
    }

    #[test]
    fn load_unknown_schema_returns_empty_parsed_config() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("mcp.json");
        std::fs::write(&p, r#"{ "totallyDifferentKey": [1, 2, 3] }"#).unwrap();
        let cfg = load_mcp_config(&p).expect("parse-as-empty");
        assert!(cfg.servers.is_empty());
    }
}
