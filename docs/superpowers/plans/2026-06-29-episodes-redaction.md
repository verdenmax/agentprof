# Episodes Redaction + `list --privacy` (F-10) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Close L-1's deferred surfaces — `analyze` html/speedscope flamegraph + `list` — by sharing redaction state so table & flamegraph turn-ids match.

**Architecture:** Factor L-1's per-`redact` accumulators into a reusable `RedactionContext { uuids, models, servers }`; add `redact_with(level, &mut ctx)` to `AnalysisReport` + `Episodes`; keep `redact(level)` as a one-shot wrapper (L-1 API/tests unchanged). `analyze` threads one ctx → both → sidecar. Add `list --privacy`.

**Tech Stack:** Rust, `agentprof-core` (redact), `agentprof-cli` (analyze/list), assert_cmd. Spec: `docs/superpowers/specs/2026-06-29-episodes-redaction-design.md`.

---

## File Structure
| File | Responsibility |
|---|---|
| `crates/agentprof-core/src/analyzer/redact.rs` | `RedactionContext`; `AnalysisReport::redact`→thin wrapper over `redact_with`; `Episodes::redact_with`. |
| `crates/agentprof-core/src/episode/episodes.rs` | (test sites) Episodes redaction unit tests. |
| `crates/agentprof-cli/src/cmd/analyze.rs` | one ctx → redact report+episodes; drop `warn_unredacted_flamegraph`. |
| `crates/agentprof-cli/src/cmd/list.rs` | `--privacy` flag; redact rows. |
| docs | ADR-0028, privacy.md, ROADMAP, CHANGELOG. |

**Tasks:** T1 RedactionContext refactor · T2 Episodes::redact_with · T3 analyze rewire · T4 list --privacy · T5 docs+ADR.

---

## Task 1: Extract `RedactionContext`, keep `redact()` wrapper

**Files:** Modify `crates/agentprof-core/src/analyzer/redact.rs`; tests in `crates/agentprof-core/tests/redact.rs`.

- [ ] **Step 1: Failing test** — append to `tests/redact.rs`:
```rust
#[test]
fn shared_ctx_gives_stable_uuids_across_reports() {
    use agentprof_core::analyzer::redact::{PrivacyLevel, RedactionContext};
    let mut ctx = RedactionContext::default();
    let r1 = ctx.redact_uuid("sess-1");
    let r2 = ctx.redact_uuid("sess-1");
    let r3 = ctx.redact_uuid("turn-9");
    assert_eq!(r1, "<uuid-0>");
    assert_eq!(r2, "<uuid-0>");
    assert_eq!(r3, "<uuid-1>");
    assert!(!ctx.into_map().uuids.is_empty());
}
```
Run `cargo test -p agentprof-core --test redact shared_ctx` → FAIL (no `RedactionContext`).

- [ ] **Step 2: Add `RedactionContext`** in `redact.rs`:
```rust
/// Mutable accumulator shared across `AnalysisReport` + `Episodes` so turn
/// UUIDs map consistently between the table and the flamegraph.
///
/// # Examples
/// ```
/// use agentprof_core::analyzer::redact::RedactionContext;
/// let mut c = RedactionContext::default();
/// assert_eq!(c.redact_uuid("a"), "<uuid-0>");
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct RedactionContext {
    pub uuids: UuidRedactor,
    pub models: BTreeMap<String, String>,
    pub servers: BTreeMap<String, String>,
}
impl RedactionContext {
    /// Stable `<uuid-N>` for `original`. # Examples
    /// ```
    /// # use agentprof_core::analyzer::redact::RedactionContext;
    /// assert_eq!(RedactionContext::default().redact_uuid("x"), "<uuid-0>");
    /// ```
    pub fn redact_uuid(&mut self, original: &str) -> String { self.uuids.redact(original) }
    /// Consume into the exported map (uuids inverted; models/servers as-is). # Examples
    /// ```
    /// # use agentprof_core::analyzer::redact::RedactionContext;
    /// assert!(RedactionContext::default().into_map().is_empty());
    /// ```
    #[must_use]
    pub fn into_map(self) -> RedactionMap {
        RedactionMap { uuids: self.uuids.into_inverse(), models: self.models, servers: self.mcp_servers_alias() }
    }
    fn mcp_servers_alias(self) -> BTreeMap<String,String> { self.servers } // placeholder; fold into into_map
}
```
> Implementer: simplify `into_map` to use the fields directly (build `RedactionMap{ uuids: self.uuids.into_inverse(), models: self.models, mcp_servers: self.servers }`); the alias above is illustrative.

- [ ] **Step 3: `AnalysisReport::redact_with(level, &mut ctx)`** — move the existing body of `redact` (lines ~201-285) into `redact_with`, using `ctx.uuids`/`ctx.models`/`ctx.servers` instead of locals; do NOT build the map there. Then `redact` wraps:
```rust
pub fn redact(&self, level: PrivacyLevel) -> (Self, RedactionMap) {
    if level == PrivacyLevel::None { return (self.clone(), RedactionMap::default()); }
    let mut ctx = RedactionContext::default();
    let out = self.redact_with(level, &mut ctx);
    (out, if level == PrivacyLevel::Anonymize { ctx.into_map() } else { RedactionMap::default() })
}
pub fn redact_with(&self, level: PrivacyLevel, ctx: &mut RedactionContext) -> Self { /* moved body, returns out */ }
```

- [ ] **Step 4** Run `cargo test -p agentprof-core --test redact` → all green (L-1 tests unchanged + new).
- [ ] **Step 5** `cargo clippy -p agentprof-core --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.
- [ ] **Step 6 Commit:** `git commit -m "refactor(core): extract RedactionContext, redact() wraps redact_with (F-10 T1)"`

---

## Task 2: `Episodes::redact_with`

**Files:** `crates/agentprof-core/src/analyzer/redact.rs` (add `impl Episodes`); tests `crates/agentprof-core/tests/redact.rs`. Episodes type: `crates/agentprof-core/src/episode/episodes.rs` (turns/tools/hooks/skills/aborts/warnings/model_metrics/loaded_mcp_tools); Turn fields: `episode/turn.rs` (id/started_at/ended_at/model/status/{tool,hook,skill}_calls: Vec<CallRef>); CallRef.name + ToolEpisode keyed by name.

- [ ] **Step 1: Failing test** — build an `Episodes` with one turn (id `"t-1"`, model `"claude-sonnet-4.6"`), one MCP tool key `"mcp__github__search"` referenced by a CallRef. `Episodes::redact_with(Anonymize, &mut ctx)`: assert turn.id==`<uuid-0>`, model==`claude-sonnet`, tools key hashed, the CallRef.name hashed to the SAME value, started_at==epoch, warnings cleared. Run → FAIL.
- [ ] **Step 2:** add `pub fn redact_with(&self, level, ctx: &mut RedactionContext) -> Self`:
  - turns: `id = ctx.redact_uuid(&id)`; model→family (record ctx.models); anon: started_at/ended_at→UNIX_EPOCH, abort.at→epoch, each `*_calls[].name`→`hash_mcp_tool_name` (record server). 
  - tools/hooks/skills: anon: rebuild BTreeMap rekeying via `hash_mcp_tool_name` (record server); **CallRef.name rewritten with the same fn → cross-ref intact**.
  - aborts: anon at→epoch. warnings: clear. model_metrics: family-merge (mirror AnalysisReport). loaded_mcp_tools: anon `hash_mcp_tool_name`. Redact level: only id+model+clear-warnings.
  - rustdoc + `# Examples`.
- [ ] **Step 3** `cargo test -p agentprof-core --test redact` green. Step 4 clippy+fmt. Step 5 commit `feat(core): Episodes::redact_with (F-10 T2)`.

## Task 3: analyze rewire — one ctx, redacted flamegraph
**Files:** `crates/agentprof-cli/src/cmd/analyze.rs` (~341-369); tests `tests/cli_privacy.rs`.
- [ ] **Step 1:** non-vacuous test: anonymize html+speedscope of an MCP fixture shows NO original turn-id / server name (assert `!contains("turn-realid")`, `!contains("github")`). FAIL today.
- [ ] **Step 2:** replace `let (report, map)=report.redact(p)` with `let mut ctx=RedactionContext::default(); let report=report.redact_with(p,&mut ctx); let episodes=episodes.redact_with(p,&mut ctx); let map = if p==Anonymize {ctx.into_map()} else {default};`. Pass redacted `episodes` to `render_report`. Delete `warn_unredacted_flamegraph` + its call.
- [ ] **Step 3** green; **Step 4** commit `feat(cli): analyze redacts episodes for flamegraph (F-10 T3)`.

## Task 4: `list --privacy`
**Files:** `crates/agentprof-cli/src/cmd/list.rs`; tests `tests/cli_config.rs`-style new `tests/cli_list_privacy.rs`.
- [ ] **Step 1:** add `#[arg(long, value_enum, default_value_t=None)] privacy` to ListCmd; e2e test: `list --privacy redact` shows no cwd/branch, session id→`<uuid-N>`, model→family. FAIL.
- [ ] **Step 2:** one `RedactionContext` per run; per row redact id→`ctx.redact_uuid`, cwd/branch→`<redacted>`, model→family. (Rows built post-filter; reuse `redact_opt`/`model_family`.) **Step 3** green; clippy/fmt; **Step 4** commit `feat(cli): list --privacy (F-10 T4)`.

## Task 5: docs + ADR-0028
- [ ] ADR-0028 (RedactionContext + Episodes); privacy.md §4.3 fully-redacted / drop deferred + §4.4 closed; ROADMAP F-10→done; CHANGELOG; remove L-1 deferred wording in adr-0026/privacy.md. Gate: fmt/clippy/test/doc green. Commit `docs: F-10 close deferred-leak + ADR-0028`.

## Self-review
Spec coverage: RedactionContext(T1) Episodes(T2) analyze(T3) list(T4) docs(T5) ✅. CallRef↔key sync pinned (T2). redact() wrapper keeps L-1 green. Placeholders: into_map alias illustrative (noted). Types: redact_with/redact_uuid/into_map consistent.
