# Privacy Redaction (`--privacy`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in `--privacy <none|redact|anonymize>` report redaction to `analyze` + `aggregate`, stripping 🔴 HIGH PII at the core report layer so every export format inherits it.

**Architecture:** New `agentprof_core::analyzer::redact` module owns `PrivacyLevel`, pure `redact()` methods on `AnalysisReport`/`AggregateReport`, and a `RedactionMap`. The cli calls `redact()` once before render dispatch and (for `anonymize`) writes a `agentprof-redaction-map.json` sidecar.

**Tech Stack:** Rust 2021, `thiserror` (none new), `serde`/`serde_json`, `clap` (via existing `clap-derive` feature on core), `assert_cmd` + `insta` for tests.

**Spec:** `docs/superpowers/specs/2026-06-28-privacy-redaction-design.md`

---

## File structure

| File | Responsibility |
|---|---|
| **Create** `crates/agentprof-core/src/analyzer/redact.rs` | `PrivacyLevel` enum, `RedactionMap`, `UuidRedactor`, `model_family`, `hash_mcp_tool_name`, `RedactBucket` trait, `AnalysisReport::redact`, `AggregateReport::redact` |
| **Modify** `crates/agentprof-core/src/analyzer/mod.rs` | `pub mod redact;` + re-export `PrivacyLevel` / `RedactionMap` |
| **Modify** `crates/agentprof-cli/src/cmd/analyze.rs` | `--privacy` arg; call `redact()` before render (line ~571 match); write sidecar |
| **Modify** `crates/agentprof-cli/src/cmd/aggregate.rs` | `--privacy` arg; call `redact()` before render; write sidecar |
| **Create** `crates/agentprof-core/tests/redact.rs` | core integration tests (full-report redaction) |
| **Create** `crates/agentprof-cli/tests/cli_privacy.rs` | `assert_cmd` e2e (grep no PII; sidecar round-trip) |
| **Create** `docs/internals/adr-0026-report-redaction.md` | ADR: redaction layer + level semantics |

**Spec corrections locked in here** (verified against source 2026-06-28):
- `SessionMeta` has **`agent_version: Option<String>` + `producer: Option<String>`** (NOT `copilot_version`).
- `SessionMeta` also has **`repository: Option<String>`** — a 🔴 HIGH git identifier the spec missed; redact it like `branch`.
- `ExitKind::OutputError = 3` exists (use it directly).

---

## Task 1: `PrivacyLevel` + redaction helpers

**Files:**
- Create: `crates/agentprof-core/src/analyzer/redact.rs`
- Modify: `crates/agentprof-core/src/analyzer/mod.rs`
- Test: inline `#[cfg(test)] mod tests` in `redact.rs`

- [ ] **Step 1: Write failing tests** (append to a new `redact.rs` with only the test module + `use super::*;`)

```rust
#[cfg(test)]
mod tests {
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
        assert_eq!(hash_mcp_tool_name("mcp__only_server"), "mcp__only_server"); // no tool sep → unchanged
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
```

- [ ] **Step 2: Run tests, verify they fail to compile** (module/functions don't exist yet)

Run: `cargo test -p agentprof-core --lib redact`
Expected: FAIL — `cannot find function model_family` / `PrivacyLevel`.

- [ ] **Step 3: Implement the helpers** (prepend above the test module in `redact.rs`)

```rust
//! Opt-in report redaction (`--privacy`). Strips 🔴 HIGH PII at the report
//! layer so every export format inherits it. See
//! `docs/superpowers/specs/2026-06-28-privacy-redaction-design.md` and
//! [ADR-0026](../../../docs/internals/adr-0026-report-redaction.md).

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
    /// `Redact` + version/started_at + MCP-name hashing + sidecar map.
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
#[derive(Debug, Default)]
pub struct UuidRedactor {
    counter: usize,
    map: BTreeMap<String, String>, // original → <uuid-N>
}

impl UuidRedactor {
    /// Return the stable replacement for `original`, allocating on first sight.
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
    #[must_use]
    pub fn into_inverse(self) -> BTreeMap<String, String> {
        self.map.into_iter().map(|(o, r)| (r, o)).collect()
    }
}

/// Replacement→original maps so a holder of a redacted report can un-redact.
/// Empty for `Redact`; filled for `Anonymize`.
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
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.uuids.is_empty() && self.models.is_empty() && self.mcp_servers.is_empty()
    }
}
```

Then in `crates/agentprof-core/src/analyzer/mod.rs`, add alongside the other `pub mod` lines:

```rust
pub mod redact;
pub use redact::{PrivacyLevel, RedactionMap};
```

- [ ] **Step 4: Run tests, verify pass + doctests**

Run: `cargo test -p agentprof-core --lib redact && cargo test -p agentprof-core --doc redact`
Expected: PASS (4 unit + 3 doctests).

- [ ] **Step 5: Commit**

```bash
git add crates/agentprof-core/src/analyzer/redact.rs crates/agentprof-core/src/analyzer/mod.rs
git commit -m "feat(core): PrivacyLevel + redaction helpers (L-1 T1)"
```

---

## Task 2: `AnalysisReport::redact`

**Files:**
- Modify: `crates/agentprof-core/src/analyzer/redact.rs` (add `impl AnalysisReport`)
- Test: `crates/agentprof-core/tests/redact.rs` (new integration test)

- [ ] **Step 1: Write the failing test** (`crates/agentprof-core/tests/redact.rs`)

```rust
//! Integration tests for `AnalysisReport::redact`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::redact::PrivacyLevel;
use agentprof_core::analyzer::{AnalysisReport, TurnSummaryRow};
use agentprof_core::model::SessionMeta;
use chrono::{TimeZone, Utc};

fn sample() -> AnalysisReport {
    let mut meta = SessionMeta::new(
        "11111111-1111-1111-1111-111111111111".into(),
        AgentKind::Copilot,
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        false,
    );
    meta.cwd = Some("/home/alice/projects/secret".into());
    meta.branch = Some("feat/secret".into());
    meta.repository = Some("alice/secret-repo".into());
    meta.agent_version = Some("1.0.54".into());
    let mut r = AnalysisReport::new(meta);
    let mut row = TurnSummaryRow::new(
        "22222222-2222-2222-2222-222222222222".into(),
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
    );
    row.model = Some("claude-opus-4.7-1m-internal".into());
    r.turn_summary.push(row);
    r
}

#[test]
fn redact_strips_high_tier_and_keeps_map_empty() {
    let (out, map) = sample().redact(PrivacyLevel::Redact);
    assert_eq!(out.meta.cwd.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.branch.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.repository.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.id, "<uuid-0>");
    assert_eq!(out.turn_summary[0].turn_id, "<uuid-1>");
    assert_eq!(out.turn_summary[0].model.as_deref(), Some("claude-opus"));
    // redact level keeps version + map empty:
    assert_eq!(out.meta.agent_version.as_deref(), Some("1.0.54"));
    assert!(map.is_empty(), "redact level → empty map");
}

#[test]
fn anonymize_strips_version_and_fills_map() {
    let (out, map) = sample().redact(PrivacyLevel::Anonymize);
    assert_eq!(out.meta.agent_version.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.started_at, chrono::DateTime::<Utc>::UNIX_EPOCH);
    // map can un-redact:
    assert_eq!(
        map.uuids.get("<uuid-0>").map(String::as_str),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(
        map.models.get("claude-opus").map(String::as_str),
        Some("claude-opus-4.7-1m-internal")
    );
}

#[test]
fn none_is_identity() {
    let (out, map) = sample().redact(PrivacyLevel::None);
    assert_eq!(out, sample());
    assert!(map.is_empty());
}
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p agentprof-core --test redact`
Expected: FAIL — `no method named redact` + `TurnSummaryRow::new` may need checking (see note).

> **Note:** if `TurnSummaryRow` has no `new` constructor, the test must build it via the analyzer or add a `#[cfg(test)]`-friendly path. Verify with `grep 'pub fn new' crates/agentprof-core/src/analyzer/turn_summary.rs`; if absent, add a minimal `pub fn new(turn_id, started_at)` constructor (it is `#[non_exhaustive]`, so external tests need one) as part of this step.

- [ ] **Step 3: Implement `AnalysisReport::redact`** (append to `redact.rs`)

```rust
impl crate::analyzer::AnalysisReport {
    /// Return a redacted copy plus a [`RedactionMap`] (non-empty only for
    /// [`PrivacyLevel::Anonymize`]). Pure: never fails, never panics.
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
        let mut models: BTreeMap<String, String> = BTreeMap::new(); // family → first original
        let mut servers: BTreeMap<String, String> = BTreeMap::new(); // hash8 → original server

        // meta — 🔴 HIGH (both levels)
        redact_opt(&mut out.meta.cwd);
        redact_opt(&mut out.meta.branch);
        redact_opt(&mut out.meta.repository);
        out.meta.id = uuids.redact(&out.meta.id);

        // meta — anonymize-only (🟡 MEDIUM)
        if anon {
            redact_opt(&mut out.meta.agent_version);
            redact_opt(&mut out.meta.producer);
            out.meta.started_at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH; // 1970 = sentinel
        }

        // turn_summary — turn_id + model→family
        for row in &mut out.turn_summary {
            row.turn_id = uuids.redact(&row.turn_id);
            if let Some(m) = row.model.take() {
                let fam = model_family(&m);
                models.entry(fam.clone()).or_insert(m);
                row.model = Some(fam);
            }
        }

        // model_metrics keys → family
        if let Some(mm) = out.model_metrics.take() {
            let mut new_mm = BTreeMap::new();
            for (model, usage) in mm {
                let fam = model_family(&model);
                models.entry(fam.clone()).or_insert(model);
                new_mm.insert(fam, usage);
            }
            out.model_metrics = Some(new_mm);
        }

        // MCP names — anonymize-only (tool_rank + loaded set)
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
```

- [ ] **Step 4: Run, verify pass**

Run: `cargo test -p agentprof-core --test redact && cargo test -p agentprof-core --doc redact`
Expected: PASS (3 integration + doctest).

- [ ] **Step 5: Commit**

```bash
git add crates/agentprof-core/src/analyzer/redact.rs crates/agentprof-core/src/analyzer/turn_summary.rs crates/agentprof-core/tests/redact.rs
git commit -m "feat(core): AnalysisReport::redact (L-1 T2)"
```

---

## Task 3: `AggregateReport::redact` (per-bucket trait)

**Files:**
- Modify: `crates/agentprof-core/src/analyzer/redact.rs` (add `RedactBucket` trait + impls + `AggregateReport::redact`)
- Test: `crates/agentprof-core/tests/redact.rs` (extend)

> **Pre-step — verify bucket construction.** Run `grep -n 'non_exhaustive\|pub fn new\|pub struct ModelBucket' crates/agentprof-core/src/analyzer/aggregate/bucket.rs`. If buckets are `#[non_exhaustive]` (likely), build test fixtures via the real `aggregate_by_*` path instead of struct literals — see Step 1 note.

- [ ] **Step 1: Write the failing test** (append to `tests/redact.rs`)

```rust
use agentprof_core::analyzer::aggregate::bucket::{DayBucket, McpServerBucket, ModelBucket};
use agentprof_core::analyzer::aggregate::AggregateReport;
use agentprof_core::analyzer::redact::RedactionMap;

// NOTE: if these *Bucket types are #[non_exhaustive], replace these literals
// with the real `aggregate_by_model(&[report])` etc. constructors (grep
// `pub fn aggregate_by_model` in analyzer/aggregate/mod.rs) and set the key
// field through that path. The redaction assertions below stay identical.

#[test]
fn aggregate_model_bucket_redacts_to_family() {
    let report: AggregateReport<ModelBucket> =
        AggregateReport::from_buckets(vec![model_bucket("claude-opus-4.7-1m-internal")]);
    let (out, _map) = report.redact(PrivacyLevel::Redact);
    assert_eq!(out.buckets[0].model, "claude-opus");
}

#[test]
fn aggregate_mcp_server_hashed_only_at_anonymize() {
    let report: AggregateReport<McpServerBucket> =
        AggregateReport::from_buckets(vec![mcp_bucket("github")]);
    let (redacted, m1) = report.clone().redact(PrivacyLevel::Redact);
    assert_eq!(redacted.buckets[0].server, "github"); // redact: unchanged
    assert!(m1.is_empty());
    let (anon, m2) = report.redact(PrivacyLevel::Anonymize);
    assert_ne!(anon.buckets[0].server, "github"); // hashed
    assert!(m2.mcp_servers.values().any(|v| v == "github")); // map can restore
}

#[test]
fn aggregate_day_bucket_never_redacted() {
    let report: AggregateReport<DayBucket> =
        AggregateReport::from_buckets(vec![day_bucket("2026-05-26")]);
    let (out, _) = report.redact(PrivacyLevel::Anonymize);
    assert_eq!(out.buckets[0].day, "2026-05-26"); // D-7: aggregation dim kept
}
```

> The `model_bucket` / `mcp_bucket` / `day_bucket` / `AggregateReport::from_buckets` helpers may need adding as `#[cfg(test)]` or thin public constructors during Step 3 if they don't exist (grep first). Keep them minimal — only the key field matters for redaction.

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p agentprof-core --test redact aggregate`
Expected: FAIL — `no method named redact` on `AggregateReport`.

- [ ] **Step 3: Implement the trait + impls** (append to `redact.rs`)

```rust
use crate::analyzer::aggregate::bucket::{DayBucket, McpServerBucket, ModelBucket, ToolBucket};
use crate::analyzer::aggregate::AggregateReport;

/// Redact a single aggregate bucket's identifying key in place.
pub trait RedactBucket {
    /// Redact the key field, recording reversible mappings into `models` /
    /// `servers`. No-op for `PrivacyLevel::None`.
    fn redact_key(
        &mut self,
        level: PrivacyLevel,
        models: &mut BTreeMap<String, String>,
        servers: &mut BTreeMap<String, String>,
    );
}

impl RedactBucket for ModelBucket {
    fn redact_key(&mut self, level: PrivacyLevel, models: &mut BTreeMap<String, String>, _s: &mut BTreeMap<String, String>) {
        if level == PrivacyLevel::None {
            return;
        }
        let fam = model_family(&self.model); // family at BOTH redact + anonymize (D-4)
        models.entry(fam.clone()).or_insert_with(|| self.model.clone());
        self.model = fam;
    }
}

impl RedactBucket for McpServerBucket {
    fn redact_key(&mut self, level: PrivacyLevel, _m: &mut BTreeMap<String, String>, servers: &mut BTreeMap<String, String>) {
        if level != PrivacyLevel::Anonymize {
            return; // MCP names are 🟡 MEDIUM → hashed only at anonymize
        }
        let h = crate::observability::pii::hash_short(&self.server);
        servers.entry(h.clone()).or_insert_with(|| self.server.clone());
        self.server = h;
    }
}

impl RedactBucket for ToolBucket {
    fn redact_key(&mut self, level: PrivacyLevel, _m: &mut BTreeMap<String, String>, servers: &mut BTreeMap<String, String>) {
        if level != PrivacyLevel::Anonymize {
            return;
        }
        record_mcp_server(&self.name, servers);
        self.name = hash_mcp_tool_name(&self.name);
    }
}

impl RedactBucket for DayBucket {
    fn redact_key(&mut self, _l: PrivacyLevel, _m: &mut BTreeMap<String, String>, _s: &mut BTreeMap<String, String>) {
        // D-7: the day is the aggregation dimension; never redacted.
    }
}

impl<B: RedactBucket + Clone> AggregateReport<B> {
    /// Return a redacted copy + map (filled only for `Anonymize`).
    #[must_use]
    pub fn redact(&self, level: PrivacyLevel) -> (Self, RedactionMap) {
        if level == PrivacyLevel::None {
            return (self.clone(), RedactionMap::default());
        }
        let anon = level == PrivacyLevel::Anonymize;
        let mut out = self.clone();
        let mut models = BTreeMap::new();
        let mut servers = BTreeMap::new();
        for b in &mut out.buckets {
            b.redact_key(level, &mut models, &mut servers);
        }
        let map = if anon {
            RedactionMap { uuids: BTreeMap::new(), models, mcp_servers: servers }
        } else {
            RedactionMap::default()
        };
        (out, map)
    }
}
```

> If `AggregateReport::from_buckets` doesn't exist, add it next to the struct in `analyzer/aggregate/mod.rs`:
> ```rust
> impl<B> AggregateReport<B> {
>     /// Test/constructor helper: build a report from pre-made buckets.
>     #[must_use]
>     pub fn from_buckets(buckets: Vec<B>) -> Self { Self { buckets, ..Default::default() } }
> }
> ```
> (Only if `AggregateReport: Default`; otherwise fill the other fields explicitly — grep the struct's full field list first.)

- [ ] **Step 4: Run, verify pass**

Run: `cargo test -p agentprof-core --test redact`
Expected: PASS (all redact tests).

- [ ] **Step 5: Commit**

```bash
git add crates/agentprof-core/src/analyzer/redact.rs crates/agentprof-core/src/analyzer/aggregate/mod.rs crates/agentprof-core/tests/redact.rs
git commit -m "feat(core): AggregateReport::redact via RedactBucket trait (L-1 T3)"
```

---

## Task 4: `analyze --privacy` + sidecar writer

**Files:**
- Create: `crates/agentprof-cli/src/cmd/privacy.rs` (shared sidecar helper)
- Modify: `crates/agentprof-cli/src/cmd/mod.rs` (`pub mod privacy;`)
- Modify: `crates/agentprof-cli/src/cmd/analyze.rs` (`--privacy` arg + redact call + sidecar)
- Test: `crates/agentprof-cli/tests/cli_privacy.rs` (new)

- [ ] **Step 1: Write the failing e2e test** (`crates/agentprof-cli/tests/cli_privacy.rs`)

```rust
//! E2E: `analyze --privacy` redacts PII; anonymize writes a sidecar.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use assert_cmd::Command;
use predicates::str::contains;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("agentprof-adapters/tests/fixtures/copilot")
}

#[test]
fn analyze_privacy_redact_strips_pii_from_md() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "analyze", "--agent", "copilot", "--root"])
        .arg(fixtures_root())
        .args(["--session", "00000000-0000-0000-0000-000000000006", "--export", "md", "--privacy", "redact"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(!s.contains("/tmp/agentprof-fixture"), "cwd leaked:\n{s}");
    assert!(s.contains("<uuid-0>") || s.contains("<redacted>"), "no redaction marker:\n{s}");
    // counts preserved (redaction keeps ROI signal):
    assert!(s.contains("Turn") || s.contains("Tool"), "report body gone:\n{s}");
}

#[test]
fn analyze_privacy_anonymize_writes_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("r.json");
    Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "analyze", "--agent", "copilot", "--root"])
        .arg(fixtures_root())
        .args(["--session", "00000000-0000-0000-0000-000000000006", "--export", "json", "--privacy", "anonymize", "--output"])
        .arg(&report)
        .assert()
        .success();
    let sidecar = dir.path().join("agentprof-redaction-map.json");
    assert!(sidecar.exists(), "sidecar not written");
    let map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
    assert!(map.get("uuids").is_some(), "map missing uuids key");
}
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p agentprof-cli --test cli_privacy`
Expected: FAIL — `unexpected argument '--privacy'`.

- [ ] **Step 3a: Create the sidecar helper** (`crates/agentprof-cli/src/cmd/privacy.rs`)

```rust
//! Shared redaction sidecar writer for `analyze` / `aggregate` `--privacy anonymize`.

use std::path::{Path, PathBuf};

use agentprof_core::analyzer::redact::RedactionMap;

/// Sidecar path: sibling of `--output`, else CWD.
#[must_use]
pub fn sidecar_path(output: Option<&Path>) -> PathBuf {
    match output {
        Some(p) => p
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("agentprof-redaction-map.json"),
        None => PathBuf::from("agentprof-redaction-map.json"),
    }
}

/// Write the sidecar. Returns the path on success, or `(path, err)` so the
/// caller can warn + set exit code 3 **after** the report is already emitted.
///
/// # Errors
///
/// Returns the io error (paired with the attempted path) if the write fails.
pub fn write_sidecar(map: &RedactionMap, output: Option<&Path>) -> Result<PathBuf, (PathBuf, std::io::Error)> {
    let path = sidecar_path(output);
    let json = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_string());
    match std::fs::write(&path, json) {
        Ok(()) => Ok(path),
        Err(e) => Err((path, e)),
    }
}
```

Add `pub mod privacy;` to `crates/agentprof-cli/src/cmd/mod.rs`.

- [ ] **Step 3b: Wire `--privacy` into `analyze`** (`analyze.rs`)

In `AnalyzeCmd` (after the `tool_descriptions` field), add:
```rust
    /// Redact PII from the report before rendering. `none` (default) =
    /// no change. `redact` strips 🔴 HIGH fields; `anonymize` also writes
    /// an `agentprof-redaction-map.json` sidecar. See docs/features/privacy.md.
    #[arg(long, value_enum, default_value_t = agentprof_core::analyzer::redact::PrivacyLevel::None)]
    pub privacy: agentprof_core::analyzer::redact::PrivacyLevel,
```

In `run`, replace the single line at **L329** (`let rendered = render_report(...)?;`) with the redact-then-render block (write-through above already used the original `report` for the cache):
```rust
    use agentprof_core::analyzer::redact::PrivacyLevel;
    let (report, redaction_map) = report.redact(cmd.privacy);
    let rendered = render_report(&report, &episodes, &raw.meta, &cmd, waste.as_ref())?;
    write_output(&rendered, cmd.output.as_deref())?;
    if cmd.privacy == PrivacyLevel::Anonymize && !redaction_map.is_empty() {
        match crate::cmd::privacy::write_sidecar(&redaction_map, cmd.output.as_deref()) {
            Ok(p) => eprintln!("agentprof: redaction map → {}", p.display()),
            Err((p, e)) => {
                eprintln!("agentprof: warn: failed to write {}: {e}", p.display());
                return Err(ExitKind::OutputError.into_anyhow(format!("sidecar write failed: {}", p.display())));
            }
        }
    }
    Ok(())
```
> **Note:** the existing L330 `write_output(...)? ; Ok(())` becomes part of the block above — delete the old standalone `write_output` + `Ok(())` lines so they aren't duplicated. Verify the surrounding lines after editing.
> **Note:** `--privacy` with `--export tui` should warn-and-ignore like `--output` does (tui isn't a shareable artifact). Add a `tracing::warn!(flag="--privacy", with="--export tui", "flag ignored")` next to the existing tui-flag-ignore warnings (~L307), and skip `redact()` in the tui path.

- [ ] **Step 4: Run, verify pass + no regressions**

Run: `cargo test -p agentprof-cli --test cli_privacy && cargo test -p agentprof-cli --test cli`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/agentprof-cli/src/cmd/privacy.rs crates/agentprof-cli/src/cmd/mod.rs crates/agentprof-cli/src/cmd/analyze.rs crates/agentprof-cli/tests/cli_privacy.rs
git commit -m "feat(cli): analyze --privacy flag + redaction sidecar (L-1 T4)"
```

---

## Task 5: `aggregate --privacy`

**Files:**
- Modify: `crates/agentprof-cli/src/cmd/aggregate.rs` (`--privacy` arg + `AnyAggregateReport::redact` + call + sidecar)
- Test: `crates/agentprof-cli/tests/cli_privacy.rs` (extend)

> **Pre-step — locate the enum + render call.** Run `grep -n 'enum AnyAggregateReport\|fn run\|render(&any_report\|any_report =' crates/agentprof-cli/src/cmd/aggregate.rs`. The redact call goes **after** `any_report` is finalized (both the filesystem path ~L505 and the dual-path/store branch ~L693 converge on an `AnyAggregateReport` before render ~L213). If they don't converge, apply the redact at each render site.

- [ ] **Step 1: Write the failing test** (append to `tests/cli_privacy.rs`)

```rust
#[test]
fn aggregate_privacy_redact_models_to_family() {
    let out = Command::cargo_bin("agentprof")
        .unwrap()
        .args(["--no-cache", "aggregate", "--agent", "copilot", "--root"])
        .arg(fixtures_root())
        .args(["--by", "model", "--since", "all", "--export", "md", "--privacy", "redact"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    // family only — no full internal model identifier:
    assert!(!s.contains("-1m-internal"), "internal model name leaked:\n{s}");
}
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p agentprof-cli --test cli_privacy aggregate`
Expected: FAIL — `unexpected argument '--privacy'`.

- [ ] **Step 3a: Add `--privacy` to `AggregateCmd`** (same arg block as analyze)

```rust
    /// Redact PII before rendering (see `analyze --privacy`).
    #[arg(long, value_enum, default_value_t = agentprof_core::analyzer::redact::PrivacyLevel::None)]
    pub privacy: agentprof_core::analyzer::redact::PrivacyLevel,
```

- [ ] **Step 3b: Add `AnyAggregateReport::redact`** (in `aggregate.rs`, next to the enum)

```rust
impl AnyAggregateReport {
    /// Redact each variant's inner `AggregateReport<B>` (Task 3 core method).
    fn redact(
        self,
        level: agentprof_core::analyzer::redact::PrivacyLevel,
    ) -> (Self, agentprof_core::analyzer::redact::RedactionMap) {
        use AnyAggregateReport as A;
        match self {
            A::Tool(r) => { let (r, m) = r.redact(level); (A::Tool(r), m) }
            A::McpServer(r) => { let (r, m) = r.redact(level); (A::McpServer(r), m) }
            A::Day(r) => { let (r, m) = r.redact(level); (A::Day(r), m) }
            A::Model(r) => { let (r, m) = r.redact(level); (A::Model(r), m) }
        }
    }
}
```
> Variant names assumed `Tool/McpServer/Day/Model` per L506/566/572/578. Confirm against the actual enum and match exactly.

- [ ] **Step 3c: Call redact before render** (in `run`, after `any_report` is finalized, before it's passed to the render dispatch)

```rust
    use agentprof_core::analyzer::redact::PrivacyLevel;
    let (any_report, redaction_map) = any_report.redact(cmd.privacy);
    // ... existing render dispatch on &any_report ...
    // after write_output of the rendered string:
    if cmd.privacy == PrivacyLevel::Anonymize && !redaction_map.is_empty() {
        match crate::cmd::privacy::write_sidecar(&redaction_map, cmd.output.as_deref()) {
            Ok(p) => eprintln!("agentprof: redaction map → {}", p.display()),
            Err((p, e)) => {
                eprintln!("agentprof: warn: failed to write {}: {e}", p.display());
                return Err(ExitKind::OutputError.into_anyhow(format!("sidecar write failed: {}", p.display())));
            }
        }
    }
```
> Apply the same `--export tui` warn-and-skip note as analyze (don't redact the TUI path).

- [ ] **Step 4: Run, verify pass + no regressions**

Run: `cargo test -p agentprof-cli --test cli_privacy && cargo test -p agentprof-cli --test aggregate`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/agentprof-cli/src/cmd/aggregate.rs crates/agentprof-cli/tests/cli_privacy.rs
git commit -m "feat(cli): aggregate --privacy flag (L-1 T5)"
```

---

## Task 6: Docs + ADR + final verification

**Files:** ADR + privacy.md + architecture.md + cli README + CHANGELOG + ROADMAP.

- [ ] **Step 1: Create ADR-0026** (`docs/internals/adr-0026-report-redaction.md`)

Sections (codify spec §3 decisions): Context (L-1, privacy.md §2 tiers) · Considered options (core-layer vs format-layer vs regex — D-1) · Decision (core `analyzer::redact`, `--privacy` enum, two levels, model→family both levels D-4, stable UUID D-5, MCP keep-tool D-6, day not redacted D-7, pure `redact()` D-8) · Consequences (all formats inherit; aggregate via `RedactBucket`; sidecar only at anonymize). Append the ADR row to `docs/architecture.md` §15 ADR table (→ ADR-0026).

- [ ] **Step 2: Update privacy.md** — flip §4 from "NOT YET IMPLEMENTED" to implemented: document `--privacy <none|redact|anonymize>`, the field-rule table (note `repository` is also redacted; `agent_version`+`producer` not `copilot_version`), the sidecar, and that `list` is still manual (future work). Update the top-of-file Status block.

- [ ] **Step 3: Update architecture.md §8** — add `--privacy <none|redact|anonymize>` to the `analyze` and `aggregate` flag blocks; one line in the §8 status summary.

- [ ] **Step 4: Update cli README** — add `--privacy` to the `analyze` + `aggregate` subcommand sections.

- [ ] **Step 5: CHANGELOG `[Unreleased]`** — `### Added`: `**cli:** \`--privacy <none|redact|anonymize>\` on analyze + aggregate — opt-in report redaction (L-1). core \`analyzer::redact\`, ADR-0026.`

- [ ] **Step 6: ROADMAP §6.1 L-1** — flip to `✅ FIXED`: `~~隐私字段默认裸露~~ → ✅ \`--privacy redact|anonymize\` (ADR-0026)`. Update the `计划修复` column to `—`.

- [ ] **Step 7: Final gate** (the §8 local gate from copilot-instructions)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace
```
Expected: all green; new redact/privacy tests pass; no clippy warnings (watch for `unwrap_used` — none in lib code).

- [ ] **Step 8: Commit**

```bash
git add docs/ crates/agentprof-cli/README.md CHANGELOG.md tasks/ROADMAP.md
git commit -m "docs: privacy redaction L1/L2/L3 sync + ADR-0026 (L-1 T6)"
```

---

## Self-review

- **Spec coverage:** §3 D-1..D-8 → T1/T2/T3 (layer, levels, model/UUID/MCP, day, pure) ✓; §4 rules → T2/T3 ✓; §5 algorithms → T1 ✓; §6 RedactionMap/sidecar → T1/T4 ✓; §7 aggregate → T3/T5 ✓; §8 errors → T4/T5 (exit 3) ✓; §9 tests → every task's TDD steps ✓; §10 manifest → all tasks ✓.
- **Spec corrections folded in:** `agent_version`+`producer` (not copilot_version); `repository` redacted (T2); `ExitKind::OutputError` confirmed.
- **Type consistency:** `PrivacyLevel` / `RedactionMap` / `redact(level)->(_,RedactionMap)` / `RedactBucket::redact_key` / `hash_mcp_tool_name` / `model_family` / `UuidRedactor::{redact,into_inverse}` consistent across T1–T5. `write_sidecar`/`sidecar_path` shared in `cmd::privacy`.
- **Known risk (noted in tasks):** test-fixture constructors (`TurnSummaryRow::new`, `AggregateReport::from_buckets`, bucket builders) may need adding if types are `#[non_exhaustive]` — each task's pre-step says to grep + add minimal constructors.
