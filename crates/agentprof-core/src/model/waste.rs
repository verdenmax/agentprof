//! Data model for MCP-server waste analysis (M1.6.5).
//!
//! See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md` §5
//! for the design rationale and `docs/internals/adr-0015-mcp-waste-architecture.md`
//! for decision records.

use serde::{Deserialize, Serialize};

/// Top-level provenance summary for a [`WasteReport`]'s token numbers.
///
/// Distinguishes between heuristic-derived totals and sidecar-exact totals.
/// `Mixed` fires when some tools in the report had sidecar entries while
/// others fell back to the heuristic constant — common when the sidecar
/// covers only some MCP servers.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::TokenProvenance;
/// let p: TokenProvenance = serde_json::from_str("\"heuristic\"")?;
/// assert_eq!(p, TokenProvenance::Heuristic);
/// # Ok::<_, serde_json::Error>(())
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TokenProvenance {
    /// All tools in the report used the heuristic constant.
    #[default]
    Heuristic,
    /// All tools had sidecar-provided entries — every count is exact.
    SidecarExact,
    /// Some tools matched the sidecar; others fell back to heuristic.
    Mixed,
}

/// Per-tool provenance — distinguishes sidecar from heuristic at row granularity.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::TokenSource;
/// let s: TokenSource = serde_json::from_str("\"heuristic\"")?;
/// assert_eq!(s, TokenSource::Heuristic);
/// # Ok::<_, serde_json::Error>(())
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TokenSource {
    /// Token count came from the heuristic constant (no sidecar match).
    #[default]
    Heuristic,
    /// Token count came from a sidecar JSON entry — exact for the configured tokenizer.
    SidecarExact,
}

/// Which `tiktoken-rs` encoding was used to count tokens.
///
/// Auto-inferred per-session from `session.meta.model` (M1.6.6 T1.3 adds
/// `analyzer::waste::infer_tokenizer`); overridable via the
/// `WasteComputeContext` builder.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::TokenizerKind;
/// let k: TokenizerKind = serde_json::from_str("\"cl100k_base\"")?;
/// assert_eq!(k, TokenizerKind::Cl100kBase);
/// # Ok::<_, serde_json::Error>(())
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TokenizerKind {
    /// `GPT-4`, `ChatGPT`, `Claude` approximation.
    #[default]
    Cl100kBase,
    /// `GPT-4o`, `GPT-5` family.
    O200kBase,
}

/// Per-session MCP-server-waste report.
///
/// "Loaded" = wire ∪ `config_tools` ∪ `called.keys` (union semantics — any
/// tool ever observed in any of the three sources is considered loaded).
/// "Called" = MCP tools with ≥ 1 `tool.execution_start` event.
/// "Unused" = loaded ∖ called.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::{WasteReport, WasteDataSource};
/// let r: WasteReport = serde_json::from_str(r#"{
///     "server_waste": [],
///     "data_source": "None",
///     "total_loaded_tool_count": 0,
///     "total_unused_tool_count": 0
/// }"#).unwrap();
/// assert_eq!(r.total_loaded_tool_count, 0);
/// assert!(matches!(r.data_source, WasteDataSource::None));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WasteReport {
    /// Per-server breakdown, sorted by server name.
    pub server_waste: Vec<McpServerWaste>,
    /// Which source(s) supplied the loaded-tool set for this report.
    pub data_source: WasteDataSource,
    /// Total number of distinct MCP tools across all servers in the loaded set.
    pub total_loaded_tool_count: usize,
    /// Total number of MCP tools in the loaded set that were never called.
    pub total_unused_tool_count: usize,

    /// Sum of `description_tokens` across all loaded tools (M1.6.6).
    #[serde(default)]
    pub total_loaded_tokens: u64,

    /// Sum of `description_tokens` across only the unused tools (M1.6.6).
    #[serde(default)]
    pub total_unused_tokens: u64,

    /// Provenance of the token counts (heuristic / sidecar-exact / mixed) (M1.6.6).
    #[serde(default)]
    pub token_provenance: TokenProvenance,

    /// Which tiktoken encoding produced these counts (M1.6.6).
    #[serde(default)]
    pub tokenizer: TokenizerKind,
}

/// Per-MCP-server waste breakdown (one row per server).
///
/// # Examples
///
/// ```
/// use agentprof_core::model::McpServerWaste;
/// let s: McpServerWaste = serde_json::from_str(r#"{
///     "server": "github",
///     "tools": [],
///     "loaded_count": 0,
///     "called_count": 0,
///     "unused_count": 0,
///     "is_fully_unused": false
/// }"#).unwrap();
/// assert_eq!(s.server, "github");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct McpServerWaste {
    /// MCP server name (e.g. `"github"`, `"filesystem"`).
    pub server: String,
    /// Per-tool rows for this server.
    pub tools: Vec<McpToolWaste>,
    /// Number of tools observed in the loaded set for this server.
    pub loaded_count: usize,
    /// Number of tools with at least one `tool.execution_start` event.
    pub called_count: usize,
    /// `loaded_count - called_count`.
    pub unused_count: usize,
    /// `true` when `called_count == 0` and `loaded_count > 0`.
    pub is_fully_unused: bool,

    /// Σ `description_tokens` for tools in this server with `call_count == 0` (M1.6.6).
    #[serde(default)]
    pub unused_tokens: u64,

    /// Σ `description_tokens` for ALL tools loaded by this server (M1.6.6).
    #[serde(default)]
    pub loaded_tokens: u64,
}

/// Per-MCP-tool waste record (one row per tool inside a server).
///
/// # Examples
///
/// ```
/// use agentprof_core::model::McpToolWaste;
/// let t: McpToolWaste = serde_json::from_str(r#"{
///     "tool_name": "mcp__github__search",
///     "short_name": "search",
///     "call_count": 0,
///     "loaded_source": "Wire"
/// }"#).unwrap();
/// assert_eq!(t.call_count, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct McpToolWaste {
    /// Fully-qualified MCP tool name, e.g. `"mcp__github__search"`.
    pub tool_name: String,
    /// Short tool name with the `mcp__<server>__` prefix stripped.
    pub short_name: String,
    /// Number of `tool.execution_start` events for this tool in the session.
    pub call_count: usize,
    /// Which source(s) contributed this tool to the loaded set.
    pub loaded_source: LoadedSource,

    /// Token count of the full tool entry JSON
    /// (`name` + `description` + `inputSchema` + …) via `ctx.tokenizer` (M1.6.6).
    #[serde(default)]
    pub description_tokens: u64,

    /// Per-tool provenance — was this count from a sidecar match or heuristic? (M1.6.6)
    #[serde(default)]
    pub token_source: TokenSource,
}

/// Which source(s) contributed a tool to the "loaded" set.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::LoadedSource;
/// assert_ne!(LoadedSource::Wire, LoadedSource::Config);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LoadedSource {
    /// Observed in a wire-protocol `tools/list` response.
    Wire,
    /// Observed in the agent's static config file.
    Config,
    /// Observed in both wire and config.
    Both,
    /// Not observed in wire or config; inferred from a `tool.execution_start` event.
    InferredFromCall,
}

/// Top-level data-source provenance for a `WasteReport`.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::WasteDataSource;
/// assert_ne!(WasteDataSource::None, WasteDataSource::Wire);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WasteDataSource {
    /// No loaded-tool source was available (no wire, config, or calls).
    #[default]
    None,
    /// Loaded set came from wire-protocol `tools/list` only.
    Wire,
    /// Loaded set came from static config only.
    Config,
    /// Loaded set is the union of wire and config.
    Both,
}

/// Cross-session aggregated waste report (used by `mcp-waste` subcommand).
///
/// # Examples
///
/// ```
/// use agentprof_core::model::AggregateWasteReport;
/// let r: AggregateWasteReport = serde_json::from_str(r#"{
///     "sessions": 0,
///     "per_server": [],
///     "never_called_tools": []
/// }"#).unwrap();
/// assert_eq!(r.sessions, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AggregateWasteReport {
    /// Number of sessions included in the aggregate.
    pub sessions: usize,
    /// Per-server cross-session rows.
    pub per_server: Vec<McpServerCrossWaste>,
    /// Fully-qualified tool names that were loaded in ≥ 1 session but never called in any.
    pub never_called_tools: Vec<String>,

    /// Sum of unused-tool token counts across all sessions in the aggregate (M1.6.6).
    #[serde(default)]
    pub total_unused_tokens: u64,
}

/// Per-MCP-server cross-session waste record.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::McpServerCrossWaste;
/// let s: McpServerCrossWaste = serde_json::from_str(r#"{
///     "server": "github",
///     "sessions_loaded": 0,
///     "sessions_with_zero_calls": 0,
///     "tool_usage": []
/// }"#).unwrap();
/// assert_eq!(s.server, "github");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct McpServerCrossWaste {
    /// MCP server name.
    pub server: String,
    /// Number of sessions where this server contributed ≥ 1 loaded tool.
    pub sessions_loaded: usize,
    /// Number of sessions where this server was loaded but had zero tool calls.
    pub sessions_with_zero_calls: usize,
    /// Per-tool cross-session rows for this server.
    pub tool_usage: Vec<McpToolUsageAcrossSessions>,

    /// Sum of unused-tool token counts for this server across all sessions (M1.6.6).
    #[serde(default)]
    pub total_unused_tokens: u64,
}

/// Per-MCP-tool cross-session usage record.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::McpToolUsageAcrossSessions;
/// let t: McpToolUsageAcrossSessions = serde_json::from_str(r#"{
///     "tool_name": "mcp__github__search",
///     "sessions_loaded": 0,
///     "sessions_called": 0,
///     "total_call_count": 0
/// }"#).unwrap();
/// assert_eq!(t.total_call_count, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct McpToolUsageAcrossSessions {
    /// Fully-qualified MCP tool name.
    pub tool_name: String,
    /// Number of sessions where this tool was in the loaded set.
    pub sessions_loaded: usize,
    /// Number of sessions where this tool had ≥ 1 call.
    pub sessions_called: usize,
    /// Sum of `call_count` across all sessions.
    pub total_call_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waste_report_serde_round_trip_empty() {
        let r = WasteReport {
            server_waste: vec![],
            data_source: WasteDataSource::None,
            total_loaded_tool_count: 0,
            total_unused_tool_count: 0,
            total_loaded_tokens: 0,
            total_unused_tokens: 0,
            token_provenance: TokenProvenance::Heuristic,
            tokenizer: TokenizerKind::Cl100kBase,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: WasteReport = serde_json::from_str(&s).unwrap();
        assert_eq!(back.total_loaded_tool_count, 0);
        assert!(matches!(back.data_source, WasteDataSource::None));
    }

    #[test]
    fn waste_report_serde_round_trip_full() {
        let r = WasteReport {
            server_waste: vec![McpServerWaste {
                server: "github".into(),
                tools: vec![McpToolWaste {
                    tool_name: "mcp__github__search".into(),
                    short_name: "search".into(),
                    call_count: 3,
                    loaded_source: LoadedSource::Both,
                    description_tokens: 0,
                    token_source: TokenSource::Heuristic,
                }],
                loaded_count: 1,
                called_count: 1,
                unused_count: 0,
                is_fully_unused: false,
                unused_tokens: 0,
                loaded_tokens: 0,
            }],
            data_source: WasteDataSource::Both,
            total_loaded_tool_count: 1,
            total_unused_tool_count: 0,
            total_loaded_tokens: 0,
            total_unused_tokens: 0,
            token_provenance: TokenProvenance::Heuristic,
            tokenizer: TokenizerKind::Cl100kBase,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: WasteReport = serde_json::from_str(&s).unwrap();
        assert_eq!(back.server_waste.len(), 1);
        assert_eq!(back.server_waste[0].server, "github");
        assert_eq!(back.server_waste[0].tools[0].call_count, 3);
    }

    #[test]
    fn loaded_source_serde_all_variants() {
        for src in [
            LoadedSource::Wire,
            LoadedSource::Config,
            LoadedSource::Both,
            LoadedSource::InferredFromCall,
        ] {
            let s = serde_json::to_string(&src).unwrap();
            let back: LoadedSource = serde_json::from_str(&s).unwrap();
            assert_eq!(back, src);
        }
    }

    #[test]
    fn waste_data_source_serde_all_variants() {
        for src in [
            WasteDataSource::None,
            WasteDataSource::Wire,
            WasteDataSource::Config,
            WasteDataSource::Both,
        ] {
            let s = serde_json::to_string(&src).unwrap();
            let back: WasteDataSource = serde_json::from_str(&s).unwrap();
            assert_eq!(back, src);
        }
    }

    #[test]
    fn aggregate_waste_report_serde_round_trip() {
        let r = AggregateWasteReport {
            sessions: 5,
            per_server: vec![McpServerCrossWaste {
                server: "github".into(),
                sessions_loaded: 5,
                sessions_with_zero_calls: 3,
                tool_usage: vec![McpToolUsageAcrossSessions {
                    tool_name: "mcp__github__search".into(),
                    sessions_loaded: 5,
                    sessions_called: 2,
                    total_call_count: 7,
                }],
                total_unused_tokens: 0,
            }],
            never_called_tools: vec!["mcp__github__delete".into()],
            total_unused_tokens: 0,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: AggregateWasteReport = serde_json::from_str(&s).unwrap();
        assert_eq!(back.sessions, 5);
        assert_eq!(back.never_called_tools.len(), 1);
    }

    #[test]
    fn token_provenance_serde_all_variants() {
        for src in [
            TokenProvenance::Heuristic,
            TokenProvenance::SidecarExact,
            TokenProvenance::Mixed,
        ] {
            let s = serde_json::to_string(&src).unwrap();
            let back: TokenProvenance = serde_json::from_str(&s).unwrap();
            assert_eq!(back, src);
        }
    }

    #[test]
    fn token_source_serde_all_variants() {
        for src in [TokenSource::Heuristic, TokenSource::SidecarExact] {
            let s = serde_json::to_string(&src).unwrap();
            let back: TokenSource = serde_json::from_str(&s).unwrap();
            assert_eq!(back, src);
        }
    }

    #[test]
    fn tokenizer_kind_serde_all_variants() {
        for k in [TokenizerKind::Cl100kBase, TokenizerKind::O200kBase] {
            let s = serde_json::to_string(&k).unwrap();
            let back: TokenizerKind = serde_json::from_str(&s).unwrap();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn waste_report_serde_default_values_for_new_fields() {
        // Backward-compat: pre-M1.6.6 JSON snapshots (without new fields)
        // must deserialize cleanly. Field defaults: 0 / Heuristic / Cl100kBase.
        // Note: `data_source` uses PascalCase variants (no rename_all) — matches
        // the actual M1.6.5 wire format, see existing doctest on `WasteReport`.
        let pre_m166_json = serde_json::json!({
            "server_waste": [],
            "data_source": "None",
            "total_loaded_tool_count": 0,
            "total_unused_tool_count": 0,
        });
        let r: WasteReport = serde_json::from_value(pre_m166_json).unwrap();
        assert_eq!(r.total_loaded_tokens, 0);
        assert_eq!(r.total_unused_tokens, 0);
        assert!(matches!(r.token_provenance, TokenProvenance::Heuristic));
        assert!(matches!(r.tokenizer, TokenizerKind::Cl100kBase));
    }
}
