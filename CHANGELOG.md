# Changelog

All notable changes to **agentprof** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries are prefixed with the affected crate when relevant:
`core:` / `adapters:` / `storage:` / `tui:` / `cli:` / `xtask:`.

Breaking changes are marked `BREAKING:` (matching the Conventional Commits
prefix used in commit messages).

## [Unreleased]

> Next milestone TBD. v0.4.0 reserved for Phase 3 multi-agent
> (M3.1 ClaudeAdapter + M3.2 CodexAdapter).

### Added

- **cli:** `config` subcommand (`path` / `show` / `edit` / `init`) to manage the user config file (`$AGENTPROF_CONFIG`, else `~/.config/agentprof/config.toml`). `show` prints the **effective** configuration (built-in defaults merged with file overrides) with `(default)` / `(from file)` source annotation, reusing the real resolvers (`resolve_storage_config` / `OtlpServerConfig::from_partial`) so shown defaults can't drift; `edit` self-heals an absent file from a template; `init [--force]` writes a commented default. Unifies config-path resolution into `resolve_config_path()`, shared by `config` / `ingest-otlp` / `serve` (dedup of two prior copies). Scoped to the wired `[storage]` / `[otlp]` / `[serve]` blocks — fixes the architecture §10 schema that advertised parse-failing `[paths]` / `[tokenizer]` / `[pricing]`. (L-4, ADR-0027)
- **cli:** `--privacy <none|redact|anonymize>` on `analyze` + `aggregate` + `list` — opt-in report redaction (L-1, F-10). New core `analyzer::redact` (PrivacyLevel/RedactionMap/AnalysisReport+AggregateReport::redact), `agentprof-redaction-map.json` sidecar at anonymize. All formats fully redacted — `analyze` threads one `RedactionContext` through both the report and its episodes (`Episodes::redact_with`) so md/json/csv/html/speedscope share turn-ids and the flamegraph leaks no original turn-id or raw MCP server name. `list` reuses one `RedactionContext` per invocation to give stable `<uuid-N>` session ids and family-ized models without a sidecar (F-10). See ADR-0026 + ADR-0028.

### Added (docs — visual guide, M2.3.x)

- **`docs/visual-guide/`**: 中文 HTML 可视化教程（14 课分两章 — 用法 6 课 + Wiki 8 课）。源码 + 14 个内容模块在 `xtask/src/visual_guide/`，模板在 `xtask/templates/visual_guide/`，资产在 `docs/visual-guide/assets/`。生成的 `*.html` 不入 git（[ADR-0025](docs/internals/adr-0025-visual-guide.md) D-2）。
- **`cargo run -p xtask -- visual-guide [--clean] [--check]`** 子命令构建可视化指南。
- **`.github/workflows/visual-guide.yml`** GH Pages 联动：main push 重生成 + 部署，PR 仅 `--check`。
- **在线阅读**：<https://verdenmax.github.io/agentprof/>
- **ADR-0025** 记录架构决策（7 个 D-* + 7 个 Implementation Notes）。
- **6 placeholder SVG assets** (`docs/visual-guide/assets/`) + 1 real 5-crate dep diagram；real PNG screenshots 作为 followup F1 替换。

### Documentation

- `README.md` 顶部加「📖 在线阅读」section。
- `docs/architecture.md` §15.1 / §15.3 / §14.4 同步：repo layout 加 `docs/visual-guide/`，CI 矩阵加 `visual-guide.yml` 行，ADR 表加 ADR-0025。
- `crates/agentprof-cli/README.md` 顶部加 pointer 到可视化指南。
- 新建 `docs/visual-guide/README.md` 本地构建说明。

### Tests

- **+27 tests**（workspace 整体 1301 → 1328）：xtask 新增 5 integration tests (`tests/visual_guide.rs`) + 22 unit tests（shell smoke + css smoke + components ×3 + highlight ×4 + pages ×2 + 14 lesson render tests + 1 surface）。
- **cli:** 修复 `cli_list_cache_column::list_header_includes_cache_pct_column`：原测试对空 `--root` 目录断言表头，但 `list` 对空目录走 `(no sessions …)` 早返回分支、从不渲染表头，故该测试自 `be592a1`（M2.5 T8）引入起一直失败。改为针对 committed Copilot fixtures + `--since all` 校验 8 列表头含 `Cache%`。

### Fixed

- **cli:** `analyze --section mcp-waste --privacy anonymize` no longer leaks raw MCP server + tool names — the `WasteReport` is now redacted (`WasteReport::redact_with`) through the *same* `RedactionContext` as the report/episodes, so server hashes match `tool-rank`/flamegraph and md/json/html show only `hash_short` server + `mcp__<hash8>__` tool names (audit leak A).
- **cli:** `list --privacy anonymize` now zeroes the `Started` column to the Unix epoch (mirroring report anonymize); `redact` keeps the real timestamp (audit leak B).

## [0.3.3] - 2026-06-11

> M2.3 web dashboard wave. New `agentprof serve` subcommand (feature
> `web`) — live HTTP dashboard backed by the SQLite store. Closes Q-7.2
> ("纯静态够用，还是要 server 模式") from `docs/plan.md` §7.2. v0.4.0
> remains reserved for Phase 3 multi-agent (M3.1 ClaudeAdapter + M3.2
> CodexAdapter); see [ADR-0024 D-7](docs/internals/adr-0024-web-dashboard-architecture.md#d-7-release-as-v033-not-v040).

### Added (M2.3 web dashboard ✅)

- **cli:** new `agentprof serve` subcommand (feature `web`) — live HTTP
  dashboard backed by the SQLite store with 5 polling views (sessions
  list, single session, aggregate, MCP-waste list, MCP-waste detail).
- **cli:** routes `GET /sessions`, `/session/:id`,
  `/aggregate?by=model|tool|day&since=...`, `/mcp-waste`,
  `/mcp-waste/:server` + matching `/api/*.html` chunk endpoints for the
  vanilla JS poller (~80 LOC, no framework). `GET /` → `303 /sessions`;
  `GET /healthz` → `200 "healthy"`.
- **cli:** `[serve]` config-file block (`bind` / `interval_default` /
  `auto_open`) with precedence CLI > file > defaults (none of those
  three fields have env vars; `[storage] path` is currently NOT wired
  into the `serve` storage resolver — CLI flag or
  `AGENTPROF_STORAGE_PATH` env var only). Mirrors the M2.2 `[otlp]`
  block's loader pattern.
- **cli:** `format::html::render_body_only` +
  `format::aggregate_html::render_body_only` — public extractors that
  slice the existing full-page render output for embedding inside the
  dashboard chrome (zero blast radius on M1.6.4 / M1.6.5 snapshots).
- **cli:** `cmd::aggregate::compute_aggregate_from_store` — store-mode
  parallel to `compute_aggregate` (supports `by=model|tool|day`;
  `mcp-server` returns HTTP 400 with a `/mcp-waste` pointer per
  ADR-0024 D-3 Consequences).
- **cli:** `cmd::mcp_waste::compute_aggregate_waste_from_store` —
  heuristic-only store-mode aggregator (no sidecar / no MCP config
  plumbing; a banner directs users to the CLI for accurate counts).
- **cli:** bundled `dashboard.css` / `dashboard.js` / `favicon.svg`
  via `include_str!` / `include_bytes!` (no runtime FS reads, no CDN).
- **cli:** default bind `127.0.0.1:4329` (loopback); non-loopback bind
  emits `tracing::warn!` recommending reverse-proxy auth (ADR-0024 D-6).
- **cli:** new feature `agentprof-cli/features = ["web"]` (included in
  `full`); pulls `axum` / `tower` / `tower-http` / `tokio` / `open`
  (the first three shared with `otlp`).

### Documentation

- **L3:** [ADR-0024 — Web dashboard
  architecture](docs/internals/adr-0024-web-dashboard-architecture.md):
  seven design decisions D-1..D-7 from the M2.3 spec §3 + four
  implementation notes from T5–T11 (axum 0.7 `:name` path-param
  syntax, matchit `:server.html` capture, askama 0.16 framework
  integration deprecation, sliced `render_body_only`).
- **L2:** [`docs/features/web-dashboard.md`](docs/features/web-dashboard.md)
  — new cross-crate feature index (crate landscape table, quickstart,
  endpoint table, caveats, test layout).
- **L1:** [`docs/architecture.md`](docs/architecture.md) §3 (cli row
  updated), §8 (new `agentprof serve` subsection), §14.4 (ADR-0021,
  0022, 0023, 0024 rows added), §15.4 (web feature flag bullet +
  `full` expansion).
- **L2:** [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md)
  — new `## agentprof serve` section (quick start + flag reference +
  `[serve]` config block + views table + security note); Modules /
  Features tables updated.
- **Root:** [`README.md`](README.md) — `agentprof serve` bullet in §8
  CLI Subcommands list + "Live dashboard" callout in Quick start.

### Tests

- **+29 tests** (workspace total 1272 → **1301**, 0 failures across all crates).
- **cli:** 24 unit tests in `cmd::serve::router_tests` (T5–T10
  `tower::ServiceExt::oneshot` against an in-process Axum `Router`
  with a fixture-populated `Arc<Mutex<Db>>`): 5 dashboard views × ~5
  cases each (200 / 404 / 400 / empty-store / content-type assertions)
  + `/healthz` + `/static/:name` MIME-type checks + `/` redirect + 2
  insta snapshots (sessions-empty-store, session-page-fixture).
- **cli:** 5 end-to-end tests in `tests/cli_serve_e2e.rs` spawn
  `agentprof serve` as a real subprocess on an ephemeral port and
  probe via `reqwest` — healthz roundtrip, missing-storage UserError,
  nonexistent-storage UserError, sessions page render, static asset
  MIME types.

### Fixed

- **cli:** layout chrome nav anchor `/waste` → `/mcp-waste` (matches
  the active-nav literal used by all four MCP-waste handlers; surfaced
  during T10 router wiring).

> Next milestone TBD. v0.4.0 reserved for Phase 3 multi-agent completion (Claude + Codex adapters).

## [0.3.2] - 2026-06-11

### Fixed

- **CI: rustls `CryptoProvider` panic** on `otlp_tls_smoke` tests
  (`grpc_serves_over_tls_with_trusted_client` /
  `grpc_mtls_rejects_client_without_cert`). The tonic 0.12 + rustls
  0.23 path requires explicit `CryptoProvider::install_default(...)`
  before any `ServerConfig::builder()` call; previously this worked
  locally because some test order happened to install a provider
  transitively, but the GitHub-hosted runner's parallel test
  scheduling made it reliably panic. Fix: new
  `agentprof_storage::otlp::tls::ensure_crypto_provider_installed()`
  helper (idempotent via `std::sync::Once`, installs `ring` provider);
  called from `load_server_tls_config` / `serve_grpc` / `serve_http`
  + `otlp_tls_smoke` test setup. Strictly internal change; no API
  surface impact.

## [0.3.1] - 2026-06-11

> Consolidated wave bundling two post-v0.3.0 efforts under one tag:
>
> 1. **Audit fixes** from the post-v0.3.0 comprehensive review
>    (originally on `fix/v0.3.1-audit-findings` branch, never
>    independently released): PII path redaction, `#[non_exhaustive]`
>    sweep on 5 pub types, exit-code-3 spec realignment, plus the
>    M1 LRU admission race fix + M2 HTTP layer-order doc that
>    already shipped in v0.3.0 (cf33b91) and the §18 open-question
>    closure (Q1/Q2/Q3/Q4).
>
> 2. **M2.5 observational cache analytics** — surfaces Anthropic
>    prompt-cache token data (`cache_read` / `cache_creation`)
>    across `analyze` / `list` / `aggregate` / TUI with hit-rate
>    and saved-tokens. Zero schema change (reuses M2.1 columns +
>    M1.6.x `model_metrics`). Closes `docs/architecture.md` §18 Q4a
>    + audit finding F-NEW-2. See
>    [ADR-0023](docs/internals/adr-0023-cache-metrics.md).
>
> The numbering jump from v0.3.0 → v0.3.1 (skipping the originally
> announced v0.4.0 for M2.5 alone) reflects the product decision to
> reserve v0.4.0 for the Phase 3 multi-agent milestone (Claude +
> Codex adapter completion). M2.5 is incremental render-surface
> polish on cache token data already in the schema since M2.1 —
> it belongs on the v0.3.x line.

### Fixed (audit-fixes wave)

- **`adapters,copilot`** — `tool_sidecar.rs` now redacts filesystem paths
  through `agentprof_core::observability::pii::hash_path` in the
  `#[tracing::instrument]` span field and in the two `tracing::warn!`
  sites for unreadable / malformed sidecar files. Closes audit
  finding F-NEW-3 (ADR-0010 D-4 compliance gap surfaced in the
  post-v0.3.0 comprehensive review). The companion `mcp_config.rs`
  `#[tracing::instrument]` span was redacted in the same pass.

### Changed (audit-fixes wave)

- **API ergonomics** — added `#[non_exhaustive]` to 5 previously
  exhaustive pub types to allow future additive variants/fields
  without major version bumps:
  - `agentprof_core::export::speedscope::EventType`
  - `agentprof_tui::{Event, Action, View}`
  - `agentprof_storage::otlp::config::PartialOtlpServerConfig`
  Closes audit finding F-NEW-4 (project rule §7-5).
  Cross-crate literal-init sites for `PartialOtlpServerConfig` (in
  `agentprof-cli` tests, `agentprof-storage` integration tests, and
  the rustdoc example) were refactored to `Default::default()` +
  mut-assign; behavior unchanged.

### Documentation (audit-fixes wave)

- **Exit code 3 — spec realignment.** `docs/architecture.md` §8.1
  and `.github/copilot-instructions.md` §7-11 both stated that
  exit code `3` meant "external service error". The actual code
  has used exit `3` consistently since M1.x for any
  output / I/O class failure (file write, non-TTY TUI start, OTLP
  listener bind, TUI runtime, JSON render, external service call —
  27 call sites). The docs now reflect that reality, and
  `crates/agentprof-cli/src/cmd/exit.rs` module rustdoc was
  broadened to match. **No user-visible behavior change**; scripts
  relying on `case $? in 3) ...` continue to work. The future option
  of splitting external-service failures to a separate code (e.g.
  `4`) is now spelled out as v0.4.0+ minor bump territory. Closes
  audit finding F-NEW-1.
- **`docs/architecture.md` §18 open-question closure.** Q1
  (Speedscope evented format), Q2 (HTML single-file), Q3 (OTLP
  receiver as `agentprof ingest-otlp` subcommand) all back-marked
  with rationale + citations to ADRs / implementing commits. Q4
  split into Q4a (observational cache analytics — closed by this
  release's M2.5 wave) + Q4b (prompt-prefix recommendation engine —
  deferred with explicit trigger criteria).
- **ADR-0021** — post-v0.3.1 addendum codifying the
  receiver-as-subcommand decision with escape-hatch note for any
  future standalone-binary use case.

### Added — M2.5 observational cache analytics

> Surfaces the prompt-cache token data (Anthropic `cache_read` /
> `cache_creation`) that the storage layer has been capturing since
> M1.6.x + M2.x but only the TUI Models view displayed. Closes
> [`docs/architecture.md`](docs/architecture.md) §18 Q4a and audit
> finding F-NEW-2 (write-only `total_cache_read` / `total_cache_creation`
> schema columns now have a read path). **ZERO schema change** — all
> wiring uses existing M2.1 columns + M1.6.x `model_metrics`. Design
> codified in
> [ADR-0023](docs/internals/adr-0023-cache-metrics.md).

- **core: new `agentprof_core::analyzer::cache` module** —
  `CacheMetrics` struct (6 fields: `creation_tokens` / `read_tokens` /
  `hit_pct_honest` / `hit_pct_naive` / `saved_tokens_net` /
  `saved_tokens_gross`) + 2 pricing constants
  (`CACHE_READ_DISCOUNT = 0.9` and `CACHE_WRITE_PREMIUM = 0.25`,
  Claude Sonnet 4.x 2026-06 rates). Dual hit-rate formulas
  ([ADR-0023](docs/internals/adr-0023-cache-metrics.md) D-2): honest
  (`read / (read + creation)`) + naive (`read / (read + input)`).
  Dual saved-token formulas: net (`read * 0.9 − creation * 0.25`,
  can be negative) + gross (`read * 0.9`). `CacheMetrics::from_totals`
  returns `Option<CacheMetrics>`, `None` on zero cache activity so
  render layers can skip cleanly (ADR-0023 D-1).
- **core: `AnalysisReport::cache_metrics() -> Option<CacheMetrics>`** —
  per-session accessor wrapping the existing M2.1 T2.4
  `total_*` accumulators.
- **core: per-bucket cache attribution for `--by model` / `--by day`**
  ([ADR-0023](docs/internals/adr-0023-cache-metrics.md) D-3). New
  `CacheAttributable` trait + `AggregateReport::cache_metrics_per_bucket()`
  accessor (trait-bound, available only on `AggregateReport<ModelBucket>`
  and `AggregateReport<DayBucket>`) returning per-bucket
  `CacheMetrics`. New `supports_cache_attribution(AggregateKey) -> bool`
  helper for render-layer / `AnyAggregateReport` dispatch where the
  bucket type has been erased. `ToolBucket` + `McpServerBucket`
  deliberately do **not** implement `CacheAttributable` — per-tool /
  per-server cache attribution is undefined because cache tokens are
  accumulated per API call, not per tool invocation. New fields on
  `ModelBucket` + `DayBucket`: `total_input_tokens`,
  `total_cache_read`, `total_cache_creation` (all `#[serde(default)]`
  for cached-JSON backward-compat). New `with_cache_metrics(input,
  read, creation)` builder method on both bucket types.
- **cli: `analyze --export md` renders a `## Cache` section** when
  `cache_metrics()` is `Some` — 6-row table (creation tokens, read
  tokens, honest hit %, naive hit %, net saved tokens, gross saved
  tokens) appended after the MCP-waste section and before warnings.
  Reports without cache activity omit the section entirely (no
  all-zero table) so default md output stays byte-identical to v0.3.0.
- **cli: `analyze --export html` renders a `<section id="cache">`
  block** mirroring the md surface. Percentages are pre-formatted in
  Rust (`"55.6%"`) via a `CacheSection` template helper, avoiding
  askama 0.16 format-filter dialect concerns. All 5 existing
  `analyze_html__*` snapshots unchanged (omission path verified
  byte-identical).
- **cli: `analyze --export json` always emits a top-level
  `cache_metrics` field** (`null` when no cache activity, the
  `CacheMetrics` object otherwise; ADR-0023 D-6 snake_case).
- **cli: `list` gains a `Cache%` column** (8th column) showing each
  session's honest cache hit rate (empty cell when no cache activity).
- **cli: `aggregate --by model` and `--by day` render 4 cache columns**
  (`CacheCr` / `CacheRd` / `Hit%` / `NetSaved`) across md, csv, and
  html ([ADR-0023](docs/internals/adr-0023-cache-metrics.md) D-3 +
  D-5). `--by tool` and `--by mcp-server` deliberately omit these
  columns. Cells are blank when a bucket has no cache activity. CSV
  header keys are snake_case (`cache_creation` / `cache_read` /
  `hit_pct_honest` / `saved_net`); md / html keep the CamelCase
  user-facing labels. Existing
  `aggregate__aggregate_md__by_day` snapshot regenerated to include
  the 4 new columns (all rows with empty cache cells);
  `by_tool` / `by_mcp_server` snapshots byte-identical.
- **tui: Models view gains a `NetSaved` column** showing per-model
  net saved tokens (`read * 0.9 − creation * 0.25`). The existing
  cache_read column is unchanged.

### Documentation

- [ADR-0023](docs/internals/adr-0023-cache-metrics.md) codifies the
  6 cache-metrics design decisions (D-1 `None`-on-zero-activity
  semantics, D-2 honest + naive formula choice, D-3 per-tool /
  per-server omission policy, D-4 render rules per surface, D-5
  CSV / md / html / json field naming conventions, D-6 forward
  compatibility for future `cost_usd` field).
- `docs/architecture.md` §17 roadmap row for Phase 2 (M2.5) +
  §18 split of Q4 → Q4a (closed by M2.5) + Q4b (recommendation
  engine, deferred).
- `docs/plan.md` §6 Phase 2 (M2.5 entry) + §7.2 Q4a / Q4b split.
- `crates/agentprof-core/README.md` modules table gains an
  `analyzer::cache` row.
- `crates/agentprof-cli/README.md` `analyze` / `list` / `aggregate`
  flag tables gain M2.5 footnotes.

### Tests

- **+36 tests** (workspace total 1226 → **1262**, 0 failures across
  `cargo test --workspace --all-features`). Coverage spans
  `agentprof-core` (unit tests for `CacheMetrics::from_totals`
  formulas + `AggregateReport::cache_metrics_per_bucket` + trait-bound
  inaccessibility for `ToolBucket` / `McpServerBucket`) and
  `agentprof-cli` (insta snapshots for md / html / json render +
  e2e via `assert_cmd` for `list` / `aggregate` cache columns +
  cache-activity vs no-cache-activity branches).

## [0.3.0] - 2026-06-10

> Supersedes v0.2.1 for any deployment binding the OTLP receiver to
> non-loopback addresses. Closes audit findings F1/F2/F3 from the
> post-M2.2 hardening review. See [ADR-0022](docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md).

### Added (M2.4 OTLP hardening — security)

- **F1 — Constant-time bearer compare** (`storage::otlp::auth`, M2.4 T5,
  `4a37cfa`): replaced `==` on `str` (timing oracle) with
  `subtle::ConstantTimeEq::ct_eq` on raw byte slices, applied
  symmetrically to the gRPC interceptor and the axum middleware. New
  workspace dep: `subtle = "2"` (no transitive deps, BSD-3-Clause OR
  Apache-2.0). 1 new regression test.

- **F2 — Per-signal request size caps** on both transports
  (`storage::otlp::config` + `server_grpc` + `server_http`, M2.4 T6,
  `ede2f98`): three new `OtlpServerConfig` fields
  (`max_{logs,metrics,traces}_request_bytes`, defaults 8/2/8 MiB per
  [ADR-0022](docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md)
  D-2). Wired into tonic via `.max_decoding_message_size(N)` per service
  and into axum via `DefaultBodyLimit::max(N)` per route. gRPC overflows
  surface as `OutOfRange` / `ResourceExhausted`; HTTP overflows as
  `413`. 6 new integration tests in `otlp_caps_smoke` + 2 round-trip
  tests.

- **F3a — `session.id` length cap** (`storage::otlp::mapper` +
  `error`, M2.4 T7, `a9f6720`): mapper now rejects `session.id` values
  longer than 256 bytes with `MapperError::SessionIdTooLong { signal,
  len }` BEFORE allocating a router buffer (ADR-0022 D-5).
  `extract_session_id` signature gains a `SignalKind` parameter for
  accurate error reporting. 2 new mapper tests (256-byte boundary +
  257-byte rejection).

- **F3b — LRU session eviction** (`storage::otlp::router` + `error`,
  M2.4 T8, `b12dc29`): `SessionRouter` now caps the number of
  concurrent sessions at `max_open_sessions` (default 1024 per ADR-0022
  D-3). When the cap is reached, the least-recently-active buffer is
  flushed with new `CloseReason::CapacityEvict` to make room for the
  incoming session. LRU tracked via a `VecDeque<SessionId>` behind
  `std::sync::Mutex` (no new dep; ~30 lines). 3 new router tests (LRU
  evict, LRU touch, LRU under pressure).

- **CLI surface** (`agentprof-cli::cmd::ingest_otlp`, M2.4 T9,
  `b0d9f63`): 4 new flags (`--max-logs-request-bytes`,
  `--max-metrics-request-bytes`, `--max-traces-request-bytes`,
  `--max-open-sessions`) with matching `[otlp]` config-file keys.
  Standard CLI > env > file > defaults priority preserved. 2 new CLI
  surface tests + 2 new config round-trip tests.

### Documentation

- [ADR-0022](docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md)
  OTLP receiver capacity caps + LRU eviction (10 sections, 6 decisions,
  3 alternatives considered per decision) — committed in M2.4 T4
  (`30443fe`).
- L1 `docs/architecture.md` §10: `[otlp]` example block extended with
  the 4 new keys.
- L1 `docs/plan.md`: M2.4 roadmap entry added after M2.2.
- L2 `crates/agentprof-storage/README.md`: status block gains an "M2.4
  hardening" paragraph; `otlp` modules row annotates auth (T5), mapper
  (T7), router (T8), server_grpc / server_http (T6); reference-ADRs
  table adds ADR-0022.
- L2 `crates/agentprof-cli/README.md`: `ingest-otlp` flag table gains
  the 4 new flags; `[otlp]` example block extended; CLI signature line
  in §"Public interface" updated to show the new flags.

### Tests

- **19 new tests** across storage + cli (1207 baseline → **1226 total,
  0 failures**) — verified via `cargo test --workspace --all-features`
  after M2.4 T9.

## [0.2.1] - 2026-06-10

### ⚠️ Security Notice

> **The OTLP receiver shipped in v0.2.1 has known DoS risks**:
>
> - Bearer-token comparison uses `==` and is vulnerable to timing
>   oracle attacks (audit finding F1).
> - Neither gRPC nor HTTP transports cap decoded message size; a
>   single ~4 MiB protobuf bomb can balloon to 100s of MiB of typed
>   events before per-session buffers trip (F2).
> - `SessionRouter` has no upper bound on the number of distinct
>   sessions tracked; UUID-spam exhausts memory before the 30 s idle
>   sweeper triggers (F3).
>
> All three are fixed in **v0.3.0** ([ADR-0022](docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md)).
> Operators running the receiver bound to non-loopback addresses
> should upgrade to v0.3.0+ before exposing the listeners. v0.2.1 is
> safe for `127.0.0.1`-only deployments with no untrusted neighbors.


### Added

- **OTLP receiver** (M2.2; feature-gated on `otlp`) — push-based
  session ingestion via OpenTelemetry. `agentprof ingest-otlp` runs both
  a gRPC listener (default `127.0.0.1:4317`, tonic + tower) and an
  HTTP/protobuf listener (default `127.0.0.1:4318`, axum), accepts
  Logs + Metrics + Traces signals, groups events by `session.id` (with
  `claude.session_id` fallback) into bounded per-session buffers, and
  persists finalized sessions to the same SQLite store as `agentprof
  analyze`. Bearer-token auth (RFC 6750) and rustls TLS/mTLS are
  supported on both transports. See
  [ADR-0021](docs/internals/adr-0021-otlp-receiver-architecture.md)
  for the 10 architecture decisions and
  [`docs/superpowers/specs/2026-06-10-m2.2-otlp-receiver-design.md`](docs/superpowers/specs/2026-06-10-m2.2-otlp-receiver-design.md)
  for the design spec.
  - New modules under `agentprof_storage::otlp`: `config`, `error`,
    `proto` (tonic-build codegen of `opentelemetry::proto::*`),
    `mapper`, `typed`, `router`, `sweeper`, `auth`, `tls`,
    `server_grpc`, `server_http`, `sink_storage`, `pipeline`.
    - `config` / `error` — `OtlpServerConfig` + `PartialOtlpServerConfig`
      (TOML-friendly), `OtlpServerError` variants (`Bind`, `Tls`,
      `Pipeline`, `Internal`, ...).
    - `proto` — codegen via `tonic_build::compile_protos_with_config`
      with proto comments disabled (upstream OTLP comments contain
      fenced text rustdoc mis-parses as doctests).
    - `typed` + `mapper` — `TypedEvent` intermediate representation
      (`SessionStart`, `SessionEnd`, `ToolDecisionStart`, `ToolResult`,
      `TokenUsage`, `UserPrompt`, `Unrecognized`); mapper lowers OTLP
      Logs / Metrics / Traces into `Vec<Result<TypedEvent, MapperError>>`.
    - `router` — `SessionRouter` + `SessionBuffer` (`DashMap`-backed,
      `#[non_exhaustive]` `SessionBufferCaps` with builder methods;
      defaults 16 MiB / 100 000 events / 5 min idle). Close triggers:
      `ExplicitEnd` / `OomBytes` / `OomEvents` / `Idle` / `Shutdown`.
      Pairing algorithm (spec §5.4): unmatched `ToolDecisionStart`
      synthesized as `ToolResult { status: OpenAtEndOfSession }` at the
      buffer's last wall-clock; event vector stable-sorted by timestamp.
      `FlushSink` trait abstracts persistence.
    - `sweeper` — `spawn_idle_sweeper(router, interval) -> SweeperHandle`
      drives periodic `router.sweep_idle()` and runs
      `router.flush_all(Shutdown)` on explicit shutdown or handle drop.
    - `auth` — `bearer_interceptor` (tonic) + `bearer_middleware`
      (axum); passthrough when `token == None`, otherwise enforces
      `Authorization: Bearer <t>` exact match (`Unauthenticated` /
      `401`) before the pipeline is invoked.
    - `tls` — rustls server config builder; optional mTLS via
      `--client-ca`.
    - `server_grpc` — `serve_grpc(cfg, pipeline)` binds via
      `tokio::net::TcpListener` + `TcpIncoming`, registers all three
      OTLP collector services, returns `(JoinHandle, oneshot::Sender)`.
    - `server_http` — axum router with `/v1/{logs,metrics,traces}`
      protobuf endpoints; `415` / `400` error paths covered.
    - `sink_storage` — `StorageFlushSink::new(Arc<Mutex<Db>>)`
      translates `PersistableSession` into the M2.1 `AnalysisReport`
      shape and persists via `upsert_report`; writes
      `sessions.raw_path = "otlp://<session_id>"` to distinguish
      OTLP-sourced rows. Token-usage data points roll into
      `model_metrics`; tool decision/result pairs roll into `tool_rank`
      with paired-timestamp percentile stats.
    - `pipeline` — `IngestPipeline` owns an `Arc<SessionRouter>`,
      runs the mapper for each signal, counts `MapperError` into an
      `error_count` atomic, and forwards into the router; lossy
      mappings (Unrecognized, per-prompt UserPrompt sizes, etc.)
      `tracing::debug!`-dropped per spec §6.
  - New CLI: `agentprof ingest-otlp` (feature-gated; included in the
    default `full` feature). Flags: `--grpc` / `--no-grpc` /
    `--http` / `--no-http` / `--bearer-token`
    (env `AGENTPROF_OTLP_TOKEN`) / `--tls-cert` / `--tls-key` /
    `--client-ca` (clap `requires =` enforces cert/key pairing and
    `--client-ca` ⇒ `--tls-cert`) / `--max-session-bytes` /
    `--max-session-events` / `--idle-seconds` / `--store`. Hidden
    `--sweeper-interval-seconds` overrides the production 30 s tick
    for tests. Lifecycle awaits SIGINT (`tokio::signal::ctrl_c`) or
    SIGTERM (`SignalKind::terminate`, Unix only) then drains in
    `stop accepting → join servers → flush sweeper` order. Validation:
    `--no-grpc --no-http` → `ExitKind::UserError`.
  - New `[otlp]` config-file block (`agentprof_cli::config::PartialConfig`
    gains `otlp: Option<PartialOtlpServerConfig>` field, feature-gated).
    Precedence per field: **CLI flag > `AGENTPROF_OTLP_TOKEN` env >
    `[otlp]` config-file block > built-in defaults**; explicit
    `--no-grpc` / `--no-http` always win. Config file resolved from
    `$AGENTPROF_CONFIG` or platform XDG config dir; missing file silent,
    malformed file `warn`-logged and falls through.
  - Workspace dependency additions (all gated under `otlp` features):
    storage gains `tonic`, `prost`, `prost-build`, `axum`, `tower`,
    `bytes`, `rustls`, `tokio-rustls`, `dashmap`, `tokio` features
    (`rt-multi-thread` + `macros` + `time` + `net` + `test-util` in
    dev-deps); cli gains `tokio` `signal` feature on the `otlp` path.
    `cargo deny check` green; no new license exceptions required.
  - 118 new tests covering config / auth / TLS / mapper / router /
    sweeper / pipeline e2e / CLI e2e (`tests/otlp_config_smoke.rs`,
    `tests/otlp_auth_smoke.rs`, `tests/otlp_router.rs`,
    `tests/otlp_sweeper.rs`, `tests/otlp_pipeline_e2e.rs`,
    `tests/cli_ingest_otlp_help.rs`,
    `tests/cli_ingest_otlp_e2e.rs` — 8 end-to-end cases through real
    `agentprof ingest-otlp` binary via `opentelemetry-proto` +
    `tonic` (gRPC) and `reqwest` (HTTP/protobuf): explicit
    `session.end` round-trip, token-usage rollup, tool-span pairing,
    bearer rejection bypass, `--max-session-events` OOM partial
    flush, idle-sweeper flush, three interleaved session ids into
    three distinct rows, explicit `session.end` persists across
    SIGKILL).

### Documentation

- **M2.2 OTLP receiver doc sweep:**
  - New [ADR-0021](docs/internals/adr-0021-otlp-receiver-architecture.md)
    documenting 10 architecture decisions (gRPC + HTTP transports,
    OTLP-is-not-an-Adapter boundary, session.id grouping with
    `claude.session_id` fallback, bounded buffers + sweeper, mapper
    lossiness latitude, bearer + rustls security model, etc.).
  - [ADR-0018](docs/internals/adr-0018-sessiondatasource-trait.md)
    footnote linking to ADR-0021 §Decision 3 (OTLP receiver does **not**
    implement the `Adapter` trait — it is a sink, not a pull-source).
  - `docs/architecture.md` §3 (dependency graph), §6 (data sources),
    §8 (CLI surface), §9 (storage layer), §10 (observability), and
    §15.4 (feature flags) updated to reflect the shipped `otlp`
    feature, `ingest-otlp` subcommand, and `otlp://<session_id>`
    raw-path convention.
  - `docs/plan.md` M2.2 marked **shipped** with commit-range pointer.
  - `docs/adapters.md` gains an "OTLP is not an adapter" disclaimer
    redirecting contributors to ADR-0021 + `agentprof-storage::otlp`.
  - L2 READMEs refreshed: `crates/agentprof-storage/README.md` adds an
    `otlp` module-tree section and `[otlp]` config block table;
    `crates/agentprof-cli/README.md` documents `ingest-otlp` flags +
    config-file precedence.

## [0.2.0] - 2026-06-10

> Captures the SQLite persistence work (M2.1 + M2.1.1) plus the M1.6.x
> token-cost views and pre-Phase-2 audit followups shipped between
> v0.1.0 and the M2.2 OTLP receiver wave. M2.2 itself is released
> separately as v0.2.1.

### Added

- **M2.1.1 aggregate dual-path** (closes the M2.1 dual-path story).
  `cmd::aggregate` now SQLite-cache-accelerated via new
  `SessionDataSource::load_episodes(id) -> Result<Episodes, _>` trait
  method backed by a separate `episodes_json` column on `sessions`
  (migration 002, additive `ALTER`, default `'{}'` for backward-compat).
  Three impls (`AdapterDataSource` / `SqliteDataSource` /
  `DualPathDataSource`) override. `cmd::aggregate` rewired to
  `build_data_source(...)` matching the `list` / `mcp-waste` pattern from
  M2.1; `cmd::analyze` write-through and `cmd::db::ingest` per-session
  loop both extended to pair `upsert_report` with new `upsert_episodes`.
  Aggregate gracefully skips empty `Episodes` (pre-M2.1.1 rows) in the
  percentile pool. `AdapterDataSource::load_episodes_by_ref` bypass keeps
  ingest O(N). Also adds `#[serde(default)]` to `Episodes` required
  fields (forward-compat improvement bundled in the same wave). See
  [ADR-0020](docs/internals/adr-0020-aggregate-dualpath.md).
- **M2.1 SQLite persistence layer** (Phase 2 entry). Activates the
  previously-stub `agentprof-storage` crate. Hybrid mode: default
  `cache` at `$XDG_CACHE_HOME/agentprof/cache.sqlite` (auto-prune
  configurable, safe to `rm`); opt-in `store` at
  `$XDG_DATA_HOME/agentprof/store.sqlite` via `[storage] mode = "store"`
  in config. 3-table normalized schema (`sessions` / `tools_loaded` /
  `turn_buckets`) reconciled against v0.1.x model + `analysis_report_json`
  blob column for disaster recovery + `loaded_mcp_tools` as part of
  the analysis report. WAL mode + per-command short connections +
  no connection pool. Migrations via `rusqlite_migration` crate.
  See ADR-0019 (hybrid storage mode) + ADR-0017 (id namespace unify).
- **`SessionDataSource` trait** in `agentprof-core` (leaf). Symmetric
  to the existing `Adapter` trait. Three impls land with M2.1:
  `AdapterDataSource<A>` (agentprof-adapters; wraps any Adapter),
  `SqliteDataSource` (agentprof-storage), and `DualPathDataSource`
  (agentprof-cli; composer). Future OTLP receiver (M2.2) will be the
  4th implementor. See ADR-0018.
- **Dual-path read** in `cmd::list` and `cmd::mcp-waste`: both adapter
  and storage queried; set-union by canonical session id; per-session
  field diff (`raw_mtime`, `started_at_ms`, `raw_path`); divergence
  warns on stderr (suppressed by `--quiet`); adapter wins. `cmd::analyze`
  write-throughs to storage after a successful analysis. `cmd::watch`
  holds one DB connection across refresh cycles (released immediately
  after the initial upsert per M2.1 audit P1-2). Known M2.1 limitation:
  `cmd::aggregate` (all 4 `--by` arms) stays single-path because
  cross-session aggregation requires Episodes per session, not just
  `AnalysisReport` — promoted to M2.1.1 follow-up.
- **`agentprof db` subcommand family**: `init`, `stats`, `ingest`,
  `prune`, `vacuum`, `export`. All accept `--storage-path` for
  isolation; `stats` supports `--export {table,json}`; `ingest`
  takes `--agent` + mutually exclusive `{--since DUR | --all | --session ID}`;
  `prune` takes `--before DUR` + `--dry-run`; `export` takes
  `<SESSION_ID>` + `--format {json,jsonl}` + `--output PATH`.
- **Three new global CLI flags**: `--no-cache` (degrade dual-path to
  single-path adapter), `--storage-path <PATH>` (override resolved DB
  path), `--quiet` (suppress per-session divergence warning lines on
  stderr).
- New workspace dependencies: `rusqlite_migration` (1.x) and
  `indicatif` (0.17, optional behind a `progress` feature on
  agentprof-storage). `dirs` (5.x) promoted to workspace dependency.
- **`AnalysisReport::loaded_mcp_tools: BTreeSet<String>`** (`#[serde(default)]`)
  + 4 new accessor methods on `AnalysisReport` for cumulative token
  totals (`total_input_tokens`, `total_output_tokens`,
  `total_cache_read`, `total_cache_creation`). Powers the storage
  layer's `dominant_model` derivation + the SQLite normalized-table
  pre-computation.
- **`SessionRef::new(id, agent, started_at_ms, raw_path, raw_mtime_ms, source)`**
  cross-crate constructor (`#[non_exhaustive]` workaround). See the
  "Known issue" footer of ADR-0018 for the typo-prone-args caveat
  and the M2.1.1 redesign plan.
- **`agentprof-cli/src/lib.rs`** thin facade — exposes
  `data_source` / `data_source_factory` / `config` modules to
  integration tests (the crate was previously bin-only). `main.rs`
  remains the bin entry point.
- **`agentprof_adapters::copilot::paths::extract_session_id_from_first_event`**
  helper. Called from `discover_sessions` to surface the canonical
  UUID (from `data.sessionId` in the first event) instead of the
  directory name. See ADR-0017.
- **`AdapterDataSource::load_session_by_ref(&AdapterRef)`** fast path
  — lets callers that already hold an `AdapterRef` from a previous
  `discover_sessions` skip the per-call session-root rescan. Used
  by `agentprof db ingest` to drop ingest cost from O(N²) to O(N).
  See M2.1 audit P1-3.
- **adapters (M2.1 T3.1):** new `agentprof_adapters::AdapterDataSource<A>`
  wrapper bridges any `Adapter` impl into the
  `agentprof_core::datasource::SessionDataSource` trait. Stores
  `(Arc<A>, PathBuf root)` and runs the full
  `discover_sessions → load_session → episode::derive_episodes →
  analyzer::analyze` pipeline inline on `load_session(id)`; `discover`
  filters by `modified_at` against the supplied `since` window and maps
  each `adapter::SessionRef` to a `datasource::SessionRef` tagged with
  `source = "adapter:{kind}"`. Adapter errors are surfaced as
  `DataSourceError::Adapter { source, underlying }`; unknown ids as
  `DataSourceError::NotFound`. The `Adapter` trait is unchanged — this
  keeps existing impls (and `CopilotAdapter`'s unit-struct shape)
  intact. Unblocks the dual-path composer (T4).
- **ADR-0017** (Unify session id namespace), **ADR-0018**
  (SessionDataSource trait + dual-path semantics), **ADR-0019**
  (Hybrid storage mode).
- **M1.6.6 MCP tool token-cost view** (extends M1.6.5; Phase 2 of the
  original "View C" brainstorm). Surfaces "how many *tokens* of my
  context budget were wasted on tool descriptions the agent never
  called?" Two data sources via fallback chain:
  - Default: heuristic constant (200 tokens/tool; `--tokens-per-tool N`)
  - Optional: `--tool-descriptions <PATH>` (auto-detects file ↔ dir;
    dir variant accepts raw MCP `tools/list` RPC responses)
  Tokenizer auto-inferred from `session.meta.model` (`gpt-5*`/`gpt-4o*`
  → `o200k_base`; else `cl100k_base`).

  New core types: `TokenProvenance` (Heuristic|SidecarExact|Mixed),
  `TokenSource` (Heuristic|SidecarExact), `TokenizerKind`
  (Cl100kBase|O200kBase). All `WasteReport` / `McpServerWaste` /
  `McpToolWaste` / `AggregateWasteReport` / `McpServerCrossWaste`
  structs gain `*_tokens: u64` fields; all `#[serde(default)]` for
  pre-M1.6.6 JSON snapshot compat.

  New `agentprof-adapters::copilot::tool_sidecar` module
  (`load_sidecar`, `Sidecar`, `ToolEntry`).

  All 3 CLI subcommands (`analyze --section mcp-waste`, `aggregate
  --by mcp-server`, `mcp-waste`) and the TUI view (key `5`) gain
  token-cost columns / lines / banner. `tiktoken-rs = "0.6"` workspace
  dep (declared since M1.6.5 but unused) becomes first activated.

  ([Design spec](docs/superpowers/specs/2026-06-08-m1.6.6-token-cost-design.md),
  [ADR-0016](docs/internals/adr-0016-mcp-token-cost-architecture.md))

- **M1.6.5 MCP server waste analysis** (Phase 1 — counts-only;
  token-cost view planned for M1.6.6). Quantify "MCP context bloat" —
  tools / servers the agent had access to but never called.
  ([Design spec](docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md),
  [ADR-0015](docs/internals/adr-0015-mcp-waste-architecture.md))

  New types in `agentprof-core::model::waste`: `WasteReport`,
  `McpServerWaste`, `McpToolWaste`, `LoadedSource`, `WasteDataSource`,
  `AggregateWasteReport`, `McpServerCrossWaste`, `McpToolUsageAcrossSessions`.

  New pure functions in `agentprof-core::analyzer::waste`:
  `compute_waste` (per-session reducer), `aggregate_waste`
  (cross-session reducer).

  New adapter helpers in `agentprof-adapters::copilot`:
  `tools_changed::extract_loaded_set_from_session` (wire parser for
  `<tools_changed_notice>` blocks embedded in
  `user.message.transformedContent`); `mcp_config::load_mcp_config`
  (best-effort `~/.copilot/mcp.json` parse, VSCode + self-describing
  schemas).

  New CLI surfaces:
  - `agentprof analyze --section mcp-waste` (md/json/html/tui)
  - `agentprof aggregate --by mcp-server` extended with
    `unused_tool_count` and `fully_unused_session_count` columns
    (all export formats)
  - `agentprof mcp-waste [--since 7d] [--top 20] [--mcp-config P]`
    new dedicated cross-session 專題 report subcommand (md/json/html)
  - TUI 5th view at key `5` ("MCP Waste"), split-pane like Models view

  New fixture `crates/agentprof-adapters/tests/fixtures/copilot/with-mcp-waste/`
  exercises the 3-tools-advertised, 1-tool-called case.

  ~52 new tests across all layers (unit + view + integration + insta
  snapshot). Algorithm complexity O(loaded + called) per session;
  100-session aggregate ~1s.

- `CHANGELOG.md` pre-seeded `### Added` / `### Changed` / `### Fixed`
  stubs under `[Unreleased]` per [Keep-a-Changelog 1.1](https://keepachangelog.com/en/1.1.0/)
  template convention — gives contributors a clear template (closes M-1
  from T8 quality review).
### Changed

- **BREAKING (agentprof-core, pre-1.0):** `analyzer::waste::compute_waste`
  signature changed from `(report, wire_loaded, config_loaded)` to
  `(report, &WasteComputeContext)` (M1.6.6 T1.4). The builder-pattern
  context struct + `#[non_exhaustive]` locks the shape — future field
  additions become non-breaking via `with_*` methods. No published
  external consumers exist.
- **BREAKING (agentprof-core, pre-1.0):** `analyzer::aggregate::group_by_mcp::aggregate_by_mcp_server`
  signature gained a 3rd parameter `waste_per_report: &[WasteReport]` (M1.6.5 T3.3).
  Callers must compute per-session waste via `compute_waste` and pass the
  resulting slice. No published external consumers exist; flagged here
  for v0.2.0 release-notes accuracy.
- **agentprof-storage (M2.1 audit P2-2):** `admin::*` and `query::*`
  migrated from `Db::conn_for_test()` to the existing `pub(crate)
  Db::conn()`. `conn_for_test` retained for external integration
  tests (where `pub(crate)` isn't reachable) but its rustdoc now
  spells out the contract and points production callers at `conn()`.
- **agentprof-storage (M2.1 audit P2-4):** `query::parse_agent`
  elevated the "unknown agent string" fallback log from
  `tracing::warn!` to `tracing::error!` (visible by default) and
  added a TODO + rustdoc explaining why the soft-fail policy
  (panicking mid-listing is worse) stays until M2.1.1 introduces a
  proper `AgentKind::Unknown(String)` variant.
- **tests: pin in-turn skill rollup contract + document `/tmp/sess` test path rationale**
  (closes P3 backlog `skill-call-count-fixture` + P4 backlog `t9-tmp-path-rationale`).
  The committed Copilot integration fixtures (`with-skill-invoked`,
  `two-skills-one-turn`, `tool-and-skill-same-turn`) all emit
  `skill.invoked` **before** `assistant.turn_start` because that
  mirrors observed Copilot CLI 1.0.x wire behavior; their snapshots
  correctly record `skill_call_count == 0`. The IN-TURN path
  (`open_turn_idx == Some` at skill-event time) was exercised
  implicitly by `payload_name_missing_warning_fires_when_adapter_returns_none`
  but never asserted; new assertions in that test pin
  `turn.skill_calls.len() == 1` and the back-reference name. Separately,
  `session_selector_parses_path_with_slash` gains a comment debunking
  a confabulated "no `/tmp` references" review rule (ADR-0003 §3
  explicitly mandates `/tmp/agentprof-fixture/*` for ephemeral fixture
  paths).
- **`core`: `Span::new` now clamps non-monotonic input to zero-duration**
  (P2 backlog `negative-duration-span`). Previously, an adapter that
  saw a `tool.execution_complete` whose `timestamp` predated its
  `tool.execution_start` (wall-clock skew, restored session, manual
  log edit) produced a Span with negative duration, which silently
  corrupted percentile sorting, TUI/HTML duration rendering, and the
  "zero == orphan-synthesized" convention. `Span::new` now reorders
  to `started_at == ended_at` whenever `ended_at < started_at`. The
  upstream `ParseWarning::OutOfOrder` continues to surface the
  underlying anomaly to the operator. **Note:** `Span::new` is no
  longer `const fn` (the comparison requires `chrono::DateTime`'s
  `PartialOrd`, which is not `const`). `Span::instant(t)` remains
  `const fn` and is functionally equivalent to the old
  `Span::new(t, t)` for the orphan-synthesis path.
- **`xtask`: `schema_audit::classifier` now realigns raw lines around
  typed `ParseWarning`s** (P2 backlog `classifier-zip-fix`). The old
  positional `raw_lines.iter().zip(typed.events.iter())` mis-attributed
  every `Unknown` event's wire `type` field after the first
  `ParseWarning::Json` / `Io` site, because the typed pass drops
  failing lines while `read_raw_lines` keeps them. A new
  `aligned_raw_lines` helper filters out raw lines whose `line_no`
  matches a recorded warning's `line_no` (bridging the 1-based / 0-based
  convention mismatch), restoring positional sync. Three unit tests
  pin the helper's contract (Json variant, Io variant, OutOfOrder
  no-op).
- **Privacy: sanitize personal absolute paths in committed docs.**
  Across 15 `docs/superpowers/plans/*.md` files (~80 occurrences),
  `/home/verden/pfind/2026-spring/code/agentprof` → `/path/to/agentprof`
  placeholder; AI subagent execution scripts no longer leak the
  maintainer's Unix username + project layout. `docs/features/privacy.md`
  L51 Tier-HIGH `meta.cwd` example similarly sanitized to
  `/home/<user>/<projects>/agentprof` placeholder (illustrative purpose
  preserved). `docs/internals/adr-0012-...md` REF-007 session path
  retained per policy decision (cited as empirical wire-survey evidence).
  Git author identity (260 commits) retained per policy decision —
  rewriting via `filter-repo` would invalidate the v0.1.0 release SHA
  `7e29d97` referenced from ADR-0014 and CHANGELOG. Secret/token scan
  performed concurrently: **zero hits** for API keys, JWT/bearer
  tokens, private keys, webhooks, passwords.
- `README.md` line 6 status badge now leads with
  **"v0.1.0 shipped 2026-06-06"** instead of "M1.6.4 — Speedscope + HTML
  …" — release deserves the lede; M1.6.4 wave is now parenthetical
  context (closes I-1 from T8 quality review).
- `README.md` PATH hint: removed parenthetical
  *"(The installer's output also reminds you.)"* — decouples our docs
  from upstream `cargo-dist` installer UX (cargo-dist may change its
  output format; our README claim becomes silently false) (closes M-2
  from T8 quality review).

### Removed

- **agentprof-cli (M2.1 audit P1-1):** Deleted dead `ReUpsertFn` /
  `DualPathDataSource::new_with_reupsert` / `re_upsert` field /
  `merge_refs` callback fan-out / and the
  `re_upsert_callback_fires_on_diverging_session` test. The async
  re-upsert design originally prototyped in M2.1 T4.2 was never
  wired up by the data-source factory, and detached threads at the
  tail of a one-shot CLI invocation are killed at process exit
  anyway. Proper async refresh deferred to M2.1.1. Net effect on
  users: zero (the surface was never invoked from any subcommand).
  See ADR-0018 §Behaviour rolled-back note +
  `crates/agentprof-cli/src/data_source.rs` module docs.

### Fixed

- **agentprof-adapters / agentprof-cli (M2.1 CI fix, 2026-06-09):** `list`
  and `aggregate` output ordering is now deterministic across CI runners
  regardless of filesystem mtime. Two layers fixed: (1) new helper
  `agentprof_adapters::copilot::paths::extract_session_start_ms_from_first_event`
  eagerly parses `data.startTime` (or envelope `timestamp`) from the
  first event of `events.jsonl` — same cheap `BufReader::read_line` pass
  the id extractor already uses; `AdapterDataSource::adapter_ref_to_datasource_ref`
  uses it to populate `DataSourceRef.started_at_ms`; (2) `cmd::list::run`
  and `DualPathDataSource::merge_refs` both sort by
  `(Reverse(started_at_ms), id)` so dual-path and `--no-cache` produce
  byte-identical stdout. Regenerated `cli_nocache_regression__list_no_cache_stable`
  snapshot. ADR-0017 amended (2026-06-09 update) to record the now-eager
  `startTime` parsing — the relaxed `diff_fields(None == no opinion)`
  semantic is retained as defense-in-depth.
- **agentprof-adapters / agentprof-cli (M2.1 P0):** `CopilotAdapter::discover_sessions`
  now sets `SessionRef.id` to the canonical UUID parsed from
  `data.sessionId` in the first event of `events.jsonl`, not the
  directory name. Previously the adapter and storage layers used
  disjoint id namespaces, so `DualPathDataSource::merge_refs` joined
  on `id` with an empty intersection → the
  `agentprof: warn: session <id>: N fields differ …` divergence line
  never fired and `--quiet` was dead code. `diff_fields` was also
  relaxed to treat `Option::None` as "no opinion" (vs. spurious
  disagreement) so the new join doesn't flag every fresh scan. New
  helper `agentprof_adapters::copilot::paths::extract_session_id_from_first_event`.
  Re-enables the `dualpath_warns_on_stale_db` test (was `#[ignore]`'d
  in T7.2 pending this fix). See `docs/internals/adr-0017-unify-session-id-namespace.md`.
  The `cli_nocache_regression::list_no_cache_stable` snapshot was
  regenerated (list now shows UUIDs instead of dir names) and the
  `with-session-shutdown` fixture's colliding `sessionId` was
  re-stamped from `…-000099` to `…-000019`.
- **agentprof-cli, watch (M2.1 audit P1-2):** `cmd::watch` no longer
  holds the SQLite `Db` connection open for the entire `run_single`
  lifetime. The handle is now dropped immediately after the initial
  upsert, releasing the WAL lock so concurrent `agentprof db
  ingest` from another shell can write while a watch session is
  attached. Per spec §8 watch performs no further writes after the
  initial flush, so the long-lived handle was idle dead weight.
- **agentprof-adapters, agentprof-cli (M2.1 audit P1-3):** `db
  ingest` was O(N²) — `AdapterDataSource::load_session(id)`
  re-scanned the entire session root on every call. New
  `AdapterDataSource::load_session_by_ref(&AdapterRef)` skips the
  rescan; the CLI's ingest loop now calls it directly with the
  `AdapterRef`s it already holds from its one up-front `discover`.
  For 100 sessions this cuts 10,000 first-line reads down to 100.
- **agentprof-cli, db (M2.1 audit P1-4):** `db ingest` now exits **2
  (`ExitKind::DataError`)** when 100 % of discovered sessions fail
  to upsert, instead of silently exit 0. Partial failures (some ok)
  still exit 0 — the user got *some* of what they asked for and
  per-session errors are already on stderr.
- **agentprof-storage (M2.1 audit P1-5 + P2-1):**
  `SqliteDataSource::{discover, load_session}` recover from a
  poisoned mutex via `PoisonError::into_inner` (matching the
  `drain_warnings` style elsewhere in the codebase) instead of
  synthesising a misleading `SqliteError::ConfigPath { kind:
  "mutex", … }`. The `poisoned_mutex_err` helper is gone;
  `SqliteError::ConfigPath` is no longer abused for non-config-path
  failures.
- **agentprof-cli (audit B1):** `analyze`, `aggregate --by mcp-server`,
  and `mcp-waste` now pick the *dominant* model (largest
  `ModelUsage::total()`) when inferring the session tokenizer. Pre-fix,
  they used `model_metrics.keys().next()` (alphabetically smallest),
  so a mixed-model session that mostly used `gpt-5-mini` but logged a
  single `claude-haiku-4.5` call was misclassified as Anthropic and
  routed to `cl100k_base`, mispricing the gpt-5-mini bulk of the work.
  The rule lives in the new `crate::cmd::model_hint::dominant_model`
  helper, shared by all three subcommands.
- **agentprof-core (audit B2):** `compute_waste` now emits a
  `tracing::warn!` when inline BPE construction returns `None`,
  separating "no sidecar configured" (heuristic by design) from
  "tokenizer init failed" (heuristic by accident). Pre-fix the
  `.ok()` swallow made both look identical.
- **agentprof-adapters (audit B3):** `tool_sidecar::load_sidecar`
  preserves the real `io::ErrorKind` on `fs::metadata()` failures.
  Pre-fix every IO error (permission denied, stale NFS, unreadable
  mount) masqueraded as `SidecarError::NotFound`, sending users to
  debug a phantom missing file. Now only `ErrorKind::NotFound`
  returns `NotFound`; everything else propagates through
  `SidecarError::Io { path, source }`.
- **agentprof-adapters (CI portability):** the B3 regression test
  `load_sidecar_permission_denied_returns_io_err_not_not_found` is
  now gated behind `#[cfg(unix)]`. `std::os::unix::fs::PermissionsExt`
  is Unix-only, so Windows CI failed to compile the test (E0433:
  `cannot find unix in os`). The Io-branch coverage stays Unix-only;
  the lib itself builds and tests on Windows.
- **`adapters`: `ToolTelemetry.restricted_properties` now skips serializing
  on `Null`** (P2 backlog `tooltelemetry-restricted-props-skip-if`).
  Older Copilot CLI versions omit the `restrictedProperties` field
  entirely; `#[serde(default)]` deserialized that into `Value::Null`,
  and re-serialization then emitted a spurious `"restrictedProperties":
  null` field absent from the source. Added `skip_serializing_if = "Value::is_null"`
  so absent-in → absent-out, while `restrictedProperties: {}` continues
  to round-trip as `{}`. Two round-trip tests pin both cases.
- **`tests`: cross-platform invalid-path recipe in `log_file_invalid_path_soft_falls_to_stderr`**
  (regression caught by first Windows CI run on commit `ea18bbe`).
  The previous recipe `/this/dir/does/not/exist/agentprof.log` relied
  on POSIX-only behavior — `create_dir_all` fails at `/` due to
  permissions on Linux/macOS, but on Windows the same path maps to
  `D:\this\dir\...` in user-writable drive-root space where
  `create_dir_all` succeeds silently. New recipe: create a regular
  *file* first, then ask the CLI to log at `<file>/sub/agentprof.log`
  — `fs::create_dir_all` rejects this with `NotADirectory` on every
  OS because the parent path component is already a non-directory
  entry. Net effect: test now exercises the same soft-fall branch
  it always intended to, on all three CI platforms.
- **`cli`: honor `$XDG_STATE_HOME` on macOS / Windows** (regression caught
  by the first public-CI run on macOS aarch64). `directories::BaseDirs::state_dir()`
  returns `None` on macOS by design (Apple has no XDG state spec), so the
  previous `enter_tui_log_guard` implementation silently fell through to
  `cache_dir()` and ignored any `$XDG_STATE_HOME` override — which broke
  hermetic test isolation in `cli_tracing::watch_run_writes_log_events_to_file`.
  `resolve_xdg_log_path` now reads `$XDG_STATE_HOME` directly first
  (cross-platform XDG-spec primary) before falling back to `BaseDirs`.
  Two unit tests pinned at the function boundary on every platform.
- **`ci`: appease clippy 1.96 `unnecessary_sort_by` lint** (18 sites
  across `agentprof-core`, `agentprof-adapters`, `agentprof-tui`, plus
  one tail-end `cli` site at `merge_refs` per M2.1 CI fix 404ebd8).
  Mechanical conversion `sort_by(|a, b| b.X.cmp(&a.X))` →
  `sort_by_key(|b| std::cmp::Reverse(b.X))`; behavior unchanged.
  Includes one tuple-destructuring variant in `turn_detail.rs:673`
  (`sort_by(|(_,_,a), (_,_,b)| ...)`). Multi-statement closures with
  secondary tiebreakers (5 sites) retained as-is — the lint does not
  flag them.
- **`ci`: pin `xtask` path-dep versions** (`agentprof-core` /
  `agentprof-adapters` now carry `version = "0.1.0"` alongside `path = ...`)
  so `cargo deny check` no longer reports `wildcard` errors against the
  unpublished helper crate.
- **`ci`: allow `CDLA-Permissive-2.0` license** for `webpki-roots` 1.0.x
  Mozilla CA root certificate data set (transitive via `reqwest` →
  `hyper-rustls`). CDLA-Permissive-2.0 is OSI-equivalent permissive
  (https://cdla.dev/permissive-2-0/): no copyleft, no patent
  restrictions, attribution only. The crate's Rust code itself is
  triple-licensed MPL/Apache/MIT; the SPDX `license` field carries
  the data-set license per CDLA convention. Rationale comment added
  in `deny.toml` next to the entry.
- **`ci`: ignore RUSTSEC-2024-0436 in `deny.toml`** with rationale
  comment (advisory ID, why-can't-fix, upstream tracking, re-evaluation
  trigger). `paste` 1.0.15 reaches us transitively through `ratatui`
  0.29.0; will be removed when ratatui drops the `paste` dependency
  (likely in 0.30+).
- **`ci`: ignore RUSTSEC-2025-0119 `number_prefix`** in `deny.toml`
  with rationale comment (M2.1 CI fix 57eb54f). Transitive via
  `indicatif`'s progress-bar formatting; advisory is unmaintained-crate
  notice, not a security vulnerability.

### Performance

- **agentprof-core (audit A1):** `WasteComputeContext` now caches the
  `tiktoken-rs` BPE encoder via a new `bpe: Option<Arc<CoreBPE>>`
  field + `with_bpe(Arc<CoreBPE>)` builder method, plus a public
  `build_bpe(TokenizerKind)` helper. CLI driver code in `analyze`,
  `aggregate --by mcp-server`, and `mcp-waste` builds the encoder
  once per command and shares it across all per-session contexts —
  on a 100-session run this avoids ~100 × (50 ms + tens of MB) of
  redundant merge-table parsing.

### Documentation

- **agentprof-core (audit B4):** `DEFAULT_HEURISTIC_TOKENS` and
  `WasteComputeContext::with_heuristic` rustdoc now call out the
  cl100k_base calibration bias — `o200k_base` (GPT-4o / GPT-5 /
  o1 / o3) sessions may overshoot real waste by ~15–20% under the
  default constant. Points users at `with_sidecar()` for precision.
- **agentprof-storage (M2.1 audit P2-3):** Fixed broken ADR link in
  `crates/agentprof-storage/src/config.rs` module doc —
  `adr-0018-storage-hybrid.md` (does not exist) →
  `adr-0019-hybrid-storage-mode.md` (correct ADR).
- **ADR-0018 (M2.1 audit P2-6):** Added "Known issue" footer
  documenting that `SessionRef::new`'s 6-positional-arg constructor
  is typo-prone (three `Option<…>` middle args), why obvious fixes
  (`Default` impl / builder) are blocked today, and that the proper
  redesign is deferred to M2.1.1.
- **ADR-0018**: 'Behaviour' rewritten to mark the async re-upsert
  design as rolled-back per audit P1-1.
- **ADR-0017 (post-audit-sweep):** "Negative consequences" and
  "Snapshot / fixture deltas" sections refreshed for the M2.1 audit
  P2-5 test rename (`cli_nocache_compat` → `cli_nocache_regression`).
- **docs: post-implementation notes on M1.3 plan and design**
  (plan_drift `parsewarning-variants`). The historical M1.3 spec/plan
  referenced `ParseWarning::MissingField` and `UnknownVariant` variants
  that were superseded during implementation; the actual shipped enum
  in `agentprof-core::error` differs. Added callouts at the top of
  both `docs/superpowers/plans/2026-05-27-m1.3-...md` and the
  corresponding `-design.md` pointing readers to the real enum and
  documenting that Task 6's `MissingField`-dominance triage was
  never executed.
- **post-M2.1 doc sweep:** `docs/architecture.md` §8 `db ingest`
  row, `crates/agentprof-cli/README.md` `db` subcommand table, and
  `crates/agentprof-adapters/README.md` `AdapterDataSource` bullet
  brought back in sync with the audit P1-3 / P1-4 code shipped on
  `main`. `docs/plan.md` §6 / §7.1 / §7.2 / §8, `tasks/ROADMAP.md`
  header, and root `README.md` updated to mark M2.1 as merged on
  `main` (was "nearly complete on `feat/m2.1-sqlite-persistence`")
  and to close the §7.2 "SQLite schema 演进策略" open question
  (decided: hybrid 3-table + `analysis_report_json` blob).

### Tests

- **M2.1.1 (aggregate dual-path):** 13 new tests across 4 files —
  4 in `crates/agentprof-cli/tests/cli_aggregate_dualpath.rs` (silent /
  warn / no-cache parity / empty-episodes), 4 in
  `crates/agentprof-storage/tests/episodes_smoke.rs` (round-trip /
  default-for-unmigrated-row / NotFound / idempotent overwrite), 2 in
  `crates/agentprof-storage/tests/sqlite_datasource_trait.rs` (impl of
  `load_episodes`), 3 in `crates/agentprof-adapters/tests/adapter_datasource.rs`
  (`load_episodes` + `load_episodes_by_ref`), 1 in
  `crates/agentprof-core/tests/datasource_load_episodes.rs` (trait
  surface compile-check), 2 in
  `crates/agentprof-cli/tests/dualpath_skeleton.rs` (dual-path
  `load_episodes` storage-hit + adapter-fallback). Also: existing
  `crates/agentprof-core/tests/datasource_reexport.rs` `Stub` patched to
  satisfy the 4-method trait; existing `crates/agentprof-cli/tests/aggregate.rs`
  cases prepended with `--no-cache` (matching the M2.1 list-test pattern)
  so they don't see the user's home cache.
- **agentprof-cli (M2.1 audit P2-5):** Renamed
  `tests/cli_nocache_compat.rs` → `tests/cli_nocache_regression.rs`
  (and the two `insta` snapshot files alongside). The previous name
  implied a "v0.1.x baseline lock" which was never accurate — the
  snapshots were captured in M2.1 T7.1. Module doc rewritten to
  describe the actual purpose: catch accidental regressions in the
  single-path `--no-cache` output during dual-path refactors.
- **agentprof-cli (M2.1 T7.1 + T7.2):** `cli_nocache_regression` insta
  baseline locks single-path `list --no-cache` + `aggregate --no-cache`
  byte-stable output. `cli_dualpath` covers dual-path silent /
  warn / quiet / write-through paths.
- **M2.1 audit regression suite — 12 new tests across 5 files**
  (commits 228cb36 → 499e702): H1 `AnalysisReport::total_*` accessor
  coverage in core; H2 `loaded_mcp_tools` `#[serde(default)]`
  backward-compat lock in core; H3 `upsert_report` → raw-SELECT
  round-trip for token totals in storage; M1 storage FK CASCADE on
  prune for `tools_loaded` / `turn_buckets`; M2 cli `diff_fields`
  None-tolerant path locked; L1 adapters `CopilotEvent::payload_loaded_mcp_tools`
  fixture coverage; L2 storage `parse_agent` unknown-fallback contract.
  All seven commits are pure regression locks — no behavior change,
  but pin contracts the M2.1 audit identified as untested.

## [0.1.0] - 2026-06-06

### Added

- **C2 — `total_wall_duration` sum-invariant tests** (closes `m1.6.2-followup-i4-total-wall-test`). The 4 cross-session aggregators (`aggregate_by_tool` / `_mcp_server` / `_day` / `_model`) all set `AggregateReport.total_wall_duration` to Σ per-session wall durations, but pre-C2 no test asserted the invariant — md/html/TUI rendered the field while a future refactor breaking the sum would silently mis-report headline totals. 5 new tests in `crates/agentprof-core/tests/aggregate.rs`: 1 per aggregator + 1 empty-input edge case (zero wall). Each test builds 2–4 synthetic sessions with known wall durations via a shared `synthetic_session(session_id, started_offset_secs, wall_secs)` helper (one closed `Turn` per session sets the latest endpoint), runs the aggregator, and asserts `report.total_wall_duration == Duration::seconds(Σ wall_secs)`. Catches regressions in the per-session `wall::compute_wall` walk AND the per-aggregator accumulation loop. Tests: 800 → 805 (+5).

- **F2 ask_user pending detection** (3 commits: F2.1 + F2.2 + F2.3) — addresses the user's recurring pain "ask_user 比较特殊，有时候用户没有确认 AI 的回复就会一直在这里卡着". When Copilot CLI invokes `ask_user`, the wire emits `tool.execution_start` then BLOCKS waiting for user input; if the user is AFK the session stalls with no visible signal that the agent is waiting on *you* rather than stuck doing work. F2 surfaces pending state in 3 places so it can't be missed.
  - **F2.1** (`agentprof-core`) — new `analyzer::pending` module. `pub const ASK_USER_THRESHOLD: Duration = 30s` / `pub const DEFAULT_THRESHOLD: Duration = 5min`; `pub fn threshold_for(tool_name) -> Duration` (USER_BLOCKING_TOOLS → ASK_USER_THRESHOLD, else DEFAULT); `pub fn is_pending(call, tool_name, now) -> bool` (returns true iff status == `OpenAtEndOfSession` AND elapsed >= threshold; defensive against clock skew where `now < started_at`); `pub fn pending_calls(episodes, now) -> Vec<PendingCall<'_>>` (deterministic sort: user-blocking first, then tool name asc, then started_at asc); `pub struct PendingCall<'a>` with `tool_name` / `turn_id` / `started_at` / `elapsed` / `is_user_blocking` (cheap borrowing view of `Episodes`) + `PendingCall::new(...)` constructor for cross-crate test code. Zero schema change — pending is a derived `(state, now, threshold)` property, not a persistent variant. `now` is a parameter so watch passes `Utc::now()` and postmortem passes session-end time. 11 new unit tests covering: threshold-for both tool classes / is_pending happy + boundary + before-threshold / non-Open status returns false / clock-skew defence / empty episodes / sorted output / non-pending omission.
  - **F2.2** (`agentprof-tui::views::flamegraph`) — Flamegraph T-id Pending Yellow color (rank 2 in `t_id_status_color` precedence — above Open, below Aborted). Pending wins over Open because a turn with stuck ask_user IS open but "pending" is the more specific + actionable signal; Aborted wins over Pending because an aborted turn that also had a pending call is, in the end, aborted. `t_id_status_color` signature extended: `(turn, episodes, now)`; `build_row` gains `now` param passed once-per-frame from `render`. New `pub fn is_turn_pending(turn, episodes, now) -> bool` helper extracted so per-turn pending aggregation can be unit-tested independently. +7 unit tests covering: empty turn / above threshold / below threshold / pending turn = Yellow / Aborted Red wins over Pending Yellow / Pending wins over Open DarkGray / Open without pending stays DarkGray. 13 existing build_row test sites + 6 t_id_status_color test sites updated to pass new args (defaults: `&Episodes::default()` + `Utc::now()`).
  - **F2.3** (`agentprof-tui::views::roi` + `tui::watch`) — RoiView Tool cell pending color + watch footer banner. RoiView: new `pub fn compose_tool_cell_style(failure: Option<Color>, is_pending: bool) -> Style` composes F1.13 failure-severity color OR Pending Yellow Bold; **failure wins over pending** (a broken tool is worse than a slow tool per spec §3.3 table). Detail strip prefixes `⚠ N pending (<longest> longest) · ` when the selected tool has any pending calls. Watch mode: `WatchRunner::render_into` Single arm computes `pending_calls()` once per frame, allocates the footer row when pending non-empty OR reload error present (error takes precedence per spec §3.4 — active reload error is more important than "you're stuck"). New `pub fn watch::format_pending_banner(pending, max_width) -> String` formats `"⚠ ask_user pending for 1m23s — your input needed"` (single) or `"⚠ N calls pending: tool(elapsed) tool(elapsed) +K more"` (multiple, truncates via fit-entries convention). Cross mode intentionally does NOT surface pending — aggregation spans many sessions, per-session "pending" is meaningless. +7 unit tests (4 compose_tool_cell_style + 3 watch banner: renders / suppressed-by-reload-error / no-banner-when-no-pending) + 1 new `PendingCall::new` doctest. Help overlay (`?`) gains 3 new lines: T-id Pending Yellow legend, RoiView Pending integration into the Tool color rule, watch footer banner reference. Help height bumped 33 → 40.
  - Workspace: 745 → 777 tests passing (+32 across the 3 commits + integrated test additions). 0 snapshot regenerations needed (color-only changes invisible to char-buffer extractor). No new dependencies. References: spec at `docs/superpowers/specs/2026-06-05-f2-askuser-pending-design.md`.

### Fixed

- **B1 — `failure_count` always 0 (M1.2 regression, closes `m1.6.2-followup-copilot-failure-bit`)** — `derive.rs:383` had been hardcoding `ToolCallStatus::Success` (with a TODO comment "Task 10b will read actual success bit") and `:490` / `:504` had been hardcoding `HookCall.success: true` since M1.2 (commit `c5716aa`). The wire payload (`ToolResultData.success` + `.error.message`, `HookEndData.success`) was fully present but never consumed, silently neutralizing three already-shipped UX features on real Copilot data:
  - **F1.13** RoiView Tool cell Red/Yellow failure-severity color
  - **F1.16** By Hook `OK%` + Hook cell color
  - **F2.3** `compose_tool_cell_style` failure-wins-over-pending precedence

  Fix: extends `Event` trait with 2 default-`None` methods (`payload_success`, `payload_error_message`); overrides them in `CopilotEvent` for `ToolExecComplete` + `HookEnd`; consumes them in `derive_episodes::on_tool_complete` + `on_hook_end`. `None` defaults to Success (forward-compat for older Copilot CLI 1.0.x / external adapters). `ToolCallStatus::Failure { message: Option<String> }` is now populated end-to-end — surfaced nowhere in UI yet but future-ready for RoiView detail / TurnDetail error display.

  Orphan `on_abort` path (line ~607) intentionally unchanged — its `Failure { message: Some("aborted") }` is wire-truth-independent ("aborted before reaching end event" IS a failure regardless). End-of-session synthesis path (line ~696) also unchanged — no triggering event there, no `payload_success` to read.

  Snapshot regen across 4 distinct suites (9 `.snap` files total), all changes verified per spec §7.4 acceptance gate (only `failure_count` flips + derived `success_count` / `OK%` / `Failure { message }` populated + RoiView ✓→✗ glyph; no episode count or schema drift):
  - `analyzer_on_fixtures`: 3 fixtures (`with-aborts`, `with-hooks-heavy`, `with-mcp-calls`)
  - `episode_derive`: same 3 fixtures (HookCall.success bit, ToolCall.status `Success`→`Failure { message: "file not found" }`)
  - `aggregate` (cross-session, md + html): `bash 15/1→14/2`, `mcp__filesystem__read_file 1/0→0/1`
  - `views::roi` (with-mcp-calls): `1/0/100%`→`0/1/0%`, `(1.0s✓)`→`(1.0s✗)` — F1.13/F2.3 finally firing

  Adds 16 new tests (9 unit + 4 e2e regression guards + 2 inherent-method doctests + 1 fmt-equivalent forwarder check); end-to-end fixture assertions (`b1_*_has_{tool,hook}_failure`) would have caught the bug in M1.2 and now serve as permanent regression guards.

  Workspace: 777 → 797 tests passing. References: ADR-0013 (`docs/internals/adr-0013-event-success-bit.md`), spec `docs/superpowers/specs/2026-06-06-b1-failure-bit-design.md`, plan `docs/superpowers/plans/2026-06-06-b1-failure-bit.md`. 5 commits: T1 (Event trait) + T2 (CopilotEvent overrides) + T3 (derive consumers + snapshots) + T4 (e2e guards) + T5 (this docs sync).

- `agentprof-tui`: F1.7.1 — `WatchRunner::render_into` Single arm now dispatches all 4 `View::*` arms (was: only `View::Models` had its own arm; Flamegraph/Roi/Aggregate fell through to `views::aggregate::render`). Pre-fix, pressing `1` / `2` / `3` in watch mode updated `view_state.view` correctly (F1.7 T10 had fixed the state round-trip) but the rendered output stayed on aggregate — state-without-display, exactly the same UX paper-cut F1.7 T10 was meant to fix only one level up. Now mirrors `AppRunner::render_into` exactly. Also: the help overlay (`?`) now renders in watch Single mode via the newly-exposed `pub(crate) crate::app::draw_help_overlay`; pre-F1.7.1 the keystroke toggled `view_state.help_overlay` (TUI #3 gated this to Single only) but no render path consumed the flag. 6 new regression tests in `tests/watch_runner.rs` cover each view's render path (`watch_single_renders_{flamegraph,roi,aggregate,models}_view_when_view_is_*`) + the help overlay (`watch_single_renders_help_overlay_when_help_open`) + a full 1/2/3/4 end-to-end keystroke→render round-trip (`watch_single_view_round_trips_render_through_all_4_views`). 739 → 745 tests.

### Removed

- **BREAKING (core)**: `CoreError::Invariant(String)` variant removed per full-review CORE #4 (`invariant-variant`). The variant was a stringly-typed catch-all callers could not pattern-match; pre-removal audit (`grep -rn 'CoreError::Invariant' crates/`) found **zero live constructors**, so the removal has no internal call-site impact. `CoreError` remains `#[non_exhaustive]` so adding the variant back later (or replacing with a typed variant) is non-breaking. External callers using `match err { ... }` with a wildcard `_ =>` arm are unaffected; explicit `CoreError::Invariant(_) =>` arms now become dead code (clippy warns).

### Changed

- **BREAKING (core + cli wire format)**: `AggregateReport.since` field type changes from `Duration` to `Option<Duration>` (Wave C item 1, closes `m1.6.2-followup-json-since-sentinel`). Pre-Wave-C the CLI's `--since all` argument flowed `Duration::MAX` as an **in-band sentinel** all the way into JSON output as the raw integer `9223372036854775807` ms (≈ 292 million years) — visibly ugly and arithmetically dangerous for any downstream consumer summing windows. Now `--since all` becomes `None` at the CLI boundary and the JSON field is **omitted entirely** (paired with `skip_serializing_if = "Option::is_none"`). Finite windows (`--since 7d`) still render as the integer ms count (`604800000`). md/html output is unchanged — `human_duration` already rendered `>= 100 years` as `"all"`. 3 new tests pin the contract (`aggregate_since_none_omits_json_field`, `aggregate_since_some_serializes_as_ms_integer`, `aggregate_since_round_trip_some_and_none`). New `bucket::ms_duration_opt` sibling serde helper; new `cli::aggregate::since_to_opt_chrono` converter at the boundary. **Rust API impact**: `AggregateReport::new(by, since, …)` now takes `Option<Duration>` — callers wrap finite windows in `Some(…)` and pass `None` for unlimited. Affects 5 test sites (mechanically wrapped). All 4 `group_by_*` aggregators now pass `None` (was `Duration::zero()` placeholder; CLI's `fill_metadata` still overwrites). Tests: 797 → 800 (+3).

- **BREAKING (core wire format)**: `AggregateReport.since` and `AggregateReport.total_wall_duration` JSON fields now serialise as **integer milliseconds** instead of integer seconds (full-review CORE #2, `wire-format-units`). Pre-fix these two fields used a private `duration_seconds` helper while every sibling bucket field (`total_duration`, `p50_duration`, etc.) used `ms_duration` — a JSON consumer summing bucket durations vs reading the outer `since` got values 1000× apart in the **same JSON object**. Unit confusion is now structurally impossible: the private `aggregate::duration_seconds` module was removed, the `aggregate::bucket::ms_duration` helper was promoted from module-private to `pub(super)`, and both outer fields point at it. Consumers reading these as durations need to divide the new integers by 1000 if they want seconds (or `Duration::milliseconds(n)` if reading via chrono). Affects `aggregate --export json` (and `watch aggregate` snapshots). No internal Rust API change (the `Duration` typing is unchanged); only the JSON wire format.

- `agentprof-core`: review-cleanup wave C — percentile unification + JSON error path context. CORE #1 (`percentile-divergence`): pre-fix `tool_rank::percentile` and `aggregate::group_by_tool::percentile` used different conventions (`round((pct/100) * (n-1))` upper-midpoint vs `ceil(p * n) - 1` lower-midpoint), silently violating the invariant "aggregate of a single session equals that session" (for `[1,2,3,4]s`, per-session reported `p50 = 3` while cross-session of that single session reported `p50 = 2`). New `analyzer::stats::percentile_nearest_rank` (upper-midpoint convention) is the single source of truth; tool_rank exposes it as a `pub use` re-export (back-compat), aggregate wraps it with a `* 100.0` scale adapter. Per-session snapshots unchanged (tool_rank's pre-fix behavior preserved); 4 aggregate snapshots regenerated for the even-pool convention shift. 11 new unit tests in `analyzer::stats::tests` cover the algorithm + edge cases + the canonical [0ms, 1000ms] guard + the aggregate-of-single-session invariant. CORE #3 (`json-error-path`): `CoreError::Json` variant gains a `path: PathBuf` field (was `Json(#[from] serde_json::Error)` with no path context); a JSON parse failure on one of N session files now surfaces "JSON error reading /tmp/session-abc.jsonl: ..." instead of bare "JSON error: ...". The `#[from]` impl was dropped along with the variant change because the code audit found zero live call sites — future producers must construct the variant explicitly so the path is never lost. 1 new unit test pins the contract.

- `agentprof-tui`: review-cleanup wave B — 4 small TUI improvements from the standing `full-review-tui-*` backlog. **TUI #1** (`phantom-event-refresh`): rewrite `Event::Refresh` variant docs to match reality (no producer in codebase — the originally-designed `WatchRunner::run` emission was shortcut to inline mpsc-drain + `do_reload()`; variant kept under `#[allow(dead_code)]` for future use, removed in a 1.0 cleanup if no use materializes). **TUI #2** (`single-mode-view-lock`): inline-document the watch transient-AppState round-trip contract — fields round-tripped (help_open / detail_view / models_selected / view) vs NOT round-tripped (flame_selected / roi_selected / roi_viewport_top / roi_sort / pending_gg) with the watch-mode rationale (selection resets between keystrokes because underlying data may change between reloads). **TUI #3** (`cross-mode-help-overlay`): gate the `?` toggle in `handle_watch_key` to Single mode (was unconditional — Cross mode mutated `view_state.help_overlay` even though `render_into`'s Cross arm has no help-overlay render path); drop the "? help" advertisement from `render_cross_header` to match. 2 new regression tests + 2 cross-session snapshots regenerated. **TUI #4** (`agg_selected-clamp`): already fixed (Down/j in `handle_watch_key` clamps to `cross_bucket_count() - 1`); closed as obsolete.

- `agentprof-cli`: review-cleanup wave A — 10 small CLI refactors from the standing `full-review-cli-*` backlog, all behavior-preserving or warning-only changes.
  - **CLI #1** — extract `parse_since` into `crate::cmd::since` module (was duplicated in `cmd/list.rs` and `cmd/aggregate.rs`); use `u64::saturating_mul` uniformly so absurd inputs like `1000000000000000000d` saturate to `Duration::from_secs(u64::MAX)` instead of panicking in debug builds (the `cmd/list.rs` copy used plain `*` and had the panic). 3 new unit tests in `cmd::since::tests` (recognises dhms/all · rejects garbage · saturates on overflow); old per-module tests preserved via the shared `use` import.
  - **CLI #2** — `agentprof aggregate --export tui` now checks TTY presence **before** walking the session root (was after `compute_aggregate` which loads every session). Pre-fix, `aggregate --export tui > foo` would parse the whole root then exit 3. New `check_tty_for_tui()` helper called both at the top of `run()` and inside `run_tui_for_aggregate` (defence-in-depth).
  - **CLI #3** — suppress the "no sessions matching" stderr warning when `--export tui` (the empty-state surfaces inside the TUI's own cross-aggregate view; the stderr flash before alt-screen take-over was visually distracting). Watch-tick suppression was already in place — this extends to the one-shot TUI case.
  - **CLI #4** — `agentprof analyze` warns `--root ignored (session path bypasses root discovery)` when both `--root` and `--session <PATH>` are passed (`SessionSelector::Path` never consults root). Mirrors the existing `--output ignored with --export tui` warning convention.
  - **CLI #5** — tighten `parse_events_jsonl`'s sibling `looks_like_incomplete_json` to `pub(crate)` (was `pub` by accident; only used by the same module).
  - **CLI #6** — document the false-negative in `cmd::watch::WatchCmd.session`: `clap`'s `default_value = "latest"` collapses "user omitted" and "user wrote `--session latest` explicitly" into the same `SessionSelector::Latest` variant, so the `"flag ignored in watch aggregate mode"` warning stays silent for the omit case. Real fix would require `Option<SessionSelector>` — deferred to avoid breaking external scripts.
  - **CLI #7** — note (rustdoc comment in `cmd::watch::run_cross`) that `--root` is resolved twice (inside `compute_aggregate` and again for the watcher target); harmless because the resolution function is pure, but architecturally smells. Real fix (have `compute_aggregate` return the resolved root) deferred.
  - **CLI #8** — `cmd::analyze::resolve_session_by_path` now emits a `tracing::warn!` when fs metadata is unavailable (was silently falling back to `UNIX_EPOCH` / size 0), so users know the displayed sort-by-mtime / size values are placeholders rather than the real file's stats.
  - **CLI #9** — rewrite the `parse_events_jsonl` doctest example to use `?` propagation with a `Result`-returning function instead of `.unwrap()` (the `no_run` tag made it harmless but the pattern was teaching the wrong idiom).
  - **CLI #10** — move `ExitKind` from `cmd::analyze` to its own `cmd::exit` module. The historical location was an accident of `analyze` being the first subcommand to define structured exit codes; later `list` / `aggregate` / `watch` imported `ExitKind` from it despite having no other dependency on `analyze`. Kept `pub use crate::cmd::exit::ExitKind` re-export in `cmd::analyze` so external callers don't break. All internal imports updated to the canonical `crate::cmd::exit::ExitKind` path.

- review-cleanup wave D — 4 small backlog clears spanning md / skill semantics / ADR template / CI. **`m1.6.5-b6-followup-md-debug-coupling`**: `cmd::format::md.rs` Tool Rank "Source" column now uses `ToolSource`'s Display impl (e.g. `skill:code-reviewer`) instead of `{:?}` Debug (e.g. `Skill { name: "code-reviewer" }`). B-5 added Display + migrated HTML; markdown was missed until now. 5 analyze_md snapshots regenerated. **`m1.6.5-b6-followup-skill-call-count`**: investigation outcome — `TurnSummaryRow.skill_call_count = 0` in `tool-and-skill-same-turn` fixture is intentional, not a bug. Copilot wire emits `skill.invoked` BEFORE the next `assistant.turn_start` (registration signal, not invocation), so `derive::on_skill_invoked`'s `if let Some(turn_idx) = self.open_turn_idx` guard skips attribution. Actual execution flows through the synthetic `skill__<name>__<method>` tool call in `tool_call_count` — counting both would double-count time. 13-line rustdoc clarification added to `TurnSummaryRow.skill_call_count`. **`workspace-4-adr-template-drift`**: `docs/internals/README.md` template now documents both in-tree ADR header styles (YAML frontmatter for adr-0001..0005; bolded-line for adr-0006/0008-0012) as acceptable; flags adr-0007's blockquote variant as historical. No ADR files modified — cost-of-migration > cost-of-inconsistency for this many files. **`workspace-5-nightly-msrv-check-not-test`**: `nightly-msrv.yml` switched from `cargo check --workspace --all-features` to `cargo check --workspace --all-features --all-targets` so dev-deps / proc-macros that only build via the test target also get MSRV-checked.

### Added

- `agentprof-tui`: F1.15+F1.16+F1.17+F1.18+F1.19 AggregateView polish wave — five small UX improvements to the single-session AggregateView (the least-polished view pre-this-wave, with no selection / no percent columns / no failure coloring / no empty-state messaging / inconsistent block titles). All changes reuse helpers already pub'd by F1.7 (Models view) and F1.11-F1.13 (RoiView) so the visual conventions stay consistent across all four views.
  - **F1.15** — By Mode table adds `Turns%` and `Dur%` columns (denominators computed once per render across all buckets; reuses `views::roi::format_ok_pct` + `views::roi::format_total_pct`). Surfaces the distribution insight `"95% of turns are interactive"` without users having to mentally divide.
  - **F1.16** — By Hook table adds `OK%` column (`views::roi::format_ok_pct`) and Hook cell color hints (`views::roi::failure_severity_color` — Red+Bold > 50% fail, Yellow+Bold any fail on ≥ 3 calls). Surfaces flaky hooks at-a-glance without sorting.
  - **F1.17** — By Mode `Out tokens` cell uses `views::models::format_token_u64_short` (5-char `k/M/G/T/P` abbreviation introduced in F1.7) so large u64 totals like `234567` render as `234k`, freeing column width.
  - **F1.18** — both By Mode and By Hook tables fall back to a vertically-centered DIM placeholder when empty (instead of header + blank rows) via the new shared `render_empty_state(frame, area, block, msg)` helper. Messages: `(no turns recorded for this session yet)` and `(no hook events recorded for this session)`. 1 new unit test (`render_empty_state_centers_message_dim`) locks the vertical-centering invariant.
  - **F1.19** — By Hook block title is now `By Hook (single session — hook events)`, parallel to the By Mode title's `(single session)` qualifier (was: bare ` By Hook `). Helps watch-mode users understand they're seeing per-session data, not cross-session aggregates.
  - 2 existing aggregate snapshots regenerated (`aggregate__cross_turn_tool`, `aggregate__with_mode_transitions`) to reflect the new columns + title + token abbreviation.

- `agentprof-tui`: F1.14 RoiView — anchor user-blocking section to the visible bottom of the table. F1.11's sequential ordering (work → separator → user-blocking) looked ugly when content underfilled the table area (e.g. 1 work + 1 `ask_user` left `ask_user` floating in the middle with empty rows below). F1.14 inserts blank non-selectable padding rows between the work section and the separator so the separator + user-blocking rows render at the table's bottom edge (sticky-footer pattern). Padding only inserted when `content_render_rows < visible_body_rows`; when content overflows the visible area, padding is 0 and the layout reverts to F1.11 sequential ordering so viewport scrolling works as before. Selection→render index mapping updated: blocking selections now translate to `work_n + padding_rows + has_separator + (selected - work_n)` rather than just `+1`. 1 snapshot (`with_ask_user_mid_session`) regenerated to show the new bottom-anchored layout.

- `agentprof-tui`: F1.13 RoiView — Tool cell failure-severity coloring. New `pub const fn views::roi::failure_severity_color(call_count, failure_count) -> Option<Color>` returns the precedence-ordered color: **Red+Bold** for `failure_rate > 50%` (likely broken; "majority of attempts fail" warrants the strongest signal regardless of call count), **Yellow+Bold** for any failure on busy tools (`call_count >= 3` AND any `failure_count > 0`; the `>= 3` guard avoids coloring one-off failures on rarely-called tools — could just be a flake), **default** otherwise. Tints only the Tool cell (not the entire row) so OK%/Tot%/p50 values stay legible on dark themes. Applies symmetrically to both work and user-blocking sections (`ask_user` cancellations now get a Yellow/Red hint). Help overlay (`?`) gains a "RoiView Tool color" section listing all 4 states + the `*` marker convention. 6 new unit tests cover each branch (zero calls, perfect, > 50% Red, exactly 50% boundary, busy-tool Yellow, single-failure noise guard).

- `agentprof-tui`: F1.12 RoiView — two new `%` columns surfaced. **`OK%`** (success rate, sister to the existing `s` sort key) eliminates the user's mental arithmetic over `Calls` / `OK` columns; `pub fn views::roi::format_ok_pct(success, calls) -> String` truncates rather than rounds so `99/100 → "99%"` (not `100%`) — keeps the value monotone with F1.13's color thresholds; zero-call rows render `"—"` distinct from `0%`. **`Tot%`** (this tool's share of session total tool duration) surfaces the core ROI insight `"bash takes 65% of all tool time"` previously requiring side-by-side comparison; `pub fn views::roi::format_total_pct(tool_total, total_all_ms) -> String` shows `"0%"` for sub-1% slices (preferable to omitting; "this tool was called but is a rounding error" is information), `"—"` only when session total is 0. Session total computed once per render across all `tool_rank` rows (work + user-blocking) — sums to 100%. Column layout: 10 columns now (was 8); narrowed Calls / OK / Fail / Total / p50 cells slightly to fit OK% (5 chars) and Tot% (5 chars) within standard 100-col terminals. 7 new unit tests in `views::roi::tests` cover the formatter happy paths (perfect 100%, truncation, basic percentages), edge cases (zero calls, zero session total, sub-1% truncation), and the dash-vs-zero distinction. 4 existing roi snapshots regenerated to include the new columns.

- `agentprof-tui`: F1.11 RoiView — unified selectable table merging work and user-blocking tools. Pre-F1.11 the user-blocking tools (e.g. `ask_user`) rendered in a separate fixed sub-table at `chunks[1]` (`Length(4)`) that was **not selectable** — users could see `ask_user` but j/k/↑/↓ refused to land on it. F1.11 merges into a single selectable table at `chunks[0]`; user-blocking rows render after a plain DIM dashed separator row, themselves DIM-styled + marked with `*` in the `#` column (instead of a numeric rank, signalling "out of rank"). Title bar appends `· DIM = user-waiting` so the legend is in-view. Layout drops one chunk (3-split → 2-split: table + detail strip; `Length(4)` user-blocking sub-table removed) — gantt area gains 4 rows. Detail strip below the table switches content based on selection kind: work selection → existing "recent 5 calls" enumeration; user-blocking selection → `"You waited N times totaling Xm Ys (not counted in agent work)"`. New public API: `partition_and_sort(rows, sort_key) -> (Vec<ToolRankRow>, Vec<ToolRankRow>)`, `roi_selected_row(work, blocking, selected) -> Option<&ToolRankRow>`, `is_selection_user_blocking(work, blocking, selected) -> bool` (last is `const fn`). State changes: `scroll_down` / `scroll_to_bottom` for `View::Roi` now clamp to `tool_rank.len()` (was `work_count` only). 5 new unit tests in `views::roi::tests` (partition + selection mapping) + 1 new app-state test (`roi_navigation_reaches_user_blocking_tools` regression guard) + 1 new snapshot test (`snapshot_roi_with_ask_user_mid_session`); 1 obsolete app-state test removed (`roi_selected_does_not_overshoot_work_partition` enforced the buggy pre-F1.11 behavior). 4 existing roi snapshots updated to reflect the chunks layout change (gantt rows expanded).

- `agentprof-tui`: F1.10 FlamegraphView T-id status color coding — split the 24-char row prefix into two spans (5-char T-id + 19-char rest) so the leftmost T-id can carry a status-encoding fg color without affecting the duration / OutTK columns. New `pub fn views::flamegraph::t_id_status_color(turn) -> Option<Color>` returns the precedence-ordered color: **Aborted** → `Color::Red` (highest; pairs with existing `UNDERLINED` modifier as color-blind backup) → **Open / in-flight** (`turn.ended_at.is_none()`) → `Color::DarkGray` (distinguishes turns still running in watch mode) → **Thinking-only** (closed turn with no tool calls) → `Color::Blue` (legacy F1.5 marker, now confined to the 5-char T-id span for consistency) → **default** (closed turn with tool calls). Composes additively with `REVERSED` (selected) and `UNDERLINED` (aborted) modifiers — a selected-aborted-thinking-only turn shows REVERSED + UNDERLINED + Red T-id (Red wins over Blue per precedence). 9 new unit tests cover each color path, the precedence override (Aborted > Open > thinking-only), the rest-span isolation (Blue does not leak into duration / OutTK columns), and the `PREFIX_WIDTH=24` invariant via a 5+19 sum assertion. 2 F1.5 tests renamed + updated to reflect the new precedence (`build_row_thinking_only_aborted_t_id_is_red_not_blue`, `build_row_selected_aborted_t_id_red_with_reversed_and_underlined`). Help overlay (`?`) Flamegraph cell legend section now lists all 4 T-id color states.

- `agentprof-tui`: F1.9 FlamegraphView detail-block refactor — replace the 3-row bordered " Detail " block at `chunks[1]` with a single no-border 1-row meta line. Drops 3 fields that were already visible elsewhere on screen (Turn UUID — flame row has `Tn`; `out_tokens=N` — flame row has `OutTK` column since F1.6; `tools=N` — footer already enumerates tools). Surfaces 3 new high-value fields the user could not see on this screen before: `+rel in` (relative start time from session start via `human_short`), uncompacted `dur` (flame row truncates to 10-char column), `N calls`. Preserves `model` and `mode` (the two valuable fields of the old detail block). Net effect: **+2 visible gantt rows**. New public helpers `pub fn views::flamegraph::format_meta_line(state, max_width) -> String` and `pub fn views::flamegraph::fit_priority_segments(segments, sep, budget) -> String` (the latter generic enough to reuse for future similar drops-from-back patterns; differs from `fit_entries` in dropping segments outright instead of adding `+K more` suffix). Narrow-terminal truncation drops fields right-to-left in priority order (lowest first: `mode → model → calls → dur → +rel in → Tn`). On `flame_selected` out of range returns `"(no turn selected)"` (parity with footer fallback). 9 new unit tests cover field presence, narrow-width truncation order, open-turn `—` for duration, missing `model` / `mode`, and `fit_priority_segments` corner cases (budget = 0, top-overflow char-truncation). 2 snapshot tests updated to show the new layout (`flamegraph__with_aborts`, `flamegraph__cross_turn_tool`).

- `agentprof-tui`: vim-style view-cycle aliases `h` / `l` — `l` (vim right) is an alias for `Tab` (cycle to next view: Flamegraph → Roi → Aggregate → Models → Flamegraph); `h` (vim left) is an alias for `Shift-Tab` (cycle backward). Mirrors the existing `j` / `k` aliases for `↓` / `↑` scrolling and the `g` / `G` aliases for jump-first / jump-last. When the TurnDetailView (F1) is open, `h` / `l` pop the detail AND cycle view in one keystroke — same pop-and-switch pattern as `1` / `2` / `3` / `4`. Does not conflict with Roi's `t` / `c` / `s` / `p` sort keys (different letters) or the Models view dispatcher (returns `None` for `h` / `l`, falling through to the global cycle). 6 new unit tests cover forward / backward cycle, from-every-view regression, detail-view pop+cycle (both directions), and the Roi-sort-key non-conflict. Help overlay (`?`) updated with a new `h / l` row in both the top-level and detail-view sections; L2 README key-bindings table mentions the aliases inline with `Tab` / `Shift-Tab`.

### Changed

- `agentprof-tui`: **F1.5 thinking-only Blue marker now applies to the 5-char T-id span only**, not the full 24-char prefix (F1.10 follow-on tightening). The Blue fg used to flood the duration + OutTK columns too, making thinking-only turns visually inconsistent with their neighbors. Tightening matches the F1.10 status-color convention. No data semantics change — same turns get marked Blue, just less visually intrusive.

- `agentprof-tui`: F1.8 sticky header for `FlamegraphView` — `pub fn views::flamegraph::header_line() -> Line<'static>` returns a single 1-line strip rendered at the top of the bordered Flamegraph block, above the scrolling rows. Labels the three prefix columns (`Turn` / `Duration` / `OutTK`) aligned over the data values + colored gantt legend (`█ tool` cyan · `░ thinking` DIM · `· padding` DarkGray) so each cell character is self-explanatory without external docs. The `OutTK` label (per-turn **output** tokens — sum of `assistant.message.outputTokens` events) is intentionally singular + abbreviated to fit the fixed 5-char tokens column without breaking `PREFIX_WIDTH=24`; per-turn input / cache tokens are NOT available on the Copilot wire and only surface at session level via the F1.7 Models view (key `4`). When the upstream wire schema starts exposing per-turn input / cache, this column can widen to multiple columns. Reserved only when `inner.height >= 3` (room for header + ≥1 row + footer); on `h == 2` the per-turn footer is prioritized over the legend. The first `PREFIX_WIDTH` (24) chars use the same `{:>5} {:>10} {:>5}  ` template as `build_row`'s prefix — 3 new unit tests (`header_line_prefix_matches_build_row_format` / `header_line_contains_three_legend_symbols` / `header_line_legend_labels_present`) lock the alignment invariant + legend completeness. 2 existing Flamegraph snapshot tests updated (`flamegraph__with_aborts`, `flamegraph__cross_turn_tool`) to include the new header row.

- `agentprof-tui`: F1.7 Models view snapshot tests — `snapshot_models_with_data` (uses `with-session-shutdown` fixture, locks table render: 5-column layout, sort by input desc with `claude-opus-4.7-1m-internal` before `gpt-5-mini`, Total footer row) + `snapshot_models_empty_state` (uses `builtin-tools-only` fixture, locks centered `(no model usage data — session has not emitted shutdown event yet)` placeholder). Help overlay (`?`) Models row already added by T7; verified accurate post-T8/9/10.

- `agentprof-tui`: WatchRunner — `WatchViewState.models_selected` field + round-trip across the transient AppState. F1.7 Models view is now available in watch mode (single-session `watch ...`). When `session.shutdown` arrives mid-watch, the next reload populates `AnalysisReport.model_metrics` and the Models view body switches from empty-state to the table on the next render. Cross-session aggregate mode (`watch aggregate ...`) does NOT support Models view (session-level data; cross-session aggregation is out of scope per ADR-0012 D-13).

- `agentprof-tui`: NEW Models view (key `4`) showing session-level per-model token rollup (input / output / cache_read / cache_write). Sorted by input desc; totals footer row + 1-line status hint. Centered placeholder + multi-line explanation when session has not emitted `session.shutdown` yet. j/k/↑/↓/G/gg navigation; `Esc` returns to Flamegraph (view 1). `Tab` / `Shift-Tab` cycle includes Models. `pub fn views::models::render(frame, area, &AppState)` + `pub fn views::models::format_token_u64_short(u64) -> String` (5-char cap, k/M/G/T/P abbreviations; sibling to F1.6's `format_tokens_short` which takes `Option<u32>`). `AppState.models_selected: usize` field tracks selection; `scroll_to_top` / `scroll_to_bottom` extended to drive it for `gg` / `G`. See [ADR-0012](docs/internals/adr-0012-session-model-metrics-and-models-view.md) D-9 / D-10 / D-11 / D-12.

- `agentprof-core`: `AnalysisReport.model_metrics: Option<BTreeMap<String, ModelUsage>>` field — cloned from `Episodes.model_metrics` by `analyze()`. Surfaces session-level per-model token rollup to all `AnalysisReport` consumers (TUI Models view, JSON / HTML / Markdown / CSV exports automatically). `#[serde(skip_serializing_if = "Option::is_none")]` keeps archives clean when absent.

- `agentprof-core`: `Episodes.model_metrics: Option<BTreeMap<String, ModelUsage>>` field — populated by `derive_episodes` from `Event::payload_model_metrics()` on `EventKind::Shutdown` events (last-wins per ADR-0012 D-6). `#[serde(skip_serializing_if = "Option::is_none")]` keeps older archives forward-compatible. See [ADR-0012](docs/internals/adr-0012-session-model-metrics-and-models-view.md) D-3 + D-6.

- **F1.7 fixture**: `crates/agentprof-adapters/tests/fixtures/copilot/with-session-shutdown/` — 1-turn-1-tool Copilot session that emits `session.shutdown` with `modelMetrics` for 2 distinct models (`claude-opus-4.7-1m-internal` + `gpt-5-mini`). Token values mirror 2026-06-03 real-session survey. Used by F1.7 Tasks 5/6/11. Brings fixture count from 21 to 22.

- `agentprof-adapters`: `CopilotEvent::payload_model_metrics()` override extracts per-model token rollup from the `Shutdown` variant's `model_metrics: BTreeMap<String, serde_json::Value>` tree. Free-form `Value` walking (`.get("usage")?.get("<key>").and_then(as_u64).unwrap_or(0)`) — robust against Copilot wire schema drift; new fields don't break parsing, renames produce `0` instead of failing. Skips models whose `usage` subtree is absent; returns `None` if no models have usage data. `ModelUsage` instances constructed via `::new()` + field assignment (required because `#[non_exhaustive]` blocks struct-literal construction from outside `agentprof-core`). See [ADR-0012](docs/internals/adr-0012-session-model-metrics-and-models-view.md) D-7.

- `agentprof-core`: `Event::payload_model_metrics()` trait method (default `None`). Extension point for adapters to expose per-model token rollups; consumed by `derive_episodes` to populate `Episodes.model_metrics`. Non-breaking: default impl means existing trait impls compile unchanged. See [ADR-0012](docs/internals/adr-0012-session-model-metrics-and-models-view.md) D-4.

- `agentprof-core`: `ModelUsage` public struct in `analyzer` module — 4 `u64` fields (`input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_write_tokens`) + `pub const fn new()` zero-ctor + `pub const fn total()` saturating sum. `#[non_exhaustive]`. Foundation for F1.7 session-level token totals (populated by `Event::payload_model_metrics`, surfaced in TUI Models view). See [ADR-0012](docs/internals/adr-0012-session-model-metrics-and-models-view.md) D-8.

- `agentprof-tui`: FlamegraphView prefix now includes a 5-char output-tokens column (`T1      9.6s    2.3k  ░░░...`), showing the LLM output token count for each turn (sum of `assistant.message.outputTokens` across the turn). `None` (turn had no `assistant.message` events) displayed as a centered `-` to remain visually distinguishable from `0`. TurnDetailView header gains a second line with detailed tokens + model (e.g. `1.2k output tokens · claude-sonnet-4.6`); either side `None` → segment omitted; both `None` → second line skipped entirely. Surfaces existing data — output tokens were already plumbed in `Turn.output_tokens` since M1.4 but not displayed. New public helpers in `views::format`: `format_tokens_short` (5-char cap, k/M abbreviations, sweep-verified up to `u32::MAX`) + `format_tokens_detailed` (no cap, returns `Option<String>`). `turn_header_text` signature changes from `-> String` to `-> Vec<String>` to carry the optional second line. 12 new unit tests (7 format helpers + 2 build_row + 3 turn_header_text). 2 flamegraph snapshots refreshed (purely additive column insertion).

- `agentprof-tui`: FlamegraphView marks thinking-only turns (turns with empty `tool_calls`) with a Blue prefix on the `T-id duration` columns. Composes additively with existing modifiers (selected `REVERSED`, aborted `UNDERLINED`). Footer hint appends `· thinking only` when the selected turn has no tool calls; help overlay (`?`) cell-legend gains a `T-id (blue)` row and the overlay height bumps 27 → 28. Helps users distinguish pure-thinking turns (text-only replies, plan-then-execute breakpoints, summary turns) from active tool-using turns at a glance. Blue is unused by the existing palette (Cyan=Builtin, Magenta=MCP, Yellow=Skill, Red=errors, DarkGray=padding) — no clash. 4 new `build_row` unit tests (Blue-only / Blue+REVERSED / Blue+UNDERLINED / non-Blue baseline) + 1 footer-marker test.

- `agentprof-tui`: TUI discoverability for F1 TurnDetailView — Flamegraph selected-turn footer appends `· Enter for detail` hint (truncated from the right on narrow terminals; `(no turn selected)` placeholder still suppresses the hint); help overlay (`?`) gains "Detail view (Flamegraph → Enter):" section listing the `Enter` / `Esc` / `j`/`k`/`G`/`gg` / `1`/`2`/`3` keys. Help overlay height bumped 22 → 27 to accommodate. `selected_turn_footer_line` rustdoc and `views::flamegraph` module doc updated; 2 flamegraph snapshots refreshed (footer additive only). New unit tests: `selected_turn_footer_line_partial_hint_on_medium_budget` covers mid-hint truncation.
- `agentprof-tui`: `WatchViewState.detail_view: Option<TurnDetailState>` field + `WatchRunner` round-trip — `watch ...` (single-session) users now get `TurnDetailView` parity with `analyze --export tui`. `WatchRunner::render_into` and the key-dispatch path clone `detail_view` into the transient `AppState` before delegating; dispatch writes back. `do_reload` validates that the cached `turn_id` is still present in the reloaded `Episodes`; if not (or if reload returns `WatchData::Cross`), `detail_view` is dropped — the disappeared-in-Single case also sets the red-banner footer to `"turn <id> disappeared after reload"` (ADR-0011 D-14, mirroring ADR-0009 D-13's reload-error UX convention). New `#[doc(hidden)] pub` test accessors: `view_state`, `view_state_mut`, `set_reload`, `do_reload_for_test`. 2 new integration tests: `watch_view_state_persists_detail_view_field` (default `None` invariant) + `reload_drops_detail_view_when_turn_disappears` (fixture-swap).
- `agentprof-tui`: `AppState.detail_view: Option<TurnDetailState>` field + dispatch wiring — `Enter` on a selected Flamegraph row opens the TurnDetailView; in-detail keys (`Esc` returns, `Enter` toggles args expand, `j`/`k`/`↑`/`↓` navigate, `G`/`gg` jump to last/first, `1`/`2`/`3` pop detail and switch view, `q` always quits, `?` toggles help overlay). `AppRunner::render_into` forks on `detail_view.is_some()` to call `render_turn_detail` in place of the per-view dispatch. Logic factored into a `dispatch_detail` helper with a `DetailFlow::{Handled, FallThrough}` outcome so `1`/`2`/`3` pop then fall through to the existing number-key view-switch. 8 new dispatch tests cover starts-None / Enter-opens / Esc-closes-preserves-flame-selected / Enter-in-detail-toggles / j-k-navigate / number-keys-pop-and-switch / Enter-on-empty-episodes-no-panic / q-always-quits. WatchRunner integration lands in F1 Task 8.
- `agentprof-tui`: `render_turn_detail(frame, area, &TurnDetailState, &AppState)` — full-screen ratatui render for F1's TurnDetailView. Shows turn-level header (`{ms}ms wall · {N} tool calls`), one block per tool call sorted by duration desc (`▶ name  dur  ✓  source` with name colored by ToolSource via `theme::tool_source_color`), args preview line (single-line truncated or multi-line expanded), and footer hint. Handles missing-turn diagnostic + empty-tool_calls placeholder. AppRunner integration lands in F1 Task 7; WatchRunner in F1 Task 8.
- `agentprof-tui`: new `views::turn_detail` module — `TurnDetailState` state-machine struct (turn_id, selected_tool_idx, expanded_tools HashSet, viewport_top, pending_gg) with `move_up`/`move_down`/`jump_first`/`jump_last`/`toggle_expand` helpers; pure formatters `format_args_preview` (80-char single-line truncated preview, or `(not captured)` placeholder when args is None) + `wrap_args_full` (multi-line pretty-printed JSON with word-wrap fallback) + `status_sigil` (✓/✗/?). No rendering yet — `render_turn_detail` lands in F1 Task 6. See [ADR-0011](docs/internals/adr-0011-turn-detail-and-args-plumbing.md) D-7 / D-8 / D-9 / D-10 / D-11.
- `agentprof-core`: `derive_episodes` PASS 0 args-map closes the F1 args-plumbing chain — walks events once to build `BTreeMap<tool_call_id, serde_json::Value>` from `Event::payload_tool_requests`, then PASS 1's tool-close paths (normal, orphan, abort, end-of-session) stamp `ToolCall.arguments` via lookup. First-occurrence-wins on duplicate `tool_call_id` (logged at `tracing::debug!` target `derive`). End-to-end: `ToolRequest.arguments` (Copilot adapter) → `payload_tool_requests` (trait) → PASS 0 `args_by_call_id` → `ToolCall.arguments`. Args land in the in-memory `Episodes` consumed by TUI `TurnDetailView`; **JSON / HTML / Markdown / CSV / Speedscope exports do NOT currently carry args** (those exports serialize aggregated `AnalysisReport.tool_rank`, not per-call `ToolCall`). Adding args to JSON export is reserved for a future enhancement. See [ADR-0011](docs/internals/adr-0011-turn-detail-and-args-plumbing.md) D-3 + D-4.
- `agentprof-core`: `Event::tool_call_id() -> Option<&str>` trait method (default `None`); override on `CopilotEvent` (`ToolExecStart` / `ToolExecComplete` / `ToolUserRequested`). Required by `derive_episodes` to look up args collected in PASS 0.
- `agentprof-core`: `ToolCall.arguments: Option<serde_json::Value>` field — adapter-supplied JSON args, `#[serde(skip_serializing_if = "Option::is_none")]` to keep older archives forward-compatible. Default `None`; populated by `derive_episodes` PASS 0 (Task 4 of F1) for adapters that implement `Event::payload_tool_requests`. `ToolCall` remains `#[non_exhaustive]` so the field add is non-breaking. See [ADR-0011](docs/internals/adr-0011-turn-detail-and-args-plumbing.md) D-5.
- `agentprof-adapters`: `CopilotEvent::payload_tool_requests()` override populates `(tool_call_id, arguments)` pairs from `AssistantMessage.tool_requests[*]` (multi) and `ToolUserRequested.arguments` (single, via `serde_json::to_value(&ToolUserArgs)`). All other variants inherit the empty-`Vec` default. Enables `derive_episodes` (later task) to stamp `ToolCall.arguments` end-to-end for Copilot CLI sessions. See [ADR-0011](docs/internals/adr-0011-turn-detail-and-args-plumbing.md) D-2.
- `agentprof-core`: `Event::payload_tool_requests()` trait method with empty-`Vec` default impl. Extension point for adapter-supplied `(tool_call_id, arguments)` pairs; consumed by `derive_episodes` PASS 0 to populate the upcoming `ToolCall.arguments` field. Non-breaking: default impl means existing adapter trait impls compile unchanged. See [ADR-0011](docs/internals/adr-0011-turn-detail-and-args-plumbing.md) D-1 + D-2.
- **FlamegraphView color coding by ToolSource**: each `█` block in the gantt is now colored — Builtin tools cyan, MCP tools magenta, Skill invocations yellow. Reuses the existing `tool_source_color` mapping from `crates/agentprof-tui/src/theme.rs` (same palette as `RoiView`'s source column). Thinking cells (`░`) stay neutral with `Modifier::DIM`; padding cells (`·`) render dark-gray + dim. Cells with adjacent identical styles are run-length compressed into single `TextSpan`s for compact output. Hooks not yet color-coded (deferred — they live in `hook_calls` not `tool_calls`).
- **FlamegraphView selected-turn footer**: navigating with `↑`/`↓`/`j`/`k`/`G`/`gg` now displays a footer line listing the selected turn's actual tool calls with per-call durations (e.g. `T3 selected:  bash(120ms) read_file(85ms) +2 more`). Truncates from the right with `+K more` when the line exceeds the gantt width. Empty `tool_calls` reads as `T3 selected:  (no tool calls)`; out-of-range selection as `(no turn selected)`.
- **TUI vim keybindings**: `j` / `k` aliases for `↓` / `↑` (move selected row); `G` jumps to last selectable row; `gg` (two-key vim sequence) jumps to first row. Applied to both the M1.5 `AppRunner` (single-session `FlamegraphView` / `RoiView` / `AggregateView`) and the M1.6.3 `WatchRunner` cross-session aggregate view. `WatchViewState` gains a `pending_gg: bool` field mirroring `AppState::pending_gg`. Help overlay (`?`) and `crates/agentprof-tui/README.md` key-bindings table updated.
- `agentprof --log-level <LEVEL>` / `--log-file <PATH>` global CLI flags (M1.6.4) — clap `global = true`, work on every subcommand (`analyze`, `list`, `aggregate`, `watch`, `watch aggregate`). Env fallback: `AGENTPROF_LOG_LEVEL` / `AGENTPROF_LOG_FILE` (existing `AGENTPROF_LOG` is kept for backwards compatibility). `--log-file -` forces stderr (overrides the TUI auto-redirect, user owns alt-screen pollution risk). See [ADR-0010](docs/internals/adr-0010-tracing-infrastructure.md) and [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md) `## Tracing & logging`.
- TUI mode (`analyze --export tui`, `watch`, `watch aggregate`) auto-switches the tracing writer to a rolling daily log file under `$XDG_STATE_HOME/agentprof/agentprof.log` (M1.6.4). The path is printed to stdout on clean exit. Powered by `tracing-appender`'s non-blocking rolling appender + a `tracing_subscriber::reload::Layer` that swaps the underlying writer at runtime. Soft-falls to stderr if XDG path is unwritable. See [ADR-0010 D-2 / D-13](docs/internals/adr-0010-tracing-infrastructure.md).
- `agentprof_core::observability::pii::{hash_path, hash_short}` PII redaction helpers (M1.6.4) — sha256[..8] hex. `hash_path` accepts `&Path` (lossy-UTF-8 string form); `hash_short` accepts arbitrary `&str`. Used by the cli + adapter + core layers to emit `session = %hash_path(p)` in tracing fields. `AGENTPROF_LOG_FULL_PATHS=1` opts out system-wide (see Fixed entry below — `hash_path` itself reads the env var at every span layer). See [ADR-0010 D-5](docs/internals/adr-0010-tracing-infrastructure.md) for the collision-vs-PII trade-off.
- 13 tracing spans across 4 layers (M1.6.4): `cmd.{analyze, list, aggregate, watch}` (`info_span!`, cli) → `adapter.{discover, parse, load_meta}` (`debug_span!`, adapters) → `analyzer.{derive_episodes, analyze}` + `aggregator.group_by{tool, mcp, day, model}` (`debug_span!`, core) → event-level `tracing::{trace, debug, info, warn, error}!` (anywhere, replaces every `eprintln!`). See [ADR-0010 D-4](docs/internals/adr-0010-tracing-infrastructure.md).
- `agentprof watch` subcommand (M1.6.3) — live-refresh TUI for single-session (default) and cross-session (`watch aggregate ...`) views. Powered by `notify-debouncer-mini` (kernel events via inotify / FSEvents / ReadDirectoryChangesW; pulls `notify` v6.1.1 transitively); default 250 ms debounce (overridable with `--debounce-ms`). `--session latest` locks to the initial session at startup (no auto-follow of newer sessions — D-5). Reload failures (e.g. transient parse error during writer mid-flush) render in a red footer banner without exiting the loop (D-13). `watch aggregate` rejects `--export` / `--output` with `UserError = 1` (output is always TUI). Requires TTY on both stdin and stdout (else `OutputError = 3`); no polling fallback (D-15) — notify init failure exits `DataError = 2` with an actionable message. See [ADR-0009](docs/internals/adr-0009-watch-runner-and-notify.md).
- `agentprof aggregate --export tui` (M1.6.3) — static cross-session aggregate TUI (deferred from M1.6.2). One-shot view of any `--by tool|mcp-server|day|model` aggregation in the new `WatchRunner` (no live refresh; use `watch aggregate ...` for that). Requires TTY on both stdin and stdout. Backed by the same `agentprof_tui::watch::WatchRunner::new_static` path as the cross-session watch mode.
- `agentprof-tui` crate: new `watch` module — `WatchRunner` (owned-data event loop coexisting with the M1.5 borrow-based `AppRunner`), `WatchData::{Single, Cross}` enum (single-session vs cross-session payload), `Event::Refresh` variant (only emitted by `WatchRunner::run`), `RefreshKind` (input from the cli-side debouncer), `ReloadError` (caller-side reload failures rendered in the footer banner), `AggSortKey` (`c` / `t` / `s` / `p` for cross-session aggregate). The existing `views::aggregate` module gains a cross-session arm so `WatchData::Cross(...)` renders without forking a new view file (D-9).
- `agentprof aggregate` subcommand (M1.6.2) — cross-session aggregation reports across 4 group-by keys (tool / mcp-server / day / model) × 4 export formats (md / json / csv / html). Day buckets carry a `utilization_pct` field (tool time / wall time × 100) with auto warn-color flag when below `--low-utilization-threshold` (default 20). Per-session parse failures degrade gracefully (skipped + summarized to stderr); empty-window exits 0. Sequential parse (rayon perf milestone deferred). New deps: `csv = "1"` (workspace). See [ADR-0008](docs/internals/adr-0008-aggregate-report-and-utilization.md).
- `agentprof analyze --export speedscope` (M1.6.4) — write a Speedscope evented JSON profile (frame-deduplicated, timestamp-normalized to session start) consumable by <https://speedscope.app>. Span-overlap within a turn is auto-adjusted with `ExportWarning::SpanAdjustedForSpeedscope` on stderr. Frame naming: builtin `<tool>` / MCP `mcp:<server>::<leaf>` / hook `hook:<name>` / skill `skill:<skill>` / synthetic `session`/`turn-<N>`/`turn-orphan`. See [ADR-0007](docs/internals/adr-0007-speedscope-export.md).
- `agentprof analyze --export html` (M1.6.4) — write a self-contained static HTML report (no JS, no external assets) with embedded build-time-rendered SVG flamegraph (responsive, ToolSource-color-coded) + Turn / Tool / Hook tables + Warnings list + print-friendly CSS. Re-activates the `askama` 0.16 workspace dep.
- `agentprof list` subcommand (M1.6.1) — cheap session discovery + 7-column plain text table (`ID / Started / Model / Turns / Out-tokens / Duration / Size`). Defaults `--since 7d --limit 20`. Per-session parse failures degrade gracefully (skipped + summarized to stderr). See [`crates/agentprof-cli/README.md`](crates/agentprof-cli/README.md) `## agentprof list` section.
- `agentprof-tui` crate: first interactive ratatui TUI shipped as M1.5 (`analyze --export tui`).
  - **FlamegraphView**: per-turn horizontal gantt; segments are tool calls; whitespace = LLM thinking time.
  - **RoiView**: interactive tool rank with sort cycling (`t`/`c`/`s`/`p` = total / calls / success% / p50); recent-calls detail strip; user-blocking tools (`ask_user`) split into separate sub-table per M1.4 post-output-audit.
  - **AggregateView**: single-session By-Mode + By-Hook tables.
  - **Panic-safe terminal lifecycle**: `install_panic_hook` (Once-guarded) → `enter` → `run` → best-effort `leave`. See [ADR-0006](docs/internals/adr-0006-panic-safe-tui.md).
  - **TTY required**: piping yields `OutputError` (exit 3) with a helpful message; use `--export md` or `--export json` for headless.
  - References: spec [`2026-05-30-m1.5-tui-design.md`](docs/superpowers/specs/2026-05-30-m1.5-tui-design.md), plan [`2026-05-30-m1.5-tui.md`](docs/superpowers/plans/2026-05-30-m1.5-tui.md).
- B-6 (M1.6.4 follow-up M-3): 3 new copilot fixtures covering combinations the existing single-feature fixture set did not exercise — `tool-and-skill-same-turn/` (one turn calls both `bash` and `skill__code-reviewer__run`), `two-skills-one-turn/` (single turn invokes `code-reviewer` + `git-flow`, locking 2 distinct `ToolSource::Skill { name }` rows in `tool_rank`), `orphan-skill-mix/` (turn closes cleanly, then an orphan `tool.execution_complete` + orphan `skill.invoked` arrive post-turn). 6 new snapshot tests in `agentprof-cli` (md + html per fixture) and 3 new analyzer snapshots in `agentprof-adapters` lock the renderer/analyzer behaviour for these combinatorial cases. Cross-session aggregate snapshots refreshed to reflect the 3 added sessions.
- **B-7 (M1.6.4 follow-up wave, 2026-06-03)**: new copilot fixture `with-ask-user-mid-session/` — 3-turn session containing a 10-minute `ask_user` turn between two ~5-second normal turns. The ~120× wall-time ratio is the scenario that the `b5c1429` `FlamegraphView` fix addresses (exclude user-blocking turns from `max_dur` scaling). End-to-end snapshots (analyzer episode derivation + cli md / html exports) lock the rendered output so a future PR reverting the user-blocking filter would visibly fail snapshot tests. Cross-session aggregate snapshots refreshed (now 20 sessions; `ask_user` appears as its own row in `by-tool` aggregation with 10.0 min total). Brings copilot fixture count to 20.
- **B-4 (M1.6.4 follow-up wave, 2026-06-03)** [`c54a1af`]: 3 new `agentprof_core::export::speedscope::ExportWarning` variants — `OpenTurnTruncated { turn_id, original_at, clamped_at }`, `OrphanTimeShifted { orphan_kind, original_at, shifted_to }`, `NegativeDurationClamped { name, started_at, ended_at }`. Emitted when `emit_turn` synthesizes an open-turn Close that would violate at-monotonicity (clamps to next-turn start), when `emit_orphans` shifts an orphan's `at` forward to maintain ordering across the in-turn → orphan boundary, or when `duration_ms` would otherwise silently clamp a negative duration to 0. Speedscope profiles no longer violate Speedscope's per-stack at-monotonicity invariant.
- **B-5 (M1.6.4 follow-up wave, 2026-06-03)** [`afae0e8`]: new `Display` impls on `agentprof_core::model::tool_source::ToolSource`, `agentprof_core::error::ParseWarning`, and `agentprof_core::episode::warning::DeriveWarning`. **API addition (SemVer minor)** — these are trait impls on public types. HTML renderer in `agentprof-cli::cmd::format::html` now uses `{}` instead of `{:?}` for these 4 types (`ToolSource::Skill { name: "foo" }` → `skill:foo`; raw Rust enum syntax no longer leaks into end-user HTML). Defensive `html_escape` helper + `serde_json::json!` fallback added in `html.rs` and `speedscope.rs` to harden against future user-controlled error messages.

### Changed

- **BREAKING (tui — internal)**: `agentprof_tui::views::View` enum gained `Models` variant (key `4`).
  Out-of-tree consumers pattern-matching exhaustively will need a `View::Models => ...` arm
  or a `_ =>` catch-all. `agentprof-tui` is a workspace leaf with no external consumers
  expected, but the change is semantically breaking per Rust semver. Consider applying
  `#[non_exhaustive]` to `View` in a follow-up to prevent recurrence (also breaking).
  See [ADR-0012](docs/internals/adr-0012-session-model-metrics-and-models-view.md) D-9.

- **FlamegraphView visual clarity (M1.6.4 follow-up wave continued)**: LLM thinking time inside a turn now renders as `░` (U+2591 LIGHT SHADE) instead of plain space, so the three gantt states are visually distinct: `█` tool execution / `░` thinking / `·` padding. Mostly a UX win for users with sessions where LLM reasoning dominates per-turn wall-time. Existing snapshots refreshed.

- **B-3 (M1.6.4 follow-up wave, 2026-06-03)** [`b376d18`]: `agentprof_core::export::speedscope::emit_turn` and `emit_orphans` refactored to take a shared `EmitCtx<'a>` struct bundling shared context refs (frame index, output Vec, warnings, etc.). `#[allow(clippy::too_many_arguments)]` removed. `speedscope::lookup` gains a `debug_assert!(idx.contains_key(name))` so debug builds catch misregistered frames; release builds keep the silent 0 fallback. No production behaviour change.

- All 13 production `eprintln!` calls in `agentprof-cli::cmd::*` have been converted to `tracing::{warn, info, error}!` (M1.6.4). **User-visible diff**: warnings that previously appeared as `agentprof: warning: ...` on stderr now appear in `tracing_subscriber::fmt` format (e.g. `WARN agentprof_cli::cmd::analyze: ...`). Shell scripts grepping stderr by the old prefix should switch to grepping for level tokens (`WARN` / `ERROR` / `INFO`) or use `--log-file <path>` to redirect tracing output away from stderr entirely. One `eprintln!` is intentionally kept in `main.rs` for the top-level error printer (must reach stderr even when tracing is pointed at a file). See [ADR-0010 D-7](docs/internals/adr-0010-tracing-infrastructure.md).
- `Cli` in `agentprof-cli` refactored from `enum` to `struct` with a `#[command(subcommand)]` field (M1.6.4) to enable the two new global args (`--log-level` / `--log-file`). Backwards-compatible at the CLI surface: every existing subcommand name and arg is unchanged. See [ADR-0010 D-10](docs/internals/adr-0010-tracing-infrastructure.md).
- `cmd::aggregate::run` refactored to extract `pub fn compute_aggregate(adapter: &CopilotAdapter, cmd: &AggregateCmd) -> Result<(AnyAggregateReport, usize)>` so both `aggregate --export tui` and `watch aggregate ...` reload share the same load + compute pipeline. The second tuple element is the total session-ref count scanned (used by `aggregate::run` for the "no sessions matching `--since=...`" warning; discarded by the watch reload closure via `.map(|(r, _)| WatchData::Cross(r))`). Visibility is `pub` (not `pub(crate)`) intentionally — `agentprof-cli` is a binary crate, so external visibility is identical for both, and clippy's `redundant_pub_crate` lint fires on `pub(crate)` in binary crates.

### Dependencies

- `agentprof-tui`: added direct dep on workspace `serde_json` (already present transitively via `agentprof-core`) — needed because `views::turn_detail::format_args_preview` / `wrap_args_full` take `Option<&serde_json::Value>` as part of the public API. No new external crate added; deny allowlist unaffected.
- Added direct workspace deps (M1.6.4): `sha2 = "0.10"` (MIT OR Apache-2.0; used only by `agentprof-core` for `observability::pii::hash_path` / `hash_short`); `tracing-appender = "0.2"` (MIT; used only by `agentprof-cli` for the non-blocking rolling-file writer behind the TUI auto-redirect). Transitive: `digest`, `block-buffer`, `crypto-common`, `generic-array`, `typenum`, `cpufeatures` (sha2 chain); `time` / `parking_lot` may also be pulled in by tracing-appender depending on platform — all under MIT or MIT OR Apache-2.0. **No `deny.toml` change required** — every transitively-added crate uses a license already in the existing allowlist. See [ADR-0010 D-9](docs/internals/adr-0010-tracing-infrastructure.md).
- Added direct workspace dep: `notify-debouncer-mini = "0.4"` (M1.6.3, used by `agentprof-cli` for the `watch` file watcher). `notify` v6.1.1 comes in **transitively** via the debouncer's `notify` re-exports — `agentprof-cli` uses `notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode}` rather than depending on `notify` directly. A direct `notify` workspace dep was intentionally NOT declared, to avoid the risk of two notify-major versions co-existing in the dependency tree (e.g. if a future change pulled `notify = "7"` while debouncer-mini still pinned v6). See [ADR-0009 D-4](docs/internals/adr-0009-watch-runner-and-notify.md).
- Transitive additions from `notify-debouncer-mini` (no direct entry in `Cargo.toml`): `notify` 6.1.1 + `crossbeam-channel` + platform backends (`inotify` on Linux, `fsevent-sys` on macOS, `kqueue` on BSD) + `mio` + `filetime`.
- **No `deny.toml` changes required** — every transitively-added crate uses a license already in the existing allowlist (`MIT`, `Apache-2.0`, `ISC`, `CC0-1.0`). The `Artistic-2.0` license concern that came up during design only applies to `notify` 7.x, which we don't ship.

### Fixed

- `agentprof-tui`: WatchRunner `view` round-trip — pre-F1.7 architectural bug discovered during T10 self-review: `WatchRunner` did NOT round-trip `AppState.view` across the transient AppState, so number-key view switches (`1`/`2`/`3` historically, `4` after F1.7) were silently dropped on the next render. Now `WatchViewState.view: View` field added (defaults `Aggregate` for M1.6.3 backward compat) + round-tripped in render / `run` dispatch / `dispatch_event_for_test` paths. F1.7 Models view is now actually reachable in watch mode. Regression test `watch_runner_dispatch_number_keys_persist_view_across_events` locks 1/2/3/4 all working; existing `watch_runner_dispatch_4_switches_to_models_view` strengthened to assert the view-switch (was previously only asserting `models_selected`). Pre-existing `watch_runner_dispatch_enter_opens_detail_view` test updated to explicitly press `1` first (Aggregate is the new default; previously the test relied on the implicit `AppState::default()` view=Flamegraph leak).

  **Known limitation**: render dispatch is still incomplete in watch mode — `WatchRunner::render_into`'s `match transient.view` block only handles `View::Models` (new in F1.7) and `_ => aggregate::render`, so pressing `1`/`2` in watch mode updates `WatchViewState.view` correctly but the rendered output stays on Aggregate. Pre-existing M1.6.3 limitation surfaced by F1.7's view round-trip fix. Tracked as F1.7.1 follow-up: extend the match to dispatch all four `View::*` arms, matching `AppRunner::render_into`. See ADR-0012 D-13.
- **FlamegraphView padding `·` invisible on dark terminals**: padding cells now render `Color::DarkGray` *without* `Modifier::DIM`. Previously `DarkGray + DIM` collapsed to imperceptible on black-background terminals (most dark themes), so users saw `█····` rows as just `█` with empty space — making it look like the gantt was broken or the colored block was the only content. Plain `DarkGray` keeps padding subtle but visible. Regression test added in `views::flamegraph::tests::build_styled_cells_handles_all_cell_types` + `build_styled_cells_handles_no_sources_thinking_only`. Reported via real-session feedback from a black-background terminal.
- **FlamegraphView scaling: switch from max to p95** to resist agent-side outlier turns. Previously even after excluding user-blocking turns (`b5c1429`), a single agent-driven long-tail turn (e.g. one containing a `task` call running 48 minutes) would set `max_dur` so high that all normal 5-30s turns rendered as ≤2 cells of `█/░` followed by all padding. Now the gantt scales by p95 of non-user-blocking turn durations; outlier rows clamp to gantt width via the existing `.min(gantt_w)` guard so they remain visible. Standard practice in profiler tooling (Speedscope / flamegraph.pl). Reported via real-session feedback after `b5c1429` fix landed.
- **FlamegraphView scaling (M1.6.4 follow-up wave continued)**: `max_dur` calculation in `agentprof-tui::views::flamegraph` now excludes user-blocking turns (e.g. those containing an `ask_user` call where the user spent minutes/hours thinking). Previously, a single hours-long `ask_user` turn would set `max_dur` so high that all other turns scaled to near-zero gantt-bar width, making the visualization useless. Fallback to original behavior when *every* turn is user-blocking (degenerate case). Adds `Turn::is_user_blocking()` method on `agentprof-core::episode::Turn`. Mirrors the existing user-blocking split in the Tool Rank table (ADR-0005 §6).
- `AGENTPROF_LOG_FULL_PATHS=1` now correctly opts out of path hashing at every span layer (L1 `cmd.*` in cli, L2 `adapter.*` in `agentprof-adapters`, L3 `analyzer.*` / `aggregator.*` in `agentprof-core`). Previously the env var only affected the cli-layer `cmd.*` spans because `agentprof_core::observability::pii::hash_path` did not check the env var — only the cli-side `agentprof_cli::observability::maybe_hash_path` wrapper did, and adapter / analyzer spans called `hash_path` directly from `#[tracing::instrument]` attributes. Now `hash_path` itself reads `AGENTPROF_LOG_FULL_PATHS` per call, so the opt-out propagates system-wide (privacy-tightening fix; M1.6.4 final-review follow-up `m1.6.4-final-followup-full-paths-l2-l3-gap`). `agentprof_cli::observability::maybe_hash_path` was removed as redundant; the 3 cli call sites switched to `hash_path` directly. `LogConfig::full_paths` field is retained but is informational only (env var is the runtime source of truth).
- TUI key bindings: `1`/`2`/`3` now ALWAYS switch view; previously they re-sorted the table when active view was Roi (spec §7 conflict rule), which made it hard to escape RoiView without remembering Tab. Sort keys are now `t`/`c`/`s`/`p` (total / calls / success% / p50), only effective when view == Roi.
- TUI viewport scroll: `↑` / `↓` in Flamegraph and Roi now auto-scroll the visible window to follow the selected row. Previously the selection could move out of the visible viewport with no visual feedback.
- TUI Flamegraph: O(N×M) per-frame turn lookup replaced with HashMap (perf, M1.5 audit #1).
- TUI: `AppRunner::set_view` and `state()` tightened to `#[doc(hidden)] pub` — discourages bypassing dispatch while remaining reachable from integration tests (M1.5 audit #2).
- TUI Event: `from_crossterm` filters `KeyEventKind::Press`; previously Windows kitty / enhanced input mode would double-toggle `?` overlay on key release/repeat (M1.5 audit #3).
- CLI `analyze --export tui`: warns instead of silently ignoring `--output` / `--section` flags (M1.5 audit #4).
- CLI `analyze --export tui`: now checks BOTH stdin and stdout for TTY; previously `< /dev/null` caused `crossterm::event::read` to block forever (M1.5 audit #5).
- `with-skill-invoked` fixture: added a `skill__<name>__<tool>` execution so the `ToolSource::Skill` source-label rendering branch is actually exercised by snapshot tests; Source column now shows `skill/synthetic` (M1.5 audit #7).

### Docs

#### F1.7 (session model metrics + Models view)

- `docs/adapters.md` — new "Optional: `Event::payload_model_metrics` (F1.7)" subsection documenting the recommended-but-optional impl for adapters wishing to enable rich Models view UX. Silent "no model usage data" empty-state fallback otherwise. Per ADR-0012 D-4 + D-7.
- Workspace `README.md` — TUI section bumped from "Three views" to "Four views"; added ModelsView (`4`) paragraph + extended key-bindings line to include `4`. (Previous F1.7 tasks shipped same-commit L2 README updates for `crates/agentprof-{core,adapters,tui}/README.md`; this commit fills the L1 workspace-README gap.)

#### F1 (TurnDetailView + args plumbing)

- `docs/features/privacy.md` §8 (new): document the no-redaction-in-v1
  posture for tool arguments (passthrough from adapter; not surfaced
  in any export format in F1). Per ADR-0011 D-13.
- `docs/adapters.md` (new "Optional `Event` overrides (F1)" section):
  document `payload_tool_requests` + `tool_call_id` as recommended-
  but-optional methods for adapters wishing to enable rich
  TurnDetailView UX. Per ADR-0011 D-2 + D-3.

#### M1.6.4 follow-up wave (2026-06-03)

Post-merge propagation + naming-clarification entries for the 8 cleanup
commits between M1.6.4 merge (`8abc590`) and `766b8f0`:

- **`d87adec` docs(m1.6.4): post-merge audit** — propagated tracing references
  into the less-trafficked L2 / L3 docs (`docs/adapters.md`,
  `docs/features/*.md`, `docs/internals/*.md`, `CONTRIBUTING.md`).
- **`95fd059` docs(arch)** — clarified `docs/architecture.md §3` no-cross-crate-deps
  rule excludes dev-dependencies (rationale: `agentprof-tui/tests/views.rs`
  legitimately dev-deps `agentprof-adapters` for fixture-driven snapshot
  tests).
- **`83d2ed0`-companion `docs/architecture.md` follow-up (this `docs(sync)` 2026-06-03)** —
  corrected the §8 `AGENTPROF_LOG_FULL_PATHS` parenthetical from
  the stale "仅影响 cli 层 emission" wording (pre-`83d2ed0`) to "系统级
  opt-out at all 4 span layers via `hash_path` env-var check"
  (post-`83d2ed0` reality).
- **Naming clarification (2026-06-03)**: commit
  `4301125 chore(m1.6.5): cleanup batch 1` uses an `m1.6.5` token in its
  subject line, but the work belongs to the **M1.6.4 follow-up wave** —
  NOT the M1.6.5 milestone, which remains reserved in
  `tasks/ROADMAP.md §6.1 L-4` for **MCP server waste analysis**
  (deferred to 0.2.0 per `docs/plan.md §8`). See `tasks/ROADMAP.md §9`
  change-log entry for 2026-06-03 v1.4.

#### Roadmap / progress sync (`docs(sync)` — 2026-05-30)

After 4 merged M1.4 followups (audit / turn-metadata / mode-vocab /
post-output-audit), several entry-point docs were misleading new readers —
`tasks/ROADMAP.md` still said "M1.2–M1.7 ❌ 未开始" and
`tasks/001-mvp-agent-token-profiler.md` had ❌ status lines for milestones
that had already shipped. This commit synchronises the docs to reality.

**docs touched** (no code change):

- `tasks/ROADMAP.md` — header (current commit / phase status), §2.2 当前位置,
  §2.3 仪表盘 (4/7 = 57%), §3.1 task table, §4.1 + §4.2 dependency graphs
  (M1.2–M1.4 now ✅, Copilot adapter no longer in Phase 3).
- `tasks/001-mvp-agent-token-profiler.md` — header status, §4 FR completion
  table, **M1.2 / M1.3 / M1.4 状态行** rewritten with merge-commit citations
  and pivot notes, §11 M3.2 CopilotAdapter entry removed (it was already
  delivered in M1.2; Phase 3 now lists only Claude / Codex / Gemini).
- `docs/plan.md` §6 + §8 — pivot note added explaining events-first
  divergence from original Phase 0/1 plan; §8 next-step now points to
  M1.5 (TUI) instead of "write Phase 0 prototype".
- `docs/architecture.md` — `AnalysisReport` struct definition updated to
  M1.4 shape (`parse_warnings`, `is_user_blocking`-bearing rollup rows),
  `analyze()` signature corrected (`&[ParseWarning]` third arg),
  `Mode` vocabulary updated (`Interactive / Plan / Autopilot / Unknown`),
  `DeriveWarning` count updated (4 → 5), `USER_BLOCKING_TOOLS` const +
  user-blocking split + post-output-audit referenced.
- `crates/agentprof-core/README.md` — `Event` trait now 8 methods (4 required + 4 default payload-*; was 4 required-only),
  `analyze()` signature corrected, `ORPHAN_TOOL_SENTINEL` /
  `USER_BLOCKING_TOOLS` / `is_user_blocking` / `parse_warnings` /
  `parent_tool_call_id` / Mode vocabulary documented; quick-start sample
  updated to demonstrate parse-warning + user-blocking inspection.
- `crates/agentprof-adapters/README.md` — `CopilotEvent` notes 4
  payload-* trait overrides; new section "Copilot CLI 1.0.x schema notes"
  documents the three `Option<String>` parser-compat fields and the
  fixture that locks them in. Phase classification corrected.
- `crates/agentprof-cli/README.md` — M1.4 status section rewritten as a
  5-row merge table; markdown output structure documented end-to-end
  (Session block with Parse warnings line, User-blocking tools split,
  Warnings two-stage breakdown); `askama` removed from dependency list
  (renderer is hand-rolled string-building since M1.4 audit followups).
- `README.md` (root) — sample markdown output updated: `Mode = auto` →
  `interactive`; `- Parse warnings: N` line added to Session block;
  `## User-blocking tools` section added with realistic `ask_user` row;
  `PreToolUse` hook example renamed to `postToolUse` (real Copilot CLI
  vocabulary).

No CHANGELOG entry was created for ADR-0005 §6 itself — that was already
shipped in the previous commit's CHANGELOG section under "Post-output
audit fixes".

### Fixed

#### Post-output audit fixes (`fix/post-output-audit`)

Closes the actionable findings from the 2026-05-29 audit of `agentprof analyze`
output against a real live Copilot CLI 1.0.54 session (11 806 lines). Three
classes of fix; one branch; documented in
[ADR-0005 §6](docs/internals/adr-0005-analyzer-and-payload-name.md#update-6-post-output-audit-fixes-parse-warning-visibility-schema-mismatches-user-blocking-split).

**adapters — schema-mismatch parser drops (~17 % event loss):**
- Real Copilot CLI 1.0.x emits multiple wire shapes for some events. Three
  payload structs required string fields that aren't actually universally
  present, causing serde to silently drop matching events with
  `"missing field X"` warnings.
  - `HookInput.source: String → Option<String>` — `postToolUse` hooks
    carry no `source` (100 % of postToolUse hooks were dropping; symptom
    was `synthesized = 100 %, total = 0ms` for the entire hook in Tool
    Rank).
  - `UserMessageData.source: String → Option<String>` — many CLI-typed
    prompts omit `source` (46 % of user.message events were dropping).
  - `AssistantMessageData.turn_id: String → Option<String>` — subagent-
    spawned messages (via `subagent.started`) carry `parentToolCallId`
    instead of `turnId` (71 % of assistant.message events were dropping,
    losing all subagent token usage).
- `AssistantMessageData` also gains a new `parent_tool_call_id:
  Option<String>` field for subagent visibility.
- New fixture `crates/agentprof-adapters/tests/fixtures/copilot/with-post-tool-use-hooks/`
  (10 events) locks all three schema variants in episode + analyzer
  snapshots. Real-session drop rate verified to go from 17 % → 0 %.

**core — parse warnings now user-visible:**
- `AnalysisReport` gains `parse_warnings: Vec<ParseWarning>` field
  (additive, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  keeps old empty reports byte-identical and old JSON deserializable).
- `analyze()` signature widens to `analyze(&Episodes, &SessionMeta,
  &[ParseWarning]) -> AnalysisReport`. Callers pass `raw.parse_warnings`;
  pure unit tests pass `&[]`. **BREAKING for any external code calling
  `analyze()` directly** (no such callers exist outside the workspace yet).
- `ParseWarning` gains `PartialEq + Eq` (needed for round-trip + test
  assertions; was an earlier oversight).
- Locked by new `analyzer::tests::analyze_carries_parse_warnings_through`
  and extended `analysis_report_json_round_trip_is_lossless`.

**cli — markdown renderer surfaces parse warnings + splits user-blocking tools:**
- Session header gains `- Parse warnings: N` line beside `- Derive warnings: N`.
- Warnings section adds a parse-stage breakdown (Json / Io / OutOfOrder
  counts) before the existing derive-stage breakdown.
- `ToolRankRow` gains `is_user_blocking: bool` (additive,
  `#[serde(default)]`). New `pub const USER_BLOCKING_TOOLS: &[&str] =
  &["ask_user"]` in `agentprof_core::analyzer::tool_rank` is the single
  source of truth.
- Markdown renderer's `write_tool_rank` now partitions rows: work tools
  render in `## Tool Rank (by total duration)` as before; user-blocking
  tools render in a new `## User-blocking tools (wall-clock includes
  user think time)` section. JSON contract is additive (still a flat
  `tool_rank` vec; each row carries the new bool).
- Real-session effect: `task` (4.85h, 136 calls) and `bash` (57m,
  1641 calls) now headline the work-tool ranking; `ask_user` (63h,
  61 calls, mostly user think time) gets its own visually-distinct
  section instead of dominating the chart.

**docs — privacy considerations (documentation-only):**
- New `docs/features/privacy.md` (L2 cross-crate feature doc) documents the PII /
  SII fields in `AnalysisReport` (Unix `cwd`, `branch`, model internal
  names, ~800 turn UUIDs per session) with a tier table + manual
  `sed`/`jq` redaction cheat sheets for both markdown and JSON outputs.
  Planned `--redact` / `--anonymize` CLI flags are scoped for M1.5+; no
  code change in this branch.

#### M1.4 audit followups (`fix/m1.4-audit-followups`)

Closes the actionable findings from the 4-part M1.4 audit. All 10 fixes
land in 10 commits on a single branch.

**core — data correctness:**
- `derive_episodes` no longer emits per-event UUIDs as `ToolEpisode`
  keys for orphan `tool.execution_complete` events (audit-a2-orphan-
  tool-uuid-key). All orphan completes now aggregate under the new
  `ORPHAN_TOOL_SENTINEL = "<orphan>"` constant (exported from
  `agentprof_core::episode`). Per-call accountability preserved via
  existing `DeriveWarning::SynthesizedStart` warnings carrying the
  original event id. Before this fix, `tool_rank` output was polluted
  with one fake "tool" per orphan event, each labeled with an opaque
  UUID and call_count=1. Snapshot updates: `orphan-events` fixture
  re-accepted in both episode + analyzer layers.

**core — defensive instrumentation:**
- New `DeriveWarning::PayloadNameMissing { kind, event_id }` variant
  emitted whenever `Event::payload_name()` returns `None` for an
  event whose kind indicates it SHOULD have a name (audit-a4-payload-
  name-silent-failure / design D1). Closes the silent-failure risk
  for upcoming Claude (Phase 2) and Codex (Phase 3) adapter authors:
  if they forget to override `payload_name` for `ToolExecStart` /
  `HookStart` / `HookEnd` / `SkillInvoked`, downstream consumers see
  a warning instead of silently degrading to one episode per event.
  Markdown renderer's `## Warnings` section gains a `PayloadNameMissing:
  N` counter. `CopilotEvent` correctly overrides all 5 name-bearing
  variants, so existing snapshots are unaffected.

**core — round-trip contract:**
- `SessionMeta` and `AnalysisReport` now derive `PartialEq`
  (audit-a4-analysisreport-round-trip-test). New unit test
  `analysis_report_json_round_trip_is_lossless` locks
  `serde_json::to_string{,_pretty}` → `from_str` equality. Closes
  spec FR-2.12 from "partial" to "fully covered".

**cli — UX polish:**
- `resolve_session_by_path` no longer double-appends `events.jsonl`
  when given a non-existent `.jsonl`-named file. Error reads
  `events.jsonl not found at /x/events.jsonl` (was
  `events.jsonl not found at /x/events.jsonl/events.jsonl (and ...)`).
  Closes `t10-path-error-msg`.
- `looks_like_uuid` now validates ASCII hex digits + dash positions
  (8/13/18/23), not just length + dash count (audit-a3-uuid-typo-
  dumps-sessions). Previously a typo like `00000000-...-0g` passed
  the heuristic, fell through to `discover_sessions`, and the error
  dumped real session UUIDs to stderr — mild info-leak risk on
  shared terminals / CI logs. +5 unit tests cover canonical
  accept (lowercase/uppercase), wrong-length reject, dash-position
  reject, non-hex reject, and integration via `SessionSelector::
  from_str`.
- `--agent claude` / `--agent codex` now returns
  `Claude adapter not yet implemented (M1.4 ships copilot only;
  claude and codex are on the M1.5+ roadmap — see docs/plan.md)`
  instead of the cryptic `no adapter wired for agent Claude`
  (audit-a3-claude-codex-unfriendly-error).
- `--export json` output gains a trailing newline so shell prompts
  don't stick to the closing `}` and file output is POSIX-compliant
  (audit-a4-json-no-trailing-newline).

**cli — markdown table safety:**
- `md::render` now escapes `|` (→ `\|`) and newlines (→ `<br>`) in
  all user-controlled cell content via new `md_cell_escape(s: &str)
  -> Cow<str>` helper (audit-a3-md-pipe-escape). Affected cells:
  `turn_id`, `model`, `cwd`, `branch`, tool/hook `name`, `source`
  (via Debug-then-escape), `fmt_status(Aborted(reason))`, and
  `fmt_mode(Mode::Unknown(s))`. Returns `Cow::Borrowed` for
  safe inputs (no allocation in the common case). +6 unit tests
  cover the escape behavior + boundary cases (mixed pipes &
  newlines, `Aborted(user|cancel)`, `Mode::Unknown("pipe|in|mode")`).

**cli — coverage gap closures:**
- New integration test
  `analyze_unparseable_session_exits_with_data_error` synthesizes
  an inline tempfile events.jsonl with no `session.start`, asserts
  exit 2 + `data error` in stderr. Closes spec FR-3.11 (audit-a4-
  corrupt-exit-2-test-missing).
- New integration test
  `analyze_output_to_unwritable_path_exits_with_output_error` —
  first E2E exit-3 test.
- New integration test
  `analyze_unsupported_agent_exits_with_friendly_message` — regression
  guard for the `--agent claude` UX fix; will fail (intentionally)
  when Claude adapter ships in Phase 2 as a "review me" signal.
- New unit test `exit_kind_downcast_survives_extra_context_layers`
  defends against the M1.4 audit design observation D5 (future
  refactors adding `.context(...)` layers must not hide `ExitKind`
  from `main::classify_error`).

**docs:**
- ADR-0005 D-1 table fixed: `ToolExecComplete` split onto its own
  row with `None (stack pop preserves name)` rationale (was
  incorrectly listed alongside `ToolExecStart` as
  `payload.tool_name`). Also corrected `HookStart`/`HookEnd` →
  `hook_type` (was `hook_name`) and `SkillInvoked` → `name` (was
  `skill_name`) to match what `CopilotEvent::payload_name` actually
  reads. Closes audit-a1-adr-0005-d1-table-stale.
- ADR-0005 gains "Update §1: Orphan tool aggregation via sentinel"
  and "Update §2: PayloadNameMissing warning addition" sections
  documenting the M1.4 audit decisions (kept ADR Status as Accepted
  since these are additive refinements, not reversals).
- `analyzer/tool_rank::percentile` rustdoc clarifies the nearest-rank
  algorithm + even-sample upper-midpoint behavior; `ToolRankRow.
  p50_duration` / `HookRankRow.p50_duration` field docs changed from
  "Median per-call duration" to "Approximate median (nearest-rank
  percentile)" (audit-a2-percentile-doc-says-median).

**chore:**
- Removed unused `askama` dependency from `agentprof-cli` and the
  workspace `[workspace.dependencies]` table (audit-a4-askama-
  unused-dep). The markdown renderer is hand-written; carrying
  the dep was supply-chain noise + a phantom signal that templates
  were in use.

**Tests:** 196 → 215 (+19 across all the new unit + integration
tests). All gates clean (fmt / clippy `-D warnings` / full workspace
tests / `cargo doc -Dwarnings`).

**Out-of-scope (still tracked in m14_followups SQL table):**
- `classifier-zip-fix` (xtask audit tool; P2-optional)
- `negative-duration-span` (non-monotonic-timestamp edge; P2-optional)
- `tooltelemetry-restricted-props-skip-if` (small serde polish;
  P2-optional)
- `skill-call-count-fixture` (fixture reshape, not a bug; P3-defer)

#### Turn metadata extraction (`feat/turn-metadata-extraction`)

Discovered while validating the M1.4 audit fixes by running `agentprof analyze` against the `minimal` fixture and a real local Copilot session. The Markdown report's **Model / Mode / Out-Tokens** columns were all `—` for every turn, despite the wire data carrying these fields (`AssistantMessageData.model`, `AssistantMessageData.output_tokens`, `ModeChangeData.new_mode`). Root cause: `derive_episodes` never read these payload fields — the existing `Turn` struct fields were initialized to `None` by `Turn::new()` and never written to. Spec FR-2.2 required only "fields exist and correctly typed", which the M1.4 audit verified as compliant — the audit had no obligation to check "fields populated with real data". This was a real audit / spec blind spot that surfaced immediately on first user inspection.

**`agentprof-core`:**
- `Event` trait extended with 3 new methods, all with default `None` (mirroring ADR-0005 D-1): `payload_model() -> Option<&str>`, `payload_output_tokens() -> Option<u32>`, `payload_mode() -> Option<&str>`.
- `DeriveState` gains a `current_mode: Option<Mode>` field tracking the active session mode across the event stream.
- New `on_assistant_message` handler populates `Turn.model` (last-wins across messages in a turn) and `Turn.output_tokens` (saturating sum). M1.5 ROI computations consume both.
- `on_mode_event` now reads `ev.payload_mode()` instead of pushing a hard-coded `Mode::Unknown("changed")` segment — the M1.3 PLACEHOLDER for "Task 10b will read actual mode value" is now resolved.
- `on_turn_start` captures `current_mode.clone_from(&...)` into `turn.mode`. Mid-turn mode changes don't retroactively update the current turn (matches user intuition: "this turn was started in X mode").
- Dispatch table gains `EventKind::AssistantMessage => state.on_assistant_message(ev)`.

**`agentprof-adapters`:**
- `CopilotEvent` overrides the 3 new trait methods for `AssistantMessage` and `ModeChanged` variants. `ModelChange` deliberately returns `None` for both `payload_model` and `payload_mode` (it announces a model switch, not a per-message model or a mode change).

**Snapshots:**
- 14 snapshots re-accepted (7 `episode_derive__*.snap` + 7 `analyzer_on_fixtures__*.snap`). `minimal` fixture now shows `model: "gpt-5-mini"`, `output_tokens: 10` (was both null). `with-mode-transitions` fixture shows populated `mode` values (`{"Unknown": "plan"}`, `{"Unknown": "autopilot"}`, etc. — wire vocabulary differs from `Mode::{Ask,Auto,Expert}` known set, so they correctly fall to the forward-compat `Unknown` variant). Fixtures without `assistant.message` events (cross-turn-tool, orphan-events) keep `model`/`output_tokens` as null — confirms we only populate fields with source data.

**Tests:**
- 3 unit tests for trait default `None` (adapter.rs)
- 5 unit tests for CopilotEvent overrides + ModelChange-vs-ModeChange disambiguation (event.rs)
- 4 unit tests for `derive.rs` aggregation semantics: single-message attribution, sum + last-wins, mode-mid-turn semantics, defensive no-message
- 1 CLI integration test asserting `minimal` fixture's `turn_summary[0].output_tokens == 10` end-to-end

**Out of scope (M1.5 deliverables):**
- Cost / ROI computation logic (price tables, per-model tokenizers, `--with-cost` flag)
- `agentprof aggregate` cross-session rollups
- This commit only provides the **inputs** M1.5 will consume.

**Test count delta:** 214 → 230 (+16: 3 + 5 + 4 + 1 unit/integration + 3 doctests, plus snapshot diffs which don't change count).

#### Mode vocabulary alignment (`fix/mode-vocabulary-alignment`)

Discovered immediately after the turn-metadata-extraction merge by running `agentprof analyze --section turn-summary` against the live local Copilot session and noticing every turn still showed `Mode: —`. Investigation via `find ~/.copilot/session-state -name events.jsonl | xargs grep '"type":"session.mode_changed"'` revealed the real Copilot CLI 1.0.54 wire vocabulary is `interactive` / `plan` / `autopilot` (73 events across 190 sessions; 0 of `ask` / `auto` / `expert`). The previous `Mode::{Ask, Auto, Expert}` enum variants were a fabricated vocabulary — likely from an early documentation guess — that never matched any real wire data.

**`agentprof-core/episode/mode_segment.rs`:**
- `Mode` enum variants renamed to match real wire vocabulary: `{Ask, Auto, Expert}` → `{Interactive, Plan, Autopilot}`. Each variant now has a doc comment with frequency from the 73-event sample (Plan 60, Interactive 52, Autopilot 34) and semantic context.
- `Mode::from_wire` rewired: `"interactive" → Interactive`, `"plan" → Plan`, `"autopilot" → Autopilot`, anything else → `Unknown(s)` for forward-compat.
- Updated unit tests assert the new vocabulary; one test explicitly verifies the OLD `ask`/`auto`/`default` strings round-trip through `Unknown` (defense against accidental reintroduction).

**`agentprof-core/episode/derive.rs`:**
- `DeriveState::new` now seeds the initial `ModeSegment` with `Mode::Interactive` (replacing the M1.3 placeholder `Mode::Unknown("default")`) AND initializes `current_mode: Some(Mode::Interactive)` (was `None`). Rationale: data analysis showed every `previousMode → newMode` transition opens with `previousMode = 'interactive'`, confirming Interactive is Copilot CLI's implicit default; sessions without explicit `mode_changed` events run entirely in Interactive.
- Updated `mode_change_attributes_to_next_turn_not_current` test to use real `interactive` / `autopilot` strings and assert against `Mode::Interactive` / `Mode::Autopilot`.

**`agentprof-cli/cmd/format/md.rs`:**
- `fmt_mode` now returns real strings: `interactive` / `plan` / `autopilot` (was `ask` / `auto` / `expert`).
- Updated `fmt_mode_handles_each_variant` test.

**User-visible impact**: every turn in every real Copilot session now shows the actual mode (typically `interactive` for the common case) instead of `—`. Sessions with mode transitions correctly show `plan` and `autopilot` at the right turn boundaries. This restores meaningful Mode column data.

**Snapshots:** 21 re-accepted (10 episode_derive + 10 analyzer_on_fixtures + 1 CLI insta md snapshot). Mode values in turn rows changed from `{"Unknown": "plan"}` → `"Plan"` (and similar), AND from `null` → `"Interactive"` for fixtures without explicit mode events. Initial `mode_segments[0]` value changed from `{"Unknown": "default"}` → `"Interactive"` across all snapshots.

**Test count delta:** 230 → 230 (renames + test rewires balance to net zero new tests, but +1 stronger assertion in `mode_from_wire_unknown_preserved` covering 3 invalid strings).

### Added

#### M1.2 — Copilot CLI adapter (`feat/m1.2-copilot-adapter`)

Reference: `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md`, ADRs 0001 / 0002 / 0003, plan `docs/superpowers/plans/2026-05-26-m1.2-copilot-adapter.md`.

**core — adapter contract layer** (`agentprof-core::adapter`, `::model`, `::error`):
- `Adapter` trait: `agent_kind` / `default_session_root` / `discover_sessions` / `load_session`, with associated `Event` type.
- `Event` trait with four methods (`id`, `kind`, `timestamp`, `parent_id`) so analyzer layers can treat per-adapter event enums uniformly.
- `EventKind` enum (19 variants: 18 named + `Unknown`) and `AgentKind` enum (`Copilot` / `Claude` / `Codex`, `#[non_exhaustive]`).
- `SessionRef` struct + `SessionRef::new` constructor (path, agent, id, modified-at, size, is-live).
- `RawSession<E>` generic + `SessionMeta` + `SessionMeta::new` + `RawSession::new` — the unified shape every adapter produces.
- `ToolSource` enum + `ToolSource::infer` classifier (`Builtin` / `Mcp { server }` / `Skill { plugin }` / `User` / `Unknown`).
- Error types: `AdapterError` (struct-variant `RootNotFound { path }`, `Io { path, source }`, `MissingSessionStart`, `UnsupportedVersion`, `Parse`), `CoreError`, and the `ParseWarning` taxonomy (`Json`, `OutOfOrder`, `MissingField`, `UnknownVariant`).

**adapters — Copilot CLI implementation** (`agentprof-adapters::copilot`):
- `CopilotEvent` enum: 18 named variants tagged by `type` field (covering `session.{start,info,mode_changed,model_change,plan_changed,shutdown}`, `user.message`, `assistant.{turn_start,message,turn_end}`, `system.message`, `tool.{execution_start,execution_complete,user_requested}`, `hook.{start,end}`, `skill.invoked`, `abort`) + `Unknown` (`#[serde(other)]`) for forward compatibility.
- `WithEnvelope<D>` generic envelope (`id`, `timestamp`, `parent_id`, `ephemeral`, `data`) plus ~25 `#[non_exhaustive]` payload structs (`SessionStartData`, `AssistantMessageData`, `ToolExecData`, `HookStartData`, `SkillData`, `AbortData`, …).
- `impl Event for CopilotEvent` — full dispatch including `EventKind::Unknown` for the catch-all variant.
- `copilot::parser::parse_events_jsonl(path, is_live)` — line-by-line streaming parser producing `RawSession<CopilotEvent>`. Per-line JSON failures accumulate as `ParseWarning::Json`; non-monotonic timestamps emit `ParseWarning::OutOfOrder`; the trailing line of a live session (`is_live=true`) is silently skipped when `looks_like_incomplete_json` detects a partial write; missing `session.start` returns `AdapterError::MissingSessionStart`.
- `copilot::parser::looks_like_incomplete_json` — brace-depth heuristic respecting string literals and escapes, used for live-session tail tolerance.
- `copilot::paths::default_session_root()` — XDG-aware resolver returning `$HOME/.copilot/session-state`.
- `copilot::paths::discover_sessions(root)` — walks `<root>/<uuid>/events.jsonl`, returns `Vec<SessionRef>` sorted by mtime descending, marks `is_live` when an `inuse.<pid>.lock` sibling file exists. Silently skips individual malformed subdirectories.
- `copilot::adapter::CopilotAdapter` — zero-sized struct implementing `Adapter`, delegating to the `parser` and `paths` modules.
- `registry::adapter_for(kind)` and `registry::supported_agents()` — agent-kind dispatch.

**adapters — test fixtures** (`agentprof-adapters/tests/fixtures/copilot/`):
- 9 synthetic JSONL fixtures per ADR-0003 (100% synthetic, stable UUIDs, `/tmp/agentprof-fixture/<slug>` paths):
  - `minimal/` (canonical 8-event happy path)
  - `corrupt/` (intentionally-broken JSON for `ParseWarning::Json` coverage)
  - `builtin-tools-only/` (5 builtin tool invocations)
  - `with-mcp-calls/` (`mcp__<server>__<tool>` flow)
  - `with-skill-invoked/` (`skill.invoked` lifecycle)
  - `with-hooks-heavy/` (72 events, 30 hook start/end pairs across phases)
  - `with-aborts/` (3 user-initiated aborts at distinct lifecycle points)
  - `with-mode-transitions/` (4 mode segments: `ask` → `auto` → `expert` → `ask`)
  - `live-truncated/` (3 valid events + truncated trailing line + `inuse.778482.lock`)
- Per-fixture `README.md` explaining the scenario.
- `copilot_event_parse` (23 round-trip tests, one per variant + Unknown), `copilot_fixture_load` (9 fixture-level tests with `insta` snapshots), `copilot_paths` (6 discovery tests).
- `copilot_smoke` integration test scaffold (`#[ignore]` by default; runs against `$AGENTPROF_LOCAL_FIXTURES_DIR` with `--include-ignored`; asserts zero `CopilotEvent::Unknown` against real local data, catching schema drift between Copilot CLI versions).

**docs:**
- ADR-0001 (events-first product pivot), ADR-0002 (Copilot event schema), ADR-0003 (synthetic-only fixture strategy).
- `crates/agentprof-adapters/README.md` rewritten per the L2 template (in-architecture context, public-interface index, modules table, supported-agent matrix, local-smoke instructions, ADR pointers).
- `docs/adapters.md` rewritten as the contribution guide (trait contract, new-adapter checklist, fixture rules, smoke-test pattern).

**chore:**
- `.gitignore` — `/local-fixtures/` and `/smoke-data/` excluded to prevent accidental commit of developer-local session data.

#### M1.3 Phase A+B — Copilot schema calibration (`feat/m1.3-episode-and-schema-fix`)

Driven by a forward-looking audit tool plus real-data analysis.

**xtask — `cargo xtask schema-audit`** (Phase A):
- New developer tool that scans `~/.copilot/session-state/` (or
  `--root`), classifies `CopilotEvent::Unknown` by wire `type` (with
  candidate Rust variant names), summarizes `ParseWarning` distribution,
  and reports `start`/`end` pair balance with severity thresholds.
- Submodules: `scanner.rs` (dual-load raw + typed), `classifier.rs`
  (group + redact + balance compute), `report.rs` (markdown).
- CLI: `--root`, `--sample-limit`, `--output`, `--sessions`.
- Documented in `xtask/README.md` with 5 invocation patterns.
- Integration test ensures all 4 report sections emit on fixture root.
- Re-runnable after every Copilot CLI upgrade.

**adapters — 10 new `CopilotEvent` variants** (Phase B, audit-driven):
- `Subagent{Started,Completed,Failed}`, `SystemNotification`,
  `Session{Warning,Resume,CompactionStart,CompactionComplete}`,
  `Permission{Requested,Completed}`.
- `WithEnvelope` gained `agent_id: Option<String>` (camelCase: `agentId`).

**adapters — `tool.execution_*` payload-shape expansion** (Phase B):
- `ToolResultData` extended with `interaction_id`, `model`,
  `result: Option<ToolResult>`, `tool_telemetry: Option<ToolTelemetry>`,
  all Optional for cross-version compatibility.
- New helper structs: `ToolResult { content, detailed_content }`,
  `ToolTelemetry { metrics, properties, restricted_properties }`.

**adapters — testing:**
- 15 new round-trip tests in `copilot_event_parse.rs` (23 → 38).

**docs:**
- ADR-0002 marked `Updated 2026-05-27`, with detailed Schema Updates section.
- 18 → 28 named variants documented.

**Audit impact** (on developer's 187-session / 117K-event data):
- `CopilotEvent::Unknown`: 3411 → 278 (−92%)
- `ParseWarning::Json`: 58339 → 38176 (−35%)

#### M1.3 Phase C — Episode aggregation (`feat/m1.3-episode-and-schema-fix`)

**core — new `agentprof_core::episode` module:**
- `Turn` + `TurnStatus` (`Open` / `Completed` / `Aborted(AbortInfo)`) +
  `Span` (with `instant()` for orphan synthesis) + `AbortInfo`.
- `ToolEpisode` + `ToolCall` + `ToolCallStatus` (`Success` / `Failure { message }` /
  `OrphanSynthesizedStart` / `OpenAtEndOfSession`).
- `HookEpisode` + `HookCall` (with `synthesized_start` flag).
- `SkillEpisode` + `SkillInvocation` (with `triggered_tools` window).
- `ModeSegment` + `Mode` (`Ask` / `Auto` / `Expert` / `Unknown(String)`).
- `Episodes` container (7 fields, snapshot-stable `BTreeMap` ordering).
- `DeriveWarning` 4-variant data-quality enum.
- `derive_episodes<E: Event>(events, meta) -> Episodes`: pure, total,
  single-pass aggregation function. Algorithm in ADR-0004.
- `CallRef { name: String, index: usize }` (added pre-merge): self-describing
  replacement for bare `Vec<usize>` indices in `Turn.{tool,hook,skill}_calls`
  and `SkillInvocation.triggered_tools`, so back-references can be
  dereferenced as `episodes.tools[r.name].calls[r.index]` without external
  context. Same commit also fixes the previous `triggered_tools`
  miscalculation where `tool_idx` was the cumulative `calls.len()` sum
  across all tool episodes; attribution now happens in `commit_tool_call`
  where the tool's real name and per-name index are in scope. ADR-0004
  updated with a CallRef section.

**adapters — testing:**
- New synthetic fixture `tests/fixtures/copilot/orphan-events/`
  exercising orphan-end synthesis + abort-without-open paths.
- `tests/episode_derive.rs` integration tests with 9 insta snapshots
  (one per fixture) + 1 no-panic test. Placed under agentprof-adapters
  to avoid dev-dep cycle.
- `orphan-events` added to `every_fixture_line_parses_as_copilot_event`.

**docs:**
- `crates/agentprof-core/README.md` (new/rewritten): full L2 README.
- `docs/architecture.md` §5.1: Episode types section added; §14.4 ADR list
  updated with ADR-0004.
- `docs/internals/adr-0004-episode-derivation.md`: cross-checked against
  implementation; no semantic changes.

**Known limitation (Event trait):**
Tool/hook/skill names in `Episodes` use `event.id()` as placeholder
because the Event trait doesn't expose payload fields. M1.4 may extend
Event with `payload_name() -> Option<&str>`. Snapshots reflect this.

#### M1.4 — CLI + analyzer rollups (`feat/m1.4-cli-and-analyzer`)

Reference: spec `docs/superpowers/specs/2026-05-29-m1.4-cli-and-analyzer-design.md`, ADR-0005, plan `docs/superpowers/plans/2026-05-29-m1.4-cli-and-analyzer.md`.

**core — Event trait extension + P0 fix (Phase A):**
- `Event::payload_name() -> Option<&str>` (default `None`) added to the trait; `CopilotEvent` overrides for `tool.execution_start` / `tool.user_requested` (→ `data.toolName`), `hook.start` / `hook.end` (→ `data.hookType`), `skill.invoked` (→ `data.name`). Other variants (incl. `tool.execution_complete`) return `None`.
- `derive_episodes` now uses `payload_name()` (with `event.id()` safety-net fallback) so tools/hooks/skills group by their real wire names ('bash', 'PreToolUse', 'brainstorming') instead of opaque event UUIDs.
- `commit_tool_call` / `commit_hook_call` now attribute back-references to the **start-time** Turn (via `call.turn_id` + `Vec::rposition` lookup), not the end-time `open_turn_idx`. Fixes `commit-call-turn-divergence` (P0 follow-up from M1.3 final review): for tool spans crossing a Turn boundary, `Turn.tool_calls` now matches `ToolCall.turn_id` (single source of truth restored).
- `cross-turn-tool` synthetic fixture (7 events; 'bash' starts in turn-A, completes in turn-B) locks the fix in via hand-verified snapshot.
- 6 M1.3 episode snapshots re-accepted with real payload names.

**core — analyzer module (Phase B):**
- New `agentprof_core::analyzer` module with `AnalysisReport` container + `analyze(&Episodes, &SessionMeta) -> AnalysisReport` bundler.
- `turn_summary(&Episodes) -> Vec<TurnSummaryRow>` — per-turn rollup (turn_id, started_at, duration, status, model, mode, output_tokens, tool/hook/skill call counts).
- `tool_rank(&Episodes) -> Vec<ToolRankRow>` — per-tool rollup with call/success/failure/orphan/user-requested counts and p50/p95/max durations; sorted by total_duration desc.
- `hook_rank(&Episodes) -> Vec<HookRankRow>` — per-hook rollup with success/failure/synthesized_start counts and p50/p95 durations.
- `tool_rank::percentile(&[Duration], f64) -> Duration` shared helper (nearest-rank algorithm).
- `duration_ms` / `duration_ms_opt` serde helpers for stable integer-ms JSON serialization (per ADR-0004 IMP-007 convention).
- New `analyzer_on_fixtures.rs` integration tests with 10 insta snapshots locking the full `load → derive → analyze` pipeline.

**cli — first real binary (Phase C):**
- `agentprof analyze` subcommand wired end-to-end: `--agent` (default copilot), `--session` (latest/previous/uuid/path; default latest), `--root`, `--export md|json` (default md), `--output`, `--section turn-summary,tool-rank,hook-rank` (default all).
- Structured `ExitKind` enum (UserError=1, DataError=2, OutputError=3) carried via `anyhow::Error::msg().context()`; `main.rs::classify_error` downcasts to pick the process exit code.
- Helpful error diagnostics: `'session UUID X not found under Y; first 5 available: a, b, c, d, e'`.
- Markdown renderer (`cmd/format/md.rs`): Session header + Turn Summary table + Tool Rank table + Hook Rank table + Warnings; durations rendered in friendly units (`500ms` / `2.50s` / `2.0m` / `2.00h`); sections filterable via `--section`.
- JSON renderer (`cmd/format/json.rs`): `serde_json::to_string_pretty(&AnalysisReport)`; stable shape with integer-ms Duration fields.
- `tracing` initialization gated by `AGENTPROF_LOG` env var; writes to stderr.
- 6 `assert_cmd` integration tests + 1 insta md snapshot (`cli__analyze_md__cross_turn_tool`).
- ADR-0005 D-2 fix confirmed at FOUR independent layers: derive unit test → episode snapshot → analyzer snapshot → CLI md/JSON snapshot+assertion.

**core — Cargo features:**
- New optional `clap-derive` feature on `agentprof-core` enabling `#[derive(clap::ValueEnum)]` on `AgentKind` via `cfg_attr` (lets `agentprof-cli` use AgentKind directly in clap-derive structs without `agentprof-core` taking a hard `clap` dep).
- `agentprof-cli` enables the feature on its `agentprof-core` dependency and adds `thiserror` for `ExitKind`.

**docs:**
- ADR-0005 (Accepted): Analyzer foundations + `Event::payload_name()` trait extension + start-time turn attribution + `AnalysisReport` placement in core (not cli) rationale.
- `docs/architecture.md` §7.2 analyzer rollups subsection added; §8 `analyze` block amended for M1.4 reality; §14 ADR list adds row for ADR-0005.
- `crates/agentprof-cli/README.md` updated to mark `analyze` as shipped (with Quick start examples); other subcommands kept as planned.
- `crates/agentprof-core/README.md` Public interface table gains `analyzer` row; Reference ADRs adds ADR-0005.
- Root `README.md` Status notice updated to "M1.4 shipped"; new Quick start section with runnable examples and sample output structure.

**Carried forward (not in M1.4 scope):**
- M1.3 P2 follow-ups remain tracked: `classifier-zip-fix`, `negative-duration-span`, `tooltelemetry-restricted-props-skip-if`.
- `with-skill-invoked` fixture's skill fires before turn_start, so `turn_summary[*].skill_call_count == 0` across all rows — derive behavior is correct (skills outside open turn aren't turn-attributed); fixture reshape deferred to P3.
- `analyze --session` Path-vs-Uuid: fixture dirs are named by purpose (not UUID), so `--session <dirname>` is rejected by `looks_like_uuid` heuristic; users (and integration tests) should use `--session <full-path-to-dir>` instead. Real `~/.copilot/session-state/<uuid>` dirs work as expected.
- `corrupt → exit 2` integration test: corrupt fixture's bad line produces parse-time warnings (not fatal); would need a fully-unparseable fixture. Defer to M1.5 polish.

#### M1.1 — pre-existing entries

- **Project roadmap entry-point** — `tasks/ROADMAP.md` (378 lines): the master document new contributors and AI agents should read first. Sections cover (1) document map across L1/L2/L3 + AI guides, (2) project phases timeline with current commit position, (3) task file index with status/release mapping, (4) milestone dependency graph (within MVP and across phases), (5) release cadence and SemVer rules, (6) how-to-use guide for 6 personas (newcomer / developer / feature author / releaser / reviewer / maintainer), (7) long-term vision and explicit "won't do" boundaries, plus self-update discipline at the bottom.
- 001 task file now back-links to `tasks/ROADMAP.md` in its authoritative-documents preamble.
- **MVP task file** — `tasks/001-mvp-agent-token-profiler.md` (1009 lines): full PRD + implementation plan covering Phase 0 + Phase 1. Format mirrors the reference `proteinCopilot/tasks/001-mvp-proteomics-search-platform.md`:
  - PRD sections §1–§9: Introduction / Goals / User Stories (US-1…US-7) / Functional Requirements (FR-1…FR-7) / Non-Goals (NG-1…NG-10) / Design / Technical / Success Metrics (SM-1…SM-10) / Open Questions (OQ-1…OQ-8).
  - §10 Implementation Milestones: M1.1 (skeleton ✅) → M1.7 (release v0.1.0). 7 main milestones broken into 46 Tasks → 222 Sub-tasks.
  - §11 Phase 2/3 outline: SQLite persistence, OTLP receiver, Codex/Copilot adapters, pricing auto-sync, v1.0.0 release (6 additional milestones).
  - Each milestone explicitly tied to the 9-stage skill pipeline (`.github/copilot-instructions.md` §5): which skill produces which artifact at each step.

### Changed
- **Pipeline 衔接性增强（§5 重写为三层结构）**：
  - 流程图现明确分为**主线**（Stage 0→1→2→3→4→7→8）、**横切层**（Stage 5/6）、**Pipeline 外**（writing-skills），避免之前画成串行的误导。
  - 新增 §5.5 「Stage 2 触发门槛」表格 + 判断口诀（*"半年后回头看会问『为什么这么做？』就写 ADR"*），区分 8 类场景。
  - 新增 §5.6 「横切层规则」：Stage 5 不打断主线、Stage 6 修完返回触发它的 stage（不跳到 Stage 7）。
  - 3 个原本"孤儿"的 skill 找到明确归属：
    - `dispatching-parallel-agents` → Stage 1（并行调研多源） + Stage 4（并行多模块影响面）
    - `using-git-worktrees` → Stage 3→4 之间的可选 env prep
    - `writing-skills` → 标明为 "Pipeline 之外的元能力"
  - §5.7 commit 粒度新增 Stage 6 规则（`fix:` 前缀 + 关联失败测试）。

### Added
- **Skill pipeline integration (corrected layout)** — five curated skills from `github/awesome-copilot` placed at `<repo>/.github/skills/<name>/SKILL.md` (project-level path per [GitHub Copilot CLI skills docs](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-skills)), plus two `.instructions.md` files at `.github/instructions/`. All checked into git and propagated by `git clone` — no global install step required:
  - `cli-mastery` (Stage 4)
  - `copilot-cli-quickstart` (Stage 4)
  - `github-release` (Stage 8)
  - `create-github-action-workflow-specification` (Stage 5)
  - `create-architectural-decision-record` (Stage 2)
  - Plus `.github/skills/README.md` documenting provenance, upstream sync command, license, and verification (`/skills list`, `/skills reload`).
- **Unified 9-stage pipeline** — `.github/copilot-instructions.md` §5 rewritten as a Boot → Discovery → Decision → Planning → Implementation → CI/Infra → Debugging → Completion → Release flowchart; covers every obra + project skill with stage, trigger, output, and exit criterion.
- `.github/copilot-instructions.md` §6 extended: §6.1/§6.2 expanded with the five new skills and the `Pipeline 阶段` column; new §6.6 "Stage 0 常驻 instructions" and §6.7 "Skill 来源说明" (obra/superpowers global vs `.github/skills/` per-repo).
- `docs/architecture.md` §14.7 rewritten to map all 19 skills to pipeline stages and document outputs; new §14.8 acknowledging the two always-on instruction files.
- Skills usage matrix integrated into both AI and architecture docs (🔴 MUST / 🟡 recommended / 🟢 optional tiers + anti-patterns).
- Workspace skeleton with five crates (`agentprof-core`, `agentprof-adapters`, `agentprof-storage`, `agentprof-tui`, `agentprof-cli`) and an `xtask` helper.
- Architecture authority document (`docs/architecture.md`, L1).
- AI-assistant guide (`.github/copilot-instructions.md`).
- Adapter contributor guide placeholder (`docs/adapters.md`, L2).
- L1/L2/L3 documentation system definition (see `docs/architecture.md` §14).
- Repository configuration: `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.editorconfig`, `.gitignore`, dual `LICENSE-*` files.

[Unreleased]: https://github.com/agentprof/agentprof/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/agentprof/agentprof/releases/tag/v0.1.0
