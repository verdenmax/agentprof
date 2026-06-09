//! `compute_waste` and `aggregate_waste` — per-session and cross-session
//! MCP-server waste analysis.
//!
//! `compute_waste` turns one (`AnalysisReport`, [`WasteComputeContext`])
//! pair into a [`WasteReport`] (the context carries `wire_loaded`,
//! optional `config_loaded`, optional sidecar, heuristic constant, and
//! tokenizer choice); `aggregate_waste` rolls many `WasteReport`s up
//! into an [`AggregateWasteReport`] (used by the `mcp-waste` subcommand).
//!
//! See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md` §6
//! for the algorithm and `docs/internals/adr-0015-mcp-waste-architecture.md`
//! (M1.6.5) + `docs/internals/adr-0016-mcp-token-cost-architecture.md`
//! (M1.6.6) for design decisions.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tiktoken_rs::CoreBPE;

use crate::adapter::SessionRef;
use crate::analyzer::AnalysisReport;
use crate::model::{
    AggregateWasteReport, LoadedSource, McpServerCrossWaste, McpServerWaste,
    McpToolUsageAcrossSessions, McpToolWaste, TokenProvenance, TokenSource, TokenizerKind,
    ToolSource, WasteDataSource, WasteReport,
};

/// Default heuristic token cost per MCP tool when no sidecar is provided.
///
/// Chosen to approximate "short description + small input schema" for a
/// typical MCP tool entry. See ADR-0016 D-3 for the rationale and the
/// follow-up calibration plan.
pub const DEFAULT_HEURISTIC_TOKENS: u64 = 200;

/// Lookup interface for a *tool sidecar* — a per-agent registry mapping a
/// fully-qualified MCP tool name (e.g. `mcp__github__search`) to the JSON
/// blob that the agent actually sees in its context window.
///
/// `agentprof-core` is a dependency-graph leaf (it cannot depend on
/// `agentprof-adapters`), so the concrete `Sidecar` type lives in
/// `agentprof-adapters::copilot::tool_sidecar` and implements this trait.
/// `WasteComputeContext::with_sidecar` and [`compute_token_cost_for_tool`]
/// take `&dyn SidecarLookup` to stay adapter-agnostic.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::waste::{SidecarLookup, SidecarToolEntry};
///
/// struct StubEntry(&'static str);
/// impl SidecarToolEntry for StubEntry {
///     fn to_json_string(&self) -> String { self.0.into() }
/// }
///
/// struct StubSidecar;
/// impl SidecarLookup for StubSidecar {
///     fn lookup(&self, _: &str) -> Option<&dyn SidecarToolEntry> { None }
/// }
///
/// let s = StubSidecar;
/// assert!(s.lookup("anything").is_none());
/// ```
pub trait SidecarLookup {
    /// Return the sidecar entry for `full_name` (e.g. `mcp__github__search`),
    /// or `None` if the tool is not in the sidecar.
    fn lookup(&self, full_name: &str) -> Option<&dyn SidecarToolEntry>;
}

/// One entry in a [`SidecarLookup`] — yields the agent-visible JSON for a
/// single MCP tool. Token cost is computed by encoding this string.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::waste::SidecarToolEntry;
///
/// struct E;
/// impl SidecarToolEntry for E {
///     fn to_json_string(&self) -> String { r#"{"name":"x"}"#.into() }
/// }
/// assert_eq!(E.to_json_string(), r#"{"name":"x"}"#);
/// ```
pub trait SidecarToolEntry {
    /// Serialize the entry to the JSON string the agent observes in its
    /// context window. Empty string is acceptable on serialization failure
    /// (caller will simply attribute 0 tokens to that tool).
    fn to_json_string(&self) -> String;
}

/// Builder-pattern context for [`compute_waste`] (T1.4 will switch the
/// signature to take `&WasteComputeContext`).
///
/// Adding fields here is non-breaking (struct is `#[non_exhaustive]`);
/// adding new `with_*` methods is purely additive. See ADR-0016 D-5 for
/// the choice of this shape over flat positional parameters.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use agentprof_core::analyzer::waste::{WasteComputeContext, DEFAULT_HEURISTIC_TOKENS};
/// use agentprof_core::model::TokenizerKind;
///
/// let wire = BTreeSet::new();
/// let ctx = WasteComputeContext::new(&wire)
///     .with_heuristic(150)
///     .with_tokenizer(TokenizerKind::O200kBase);
/// assert_eq!(ctx.heuristic_tokens_per_tool, 150);
/// assert_eq!(ctx.tokenizer, TokenizerKind::O200kBase);
/// ```
#[non_exhaustive]
pub struct WasteComputeContext<'a> {
    /// Tools observed in `<tools_changed_notice>` blocks.
    pub wire_loaded: &'a BTreeSet<String>,
    /// Server → tool-list map from `mcp.json` (`None` if absent/unparseable).
    pub config_loaded: Option<&'a BTreeMap<String, Vec<String>>>,
    /// Sidecar for exact token counts; `None` falls back to heuristic only.
    pub sidecar: Option<&'a dyn SidecarLookup>,
    /// Token cost assumed for tools without a sidecar match.
    pub heuristic_tokens_per_tool: u64,
    /// Which `tiktoken-rs` encoding to use for sidecar JSON.
    pub tokenizer: TokenizerKind,
    /// Pre-built BPE encoder, shared across many `compute_waste` calls.
    ///
    /// When `Some`, [`compute_waste`] reuses this encoder rather than
    /// re-building `cl100k_base()` / `o200k_base()` per call (~50 ms +
    /// tens of MB allocate-drop each time). CLI driver code constructs
    /// one [`Arc<CoreBPE>`] per invocation (via [`build_bpe`]) and
    /// hands it to every per-session context via [`Self::with_bpe`].
    /// When `None`, `compute_waste` falls back to per-call construction
    /// (preserving the single-session ergonomic path).
    pub bpe: Option<Arc<CoreBPE>>,
}

impl<'a> WasteComputeContext<'a> {
    /// Minimal constructor.
    ///
    /// Defaults: no config, no sidecar, heuristic = [`DEFAULT_HEURISTIC_TOKENS`],
    /// tokenizer = [`TokenizerKind::Cl100kBase`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeSet;
    /// use agentprof_core::analyzer::waste::WasteComputeContext;
    /// let wire = BTreeSet::new();
    /// let ctx = WasteComputeContext::new(&wire);
    /// assert!(ctx.config_loaded.is_none());
    /// ```
    #[must_use]
    pub fn new(wire_loaded: &'a BTreeSet<String>) -> Self {
        Self {
            wire_loaded,
            config_loaded: None,
            sidecar: None,
            heuristic_tokens_per_tool: DEFAULT_HEURISTIC_TOKENS,
            tokenizer: TokenizerKind::Cl100kBase,
            bpe: None,
        }
    }

    /// Attach an optional `mcp.json`-derived config map (M1.6.5 source).
    #[must_use]
    pub const fn with_config(mut self, config_loaded: &'a BTreeMap<String, Vec<String>>) -> Self {
        self.config_loaded = Some(config_loaded);
        self
    }

    /// Attach an optional sidecar — when present, sidecar-matching tools
    /// switch to exact-token mode via [`compute_token_cost_for_tool`].
    #[must_use]
    pub fn with_sidecar(mut self, sidecar: &'a dyn SidecarLookup) -> Self {
        self.sidecar = Some(sidecar);
        self
    }

    /// Override the heuristic token-per-tool constant (default
    /// [`DEFAULT_HEURISTIC_TOKENS`]).
    #[must_use]
    pub const fn with_heuristic(mut self, heuristic_tokens_per_tool: u64) -> Self {
        self.heuristic_tokens_per_tool = heuristic_tokens_per_tool;
        self
    }

    /// Override the tokenizer (otherwise pair with [`infer_tokenizer`] at
    /// the call site).
    #[must_use]
    pub const fn with_tokenizer(mut self, tokenizer: TokenizerKind) -> Self {
        self.tokenizer = tokenizer;
        self
    }

    /// Attach a pre-built BPE encoder, shared across many sessions.
    ///
    /// Building `cl100k_base()` / `o200k_base()` parses ~100–200k lines
    /// of embedded merge tables and compiles a regex — ~50 ms + tens
    /// of MB allocate-drop per invocation. In aggregate / mcp-waste
    /// workflows (N sessions in a loop) the CLI should call
    /// [`build_bpe`] once per command, wrap the result in
    /// [`Arc<CoreBPE>`], and pass `bpe.clone()` to every per-session
    /// context. The encoder is referenced read-only inside
    /// [`compute_waste`], so cloning the [`Arc`] is the cheap path.
    ///
    /// When this is *not* set, [`compute_waste`] falls back to
    /// per-call construction — convenient for the single-session
    /// `analyze` path, but quadratic for batch commands.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeSet;
    /// use std::sync::Arc;
    /// use agentprof_core::analyzer::waste::{build_bpe, WasteComputeContext};
    /// use agentprof_core::model::TokenizerKind;
    ///
    /// let wire = BTreeSet::new();
    /// if let Some(bpe) = build_bpe(TokenizerKind::Cl100kBase) {
    ///     let bpe = Arc::new(bpe);
    ///     let ctx = WasteComputeContext::new(&wire).with_bpe(bpe.clone());
    ///     assert!(ctx.bpe.is_some());
    /// }
    /// ```
    #[must_use]
    pub fn with_bpe(mut self, bpe: Arc<CoreBPE>) -> Self {
        self.bpe = Some(bpe);
        self
    }
}

/// Build a [`CoreBPE`] encoder for the given [`TokenizerKind`].
///
/// Returns `None` only on the unlikely embedded-asset failure (memory
/// exhaustion in practice). Intended for CLI driver code that wants to
/// build one encoder per invocation and share it across many
/// [`WasteComputeContext`]s via [`WasteComputeContext::with_bpe`] —
/// avoids the per-call ~50 ms + tens-of-MB reconstruction cost when
/// looping over many sessions.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::waste::build_bpe;
/// use agentprof_core::model::TokenizerKind;
///
/// let _bpe = build_bpe(TokenizerKind::Cl100kBase);
/// ```
#[must_use]
pub fn build_bpe(kind: TokenizerKind) -> Option<CoreBPE> {
    match kind {
        TokenizerKind::O200kBase => tiktoken_rs::o200k_base().ok(),
        TokenizerKind::Cl100kBase => tiktoken_rs::cl100k_base().ok(),
    }
}

/// Map a session's `model` string to a [`TokenizerKind`].
///
/// `gpt-5*` / `gpt-4o*` → [`TokenizerKind::O200kBase`]; everything else
/// (including `None`, unknown model names, and Claude models) →
/// [`TokenizerKind::Cl100kBase`] — the safer default and a reasonable
/// approximation for Anthropic's tokenizer.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::waste::infer_tokenizer;
/// use agentprof_core::model::TokenizerKind;
/// assert_eq!(infer_tokenizer(Some("gpt-5-mini")), TokenizerKind::O200kBase);
/// assert_eq!(infer_tokenizer(Some("gpt-4-turbo")), TokenizerKind::Cl100kBase);
/// assert_eq!(infer_tokenizer(None), TokenizerKind::Cl100kBase);
/// ```
#[must_use]
#[tracing::instrument(
    name = "analyzer.infer_tokenizer",
    level = "debug",
    skip_all,
    fields(model = ?model)
)]
pub fn infer_tokenizer(model: Option<&str>) -> TokenizerKind {
    match model {
        Some(m)
            if m.starts_with("gpt-5")
                || m.starts_with("gpt-4o")
                || m.starts_with("o1")
                || m.starts_with("o3") =>
        {
            TokenizerKind::O200kBase
        }
        _ => TokenizerKind::Cl100kBase,
    }
}

/// Compute the token cost of a single MCP tool entry.
///
/// If `sidecar` is `Some` and contains `tool_name`, returns
/// `(encoded_len, TokenSource::SidecarExact)` — the entry's JSON is
/// encoded with `tokenizer`. Otherwise returns
/// `(heuristic, TokenSource::Heuristic)`.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::waste::{compute_token_cost_for_tool, DEFAULT_HEURISTIC_TOKENS};
/// use agentprof_core::model::TokenSource;
///
/// let bpe = tiktoken_rs::cl100k_base().unwrap();
/// let (n, src) = compute_token_cost_for_tool(
///     "mcp__github__search",
///     None,
///     DEFAULT_HEURISTIC_TOKENS,
///     &bpe,
/// );
/// assert_eq!(n, DEFAULT_HEURISTIC_TOKENS);
/// assert_eq!(src, TokenSource::Heuristic);
/// ```
#[must_use]
pub fn compute_token_cost_for_tool(
    tool_name: &str,
    sidecar: Option<&dyn SidecarLookup>,
    heuristic: u64,
    tokenizer: &CoreBPE,
) -> (u64, TokenSource) {
    if let Some(entry) = sidecar.and_then(|s| s.lookup(tool_name)) {
        let json = entry.to_json_string();
        let n = tokenizer.encode_ordinary(&json).len() as u64;
        return (n, TokenSource::SidecarExact);
    }
    (heuristic, TokenSource::Heuristic)
}

/// Compute per-session MCP-server waste from an analysis report and a
/// [`WasteComputeContext`] carrying the wire / config / sidecar inputs.
///
/// The context's `wire_loaded` set comes from `<tools_changed_notice>`
/// blocks; `config_loaded` (optional) is the `mcp.json`-derived
/// server → tool-list map; `sidecar` (optional) provides agent-visible
/// JSON for exact token counting; `tokenizer` selects the `tiktoken-rs`
/// encoding; `heuristic_tokens_per_tool` is the fallback when no sidecar
/// match exists.
///
/// Returns a fully-populated [`WasteReport`] with `server_waste` sorted
/// by `unused_count` descending (ties by `server` ascending), tools
/// within each server sorted alphabetically by `short_name`, per-server
/// `loaded_tokens` / `unused_tokens` populated, and `token_provenance`
/// derived from the sidecar hit/miss distribution
/// (all-hit → `SidecarExact`, all-miss → `Heuristic`, mix → `Mixed`).
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::analyzer::{AnalysisReport, compute_waste};
/// use agentprof_core::analyzer::waste::WasteComputeContext;
/// use agentprof_core::model::SessionMeta;
/// use chrono::{TimeZone, Utc};
///
/// let meta = SessionMeta::new(
///     "s1".into(),
///     AgentKind::Copilot,
///     Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap(),
///     false,
/// );
/// let report = AnalysisReport::new(meta);
/// let wire = BTreeSet::new();
/// let ctx = WasteComputeContext::new(&wire);
/// let r = compute_waste(&report, &ctx);
/// assert_eq!(r.total_loaded_tool_count, 0);
/// assert_eq!(r.total_loaded_tokens, 0);
/// ```
#[must_use]
#[allow(clippy::too_many_lines)] // 7-step pipeline reads better as one fn; see ADR-0016 D-5
#[tracing::instrument(
    name = "analyzer.waste",
    skip_all,
    fields(
        wire_size = ctx.wire_loaded.len(),
        has_config = ctx.config_loaded.is_some(),
        has_sidecar = ctx.sidecar.is_some(),
        heuristic = ctx.heuristic_tokens_per_tool,
        tokenizer = ?ctx.tokenizer,
    )
)]
pub fn compute_waste(report: &AnalysisReport, ctx: &WasteComputeContext) -> WasteReport {
    // Step 1: extract `called` map from report.tool_rank, MCP-only.
    let mut called: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for row in &report.tool_rank {
        if let ToolSource::Mcp { server } = &row.source {
            if let Some(short) = short_name(&row.name) {
                called
                    .entry(server.clone())
                    .or_default()
                    .insert(short.to_string(), row.call_count);
            }
        }
    }

    // Step 2: build loaded super-set with provenance.
    //   Initialize from wire (Wire), then merge config (Wire→Both / new→Config),
    //   then merge called (any new → InferredFromCall).
    let mut loaded: BTreeMap<(String, String), LoadedSource> = BTreeMap::new();
    for tool_name in ctx.wire_loaded {
        if let Some((server, short)) = split_full_name(tool_name) {
            loaded.insert((server, short), LoadedSource::Wire);
        }
    }
    if let Some(cfg) = ctx.config_loaded {
        for (server, tools) in cfg {
            for short in tools {
                let key = (server.clone(), short.clone());
                loaded
                    .entry(key)
                    .and_modify(|src| {
                        if matches!(src, LoadedSource::Wire) {
                            *src = LoadedSource::Both;
                        }
                    })
                    .or_insert(LoadedSource::Config);
            }
        }
    }
    for (server, tools_map) in &called {
        for short in tools_map.keys() {
            loaded
                .entry((server.clone(), short.clone()))
                .or_insert(LoadedSource::InferredFromCall);
        }
    }

    // Step 3: build (or reuse) the tokenizer and compute per-tool token cost.
    //
    // Fast path: `ctx.bpe` carries an `Arc<CoreBPE>` pre-built by the
    // CLI driver (one per command, shared across all sessions in a
    // batch). Slow path: re-build per call — `cl100k_base()` /
    // `o200k_base()` load embedded constants and only fail on memory
    // exhaustion in practice; on the unlikely error we fall back to
    // heuristic-only mode (no sidecar lookups), which keeps
    // `compute_waste` infallible while still producing meaningful counts.
    let owned_bpe: Option<CoreBPE> = if ctx.bpe.is_none() {
        build_bpe(ctx.tokenizer)
    } else {
        None
    };
    let bpe: Option<&CoreBPE> = ctx.bpe.as_deref().or(owned_bpe.as_ref());

    let mut by_server: BTreeMap<String, Vec<McpToolWaste>> = BTreeMap::new();
    let mut sidecar_hits: usize = 0;
    let mut sidecar_misses: usize = 0;

    for ((server, short), src) in &loaded {
        let full_name = format!("mcp__{server}__{short}");
        let call_count = called
            .get(server)
            .and_then(|m| m.get(short))
            .copied()
            .unwrap_or(0);
        let (description_tokens, token_source) = bpe.map_or(
            (ctx.heuristic_tokens_per_tool, TokenSource::Heuristic),
            |b| {
                compute_token_cost_for_tool(
                    &full_name,
                    ctx.sidecar,
                    ctx.heuristic_tokens_per_tool,
                    b,
                )
            },
        );
        match token_source {
            TokenSource::SidecarExact => sidecar_hits += 1,
            TokenSource::Heuristic => sidecar_misses += 1,
        }
        by_server
            .entry(server.clone())
            .or_default()
            .push(McpToolWaste {
                tool_name: full_name,
                short_name: short.clone(),
                call_count,
                loaded_source: *src,
                description_tokens,
                token_source,
            });
    }

    // Step 4: build McpServerWaste vec, sort tools alphabetically, server-level totals.
    let mut server_waste: Vec<McpServerWaste> = by_server
        .into_iter()
        .map(|(server, mut tools)| {
            tools.sort_by(|a, b| a.short_name.cmp(&b.short_name));
            let loaded_count = tools.len();
            let called_count = tools.iter().filter(|t| t.call_count > 0).count();
            let unused_count = loaded_count - called_count;
            let loaded_tokens: u64 = tools.iter().map(|t| t.description_tokens).sum();
            let unused_tokens: u64 = tools
                .iter()
                .filter(|t| t.call_count == 0)
                .map(|t| t.description_tokens)
                .sum();
            McpServerWaste {
                server,
                tools,
                loaded_count,
                called_count,
                unused_count,
                is_fully_unused: called_count == 0,
                unused_tokens,
                loaded_tokens,
            }
        })
        .collect();

    // Step 5: sort servers by unused_count desc, ties by server asc.
    server_waste.sort_by(|a, b| {
        b.unused_count
            .cmp(&a.unused_count)
            .then_with(|| a.server.cmp(&b.server))
    });

    // Step 6: derive data_source enum + totals.
    //
    // Treat `Some(empty)` as `None` for banner purposes: the VSCode-shape
    // `~/.copilot/mcp.json` (server names only, `tools = None` per entry)
    // collapses to an empty `BTreeMap` after the cli-side filter, but
    // contributes nothing to the analysis — flipping the banner to
    // `Both`/`Config` in that case broke snapshot hermeticity for hosts
    // that happened to have a real mcp.json installed. See M1.6.5 review
    // M-1 + M-2 (2026-06-08).
    let config_contributed = ctx.config_loaded.is_some_and(|c| !c.is_empty());
    let data_source = match (ctx.wire_loaded.is_empty(), config_contributed) {
        (true, true) => WasteDataSource::Config,
        (false, true) => WasteDataSource::Both,
        (false, false) => WasteDataSource::Wire,
        (true, false) => WasteDataSource::None,
    };

    // Step 7 (M1.6.6): derive TokenProvenance from sidecar hit/miss distribution.
    let token_provenance = match (sidecar_hits, sidecar_misses) {
        (0, _) => TokenProvenance::Heuristic,
        (_, 0) => TokenProvenance::SidecarExact,
        (_, _) => TokenProvenance::Mixed,
    };

    let total_loaded_tool_count = server_waste.iter().map(|s| s.loaded_count).sum();
    let total_unused_tool_count = server_waste.iter().map(|s| s.unused_count).sum();
    let total_loaded_tokens = server_waste.iter().map(|s| s.loaded_tokens).sum();
    let total_unused_tokens = server_waste.iter().map(|s| s.unused_tokens).sum();

    WasteReport {
        server_waste,
        data_source,
        total_loaded_tool_count,
        total_unused_tool_count,
        total_loaded_tokens,
        total_unused_tokens,
        token_provenance,
        tokenizer: ctx.tokenizer,
    }
}

/// Split `mcp__<server>__<short>` into `("<server>", "<short>")`.
/// Returns `None` if the name does not match the MCP convention.
fn split_full_name(full: &str) -> Option<(String, String)> {
    let after_prefix = full.strip_prefix("mcp__")?;
    let (server, short) = after_prefix.split_once("__")?;
    if server.is_empty() || short.is_empty() {
        return None;
    }
    Some((server.to_string(), short.to_string()))
}

/// Extract the `short_name` (`<short>`) from `mcp__<server>__<short>`.
/// Returns `None` if the name does not match the MCP convention.
fn short_name(full: &str) -> Option<&str> {
    let after_prefix = full.strip_prefix("mcp__")?;
    let (_server, short) = after_prefix.split_once("__")?;
    if short.is_empty() {
        return None;
    }
    Some(short)
}

/// Roll up per-session `WasteReport`s into an `AggregateWasteReport`
/// (used by the `mcp-waste` subcommand for cross-session summaries).
///
/// Walks each `(SessionRef, WasteReport)` pair and accumulates per-server
/// (`sessions_loaded`, `sessions_with_zero_calls`) and per-tool
/// (`sessions_loaded`, `sessions_called`, `total_call_count`) counters,
/// then derives `never_called_tools` — fully-qualified tools loaded in
/// ≥ 1 session but never called in any (the strongest "remove from
/// `mcp.json`" candidates).
///
/// Servers are sorted by `sessions_with_zero_calls` descending (ties by
/// server name ascending); tools within each server are sorted
/// alphabetically by `tool_name`; `never_called_tools` is sorted and
/// de-duplicated.
///
/// M1.6.6: also accumulates `McpServerCrossWaste.total_unused_tokens`
/// (Σ per-session `McpServerWaste.unused_tokens`) and the report-level
/// `AggregateWasteReport.total_unused_tokens` (Σ across servers) so
/// `mcp-waste` can surface the "Largest waste by tokens" summary line.
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::aggregate_waste;
///
/// let r = aggregate_waste(&[]);
/// assert_eq!(r.sessions, 0);
/// assert!(r.per_server.is_empty());
/// assert!(r.never_called_tools.is_empty());
/// ```
#[must_use]
#[tracing::instrument(
    name = "analyzer.waste_aggregate",
    skip_all,
    fields(sessions = per_session.len())
)]
pub fn aggregate_waste(per_session: &[(SessionRef, WasteReport)]) -> AggregateWasteReport {
    let mut acc: BTreeMap<String, ServerAcc> = BTreeMap::new();

    for (_sref, wreport) in per_session {
        for sw in &wreport.server_waste {
            let server_acc = acc.entry(sw.server.clone()).or_default();
            server_acc.sessions_loaded += 1;
            if sw.is_fully_unused {
                server_acc.sessions_with_zero_calls += 1;
            }
            // M1.6.6 T4.1 — accumulate per-server unused-token totals
            // across sessions so the cross-session report can surface
            // "Largest waste by tokens" alongside session counts.
            server_acc.total_unused_tokens += sw.unused_tokens;
            for t in &sw.tools {
                let tool_acc = server_acc.tools.entry(t.tool_name.clone()).or_default();
                tool_acc.sessions_loaded += 1;
                if t.call_count > 0 {
                    tool_acc.sessions_called += 1;
                }
                tool_acc.total_call_count += t.call_count;
            }
        }
    }

    let mut never_called_tools: Vec<String> = Vec::new();
    let mut per_server: Vec<McpServerCrossWaste> = acc
        .into_iter()
        .map(|(server, sacc)| {
            let mut tool_usage: Vec<McpToolUsageAcrossSessions> = sacc
                .tools
                .into_iter()
                .map(|(tool_name, tacc)| {
                    if tacc.sessions_called == 0 && tacc.sessions_loaded > 0 {
                        never_called_tools.push(tool_name.clone());
                    }
                    McpToolUsageAcrossSessions {
                        tool_name,
                        sessions_loaded: tacc.sessions_loaded,
                        sessions_called: tacc.sessions_called,
                        total_call_count: tacc.total_call_count,
                    }
                })
                .collect();
            tool_usage.sort_by(|a, b| a.tool_name.cmp(&b.tool_name));
            McpServerCrossWaste {
                server,
                sessions_loaded: sacc.sessions_loaded,
                sessions_with_zero_calls: sacc.sessions_with_zero_calls,
                tool_usage,
                total_unused_tokens: sacc.total_unused_tokens,
            }
        })
        .collect();

    per_server.sort_by(|a, b| {
        b.sessions_with_zero_calls
            .cmp(&a.sessions_with_zero_calls)
            .then_with(|| a.server.cmp(&b.server))
    });

    never_called_tools.sort();
    never_called_tools.dedup();

    let total_unused_tokens = per_server.iter().map(|s| s.total_unused_tokens).sum();

    AggregateWasteReport {
        sessions: per_session.len(),
        per_server,
        never_called_tools,
        total_unused_tokens,
    }
}

#[derive(Default)]
struct ServerAcc {
    sessions_loaded: usize,
    sessions_with_zero_calls: usize,
    total_unused_tokens: u64,
    tools: BTreeMap<String, ToolAcc>,
}

#[derive(Default)]
struct ToolAcc {
    sessions_loaded: usize,
    sessions_called: usize,
    total_call_count: usize,
}

#[cfg(test)]
#[allow(clippy::iter_on_single_items)]
mod tests {
    use super::*;
    use crate::adapter::AgentKind;
    use crate::analyzer::ToolRankRow;
    use crate::model::SessionMeta;
    use chrono::{TimeZone, Utc};

    fn empty_report() -> AnalysisReport {
        AnalysisReport::new(SessionMeta::new(
            "s1".into(),
            AgentKind::Copilot,
            Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap(),
            false,
        ))
    }

    fn ctx(wire: &BTreeSet<String>) -> WasteComputeContext<'_> {
        WasteComputeContext::new(wire)
    }

    fn mcp_row(server: &str, tool: &str, calls: usize) -> ToolRankRow {
        ToolRankRow {
            name: format!("mcp__{server}__{tool}"),
            source: ToolSource::Mcp {
                server: server.into(),
            },
            call_count: calls,
            success_count: calls,
            failure_count: 0,
            orphan_count: 0,
            user_requested_count: 0,
            total_duration: chrono::Duration::zero(),
            p50_duration: chrono::Duration::zero(),
            p95_duration: chrono::Duration::zero(),
            max_duration: chrono::Duration::zero(),
            is_user_blocking: false,
        }
    }

    fn builtin_row(name: &str, calls: usize) -> ToolRankRow {
        ToolRankRow {
            name: name.into(),
            source: ToolSource::Builtin,
            call_count: calls,
            success_count: calls,
            failure_count: 0,
            orphan_count: 0,
            user_requested_count: 0,
            total_duration: chrono::Duration::zero(),
            p50_duration: chrono::Duration::zero(),
            p95_duration: chrono::Duration::zero(),
            max_duration: chrono::Duration::zero(),
            is_user_blocking: false,
        }
    }

    #[test]
    fn empty_inputs_produce_empty_report_with_none_source() {
        let r = compute_waste(&empty_report(), &ctx(&BTreeSet::new()));
        assert!(r.server_waste.is_empty());
        assert!(matches!(r.data_source, WasteDataSource::None));
        assert_eq!(r.total_loaded_tool_count, 0);
        assert_eq!(r.total_unused_tool_count, 0);
    }

    #[test]
    fn wire_only_all_unused_yields_wire_data_source() {
        let wire: BTreeSet<String> = ["mcp__github__search", "mcp__github__create"]
            .into_iter()
            .map(String::from)
            .collect();
        let r = compute_waste(&empty_report(), &ctx(&wire));
        assert!(matches!(r.data_source, WasteDataSource::Wire));
        assert_eq!(r.server_waste.len(), 1);
        assert_eq!(r.server_waste[0].server, "github");
        assert_eq!(r.server_waste[0].loaded_count, 2);
        assert_eq!(r.server_waste[0].called_count, 0);
        assert_eq!(r.server_waste[0].unused_count, 2);
        assert!(r.server_waste[0].is_fully_unused);
        for t in &r.server_waste[0].tools {
            assert!(matches!(t.loaded_source, LoadedSource::Wire));
        }
    }

    #[test]
    fn called_only_no_baseline_yields_inferred_from_call_source() {
        let mut report = empty_report();
        report.tool_rank.push(mcp_row("github", "search", 3));
        let r = compute_waste(&report, &ctx(&BTreeSet::new()));
        assert_eq!(r.server_waste.len(), 1);
        assert!(matches!(
            r.server_waste[0].tools[0].loaded_source,
            LoadedSource::InferredFromCall
        ));
        // Note: `data_source` is None — neither wire nor config contributed.
        assert!(matches!(r.data_source, WasteDataSource::None));
    }

    #[test]
    fn wire_plus_config_marks_tool_as_both() {
        let wire: BTreeSet<String> = ["mcp__github__search"]
            .into_iter()
            .map(String::from)
            .collect();
        let cfg: BTreeMap<String, Vec<String>> =
            [("github".to_string(), vec!["search".to_string()])]
                .into_iter()
                .collect();
        let r = compute_waste(&empty_report(), &ctx(&wire).with_config(&cfg));
        assert!(matches!(r.data_source, WasteDataSource::Both));
        assert_eq!(r.server_waste[0].tools[0].loaded_source, LoadedSource::Both);
    }

    #[test]
    fn config_only_no_wire_yields_config_data_source() {
        let cfg: BTreeMap<String, Vec<String>> = [(
            "github".to_string(),
            vec!["search".to_string(), "create".to_string()],
        )]
        .into_iter()
        .collect();
        let r = compute_waste(&empty_report(), &ctx(&BTreeSet::new()).with_config(&cfg));
        assert!(matches!(r.data_source, WasteDataSource::Config));
        assert_eq!(r.server_waste[0].tools.len(), 2);
        for t in &r.server_waste[0].tools {
            assert!(matches!(t.loaded_source, LoadedSource::Config));
        }
    }

    #[test]
    fn partial_usage_is_not_fully_unused() {
        let wire: BTreeSet<String> = ["mcp__github__search", "mcp__github__create"]
            .into_iter()
            .map(String::from)
            .collect();
        let mut report = empty_report();
        report.tool_rank.push(mcp_row("github", "search", 5));
        let r = compute_waste(&report, &ctx(&wire));
        assert_eq!(r.server_waste[0].called_count, 1);
        assert_eq!(r.server_waste[0].unused_count, 1);
        assert!(!r.server_waste[0].is_fully_unused);
    }

    #[test]
    fn multi_server_sorts_by_unused_count_desc() {
        let wire: BTreeSet<String> = [
            "mcp__a__t1",
            "mcp__a__t2",
            "mcp__a__t3",
            "mcp__b__t1",
            "mcp__c__t1",
            "mcp__c__t2",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let r = compute_waste(&empty_report(), &ctx(&wire));
        assert_eq!(r.server_waste[0].server, "a");
        assert_eq!(r.server_waste[1].server, "c");
        assert_eq!(r.server_waste[2].server, "b");
    }

    #[test]
    fn builtin_tools_are_filtered_out() {
        let mut report = empty_report();
        report.tool_rank.push(builtin_row("bash", 10));
        report.tool_rank.push(mcp_row("github", "search", 1));
        let r = compute_waste(&report, &ctx(&BTreeSet::new()));
        assert_eq!(r.server_waste.len(), 1, "only MCP servers in report");
        assert_eq!(r.server_waste[0].server, "github");
    }

    #[test]
    fn totals_sum_across_servers() {
        let wire: BTreeSet<String> = ["mcp__a__t1", "mcp__a__t2", "mcp__b__t1"]
            .into_iter()
            .map(String::from)
            .collect();
        let r = compute_waste(&empty_report(), &ctx(&wire));
        assert_eq!(r.total_loaded_tool_count, 3);
        assert_eq!(r.total_unused_tool_count, 3);
    }

    #[test]
    fn tools_within_server_sorted_alphabetically_by_short_name() {
        let wire: BTreeSet<String> = [
            "mcp__github__zebra",
            "mcp__github__alpha",
            "mcp__github__mango",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let r = compute_waste(&empty_report(), &ctx(&wire));
        let shorts: Vec<&str> = r.server_waste[0]
            .tools
            .iter()
            .map(|t| t.short_name.as_str())
            .collect();
        assert_eq!(shorts, vec!["alpha", "mango", "zebra"]);
    }

    #[test]
    fn empty_config_map_does_not_flip_to_both() {
        // Regression for M-1 + M-2 (review 2026-06-08): the VSCode-shape
        // mcp.json schema produces Some(BTreeMap::new()) after cli-side
        // tools filter. Don't flip the data_source banner to Both/Config
        // when the config map carried zero tools — that broke snapshot
        // hermeticity for anyone with a real ~/.copilot/mcp.json installed.
        let wire: BTreeSet<String> = ["mcp__github__search"]
            .into_iter()
            .map(String::from)
            .collect();
        let cfg: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let r = compute_waste(&empty_report(), &ctx(&wire).with_config(&cfg));
        assert!(
            matches!(r.data_source, WasteDataSource::Wire),
            "empty config map must NOT promote Wire to Both; got {:?}",
            r.data_source
        );
    }

    #[test]
    fn empty_config_map_with_no_wire_yields_none_not_config() {
        let wire: BTreeSet<String> = BTreeSet::new();
        let cfg: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let r = compute_waste(&empty_report(), &ctx(&wire).with_config(&cfg));
        assert!(
            matches!(r.data_source, WasteDataSource::None),
            "empty config + empty wire must yield None; got {:?}",
            r.data_source
        );
    }

    fn session_ref(id: &str) -> SessionRef {
        SessionRef::new(
            id.to_string(),
            AgentKind::Copilot,
            std::path::PathBuf::from(format!("./fixtures/{id}.jsonl")),
            std::time::SystemTime::UNIX_EPOCH,
            0,
            false,
        )
    }

    fn waste(server: &str, tools: &[(&str, usize, LoadedSource)]) -> WasteReport {
        let tool_waste: Vec<McpToolWaste> = tools
            .iter()
            .map(|(short, calls, src)| McpToolWaste {
                tool_name: format!("mcp__{server}__{short}"),
                short_name: (*short).to_string(),
                call_count: *calls,
                loaded_source: *src,
                description_tokens: 0,
                token_source: TokenSource::Heuristic,
            })
            .collect();
        let loaded_count = tool_waste.len();
        let called_count = tool_waste.iter().filter(|t| t.call_count > 0).count();
        WasteReport {
            server_waste: vec![McpServerWaste {
                server: server.into(),
                tools: tool_waste,
                loaded_count,
                called_count,
                unused_count: loaded_count - called_count,
                is_fully_unused: called_count == 0,
                unused_tokens: 0,
                loaded_tokens: 0,
            }],
            data_source: WasteDataSource::Wire,
            total_loaded_tool_count: loaded_count,
            total_unused_tool_count: loaded_count - called_count,
            ..Default::default()
        }
    }

    #[test]
    fn aggregate_empty_input_yields_empty_output() {
        let r = aggregate_waste(&[]);
        assert_eq!(r.sessions, 0);
        assert!(r.per_server.is_empty());
        assert!(r.never_called_tools.is_empty());
    }

    #[test]
    fn aggregate_single_session_passes_through() {
        let w = waste(
            "github",
            &[
                ("search", 3, LoadedSource::Wire),
                ("create", 0, LoadedSource::Wire),
            ],
        );
        let r = aggregate_waste(&[(session_ref("s1"), w)]);
        assert_eq!(r.sessions, 1);
        assert_eq!(r.per_server.len(), 1);
        assert_eq!(r.per_server[0].sessions_loaded, 1);
        assert_eq!(r.per_server[0].sessions_with_zero_calls, 0);
        assert_eq!(r.per_server[0].tool_usage.len(), 2);
    }

    #[test]
    fn aggregate_counts_zero_call_sessions() {
        let s1 = waste("github", &[("search", 0, LoadedSource::Wire)]);
        let s2 = waste("github", &[("search", 0, LoadedSource::Wire)]);
        let s3 = waste("github", &[("search", 5, LoadedSource::Wire)]);
        let r = aggregate_waste(&[
            (session_ref("s1"), s1),
            (session_ref("s2"), s2),
            (session_ref("s3"), s3),
        ]);
        assert_eq!(r.per_server[0].sessions_with_zero_calls, 2);
        assert_eq!(r.per_server[0].tool_usage[0].sessions_called, 1);
        assert_eq!(r.per_server[0].tool_usage[0].total_call_count, 5);
    }

    #[test]
    fn aggregate_lists_never_called_tools() {
        let s1 = waste(
            "github",
            &[
                ("create", 0, LoadedSource::Wire),
                ("search", 3, LoadedSource::Wire),
            ],
        );
        let s2 = waste(
            "github",
            &[
                ("create", 0, LoadedSource::Wire),
                ("search", 1, LoadedSource::Wire),
            ],
        );
        let r = aggregate_waste(&[(session_ref("s1"), s1), (session_ref("s2"), s2)]);
        assert_eq!(
            r.never_called_tools,
            vec!["mcp__github__create".to_string()]
        );
    }

    #[test]
    fn aggregate_merges_multi_server() {
        let s1 = waste("a", &[("t1", 0, LoadedSource::Wire)]);
        let s2 = waste("b", &[("t1", 0, LoadedSource::Wire)]);
        let r = aggregate_waste(&[(session_ref("s1"), s1), (session_ref("s2"), s2)]);
        assert_eq!(r.per_server.len(), 2);
        assert_eq!(r.sessions, 2);
    }

    // -- T1.3: infer_tokenizer + WasteComputeContext + compute_token_cost_for_tool --

    #[test]
    fn infer_tokenizer_gpt_5_returns_o200k() {
        assert_eq!(infer_tokenizer(Some("gpt-5")), TokenizerKind::O200kBase);
        assert_eq!(
            infer_tokenizer(Some("gpt-5-mini")),
            TokenizerKind::O200kBase
        );
        assert_eq!(
            infer_tokenizer(Some("gpt-5-turbo")),
            TokenizerKind::O200kBase
        );
    }

    #[test]
    fn infer_tokenizer_gpt_4o_returns_o200k() {
        assert_eq!(infer_tokenizer(Some("gpt-4o")), TokenizerKind::O200kBase);
        assert_eq!(
            infer_tokenizer(Some("gpt-4o-mini")),
            TokenizerKind::O200kBase
        );
    }

    #[test]
    fn infer_tokenizer_gpt_4_returns_cl100k() {
        assert_eq!(infer_tokenizer(Some("gpt-4")), TokenizerKind::Cl100kBase);
        assert_eq!(
            infer_tokenizer(Some("gpt-4-turbo")),
            TokenizerKind::Cl100kBase
        );
    }

    #[test]
    fn infer_tokenizer_unknown_or_none_returns_cl100k() {
        assert_eq!(infer_tokenizer(None), TokenizerKind::Cl100kBase);
        assert_eq!(
            infer_tokenizer(Some("claude-3-5-sonnet")),
            TokenizerKind::Cl100kBase
        );
        assert_eq!(infer_tokenizer(Some("")), TokenizerKind::Cl100kBase);
    }

    #[test]
    fn infer_tokenizer_o1_reasoning_returns_o200k() {
        assert_eq!(infer_tokenizer(Some("o1")), TokenizerKind::O200kBase);
        assert_eq!(
            infer_tokenizer(Some("o1-preview")),
            TokenizerKind::O200kBase
        );
    }

    #[test]
    fn infer_tokenizer_o3_reasoning_returns_o200k() {
        assert_eq!(infer_tokenizer(Some("o3")), TokenizerKind::O200kBase);
        assert_eq!(infer_tokenizer(Some("o3-mini")), TokenizerKind::O200kBase);
    }

    #[test]
    fn waste_compute_context_builder_defaults() {
        let wire = BTreeSet::new();
        let ctx = WasteComputeContext::new(&wire);
        assert_eq!(ctx.heuristic_tokens_per_tool, DEFAULT_HEURISTIC_TOKENS);
        assert_eq!(ctx.tokenizer, TokenizerKind::Cl100kBase);
        assert!(ctx.config_loaded.is_none());
        assert!(ctx.sidecar.is_none());
    }

    #[test]
    fn waste_compute_context_builder_chain_overrides() {
        let wire = BTreeSet::new();
        let ctx = WasteComputeContext::new(&wire)
            .with_heuristic(123)
            .with_tokenizer(TokenizerKind::O200kBase);
        assert_eq!(ctx.heuristic_tokens_per_tool, 123);
        assert_eq!(ctx.tokenizer, TokenizerKind::O200kBase);
    }

    #[test]
    fn compute_token_cost_for_tool_heuristic_when_no_sidecar() {
        let bpe = tiktoken_rs::cl100k_base().unwrap();
        let (n, src) = compute_token_cost_for_tool(
            "mcp__github__search",
            None,
            DEFAULT_HEURISTIC_TOKENS,
            &bpe,
        );
        assert_eq!(n, DEFAULT_HEURISTIC_TOKENS);
        assert_eq!(src, TokenSource::Heuristic);
    }

    struct FakeSidecar(BTreeMap<String, FakeEntry>);
    struct FakeEntry {
        name: String,
        description: String,
    }
    impl crate::analyzer::waste::SidecarLookup for FakeSidecar {
        fn lookup(&self, full_name: &str) -> Option<&dyn crate::analyzer::waste::SidecarToolEntry> {
            self.0
                .get(full_name)
                .map(|e| e as &dyn crate::analyzer::waste::SidecarToolEntry)
        }
    }
    impl crate::analyzer::waste::SidecarToolEntry for FakeEntry {
        fn to_json_string(&self) -> String {
            format!(
                "{{\"name\":\"{}\",\"description\":\"{}\"}}",
                self.name, self.description
            )
        }
    }

    #[test]
    fn compute_token_cost_for_tool_sidecar_hit_uses_exact_count() {
        let mut entries = BTreeMap::new();
        entries.insert(
            "mcp__github__search".into(),
            FakeEntry {
                name: "search".into(),
                description: "Find issues".into(),
            },
        );
        let sidecar = FakeSidecar(entries);

        let bpe = tiktoken_rs::cl100k_base().unwrap();
        let (n, src) = compute_token_cost_for_tool(
            "mcp__github__search",
            Some(&sidecar as &dyn crate::analyzer::waste::SidecarLookup),
            DEFAULT_HEURISTIC_TOKENS,
            &bpe,
        );
        // The fake JSON `{"name":"search","description":"Find issues"}` is
        // ~14-18 tokens in cl100k_base — well below the 200 heuristic.
        assert!(
            n > 0 && n < DEFAULT_HEURISTIC_TOKENS,
            "exact count {n} should be smaller than heuristic 200"
        );
        assert_eq!(src, crate::model::TokenSource::SidecarExact);
    }

    // -- M1.6.6 audit A1: shared-BPE fast path matches per-call slow path --

    #[test]
    fn build_bpe_returns_some_for_both_kinds() {
        assert!(build_bpe(TokenizerKind::Cl100kBase).is_some());
        assert!(build_bpe(TokenizerKind::O200kBase).is_some());
    }

    #[test]
    fn compute_waste_with_bpe_matches_without_bpe() {
        // Construct a report with a sidecar hit so the BPE actually gets
        // exercised (heuristic-only would never call `encode_ordinary`).
        let wire: BTreeSet<String> = ["mcp__github__search"]
            .into_iter()
            .map(String::from)
            .collect();
        let report = empty_report();

        let mut entries = BTreeMap::new();
        entries.insert(
            "mcp__github__search".into(),
            FakeEntry {
                name: "search".into(),
                description: "Find issues".into(),
            },
        );
        let sidecar = FakeSidecar(entries);

        // Slow path: ctx builds BPE inline per call.
        let ctx_inline = WasteComputeContext::new(&wire).with_sidecar(&sidecar);
        let r_inline = compute_waste(&report, &ctx_inline);

        // Fast path: caller supplies a pre-built Arc<CoreBPE>.
        let shared = Arc::new(build_bpe(TokenizerKind::Cl100kBase).unwrap());
        let ctx_shared = WasteComputeContext::new(&wire)
            .with_sidecar(&sidecar)
            .with_bpe(shared);
        let r_shared = compute_waste(&report, &ctx_shared);

        assert_eq!(
            r_inline.total_loaded_tokens, r_shared.total_loaded_tokens,
            "shared-BPE fast path must produce identical token counts"
        );
        assert_eq!(r_inline.server_waste.len(), r_shared.server_waste.len());
        assert_eq!(
            r_inline.server_waste[0].tools[0].description_tokens,
            r_shared.server_waste[0].tools[0].description_tokens
        );
        assert_eq!(
            r_inline.server_waste[0].tools[0].token_source,
            r_shared.server_waste[0].tools[0].token_source
        );
    }
}
