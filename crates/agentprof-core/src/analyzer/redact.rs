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

/// Mutable accumulator shared across reports so turn UUIDs stay consistent.
///
/// Used by [`AnalysisReport`](crate::analyzer::AnalysisReport) and (later)
/// episodes so UUIDs map identically between the table and the flamegraph.
/// Build one, feed it through [`redact_with`](crate::analyzer::AnalysisReport::redact_with),
/// then consume it via [`RedactionContext::into_map`].
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::redact::RedactionContext;
/// let mut c = RedactionContext::default();
/// assert_eq!(c.redact_uuid("a"), "<uuid-0>");
/// assert_eq!(c.redact_uuid("a"), "<uuid-0>"); // stable
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct RedactionContext {
    /// Allocates stable `<uuid-N>` replacements in first-seen order.
    pub uuids: UuidRedactor,
    /// family → original model identifier (filled at `Anonymize`).
    pub models: BTreeMap<String, String>,
    /// `<hash8>` → original MCP server (filled at `Anonymize`).
    pub servers: BTreeMap<String, String>,
}

impl RedactionContext {
    /// Stable `<uuid-N>` for `original`, delegating to [`UuidRedactor::redact`].
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::redact::RedactionContext;
    /// let mut c = RedactionContext::default();
    /// assert_eq!(c.redact_uuid("x"), "<uuid-0>");
    /// assert_eq!(c.redact_uuid("y"), "<uuid-1>");
    /// ```
    pub fn redact_uuid(&mut self, original: &str) -> String {
        self.uuids.redact(original)
    }

    /// Consume into the exported [`RedactionMap`] (uuids inverted; models and
    /// servers carried as-is).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::redact::RedactionContext;
    /// assert!(RedactionContext::default().into_map().is_empty());
    /// ```
    #[must_use]
    pub fn into_map(self) -> RedactionMap {
        RedactionMap {
            uuids: self.uuids.into_inverse(),
            models: self.models,
            mcp_servers: self.servers,
        }
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
    /// names, zeroes each per-turn `turn_summary[i].started_at` (a 🟡 MEDIUM
    /// wall-clock instant leaking working-hours/timezone) — including the
    /// nested `TurnStatus::Aborted(AbortInfo { at, .. })` instant — and fills
    /// the [`RedactionMap`] so a trusted holder can invert. Per-turn
    /// `duration` is preserved — it is the ROI signal, not PII.
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
        let mut ctx = RedactionContext::default();
        let out = self.redact_with(level, &mut ctx);
        let map = if level == PrivacyLevel::Anonymize {
            ctx.into_map()
        } else {
            RedactionMap::default()
        };
        (out, map)
    }

    /// Redact into a caller-supplied [`RedactionContext`] so several reports
    /// share one turn-id mapping. Returns the redacted copy; the map is built
    /// by the caller from `ctx` (see [`redact`](Self::redact)). Identity at
    /// [`PrivacyLevel::None`]; otherwise applies the same 🔴 HIGH / Anonymize
    /// rules documented on [`redact`](Self::redact). Pure: never fails, never
    /// panics.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::analyzer::AnalysisReport;
    /// use agentprof_core::analyzer::redact::{PrivacyLevel, RedactionContext};
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let r = AnalysisReport::new(SessionMeta::new("id".into(), AgentKind::Copilot, Utc::now(), false));
    /// let mut ctx = RedactionContext::default();
    /// let red = r.redact_with(PrivacyLevel::Anonymize, &mut ctx);
    /// assert_eq!(red.meta.id, "<uuid-0>");
    /// ```
    #[must_use]
    pub fn redact_with(&self, level: PrivacyLevel, ctx: &mut RedactionContext) -> Self {
        let mut out = self.clone();
        if level == PrivacyLevel::None {
            return out;
        }
        let anon = level == PrivacyLevel::Anonymize;

        // meta — 🔴 HIGH (both levels)
        redact_opt(&mut out.meta.cwd);
        redact_opt(&mut out.meta.branch);
        redact_opt(&mut out.meta.repository);
        out.meta.id = ctx.uuids.redact(&out.meta.id);

        // meta — anonymize-only
        if anon {
            redact_opt(&mut out.meta.agent_version);
            redact_opt(&mut out.meta.producer);
            // 1970 = sentinel (started_at can't be "<redacted>")
            out.meta.started_at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
        }

        // turn rows — stable UUIDs (cross-site stable with meta.id) + model→family
        for row in &mut out.turn_summary {
            row.turn_id = ctx.uuids.redact(&row.turn_id);
            if let Some(m) = row.model.take() {
                let fam = model_family(&m);
                ctx.models.entry(fam.clone()).or_insert(m);
                row.model = Some(fam);
            }
        }

        // turn rows — anonymize-only: per-turn wall-clock instant is 🟡 MEDIUM
        // (working hours/timezone). Consistent with `meta.started_at` (kept at
        // Redact, zeroed at Anonymize). `duration` is preserved (ROI signal).
        if anon {
            for row in &mut out.turn_summary {
                row.started_at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
                // C1: TurnStatus::Aborted embeds an `at` wall-clock instant
                // (🟡 MEDIUM) that escapes into json + analyze-html otherwise.
                if let crate::episode::TurnStatus::Aborted(info) = &mut row.status {
                    info.at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
                }
            }
        }

        // model_metrics — collapse to family, MERGING counters on collision
        if let Some(mm) = out.model_metrics.take() {
            let mut new_mm: BTreeMap<String, crate::analyzer::ModelUsage> = BTreeMap::new();
            for (model, usage) in mm {
                let fam = model_family(&model);
                ctx.models.entry(fam.clone()).or_insert(model);
                let slot = new_mm.entry(fam).or_default();
                slot.merge(&usage);
            }
            out.model_metrics = Some(new_mm);
        }

        // tool names — anonymize-only: hash MCP server segment
        if anon {
            for row in &mut out.tool_rank {
                record_mcp_server(&row.name, &mut ctx.servers);
                row.name = hash_mcp_tool_name(&row.name);
                // I-1: scrub the parallel raw server in `source` with the
                // SAME hash already embedded in `name` (no double-record).
                if let crate::model::ToolSource::Mcp { server } = &mut row.source {
                    *server = crate::observability::pii::hash_short(server);
                }
            }
            out.loaded_mcp_tools = out
                .loaded_mcp_tools
                .iter()
                .map(|t| {
                    record_mcp_server(t, &mut ctx.servers);
                    hash_mcp_tool_name(t)
                })
                .collect();
        }

        // diagnostics cleared — embed raw ids/timestamps
        out.warnings = Vec::new();
        out.parse_warnings = Vec::new();

        out
    }
}

impl crate::episode::Episodes {
    /// Return a redacted copy of these episodes sharing `ctx` with the report
    /// it was derived from, so the flamegraph and ROI table agree on `<uuid-N>`
    /// placeholders, model families and MCP-server hashes. Symmetric with
    /// [`AnalysisReport::redact_with`](crate::analyzer::AnalysisReport::redact_with).
    ///
    /// Identity at [`PrivacyLevel::None`]. At [`PrivacyLevel::Redact`] each turn
    /// id becomes a stable `<uuid-N>` (including every `tools`/`hooks` call and
    /// `skills` invocation `turn_id`), models collapse to their family (recorded
    /// in `ctx.models`) and `warnings` are cleared. [`PrivacyLevel::Anonymize`]
    /// additionally zeroes every wall-clock instant (turn `started_at`/`ended_at`,
    /// `TurnStatus::Aborted.at`, `aborts[].at`, each `ToolCall`/`HookCall`
    /// `span.{started_at,ended_at}` and `SkillInvocation.at`) to the UNIX epoch
    /// and rekeys the
    /// `tools` / `hooks` / `skills` maps plus `loaded_mcp_tools` via
    /// [`hash_mcp_tool_name`] — rewriting every `Turn.*_calls[].name`,
    /// `SkillInvocation.triggered_tools[].name` and each `ToolEpisode.source`'s
    /// `Mcp { server }` with the **same** function so the `CallRef.name ↔ map-key`
    /// and `mcp:{server}` frame cross-references stay valid. Durations are kept
    /// (the ROI signal). Pure: never fails, never panics.
    ///
    /// # Caveat
    ///
    /// `ToolCall.arguments` are retained verbatim at every level — tool-arg
    /// scrubbing is a separate RFC (privacy.md §8), so JSON export of anonymized
    /// episodes may still carry path/secret PII in args.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::redact::{PrivacyLevel, RedactionContext};
    /// use agentprof_core::episode::{CallRef, Episodes, ToolEpisode, Turn};
    /// use agentprof_core::model::ToolSource;
    /// use chrono::Utc;
    ///
    /// let mut e = Episodes::new();
    /// let mut t = Turn::new("t-1".into(), Utc::now());
    /// t.tool_calls.push(CallRef::new("mcp__github__search".into(), 0));
    /// e.turns.push(t);
    /// e.tools.insert(
    ///     "mcp__github__search".into(),
    ///     ToolEpisode::new("mcp__github__search".into(), ToolSource::Mcp { server: "github".into() }),
    /// );
    /// let mut ctx = RedactionContext::default();
    /// let red = e.redact_with(PrivacyLevel::Anonymize, &mut ctx);
    /// assert_eq!(red.turns[0].id, "<uuid-0>");
    /// assert_eq!(&red.turns[0].tool_calls[0].name, red.tools.keys().next().unwrap());
    /// ```
    #[must_use]
    pub fn redact_with(&self, level: PrivacyLevel, ctx: &mut RedactionContext) -> Self {
        let mut out = self.clone();
        if level == PrivacyLevel::None {
            return out;
        }
        let anon = level == PrivacyLevel::Anonymize;
        let epoch = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;

        for turn in &mut out.turns {
            turn.id = ctx.uuids.redact(&turn.id);
            if let Some(m) = turn.model.take() {
                let fam = model_family(&m);
                ctx.models.entry(fam.clone()).or_insert(m);
                turn.model = Some(fam);
            }
            if anon {
                turn.started_at = epoch;
                turn.ended_at = turn.ended_at.map(|_| epoch);
                if let crate::episode::TurnStatus::Aborted(info) = &mut turn.status {
                    info.at = epoch;
                }
                for c in turn
                    .tool_calls
                    .iter_mut()
                    .chain(&mut turn.hook_calls)
                    .chain(&mut turn.skill_calls)
                {
                    record_mcp_server(&c.name, &mut ctx.servers);
                    c.name = hash_mcp_tool_name(&c.name);
                }
            }
        }

        // call-level turn_id (tool/hook/skill) → same `<uuid-N>` as turns[].id,
        // at BOTH Redact and Anonymize, so speedscope/html frames stay joinable.
        // At Anonymize also zero each call's absolute timestamp (ToolCall/HookCall
        // span + SkillInvocation.at) to mirror turn.started_at: keeping wall-clock
        // here would leak working hours and break flamegraph zero offsets.
        for tool in out.tools.values_mut() {
            for call in &mut tool.calls {
                if let Some(id) = &call.turn_id {
                    call.turn_id = Some(ctx.uuids.redact(id));
                }
                if anon {
                    call.span = crate::episode::Span::new(epoch, epoch);
                }
            }
        }
        for hook in out.hooks.values_mut() {
            for call in &mut hook.calls {
                if let Some(id) = &call.turn_id {
                    call.turn_id = Some(ctx.uuids.redact(id));
                }
                if anon {
                    call.span = crate::episode::Span::new(epoch, epoch);
                }
            }
        }
        for skill in out.skills.values_mut() {
            for inv in &mut skill.invocations {
                if let Some(id) = &inv.turn_id {
                    inv.turn_id = Some(ctx.uuids.redact(id));
                }
                if anon {
                    inv.at = epoch;
                }
            }
        }

        if anon {
            out.tools = rekey_named(out.tools, &mut ctx.servers, |t| &mut t.name);
            out.hooks = rekey_named(out.hooks, &mut ctx.servers, |h| &mut h.name);
            out.skills = rekey_named(out.skills, &mut ctx.servers, |s| &mut s.name);
            // scrub the parallel raw server in `tool.source` with the SAME hash
            // already embedded in the rekeyed name (speedscope:722 reads this).
            for t in out.tools.values_mut() {
                if let crate::model::ToolSource::Mcp { server } = &mut t.source {
                    *server = crate::observability::pii::hash_short(server);
                }
            }
            for s in out.skills.values_mut() {
                for inv in &mut s.invocations {
                    for c in &mut inv.triggered_tools {
                        record_mcp_server(&c.name, &mut ctx.servers);
                        c.name = hash_mcp_tool_name(&c.name);
                    }
                }
            }
            for a in &mut out.aborts {
                a.at = epoch;
            }
            out.loaded_mcp_tools = out
                .loaded_mcp_tools
                .iter()
                .map(|t| {
                    record_mcp_server(t, &mut ctx.servers);
                    hash_mcp_tool_name(t)
                })
                .collect();
        }

        if let Some(mm) = out.model_metrics.take() {
            let mut new_mm: BTreeMap<String, crate::analyzer::ModelUsage> = BTreeMap::new();
            for (model, usage) in mm {
                let fam = model_family(&model);
                ctx.models.entry(fam.clone()).or_insert(model);
                new_mm.entry(fam).or_default().merge(&usage);
            }
            out.model_metrics = Some(new_mm);
        }

        out.warnings = Vec::new();
        out
    }
}

/// Rekey a `name`-keyed map by hashing each MCP server segment, keeping the
/// stored `name` field consistent with the new key. Records `hash8 → server`.
fn rekey_named<V>(
    map: BTreeMap<String, V>,
    servers: &mut BTreeMap<String, String>,
    name_field: impl Fn(&mut V) -> &mut String,
) -> BTreeMap<String, V> {
    map.into_iter()
        .map(|(k, mut v)| {
            record_mcp_server(&k, servers);
            let hashed = hash_mcp_tool_name(&k);
            name_field(&mut v).clone_from(&hashed);
            (hashed, v)
        })
        .collect()
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

    /// Fold buckets that collapsed to the same key after [`redact_key`](Self::redact_key).
    ///
    /// Default: identity — most bucket keys stay distinct after redaction
    /// (server/tool hashing is injective, day is never redacted). Only
    /// [`ModelBucket`] overrides this: `model → family` can map several ids
    /// onto one family, so same-family buckets are merged (summing counters)
    /// to avoid duplicate-keyed rows. Order: each merged bucket keeps the
    /// position of its first-seen member (stable, deterministic).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::aggregate::ModelBucket;
    /// use agentprof_core::analyzer::redact::RedactBucket;
    /// use chrono::Duration;
    ///
    /// // Two ids in the same `claude-sonnet` family, already keyed to family.
    /// let a = ModelBucket::new("claude-sonnet".into(), 1, 0, 0, Duration::zero());
    /// let b = ModelBucket::new("claude-sonnet".into(), 2, 0, 0, Duration::zero());
    /// let merged = ModelBucket::consolidate(vec![a, b]);
    /// assert_eq!(merged.len(), 1);
    /// assert_eq!(merged[0].session_count, 3); // counts summed
    /// ```
    #[must_use]
    fn consolidate(buckets: Vec<Self>) -> Vec<Self>
    where
        Self: Sized,
    {
        buckets
    }
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

    fn consolidate(buckets: Vec<Self>) -> Vec<Self> {
        // `model → family` can map distinct ids (e.g. claude-sonnet-4.5 +
        // claude-sonnet-4.6) onto one family key, so fold same-family buckets
        // into their first-seen member, summing ALL 7 non-key counters.
        // First-seen order is preserved (linear scan, not BTreeMap) so the
        // report's existing metric-sorted display order is unchanged.
        let mut out: Vec<Self> = Vec::with_capacity(buckets.len());
        for b in buckets {
            if let Some(existing) = out.iter_mut().find(|e| e.model == b.model) {
                existing.session_count += b.session_count;
                existing.turn_count += b.turn_count;
                existing.total_output_tokens += b.total_output_tokens;
                existing.total_input_tokens += b.total_input_tokens;
                existing.total_cache_read += b.total_cache_read;
                existing.total_cache_creation += b.total_cache_creation;
                existing.total_duration += b.total_duration;
            } else {
                out.push(b);
            }
        }
        out
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
        // are left untouched. The parallel `source.server` (raw server
        // name) is scrubbed with the SAME hash so it can't leak (I-1).
        if level != PrivacyLevel::Anonymize {
            return;
        }
        record_mcp_server(&self.name, servers);
        self.name = hash_mcp_tool_name(&self.name);
        if let crate::model::ToolSource::Mcp { server } = &mut self.source {
            *server = crate::observability::pii::hash_short(server);
        }
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
    /// PII and are preserved verbatim. Buckets that collapse to the same key
    /// (e.g. two `claude-sonnet-*` ids → one `claude-sonnet` family) are then
    /// folded via [`RedactBucket::consolidate`] so the report never emits
    /// duplicate-keyed rows with split counts. At [`PrivacyLevel::None`] this
    /// is the identity (clone + empty map).
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
        // I1: redact_key may map several ids onto one family key; fold the
        // resulting same-key buckets so they don't render as duplicate rows.
        out.buckets = B::consolidate(std::mem::take(&mut out.buckets));
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

impl crate::model::WasteReport {
    /// Redact MCP server + tool names through a shared [`RedactionContext`]
    /// so the hashes match the report's `tool_rank` and flamegraph.
    ///
    /// At [`PrivacyLevel::None`] / [`PrivacyLevel::Redact`] this is the
    /// identity (clone). At [`PrivacyLevel::Anonymize`] every
    /// `server_waste[].server` becomes `hash_short(server)` (recording
    /// `hash8 → server` into `ctx.servers`) and every `tools[].tool_name`
    /// is rewritten via [`hash_mcp_tool_name`]; `short_name` is the bare
    /// tool tail and carries no server PII, so it is kept. Pure: never
    /// fails, never panics.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::model::WasteReport;
    /// use agentprof_core::analyzer::redact::{PrivacyLevel, RedactionContext};
    /// let r: WasteReport = serde_json::from_str(r#"{
    ///     "server_waste": [], "data_source": "None",
    ///     "total_loaded_tool_count": 0, "total_unused_tool_count": 0
    /// }"#).unwrap();
    /// let mut ctx = RedactionContext::default();
    /// let out = r.redact_with(PrivacyLevel::Anonymize, &mut ctx);
    /// assert!(out.server_waste.is_empty());
    /// ```
    #[must_use]
    pub fn redact_with(&self, level: PrivacyLevel, ctx: &mut RedactionContext) -> Self {
        let mut out = self.clone();
        if level != PrivacyLevel::Anonymize {
            return out;
        }
        for sw in &mut out.server_waste {
            let h = crate::observability::pii::hash_short(&sw.server);
            ctx.servers
                .entry(h.clone())
                .or_insert_with(|| sw.server.clone());
            sw.server = h;
            for t in &mut sw.tools {
                record_mcp_server(&t.tool_name, &mut ctx.servers);
                t.tool_name = hash_mcp_tool_name(&t.tool_name);
            }
        }
        out
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
