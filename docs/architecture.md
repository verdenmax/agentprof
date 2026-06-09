# agentprof —— 架构设计

> 状态：架构定稿（Phase 1+2+3 整体设计）
> 最后更新：2026-05-25
> 关联文档：[`plan.md`](./plan.md)（问题陈述 / 现状盘点 / 路线图）

本文档是 agentprof 项目的**长期架构档案**。所有 crate 边界、数据模型、CLI 协议、工程规范在此定稿；后续每个 feature 的实施计划另行写在 `docs/superpowers/specs/`。

---

## 1. 一句话定位

> **agentprof = 给 AI agent 的 perf flamegraph + ROI 报告器**。读 Claude / Codex / Copilot CLI 留下的 session 日志，把 context window 里 `system / tools_schema / history / user / tool_result / output` 各自占多少 token 算清楚，标出"加载了但从没被调用"的 tools，导出 TUI / Speedscope JSON / HTML 三种视图。

差异化：市面上同类工具（ccusage, tokscale, splitrail, claude-usage 等）都在做 "花了多少 token / 多少钱"，本项目做 "**花得值不值**"——schema 利用率 + Tool ROI 矩阵 + MCP server 浪费榜。

---

## 2. 技术栈

| 维度 | 选择 | 备注 |
|---|---|---|
| 语言 | **Rust 2021**, MSRV **1.78** | 跟随 tokscale/splitrail 主流，单 binary 用户体验 |
| 工程组织 | Cargo **workspace**, 5 个 lib/bin crate + 1 个 xtask | 边界清晰、编译可控 |
| 异步运行时 | `tokio`（仅在 storage / OTLP / Anthropic API 处用） | core/adapters/tui 不强依赖 async |
| TUI | `ratatui` + `crossterm` | 火焰图、ROI 表、聚合视图 |
| Tokenizer | `tiktoken-rs`（本地）+ Anthropic `count_tokens` API（可选） | 本地优先策略，离线可用 |
| 存储 | `rusqlite`（bundled） | 单文件 SQLite，XDG 路径 |
| Telemetry receiver | `opentelemetry-otlp` + `tonic`（feature gated） | Phase 2 启用 |
| HTML 渲染 | `askama`（编译期模板） + 内嵌 d3.js | 单文件 HTML 报告 |
| CLI 解析 | `clap` derive | env + 默认值整合 |
| 错误模型 | `thiserror`（lib） / `anyhow`（bin） | 严格分层 |
| 日志 | `tracing` + `tracing-subscriber` | `RUST_LOG` env 控制 |
| 测试 | 单元 + `assert_cmd` 集成 + `insta` snapshot + 可选 `proptest` | snapshot 用于 TUI/HTML |
| 许可 | **MIT OR Apache-2.0**（双协议，Rust 生态惯例） | |
| 发布 | `cargo-dist` 多平台 release + `cargo install agentprof` | tag push 触发 |

---

## 3. 系统分层

```
                         ┌───────────────────────────┐
   User runs CLI ───────▶│   agentprof-cli (binary)   │
                         └─────────────┬──────────────┘
                                       │ uses
       ┌───────────────────────────────┼───────────────────────────────┐
       ▼                               ▼                               ▼
┌──────────────┐           ┌──────────────────┐           ┌──────────────────┐
│ agentprof-   │           │ agentprof-tui    │           │ agentprof-       │
│  adapters    │           │ (ratatui views   │           │  storage         │
│  - claude    │           │  + flamegraph    │           │  - SQLite        │
│  - codex     │           │  + ROI table)    │           │  - OTLP receiver │
│  - copilot   │           └────────┬─────────┘           │   (feature:otlp) │
│  (impl       │                    │                     └────────┬─────────┘
│   Adapter    │                    │                              │
│   trait)     │                    │ renders                      │ persists
└──────┬───────┘                    │                              │
       │ produces                   │                              │
       ▼                            ▼                              ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                          agentprof-core                                  │
│   model::{Session, Turn, ToolDef, ToolCall, TokenBucket, RoiRow, ...}    │
│   tokenizer::{count_tokens(model, text), AnthropicApi (feature gated)}   │
│   analyzer::{compute_roi, schema_utilization, waste_estimate, ...}       │
│   export::{speedscope_json, markdown_report, html_report}                │
└──────────────────────────────────────────────────────────────────────────┘
```

### 3.1 依赖规则（无环）

```
agentprof-cli  ──▶  agentprof-tui
       │                │
       ├──────────────▶ agentprof-adapters ──▶ agentprof-core
       │                                          ▲
       └──▶ agentprof-storage ───────────────────┘
```

- `agentprof-core` 不依赖任何其他 workspace crate（叶子节点）
- `agentprof-adapters` / `agentprof-storage` / `agentprof-tui` 运行时只依赖 `core`（dev-dependencies 可包含其他 workspace crate 以便 snapshot / fixture-driven tests，例如 `agentprof-tui` dev-deps `agentprof-adapters` 用于 `tests/views.rs` 加载真实 Copilot fixture）
- `agentprof-cli` 是唯一的组装层；**不允许**任何 lib crate 依赖 `agentprof-cli`
- CI 用 `cargo deny` + 自定义 grep 校验依赖图

---

## 4. Crate 一览

| Crate | 类型 | 关键模块 | 关键外部依赖 |
|---|---|---|---|
| `agentprof-core` | lib | `model`, `tokenizer`, `analyzer` (+ `analyzer::aggregate` M1.6.2), `export` (M1.6.4: speedscope JSON + SVG flamegraph), `observability::pii::{hash_path, hash_short}` (M1.6.4 tracing), `error` | `serde`, `serde_json`, `tiktoken-rs`, `chrono`, `thiserror`, **`sha2`** (M1.6.4 tracing), `reqwest`(opt, feature `anthropic-api`) |
| `agentprof-adapters` | lib | `claude`, `codex`, `copilot`, `registry`, `discovery` | `serde_json`, `walkdir`, `globset` |
| `agentprof-storage` | lib | SQLite persistence layer with hybrid cache/store mode (M2.1 ✅): `config::{StorageConfig, StorageMode, PartialStorageConfig}` (XDG-aware path resolution per [ADR-0019](internals/adr-0019-hybrid-storage-mode.md)), `db::Db` (handle + embedded migrations under `migrations/001_initial.sql`; pragmas `journal_mode=WAL` / `synchronous=NORMAL` / `foreign_keys=ON`), `upsert::upsert_report(db, report, raw_path, ingested_at_secs)` (atomic 3-table write), `query::{query_sessions_since, load_session}` (read API), `datasource::SqliteDataSource` (impl of `agentprof_core::datasource::SessionDataSource`, see [ADR-0018](internals/adr-0018-session-datasource-trait.md)), `admin::{stats, prune_before, vacuum, export_session_json}` (backs the `agentprof db` family), `error::SqliteError` (thiserror); `otlp`(feature, planned M2.2) | `rusqlite`(bundled), `serde_json`, `chrono`, `directories`, `opentelemetry-otlp`(opt), `tonic`(opt), `tokio`(opt) |
| `agentprof-tui` | lib | `app` (with `AppRunner`) + `app::{terminal,event,state}`, `views::{flamegraph, roi, aggregate, models, turn_detail, format}`, `theme`, `error` — **shipped M1.5** ([`README`](../crates/agentprof-tui/README.md), [ADR-0006](internals/adr-0006-panic-safe-tui.md)); + `watch::{WatchRunner, WatchData, RefreshKind, ReloadError, AggSortKey}` + cross-session arm in `views::aggregate` + `Event::Refresh` — **shipped M1.6.3** ([ADR-0009](internals/adr-0009-watch-runner-and-notify.md)); + `views::turn_detail` (F1 Enter-to-open) + `views::models` (F1.7 Models view, key `4`, surfaces session-level per-model token totals — see [ADR-0012](internals/adr-0012-session-model-metrics-and-models-view.md)) — post-MVP UX waves F1, F1.5–F1.19 layered on the M1.5 base | `ratatui 0.29`, `crossterm 0.28` |
| `agentprof-cli` | bin (`agentprof`) | `cmd::analyze` ✅ M1.4 + `--export tui` (M1.5) + `--export speedscope\|html` ✅ M1.6.4，`cmd::list` ✅ M1.6.1，`cmd::aggregate` ✅ M1.6.2 (--by tool\|mcp-server\|day\|model, --export md\|json\|csv\|html) + `--export tui` ✅ M1.6.3（deferred from M1.6.2），`cmd::watch` ✅ M1.6.3 (单 session + `watch aggregate ...` 子模式)，`observability::{config, init, tui_guard}` ✅ M1.6.4 (tracing infra — global `--log-level` / `--log-file` + TUI auto-redirect + reload-Layer)，`cmd::{ingest_otlp, config}` 规划中（Phase 2），`export` 已取消（与 `analyze --export` 重复），`config`, `main` | `clap`, `tracing`, `tracing-subscriber`, **`tracing-appender`**（M1.6.4 tracing，rolling-file writer），`anyhow`, `directories`, **`askama`**, **`csv`**, **`notify-debouncer-mini`**（M1.6.3，含 `notify` v6.1.1 transitive） |
| `xtask` | bin | `anonymize`, `dist-check`, `release-notes` | `xshell` |

---

## 5. 核心数据模型

```rust
// agentprof-core/src/model/session.rs

pub struct SessionId(pub String);  // adapter 定义，对全局唯一
pub enum AgentKind { Claude, Codex, Copilot }
pub struct ModelId(pub String);    // "claude-sonnet-4.5" 等

pub struct RawSession {
    pub id: SessionId,
    pub agent: AgentKind,
    pub started_at: DateTime<Utc>,
    pub model: ModelId,
    pub tool_defs: Vec<ToolDef>,
    pub turns: Vec<Turn>,
    pub raw_path: PathBuf,
}

pub enum ToolSource {
    Builtin,
    Mcp { server: String },
    Skill { name: String },
}

pub struct ToolDef {
    pub name: String,
    pub source: ToolSource,
    pub schema_text: String,         // 给 LLM 的 JSON 字符串原文
    pub schema_tokens: u32,          // 由 tokenizer pass 填充
}

pub struct Turn {
    pub index: u32,
    pub timestamp: DateTime<Utc>,
    pub payload: TurnPayload,
    pub usage: Option<ApiUsage>,
}

pub enum TurnPayload {
    System(String),
    User(String),
    Assistant { text: Option<String>, tool_calls: Vec<ToolCall> },
    ToolResult { tool_name: String, content: String, is_error: bool },
}

pub struct ToolCall {
    pub tool_name: String,
    pub arguments_text: String,
}

pub struct ApiUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_tokens: u32,
    pub cache_read_tokens: u32,
}

#[non_exhaustive]
pub struct TokenBucket {              // 每个 assistant turn 一份
    pub system: u32,
    pub tools_schema: u32,
    pub history: u32,
    pub user: u32,
    pub tool_result: u32,
    pub assistant_output: u32,
    pub cache_read: u32,
    pub cache_creation: u32,
}

pub enum RoiScore { Star5, Star4, Star3, Star2, Star1, Wasted }

#[non_exhaustive]
pub struct RoiRow {
    pub tool: String,
    pub source: ToolSource,
    pub schema_tokens: u32,
    pub call_count: u32,
    pub avg_result_tokens: u32,
    pub roi_score: RoiScore,
}

#[non_exhaustive]
pub struct AnalysisReport {
    pub meta: SessionMeta,
    pub turn_summary: Vec<TurnSummaryRow>,
    pub tool_rank: Vec<ToolRankRow>,                // 含 is_user_blocking: bool
    pub hook_rank: Vec<HookRankRow>,
    pub warnings: Vec<DeriveWarning>,
    pub parse_warnings: Vec<ParseWarning>,          // M1.4 post-output-audit: 让用户看到 silent event drops
    pub model_metrics: Option<BTreeMap<String, ModelUsage>>,  // F1.7: per-model session totals (input/output/cache R/cache W), populated from session.shutdown.modelMetrics — see ADR-0012
}
```

> **历史注**：上面 §5 列出的 `TokenBucket` / `RoiRow` / `schema_utilization` / `estimated_waste_usd` 是原 PRD 设计的 tokenizer/ROI/waste 输出形态。**events-first pivot 后这些被推迟到 M1.5+**；M1.4 实际交付的 `AnalysisReport` 是上面这个简化形态（三表 + 两类 warnings）。Pivot 详见 ADR-0001 + ADR-0005 §1–§6。

### 5.1 Episode aggregation (`agentprof-core::episode`)

Shared derived types built on top of `RawSession<E>` for any adapter. These are
**not produced by the adapter**; they are computed by a pure, total, single-pass
aggregator (`derive_episodes`) that consumes a stream of `impl Event`.

| Type | Module | Purpose |
|---|---|---|
| `Turn` + `TurnStatus` + `Span` + `AbortInfo` | `episode::turn` | Per-assistant-turn aggregation with status (`Open` / `Completed` / `Aborted(AbortInfo)`) |
| `ToolEpisode` + `ToolCall` + `ToolCallStatus` | `episode::tool` | Tool-name-keyed call history with 4-status enum (`Success` / `Failure { message }` / `OrphanSynthesizedStart` / `OpenAtEndOfSession`). F1: `ToolCall.arguments: Option<serde_json::Value>` field carries per-call argument JSON for TurnDetailView rendering |
| `HookEpisode` + `HookCall` | `episode::hook` | Hook-name-keyed call history with `synthesized_start` flag |
| `SkillEpisode` + `SkillInvocation` | `episode::skill` | Skill invocations with a 50-event `triggered_tools` window |
| `ModeSegment` + `Mode` | `episode::mode_segment` | Time-ranged `Interactive` / `Plan` / `Autopilot` / `Unknown(String)` segments — 对齐真实 Copilot wire vocabulary（M1.4 `fix/mode-vocabulary-alignment`，替换旧的 `Ask` / `Auto` / `Expert`） |
| `Episodes` | `episode::episodes` | Top-level container (8 fields, F1.7 added `model_metrics: Option<BTreeMap<String, ModelUsage>>` from session-shutdown events); deterministic `BTreeMap` ordering for snapshot stability |
| `ModelUsage` | `analyzer::mod` | F1.7: 4-u64 per-model token counters (`input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_write_tokens`) + `total()` saturating sum; populated from Copilot wire's `session.shutdown.modelMetrics` block per ADR-0012 D-1/D-3/D-8 |
| `DeriveWarning` | `episode::warning` | 5-variant data-quality enum (`SynthesizedStart`, `OpenAtEndOfSession`, `AbortWithoutOpenElement`, `NonMonotonicTimestamp`, `PayloadNameMissing`) |

```rust
pub fn derive_episodes<E: Event>(
    events: &[E],
    meta: &SessionMeta,
) -> Episodes;
```

Pure, total, single-pass aggregation. Algorithm in
[`docs/internals/adr-0004-episode-derivation.md`](internals/adr-0004-episode-derivation.md).

#### Event trait extension methods (post-MVP additions)

The `Event` trait (in `agentprof-core::adapter`) carries a small set of
default-`None` extension methods that adapters override when their wire
schema can supply optional metadata. Default impls return `None`, so new
extensions are non-breaking for existing adapter impls:

| Method | Added in | Purpose |
|---|---|---|
| `payload_tool_requests() -> Option<Vec<(String, serde_json::Value)>>` | F1 | Per-tool-call argument JSON for the F1 TurnDetailView (`Enter` on a flame row); CopilotAdapter walks `assistant.message.toolUses[].input` |
| `payload_model_metrics() -> Option<BTreeMap<String, serde_json::Value>>` | F1.7 | Per-model session token totals from `session.shutdown.modelMetrics`; consumed by `derive_episodes` PASS-1 to populate `Episodes.model_metrics`, surfaced via Models view (key `4`) — see [ADR-0012](internals/adr-0012-session-model-metrics-and-models-view.md) |
| `payload_success() -> Option<bool>` | B1 | Wire-format success bit for `tool.execution_complete` + `hook.end`; consumed by `derive_episodes::on_tool_complete` to produce `ToolCallStatus::Failure` and by `on_hook_end` to set `HookCall.success`. `None` defaults to Success (forward-compat for older Copilot 1.0.x / adapters that don't override). Closes the silent F1.13/F1.16/F2.3 misfire — see [ADR-0013](internals/adr-0013-event-success-bit.md) |
| `payload_error_message() -> Option<&str>` | B1 | Wire-format error message for `tool.execution_complete` failures; consumed by `derive_episodes::on_tool_complete` to populate `ToolCallStatus::Failure { message }` (currently surfaced nowhere in UI but future-ready for RoiView detail / TurnDetail error display). Copilot `hook.end` has no equivalent wire field — see [ADR-0013 D-6](internals/adr-0013-event-success-bit.md) |

Both are pure extension points: 0 changes to existing adapter trait
methods, 0 dependencies added, free-form `serde_json::Value` walk
isolates the adapter from upstream wire-schema drift.

---

## 6. Adapter trait（多 agent 适配的核心抽象）

```rust
// agentprof-core/src/model/adapter.rs

pub trait Adapter: Send + Sync {
    fn agent_kind(&self) -> AgentKind;
    fn default_session_root(&self) -> PathBuf;
    fn discover_sessions(&self, root: &Path) -> Result<Vec<SessionRef>, CoreError>;
    fn load_session(&self, sref: &SessionRef) -> Result<RawSession, CoreError>;
}

pub struct SessionRef {
    pub id: SessionId,
    pub agent: AgentKind,
    pub path: PathBuf,
    pub modified_at: SystemTime,
}
```

**规则**：
- Adapter 必须把"附在 prompt 里的 tools JSON"还原成发给 LLM 的实际 wire format（含 name + description + parameters JSON schema），后续 tokenizer 才能算准。各家 wire format 不同，由 Adapter 负责。
- 新增 agent ＝ 在 `agentprof-adapters/src/{name}.rs` 实现 `Adapter` + 在 `registry.rs` 注册 + 至少 1 个 fixture + 至少 1 个 `assert_cmd` 集成测试。

### 数据源默认路径

| Agent | 默认路径 |
|---|---|
| Claude Code | `~/.claude/projects/**/*.jsonl`（Phase 3 / M3.1） |
| Codex CLI | `~/.codex/sessions/...`（Phase 3 / M3.2，具体格式以实际抓取为准） |
| Copilot CLI | `~/.copilot/session-state/<uuid>/events.jsonl`（M1.2 已实现；schema 详见 [ADR-0002](internals/adr-0002-copilot-event-schema.md) + [ADR-0005 §6](internals/adr-0005-analyzer-and-payload-name.md)） |

可在配置文件中覆盖。

---

## 7. 主数据流

```
       ┌─────────────────────┐
       │ 1. Discover          │  Adapter::discover_sessions
       │    ~/.claude/...     │  → Vec<SessionRef>
       └──────────┬───────────┘
                  ▼
       ┌──────────────────────┐
       │ 2. Load JSONL         │  Adapter::load_session
       │    + parse turns      │  → RawSession { turns, tool_defs }
       └──────────┬───────────┘
                  ▼
       ┌──────────────────────┐
       │ 3. Tokenize           │  tokenizer::count_tokens
       │    schema/system/etc  │  → TokenizedSession (buckets)
       └──────────┬───────────┘
                  ▼
       ┌──────────────────────┐
       │ 4. Analyze            │  analyzer::compute_roi
       │    schema utilization │  analyzer::waste_estimate
       │    ROI matrix         │
       └──────────┬───────────┘
                  ▼
       ┌──────────────────────┐         persist
       │ 5. AnalysisReport     │ ──────────────────▶ SQLite
       └──────────┬───────────┘
                  ▼
       ┌──────────────────────┐
       │ 6. Render (one of)    │
       │    ratatui TUI        │
       │    speedscope JSON    │
       │    HTML (askama)      │
       │    Markdown / CSV     │
       └──────────────────────┘
```

### 7.1 关键算法

- **schema_tokens(tool)** = tokenize 该 tool 在 wire format 中的完整 JSON 表示。
- **schema_utilization(session)** = `Σ schema_tokens(called_tools) / Σ schema_tokens(loaded_tools)`，范围 `[0.0, 1.0]`。
- **waste_estimate_usd(session)** = `Σ schema_tokens(unused_tools) × assistant_turn_count × input_price_per_token`。其中 `assistant_turn_count` 是 schema 实际被附加到 prompt 的次数（每个 assistant turn 一次）。
- **roi_score(tool)**：基于 `tokens_per_call = schema_tokens / max(call_count, 1)` 的分位数打 1–5 星；`call_count == 0` → `RoiScore::Wasted`（建议 kill）。

### 7.2 分析层 rollups（`agentprof-core::analyzer`）

M1.4 引入的纯函数 rollups，消费 `&Episodes` 产出 per-row 分析数据：

| 函数 | 输出 | 排序 |
|---|---|---|
| `turn_summary(&Episodes)` | `Vec<TurnSummaryRow>` | 时间顺序（保持 `Episodes.turns` 次序） |
| `tool_rank(&Episodes)` | `Vec<ToolRankRow>` (每行含 `is_user_blocking: bool`) | `total_duration` 降序 |
| `hook_rank(&Episodes)` | `Vec<HookRankRow>` | `total_duration` 降序 |
| `analyze(&Episodes, &SessionMeta, &[ParseWarning])` | `AnalysisReport` | 打包 meta + 上述 3 个 rollups + `warnings` + `parse_warnings` |
| `pending::pending_calls(&Episodes, now)` (F2.1) | `Vec<PendingCall<'_>>` | `is_user_blocking` desc → `tool_name` asc → `started_at` asc |
| `pending::is_pending(&ToolCall, tool_name, now)` (F2.1) | `bool` | derived — true iff status is `OpenAtEndOfSession` AND `now - started >= threshold_for(tool_name)` |

`AnalysisReport` 是导出层（markdown / JSON 渲染器，未来 TUI / 存储层）共享的稳定结构。所有 `Duration` 字段通过 `duration_ms` / `duration_ms_opt` serde helper 序列化为整型毫秒（per ADR-0004 IMP-007，保证快照稳定）。

**关键不变量（L1 视角）**：
- `tool_rank` / `hook_rank` 都按 `total_duration` 降序；`turn_summary` 保持 `Episodes.turns` 时间序。
- 所有 `Duration` 字段统一序列化为整型毫秒（ADR-0004 IMP-007，保证快照稳定）。
- `commit_tool_call` / `commit_hook_call` 把 cross-turn span 归到**开始时所在的 Turn**而非结束时的 `open_turn_idx`（ADR-0005 D-2）。
- 用户阻塞型 tool（当前仅 `ask_user`，详见 `USER_BLOCKING_TOOLS` 常量）在 `ToolRankRow.is_user_blocking` 上 set true，markdown 渲染器据此拆出独立区，避免用户思考时间冲乱 Tool Rank（ADR-0005 §6）。

具体算法实现（`percentile()` 的 round-half-up 行为、`rposition` 查找的细节、partition 顺序等）见 rustdoc 与 [ADR-0005 D-1 / D-2 / D-3 + Update §1–§6](internals/adr-0005-analyzer-and-payload-name.md)。

> M1.4 post-output-audit (ADR-0005 §6) 让 `AnalysisReport` 同时携带 parser-stage warnings (`parse_warnings`) 和 derive-stage warnings (`warnings`)；markdown 渲染器在 Warnings 区分两段输出。修了 3 个 Copilot CLI 1.0.x schema 错配字段 (`HookInput.source` / `UserMessageData.source` / `AssistantMessageData.turn_id` 全部 `Option<String>`)，real-session drop rate 17 % → 0 %。隐私考虑参考 [`docs/features/privacy.md`](features/privacy.md) (PII 分级表 + 手动脱敏指南 + 计划中的 `--redact` flag)。

---

## 8. CLI 协议（`agentprof <COMMAND>`）

> **当前实现状态**（2026-06-08）：`analyze` ✅ M1.4 + `--export tui` M1.5 + `--export speedscope|html` M1.6.4 + `--section mcp-waste` M1.6.5 + `--tokens-per-tool` / `--tool-descriptions` M1.6.6 / `list` ✅ M1.6.1 / `aggregate` ✅ M1.6.2 + `--export tui` M1.6.3 + waste cols M1.6.5 + wasted-tokens col M1.6.6 / `watch` ✅ M1.6.3 / `mcp-waste` ✅ M1.6.5 + token-cost flags M1.6.6 已 ship；`ingest-otlp` / `config` 规划中（Phase 2）；`export` 已取消（与 `analyze --export` 100% 重复，CLI surface 已移除）。

```
analyze [--agent copilot]                     # ✅ M1.4: copilot only; auto/claude/codex 留给 Phase 3
        [--session latest|previous|<uuid>|<path>]   # 默认 latest
        [--root <dir>]                        # 覆盖 adapter 默认 session-state 根
        [--export md|json|tui|speedscope|html]   # ✅ M1.4 (md/json) + M1.5 (tui) + M1.6.4 (speedscope/html); csv 推迟到 M1.6.5
        [--output <file>]                     # 写文件而非 stdout（--export tui 时会 warn 并忽略）
        [--section turn-summary,tool-rank,hook-rank,mcp-waste]   # 只影响 --export md；默认全部（--export tui 时会 warn 并忽略）；`mcp-waste` ✅ M1.6.5
        [--tokens-per-tool 200]               # ✅ M1.6.6 — heuristic token cost per MCP tool when no sidecar covers it; only consulted by --section mcp-waste
        [--tool-descriptions <path>]          # ✅ M1.6.6 — sidecar (file or dir, ~ expanded) of per-tool descriptions for exact tiktoken counts; only consulted by --section mcp-waste
    分析单个 session（默认 latest），输出 markdown / JSON 报告或进入 TUI。
    Session 选择优先级：显式 path > UUID > latest/previous（按 mtime 排序）。
    --export tui 要求 stdin 和 stdout 都是 TTY；否则提示并退出。
    退出码：0 成功 / 1 用户错误 / 2 数据错误 / 3 输出错误 / 130 SIGINT。

list    [--agent copilot]                     # ✅ M1.6.1: copilot only
        [--root <dir>]                        # 覆盖 adapter 默认 session-state 根
        [--since 7d]                          # 按 mtime 过滤；接受 <N>d/h/m/s 或 all；默认 7d
        [--limit 20]                          # 最多展示数；0 = 无上限；默认 20
    列出最近的 session，7 列紧凑表格：ID / Started (UTC) / Model / Turns / Out-tokens / Duration / Size。
    单 session 解析失败不会拖垮命令；成功行正常输出，失败汇总到 stderr。
    全部失败时退出 DataError (2)。

aggregate [--agent copilot]                  # ✅ M1.6.2: copilot only
          [--root <dir>]                     # adapter session-state root override
          [--by <tool|mcp-server|day|model>] # REQUIRED — pick the group-by key
          [--since 30d]                      # filter by mtime; <N>d/h/m/s or all
          [--limit N]                        # cap bucket count; 0 = unlimited
          [--export md|json|csv|html|tui]    # ✅ M1.6.2 (md/json/csv/html) + ✅ M1.6.3 (tui, 静态跨 session 聚合 TUI)
          [--output <file>]                  # write to file instead of stdout (ignored for tui)
          [--low-utilization-threshold 20]   # day bucket warn threshold (0-100)
          [--tokens-per-tool 200]            # ✅ M1.6.6 — heuristic token cost per MCP tool when no sidecar covers it; only consulted by --by mcp-server
          [--tool-descriptions <path>]       # ✅ M1.6.6 — sidecar (file or dir, ~ expanded) of per-tool descriptions for exact tiktoken counts; only consulted by --by mcp-server
    Cross-session aggregation:
      --by tool        — per-tool ranks (sum calls/duration; re-computed p50/p95 from pooled per-call data)
      --by mcp-server  — per-MCP-server stats + unused_tool_count + fully_unused_session_count columns ✅ M1.6.5
      --by day         — per-UTC-day with utilization_pct + auto warn rows
      --by model       — per-first-turn-model session counts + totals
    --export tui 要求 stdin + stdout 都是 TTY；适合一次性快速查看，不带 live-refresh
    （要 live-refresh 用 `watch aggregate ...`）。
    Sequential parse (rayon deferred to future perf milestone). Per-session
    failures fail-soft; stderr summary at end. `--since all` renders as "Window: all"
    in human-readable output. See [ADR-0008](internals/adr-0008-aggregate-report-and-utilization.md).

watch   [--agent copilot]                     # ✅ M1.6.3: copilot only
        [--session latest|previous|<uuid>|<path>]   # 默认 latest
        [--root <dir>]                        # 覆盖 adapter 默认 session-state 根
        [--debounce-ms 250]                   # notify-debouncer-mini 去抖窗口
        [aggregate --by ... [...aggregate flags]]   # 子模式：跨 session 聚合 watch
    实时刷新 TUI：
      - 单 session（默认）：监听 <session>/events.jsonl，单次任何写入触发整 session 重 parse + 重 render。
      - watch aggregate ...：监听 <root>/ 递归，任何 session 文件变化触发跨 session 重聚合。复用所有
        `agentprof aggregate` 参数（`--by` / `--since` / `--limit` / `--low-utilization-threshold`）；
        `--export` / `--output` 在此模式被显式拒绝（UserError = 1）—— 输出永远是 TUI。
      - 默认 250 ms 去抖（D-6），通过 `notify-debouncer-mini`（含 `notify` v6.1.1）实现。
      - --session latest（默认）：启动时锁定当前最新 session，后续新 session 不跟（D-5；`q` + 重启即可）。
      - 要求 stdin + stdout 都是 TTY；不满足时退出 OutputError (3)。
      - reload 失败显示 footer banner（红色单行），watch 循环不退出（D-13）。
      - notify init 失败时退出 DataError (2) 并提示用 `--export md` 走 headless 一次性输出（无 polling fallback，D-15）。
    见 [ADR-0009](internals/adr-0009-watch-runner-and-notify.md)。

mcp-waste [--root <dir>]                       # ✅ M1.6.5
          [--since 7d]                         # 时间窗口：<N>d/h/m/s 或 all（默认 7d）
          [--top 20]                           # "Always unused" 表格 Top-N（默认 20）
          [--mcp-config <path>]                # 覆盖 ~/.copilot/mcp.json
          [--tokens-per-tool 200]              # ✅ M1.6.6 — heuristic token cost per MCP tool when no sidecar covers it; folded into Summary / per-server / per-tool token columns
          [--tool-descriptions <path>]         # ✅ M1.6.6 — sidecar (file or dir, ~ expanded) of per-tool descriptions for exact tiktoken counts; same on-disk schema as `analyze --tool-descriptions`
          [--export md|json|html]              # 输出格式（**不含 tui**，spec §7.3 / §10）
          [--output <file>]                    # 默认 stdout
    跨 session 的 MCP 服务器浪费报告（"加载了但从未被调用"）：
      - 读 `user.message.transformedContent` 中的 `<tools_changed_notice>` blocks
        + 可选 `~/.copilot/mcp.json` baseline，计算 loaded − called 集合。
      - 与 `analyze --section mcp-waste` 共享 `resolve_mcp_config_path` + 同一
        `compute_waste` / `aggregate_waste` 实现（见 agentprof-core::analyzer::waste）。
      - 跨 session 视角在 TUI 里通过 `aggregate --export tui` 的 5th view（key `5`，
        "MCP Waste"）打开；mcp-waste 子命令本身只产 md/json/html。
      - 见 [ADR-0015](internals/adr-0015-mcp-waste-architecture.md) 与 spec
        `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md`。
      - M1.6.6 token-cost flags：`--tokens-per-tool` (default 200) 是 sidecar 缺失时
        的 per-tool heuristic；`--tool-descriptions <path>` 接受 file（全局 JSON）
        或 dir（per-server `*.json`，支持 `{"tools":[…]}` 与 bare-array 两种 shape），
        命中描述时走 tiktoken 精确计数。Loaded 一次后在每个 session 复用（spec §6.2 +
        [ADR-0016](internals/adr-0016-mcp-token-cost-architecture.md)）。

ingest-otlp [--listen 0.0.0.0:4317]            # 🚧 规划中 — Phase 2
    启动 OTLP receiver，订阅 Claude Code telemetry（feature: otlp）。

config  [show | edit | path]                   # 🚧 规划中 — Phase 2
    XDG 配置：~/.config/agentprof/config.toml。

db <SUBCOMMAND>                                # ✅ M2.1 (v0.2.0)
    SQLite cache/store lifecycle and inspection. All six actions honour
    the global --storage-path flag for hermetic per-invocation DB targeting.
      init                     Create DB + run migrations (idempotent)
      stats [--export table|json]
                               Show mode / path / file size / row counts / oldest+newest
      ingest --agent X (--all | --since DUR | --session ID)
                               Batch-import sessions into the DB; per-session
                               failures logged via tracing + counted (exit 0).
                               Uses AdapterDataSource directly (pure write — no
                               dual-path read fan-out).
      prune --before DUR [--dry-run]
                               Delete sessions older than N days; FK CASCADE drops
                               tools_loaded / turn_buckets rows. --dry-run = preview.
      vacuum                   Run SQLite VACUUM; prints before/after file size.
      export <SESSION_ID> [--format json|jsonl] [--output PATH]
                               json = stored AnalysisReport verbatim;
                               jsonl = one line per top-level report field.
    See ADR-0019 (hybrid storage) + spec §9.
```

### 全局 flags (M1.6.4 + M2.1)

适用于所有子命令（clap `global = true`），由 `agentprof-cli::observability` 模块解析。

`--log-level <LEVEL>` — tracing level filter (`trace` / `debug` / `info` / `warn` / `error`
或完整 env-filter 语法 如 `warn,agentprof_core=debug`)。env fallback：
`AGENTPROF_LOG_LEVEL`，然后 `AGENTPROF_LOG`（向后兼容），默认 `warn`。

`--log-file <PATH>` — tracing 事件写入文件而非 stderr。`-` 表示强制 stderr
（即使在 TUI 模式下，用户自负 alt-screen 污染风险）。默认：
非 TUI = stderr；TUI 模式（`analyze --export tui` / `watch` / `watch aggregate`）
自动到 `$XDG_STATE_HOME/agentprof/agentprof.log`（按天滚动；干净退出后 stdout
打印路径）。env fallback：`AGENTPROF_LOG_FILE`。

`AGENTPROF_LOG_FULL_PATHS=1` — 关闭 session 路径默认的 sha256[..8] hash
（系统级 opt-out：`agentprof_core::observability::pii::hash_path` 自身在每次调用
都读 env var，所以 L1 cmd / L2 adapter / L3 analyzer + aggregator 四层 span
同步生效。详见 2026-06-03 fix `83d2ed0`）。

`--no-cache`（M2.1） — 跳过 SQLite 存储 I/O，dual-path data source 降级为
单路径 adapter view。一次性 inspection 不想触碰缓存时使用。详见
[ADR-0018](internals/adr-0018-session-datasource-trait.md) + [ADR-0019](internals/adr-0019-hybrid-storage-mode.md)。

`--storage-path <PATH>`（M2.1） — 覆盖解析后的 DB 文件路径。优先级高于
`[storage].path` 配置项与 XDG 默认值。`db export` 输出到 alt 文件、
multi-tenant CI 场景需要。

`--quiet`（M2.1） — 抑制 `agentprof: warn: session <id>: N fields differ …`
divergence warning lines（stderr）。结构化 `tracing` 事件不受影响。
背景：`DualPathDataSource` 检测到 adapter 与 storage 同一 id 的字段不一致
时（id-namespace 已由 ADR-0017 统一），会按 adapter-wins 规则提示并
opportunistic re-upsert。详见 [ADR-0018](internals/adr-0018-session-datasource-trait.md)。

> **M2.1 缓存覆盖范围**：`list` / `mcp-waste` 走 dual-path（adapter +
> SQLite，read-fast、write-through-on-analyze、warn-on-drift）；
> `analyze` write-through 缓存 + `watch` 长持连接（spec §10.2）；
> `aggregate` **暂未** 接入 dual-path —— 需要 `Episodes` 数据而当前
> `AnalysisReport` 不携带，hoist 是 M2.1.1 的工作。用户运行
> `agentprof aggregate ...` 暂时不会看到来自 SQLite 缓存的加速。

详见 [ADR-0010](internals/adr-0010-tracing-infrastructure.md) 与 §15.5 Observability。

> **export 子命令已取消**：原 spec 包含 `export <session> --format speedscope|html|md|csv`，但与 `analyze --session X --export <fmt> --output Y` 功能完全重叠。M1.6.1 decomposition 决定取消该 surface；Speedscope / HTML 导出走 `analyze --export speedscope|html`（M1.6.4 ✅ 已 ship）；CSV 推迟到 M1.6.5。

**`--export speedscope`**：写一个 Speedscope evented JSON profile（schema 见 <https://github.com/jlfwong/speedscope/blob/main/file-format.md>），可拖入 https://speedscope.app 渲染交互火焰图。`--section` 在此模式下被忽略并发警告。frame 命名 + 重叠处理见 [ADR-0007](internals/adr-0007-speedscope-export.md)。

**`--export html`**：写一个自包含的静态 HTML 报告（无 JS、含 print-friendly CSS、内嵌响应式 SVG 火焰图）。`--section` 与 md 一致。`--output` 缺失时写到 stdout 并发警告。

### 8.1 退出码

| code | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 用户错误（参数 / 路径 / 配置） |
| 2 | 数据错误（session 无法解析）—— stderr 列出失败列表 |
| 3 | 外部服务错误（Anthropic API / OTLP receiver） |
| 130 | SIGINT |

---

## 9. SQLite Schema

> **Status (M2.1, schema_version=1)**: shipped. Final DDL lives in
> `crates/agentprof-storage/migrations/001_initial.sql` and is normative
> against this section. See [ADR-0019](internals/adr-0019-hybrid-storage-mode.md)
> for the cache-vs-store decision and M2.1 spec §5 for the column-level
> design rationale.

文件路径默认按 mode 派生：
- **cache mode** (default): `${XDG_CACHE_HOME:-~/.cache}/agentprof/cache.sqlite`
- **store mode** (opt-in): `${XDG_DATA_HOME:-~/.local/share}/agentprof/store.sqlite`

可在 `[storage].path` 配置或 `--storage-path <PATH>` flag 覆盖（precedence
flag > config > XDG default）。详见 §10 与 [ADR-0019](internals/adr-0019-hybrid-storage-mode.md)。

```sql
-- agentprof v0.2.0 M2.1 initial schema (schema_version=1)

CREATE TABLE sessions (
    id                    TEXT    PRIMARY KEY,
    agent                 TEXT    NOT NULL,              -- 'copilot' | 'claude' | 'codex'
    dominant_model        TEXT,                          -- nullable; from model_metrics by total tokens
    started_at            INTEGER,                       -- unix epoch (ms)
    duration_ms           INTEGER,
    raw_path              TEXT    NOT NULL,              -- absolute source jsonl path
    raw_mtime             INTEGER NOT NULL,              -- mtime (ms) — drives dual-path freshness
    total_input_tokens    INTEGER,
    total_output_tokens   INTEGER,
    total_cache_read      INTEGER,
    total_cache_creation  INTEGER,
    schema_version        INTEGER NOT NULL DEFAULT 1,    -- analyzer version; bumped breaking-ly to force recompute
    ingested_at           INTEGER NOT NULL,              -- write time (ms)
    analysis_report_json  TEXT    NOT NULL               -- complete AnalysisReport serde JSON (disaster recovery)
);
CREATE INDEX idx_sessions_started       ON sessions(started_at DESC);
CREATE INDEX idx_sessions_agent_started ON sessions(agent, started_at DESC);

CREATE TABLE tools_loaded (
    session_id        TEXT    NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tool_name         TEXT    NOT NULL,
    source            TEXT    NOT NULL,                  -- 'builtin' | 'mcp:<server>' | 'skill:<name>'
    call_count        INTEGER NOT NULL,
    total_duration_ms INTEGER NOT NULL,
    tokens            INTEGER,                           -- M1.6.6 token cost (NULL when unknown)
    token_source      TEXT,                              -- 'heuristic' | 'sidecar'
    PRIMARY KEY (session_id, tool_name)
);
CREATE INDEX idx_tools_call_count ON tools_loaded(session_id, call_count DESC);

CREATE TABLE turn_buckets (
    session_id      TEXT    NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_index      INTEGER NOT NULL,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    cache_read      INTEGER,
    cache_creation  INTEGER,
    model           TEXT,
    PRIMARY KEY (session_id, turn_index)
);
```

**Storage strategy**:
- `analysis_report_json` carries the **complete** serialised
  `AnalysisReport`. The cli's read path hydrates from this column;
  the normalised child tables (`tools_loaded` / `turn_buckets`) are
  for future SQL-level queries and `agentprof db export --format jsonl`,
  not the hot read path.
- `loaded_mcp_tools` (M2.1 T5.2.5 hoist into `AnalysisReport`) is
  serialised inside `analysis_report_json` — no separate table — so
  the dual-path read can answer mcp-waste questions without
  re-deriving from raw events.
- **`upsert_report` semantics** (spec §10.2): `INSERT OR REPLACE` on
  `sessions` does **not** cascade to children (it updates in place,
  no row delete). The implementation explicitly `DELETE FROM <child>
  WHERE session_id = ?` then re-INSERTs, all inside one transaction.

**Index choices**:
- `idx_sessions_started` — `list --since 7d` is the hottest query.
- `idx_sessions_agent_started` — multi-agent filter once M3.1 / M3.2 land.
- `idx_tools_call_count` — `aggregate --by tool` low-utilization scan.

**`PRAGMA` set on every open** (`Db::open`):
- `journal_mode = WAL` — concurrent read + serialized write.
- `synchronous = NORMAL` — sync on checkpoint, not per-tx (fine for cache).
- `foreign_keys = ON` — cascade deletes for `db prune`.

Schema is owned by `agentprof-storage/migrations/`; each migration file
`NNN_<name>.sql`, applied on `Db::open` (idempotent).

---

## 10. 配置文件

`~/.config/agentprof/config.toml`（XDG）：

```toml
[paths]
claude  = "~/.claude/projects"
codex   = "~/.codex/sessions"
copilot = "~/.copilot/session-state"
db      = "~/.local/share/agentprof/agentprof.sqlite"   # legacy alias; see [storage].path below

[storage]                                  # ✅ M2.1 (v0.2.0)
# "cache" (default; XDG_CACHE_HOME) | "store" (XDG_DATA_HOME, opt-in)
mode = "cache"
# Override default XDG-derived path; takes effect for the active mode.
# path = "~/.cache/agentprof/cache.sqlite"
# Cache mode only — ignored in store mode. 0 disables auto-pruning.
auto_prune_days = 30

[tokenizer]
anthropic_estimator   = "cl100k"          # cl100k | api
anthropic_api_key_env = "ANTHROPIC_API_KEY"

[pricing]                                  # USD per 1M tokens
"claude-sonnet-4.5" = { input = 3.0,  output = 15.0 }
"gpt-5.2"           = { input = 1.25, output = 10.0 }

[otlp]
listen  = "127.0.0.1:4317"
enabled = false
```

---

## 11. Tokenizer 策略

- **默认（离线、无 API key）**：
  - OpenAI/Codex/Copilot 系：用 `tiktoken-rs` 的对应 encoding（`cl100k_base` / `o200k_base` 等）
  - Anthropic 系：用 `cl100k_base` 做近似估算，误差 ±5–10%
- **可选精确化**（`--use-anthropic-api` 或配置 `anthropic_estimator = "api"`）：
  - 调 Anthropic `count_tokens` HTTP API
  - 需要环境变量（默认 `ANTHROPIC_API_KEY`）
  - 启用 `agentprof-core` 的 `anthropic-api` feature
- Tokenizer 缓存按 `(model, text_hash)` 内存缓存，避免重复 tokenize 大 schema

### 11.1 MCP waste tokenizer 推断（M1.6.6）

`mcp-waste` / `analyze --section mcp-waste` / `aggregate --by mcp-server` 三个
surface 不接受手动 tokenizer 选择，而是按以下规则自动推断：

1. **优先按 session 主导 model**：扫描 `AnalysisReport.model_metrics`，按
   `ModelUsage::total()` 降序取最大者（M1.6.6 audit B1 fix — 此前用
   `keys().next()` 取首键，导致混合 model session 误分类，详见
   `agentprof-cli::cmd::model_hint::dominant_model`）。
2. **按 model 名前缀分流**（`agentprof_core::analyzer::waste::infer_tokenizer`）：
   - `gpt-5*` / `gpt-4o*` / `o1*` / `o3*` → `TokenizerKind::O200kBase`
   - 其余（含 Anthropic / 旧 OpenAI / Copilot CLI 自有 model）→ `TokenizerKind::Cl100kBase`
3. **tokenizer 实例复用**：CLI 三处子命令在 command 级**只构建一次** `CoreBPE`
   并通过 `WasteComputeContext::with_bpe(Arc<CoreBPE>)` 注入到每个 per-session
   context，避免在 100-session 跑里重复解析 merge tables（M1.6.6 audit A1）。
4. **Sidecar 命中时走精确路径**：`--tool-descriptions <path>` 提供的 sidecar
   覆盖到某 tool 时，per-tool 计数切换为 `TokenSource::SidecarExact`；未覆盖到
   的 tool 仍走 `--tokens-per-tool N`（默认 200）启发式，per-report
   `TokenProvenance` 汇总为 `Heuristic` / `SidecarExact` / `Mixed`。
5. **启发式偏差注脚**：默认 200 tokens/tool 按 `cl100k_base` 校准，`o200k_base`
   session 在纯启发式模式下可能高估 waste 约 15–20%；rustdoc 在
   `DEFAULT_HEURISTIC_TOKENS` 与 `with_heuristic` 点明并建议补 sidecar 精确化
   （M1.6.6 audit B4）。

详见 [ADR-0016](internals/adr-0016-mcp-token-cost-architecture.md) D-2 / D-3 +
spec `docs/superpowers/specs/2026-06-08-m1.6.6-mcp-waste-token-cost-design.md`。

---

## 12. 错误处理策略

### 12.1 分层

| 层 | 错误类型 | 工具 |
|---|---|---|
| lib（core/adapters/storage/tui） | `CoreError`、`AdapterError`、`StorageError`、`TuiError`（强类型） | `thiserror` |
| bin（cli） | `anyhow::Result<()>` | `anyhow` |

**禁止 lib crate 用 `anyhow`**；**禁止 bin 把 anyhow Error 暴露成 pub API**。

### 12.2 关键错误枚举（示例）

```rust
// agentprof-core/src/error.rs
#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("io error at {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },

    #[error("failed to parse session at {path}")]
    Parse { path: PathBuf, #[source] source: serde_json::Error },

    #[error("unknown agent: {0}")]
    UnknownAgent(String),

    #[error("unknown model: {0}")]
    UnknownModel(String),

    #[error("tokenizer error: {0}")]
    Tokenizer(#[from] tiktoken_rs::error::TiktokenError),

    #[cfg(feature = "anthropic-api")]
    #[error("anthropic api error ({status}): {body}")]
    AnthropicApi { status: u16, body: String },
}
```

### 12.3 韧性规则

- 解析单个 session 失败 **不能** 让 `aggregate` 命令整体崩溃 → 用 `Vec<Result<…>>` + `tracing::warn!`，末尾 stderr 输出失败计数和样例
- TUI 中所有可能 panic 的调用走 `Result`；`main()` 安装 panic hook 还原终端 raw mode 再打印 backtrace
- 错误消息必须包含：session id、文件路径、可执行的修复建议

---

## 13. 测试策略

| 类型 | 位置 | 覆盖范围 | 命令 |
|---|---|---|---|
| 单元 | 各 crate `src/**/*.rs` 内 `#[cfg(test)] mod tests` | 纯函数：tokenize、ROI 打分、waste 公式、SQL builder | `cargo test -p <crate>` |
| Adapter fixture | `crates/agentprof-adapters/tests/fixtures/{claude,codex,copilot}/*.jsonl` | 匿名化真实日志 → 解析正确性 | `cargo test -p agentprof-adapters` |
| CLI 集成 | `crates/agentprof-cli/tests/cli.rs` | `assert_cmd` + `predicates`：子命令成功路径、错误路径、退出码 | `cargo test --workspace` |
| Snapshot | `insta` | TUI 渲染（`ratatui::backend::TestBackend`）、HTML 模板、md/csv 报告 | `cargo insta test` / `cargo insta review` |
| Property | `proptest`（feature `proptest-tests`） | analyzer ROI 排序、waste 单调性 | `cargo test --features proptest-tests` |
| 文档 | `cargo test --doc` | 公开 API 的 `# Examples` 段 | 默认 |

### 13.1 Fixture 规范

- 每个 adapter 至少包含：≥1 个完全未调用的 tool / ≥1 个高频调用的 tool / ≥1 个失败的 tool_result，覆盖 ROI 三档
- 真实日志通过 `cargo xtask anonymize <real-log> > fixture.jsonl` 匿名化路径、邮箱、token

### 13.2 TDD 默认

新增公开 API / bug fix 都先写测试再写实现。详见 `CONTRIBUTING.md`。

---

## 14. 文档体系（L1 / L2 / L3）—— 边写代码边写文档

**核心原则**：文档与代码是一对孪生交付物。**不允许**"先合代码、文档后补"。任何变动代码语义、接口、模块边界、算法的 PR，都必须在**同一 commit** 内更新对应等级的文档。CI 会用启发式检查强制（见 §14.5）。

### 14.1 三个等级的定义

| 等级 | 名称 | 范围 / 受众 | 存放位置 | 长度参考 |
|---|---|---|---|---|
| **L1** | 代码架构 | 全局：分层、crate 边界、依赖图、数据流、CLI 协议、配置、关键规约。<br>受众：第一次进项目的人、AI agent、reviewer | `docs/architecture.md`（本文档，单一来源）<br>`docs/plan.md`（产品/路线图） | 单文件 1k–3k 行 |
| **L2** | 每个功能 | crate / 模块 / feature 级：做什么、不做什么、对外接口、依赖、典型用法、与其他 crate 的关系。<br>受众：使用 / 修改这个 crate 的开发者 | `crates/<name>/README.md`（crate 级，**每个 crate 必有**）<br>`docs/features/<feature>.md`（跨 crate feature 级，如"OTLP receiver"、"HTML 报告"） | 每文件 100–500 行 |
| **L3** | 详细细节 | 实现：函数/类型/字段语义、参数边界、错误条件、算法、为什么这么写。<br>受众：要改这段代码的人 | **首选**：Rust **rustdoc**（`///` + `# Examples` + `# Errors` + `# Panics`，强制 `missing_docs` warn）<br>**辅助**：`docs/internals/<topic>.md`（复杂算法 / 决策记录 / ADR） | rustdoc 跟代码走；internals 文件 100–800 行 |

### 14.2 三个等级的写作时机与变更规则

| 触发 | 必须同步更新的文档 |
|---|---|
| 新增 / 重命名 / 删除 crate | L1（架构图、依赖图、crate 一览）+ 对应 L2 `README.md` |
| 新增 / 重命名 / 删除模块（mod） | 对应 crate 的 L2 README + 模块顶部 `//!` rustdoc |
| 新增公开 trait / pub 函数 / pub struct | L3 rustdoc（**包含 `# Examples`**，缺失 → CI fail）+ 必要时 L2 README 的"对外接口"段 |
| 修改公开 API 签名或语义 | L3 rustdoc 修改 + `CHANGELOG.md` 条目（破坏性 → `BREAKING:` 前缀） |
| 新增 / 修改算法（analyzer / tokenizer / waste 公式等） | L3 rustdoc 解释 *what* + `docs/internals/<topic>.md` 解释 *why*（含被否决的方案） |
| 新增 / 删除 CLI 子命令或参数 | L1（§8 CLI 协议）+ L2（`agentprof-cli/README.md`）+ rustdoc + `README.md`（用户向） |
| 新增 / 修改 SQLite migration | L1（§9 schema）+ L2（`agentprof-storage/README.md`） |
| 新增 / 修改配置字段 | L1（§10 配置）+ L2（`agentprof-cli/README.md`） |
| 新增 adapter | L1（§6 适配 + §17 路线图）+ `crates/agentprof-adapters/src/<name>.rs` 顶部 `//!` + `docs/adapters.md` |

### 14.3 L2 `crates/<name>/README.md` 模板（每个 crate 复用）

```markdown
# <crate-name>

> 一句话定位（"做什么、不做什么"）

## 在 agentprof 架构中的位置
（一段，指向 docs/architecture.md 相应小节）

## 对外接口
- 关键 trait / struct（链接到 rustdoc）
- 典型用法（一段最短的 Rust 代码片段）

## 模块（mod）一览
| 模块 | 用途 |
|---|---|
| ... | ... |

## Features
| Feature | 默认 | 作用 |
|---|---|---|
| ... | ... | ... |

## 依赖
- workspace 内：...
- 外部关键依赖：...

## 测试与本地命令
\`\`\`
cargo test -p <crate>
cargo doc -p <crate> --open
\`\`\`

## 变更历史
（指向 CHANGELOG.md 中本 crate 的章节）
```

### 14.4 L3 rustdoc 最低要求

```rust
/// 一行简介，动词开头。
///
/// 多行展开：语义、副作用、与相邻 API 的关系。
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::compute_roi;
/// // ...
/// ```
///
/// # Errors
///
/// 列举何种 `CoreError` 变体会被返回，及对应的触发条件。
///
/// # Panics
///
/// 如果会 panic，明确说明。否则**不需要**写此段（默认不 panic）。
pub fn compute_roi(/* ... */) -> Result<Vec<RoiRow>, CoreError> {
    // ...
}
```

**`docs/internals/<topic>.md`** 用 ADR（Architecture Decision Record）风格：
```markdown
# <主题>

## Context
（要解决什么问题、为什么现在解决）

## Considered options
1. 方案 A —— 利弊
2. 方案 B —— 利弊

## Decision
（选了哪个、为什么）

## Consequences
（带来的好处、付出的代价、留下的尾巴）
```

#### 现有 ADR 一览

| ADR | 主题 | 状态 | 日期 |
|---|---|---|---|
| 0001 | Events-first product pivot | Accepted | 2026-05-26 |
| 0002 | Copilot event schema | Accepted（Updated 2026-05-27 for M1.3 Phase B） | 2026-05-26 |
| 0003 | Synthetic-only fixture strategy | Accepted | 2026-05-26 |
| 0004 | Episode derivation — lenient single-pass algorithm | Accepted | 2026-05-27 |
| 0005 | Analyzer foundations — payload_name + start-time turn attribution + AnalysisReport placement (含 Update §1–§6：D-1 表修正 / percentile 精度 / turn metadata extraction / mode vocabulary alignment / post-output audit schema fixes) | Accepted | 2026-05-30 |
| 0006 | Panic-safe TUI lifecycle (install_panic_hook + enter/leave) | Accepted | 2026-05-30 |
| 0007 | Speedscope evented JSON exporter + SVG flamegraph (M1.6.4) | Accepted | 2026-05-31 |
| 0008 | `aggregate` report shape + utilization metric (M1.6.2) | Accepted | 2026-05-31 |
| 0009 | `watch` runner + notify-debouncer-mini (M1.6.3) | Accepted | 2026-06-01 |
| 0010 | Tracing infrastructure (4-layer span topology + reload-Layer + PII hash) (M1.6.4) | Accepted | 2026-06-02 |
| 0011 | TurnDetailView + adapter args plumbing (F1) | Accepted | 2026-06-02 |
| 0012 | Session-level per-model metrics + Models view (F1.7) | Accepted | 2026-06-03 |
| 0013 | Event success bit (`payload_success` + `payload_error_message`) | Accepted | 2026-06-03 |
| 0014 | v0.1.0 release strategy (cargo-dist + GitHub Release + CHANGELOG) (M1.7) | Accepted | 2026-06-04 |
| 0015 | MCP waste architecture — `compute_waste` + `aggregate_waste` + provenance + 5th TUI view (M1.6.5) | Accepted | 2026-06-08 |
| 0016 | MCP tool token-cost — `WasteComputeContext` builder + heuristic/sidecar fallback + tokenizer auto-inference (M1.6.6) | Accepted | 2026-06-08 |
| 0017 | Unify session id namespace across adapter and storage (M2.1 hotfix — P0) | Accepted | 2026-06-10 |
| 0018 | `SessionDataSource` trait abstraction + dual-path semantics (warn + adapter-wins + async re-upsert) (M2.1) | Accepted | 2026-06-10 |
| 0019 | Hybrid storage mode — cache (default, `$XDG_CACHE_HOME`) vs store (opt-in, `$XDG_DATA_HOME`) (M2.1) | Accepted | 2026-06-10 |

### 14.5 文档同步的 CI 强制

启发式检查（`ci.yml` 中一个 `docs-sync` job）：

1. **rustdoc 缺失** → `RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --workspace`（`missing_docs` warn 已升级为 error）
2. **新建 crate 但无 `README.md`** → 检测 `crates/<name>/Cargo.toml` 存在而 `crates/<name>/README.md` 不存在 → fail
3. **`pub fn` / `pub struct` / `pub trait` 新增但无 `# Examples`** → 用 `cargo-rdme` 或脚本 grep `pub (fn|struct|trait)` 后检查 rustdoc 段
4. **API 改动但 CHANGELOG 未变** → `git diff` 检测 `pub fn` 签名变化 但 `CHANGELOG.md` 未在本 PR 修改 → warn（PR 必填一条豁免理由）

### 14.6 工作流：边写代码边写文档的实际步骤

每个 feature / bug-fix 的 commit 顺序：

1. 在 `docs/superpowers/specs/YYYY-MM-DD-<topic>.md` 写 spec（决策点 / API 草案 / 测试列表）
2. 写 failing test（TDD）
3. 实现代码 + **同一 commit** 内写 L3 rustdoc
4. 如果引入新模块/feature/crate → **同一 PR** 内更新 L2 README + L1 architecture.md
5. PR 描述里列出"动了哪些文档"，reviewer 用清单核对
6. 合并前 `docs-sync` job 必须绿

### 14.7 Skills 与文档体系的映射（统一 9 阶段 pipeline）

本项目使用**两个独立来源**的 skill，Copilot CLI 启动时自动合并：

| 来源 | 物理位置 | 范围 | Skill 数 | 用途 |
|---|---|---|---|---|
| `obra/superpowers` | `~/.copilot/installed-plugins/_direct/obra--superpowers/` | 全局 plugin | 14 | 工作流主框架（meta、TDD、brainstorming、verification 等） |
| 本项目 project skills | `<repo>/.github/skills/` | 入 git、跟随 clone | 5 ★ | 项目专属补充（Rust CLI / ADR / release / CI spec），vendored from `github/awesome-copilot` |

**完整调用规约（含 9 阶段 pipeline、必选 / 推荐 / 可选三档、反模式）**统一在
[`.github/copilot-instructions.md`](../.github/copilot-instructions.md) §5（pipeline）+ §6（清单）+ §6.7（来源对照）；
本节只给出**文档归宿**与本架构 L1/L2/L3 体系的对应关系。

| Skill（★ = `.github/skills/` 项目专属） | Pipeline 阶段 | 触发场景 | 产物落点（文档等级） |
|---|---|---|---|
| `using-superpowers` | 0 | 每次会话开头（meta） | 无产物，决定后续 skills 用法 |
| `brainstorming` | 1 | 新 feature / 新 adapter / 新 CLI 子命令 / 架构变更 | `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`；架构变更同步落 §3 / §4 / §6 / §8 / §9 / §10（**L1**） |
| `create-architectural-decision-record` ★ | 2 | brainstorming 含"considered options"；关键技术选型 | `docs/internals/adr-NNNN-<topic>.md`（**L3** ADR） |
| `writing-plans` | 3 | `brainstorming` 通过后、动手前 | `docs/superpowers/specs/YYYY-MM-DD-<topic>-plan.md`（**L2** 计划） |
| `executing-plans` | 4 | 跨 checkpoint 执行 plan 时 | 无独立产物；commit 信息引用 plan 文件 |
| `test-driven-development` | 4 | 任何新实现 / bug fix | failing test → `crates/<name>/tests/` 或 `#[cfg(test)] mod tests`；rustdoc `# Examples` 同时作 doctest |
| `subagent-driven-development` | 4 | 同会话并行多 crate 改动 | 无独立产物；每子任务走完整 L1/L2/L3 规则 |
| `dispatching-parallel-agents` | 1 / 4 | 并行 explore / research | 调研结论汇总到 `docs/internals/<topic>.md` 或对应 spec |
| `cli-mastery` ★ | 4 | 写 `agentprof-cli` 子命令、clap derive 结构、CLI UX | 代码 + L3 rustdoc + L2 `agentprof-cli/README.md` 更新 |
| `copilot-cli-quickstart` ★ | 4 | 集成 Copilot CLI 适配器、Copilot session 识别 | 代码 + L2 `agentprof-adapters/README.md` 更新 |
| `create-github-action-workflow-specification` ★ | 5（横切） | 改 `.github/workflows/*.yml` | `docs/internals/ci-<workflow>.md`（**L3**）+ §15.3 表更新（**L1**） |
| `systematic-debugging` | 6（横切，返回原 stage） | 任何 bug / test 失败 / CI 红 | 复杂决策落 `docs/internals/<topic>.md`（**L3** ADR） |
| `verification-before-completion` | 7 | 声称"完成 / 通过 / 修复"之前 | 跑本地 gate（见 [`.github/copilot-instructions.md`](../.github/copilot-instructions.md) §8）；输出证据写进 PR 描述 |
| `requesting-code-review` | 7 | 主要 feature 完成 / merge 前 | review 结论落 PR 描述；不进文档 |
| `receiving-code-review` | 7 | 收到 review feedback | 同上；如改架构 → 同步 L1 文档 |
| `finishing-a-development-branch` | 7 | 实现完、所有测试通过、准备 merge/PR | 触发 §14.5 `docs-sync` CI；CHANGELOG 必更 |
| `github-release` ★ | 8 | 准备打 tag / cargo publish / 出 binary | `CHANGELOG.md` Keep-a-Changelog 段（**L1**）+ SemVer tag + GitHub Release |
| `using-git-worktrees` | — (Stage 3→4 可选 env prep) | feature 需隔离 / 多 Phase 并行 | 无文档产物；worktree 内同样要满足 docs-sync |
| `writing-skills` | — (Pipeline 之外的元能力) | 为本项目写自定义 skill 时（如未来补 ratatui 测试 / OTel Rust 缺口） | 自定义 skill 放 `.github/skills/<name>/SKILL.md`（入 git，跟随 clone） |

### 14.8 Stage 0 常驻 instructions（非 skill）

下面两个 instruction 文件**永远生效**，由 Copilot CLI / VS Code Copilot 在每次会话和每次编辑自动加载，不需要也不可能 "invoke"——它们是工作上下文的一部分。

| 文件 | applyTo | 与本架构的关系 |
|---|---|---|
| `.github/instructions/rust.instructions.md` | `**/*.rs` | 基于 Rust API Guidelines + RFC 430 + The Rust Book；与 §16 编码规约**逻辑兼容**（前者细，后者项目特定）。冲突时以本文档（§16）为准。 |
| `.github/instructions/update-docs-on-code-change.instructions.md` | `**/*.{md,rs,…}` | 是 §14 L1/L2/L3 文档同步系统的官方机械化版本，与之**逻辑等价**。冲突时以本文档（§14）为准。 |

两个文件均来自 `github/awesome-copilot` 上游、未本地修改，便于将来按需同步更新。

---

## 15. 工程化

### 15.1 仓库结构

```
agentprof/
├── Cargo.toml                    # workspace + 共享 lints
├── rust-toolchain.toml           # channel = "stable", components = [rustfmt, clippy]
├── rustfmt.toml                  # max_width=100, group_imports=StdExternalCrate
├── clippy.toml                   # MSRV、avoid-breaking-exported-api
├── deny.toml                     # cargo-deny: license allowlist, banned crates
├── .editorconfig
├── .gitignore
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md                     # 用户向（L1 的极简入口）
├── CHANGELOG.md                  # Keep-a-Changelog + SemVer
├── CONTRIBUTING.md               # 包含 "边写代码边写文档" 规则
├── docs/
│   ├── plan.md                   # L1：产品/路线图
│   ├── architecture.md           # L1：代码架构（本文档）
│   ├── adapters.md               # L2：怎么加新 agent
│   ├── features/                 # L2：跨 crate feature 文档
│   │   ├── otlp-receiver.md
│   │   ├── html-report.md
│   │   └── ...
│   ├── internals/                # L3：算法/决策记录（ADR 风格）
│   │   ├── waste-formula.md
│   │   ├── tokenizer-strategy.md
│   │   └── ...
│   └── superpowers/specs/        # 每个 feature 的 spec（brainstorming 产物）
├── .github/
│   ├── copilot-instructions.md   # Copilot/AI agent 入口指南
│   ├── workflows/
│   │   ├── ci.yml                # 含 docs-sync job
│   │   ├── release.yml
│   │   └── nightly-msrv.yml
│   └── ISSUE_TEMPLATE/
├── crates/
│   ├── agentprof-core/
│   │   ├── Cargo.toml
│   │   ├── README.md             # L2：本 crate 的"对外接口 + 模块一览"
│   │   └── src/
│   │       └── lib.rs            # 顶部 //! 与 README 内容呼应
│   ├── agentprof-adapters/
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src/
│   ├── agentprof-storage/
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src/
│   ├── agentprof-tui/
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   └── src/
│   └── agentprof-cli/
│       ├── Cargo.toml
│       ├── README.md
│       └── src/
└── xtask/
    ├── Cargo.toml
    ├── README.md
    └── src/
```

### 15.2 workspace `Cargo.toml` 共享配置

```toml
[workspace]
members  = ["crates/*", "xtask"]
resolver = "2"

[workspace.package]
edition      = "2021"
rust-version = "1.78"
license      = "MIT OR Apache-2.0"
repository   = "https://github.com/<user>/agentprof"

[workspace.lints.rust]
unsafe_code      = "forbid"
missing_docs     = "warn"
unused_must_use  = "deny"

[workspace.lints.clippy]
pedantic    = { level = "warn", priority = -1 }
nursery     = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "warn"     # 仅允许 main.rs / tests
dbg_macro   = "deny"
todo        = "warn"
```

### 15.3 CI 矩阵（`.github/workflows/ci.yml`）

| Job | 触发 | 步骤 |
|---|---|---|
| `lint` | PR + push | `cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| `test` | PR + push | matrix: Linux/macOS/Windows × stable/beta；`cargo test --workspace --all-features`、`cargo insta test --check` |
| `deny` | PR + push | `cargo deny check` |
| `msrv` | weekly | `cargo +1.78 check --workspace` |
| `docs` | PR + push | `RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --workspace` |
| `docs-sync` | PR + push | 见 §14.5：rustdoc 完整性、L2 README 存在性、CHANGELOG 联动检查 |
| `release` | tag `v*` | `cargo-dist`：x86_64/aarch64 × Linux/macOS/Windows |

### 15.4 Feature flags

- `agentprof-core/features = ["anthropic-api"]` —— 启用真实 token API 精确化
- `agentprof-storage/features = ["otlp"]` —— 启用 OTLP receiver（Phase 2）
- `agentprof-cli/features = ["full"]` = `core/anthropic-api + storage/otlp`（默认）

### 15.5 Observability (M1.6.4)

agentprof 使用 `tracing` 0.1 作为 single canonical 诊断 / 警告 / 调试输出渠道
（取代 `eprintln!`）。配置由 `agentprof-cli::observability::LogConfig::resolve_from_env_and_flags`
合并 CLI flags / env vars / 默认值，由 `init_tracing` 安装 subscriber
（含 `reload::Layer` 以支持运行时把 writer 从 stderr 换成 rolling file）。
全局 flags 协议见 §8。

Span 拓扑 4 层（13 spans 总计）：

| Layer | Span name | Emitted at | Level |
|---|---|---|---|
| 1 | `cmd.{analyze, list, aggregate, watch}` | `agentprof-cli::cmd::*::run` | `info_span!` |
| 2 | `adapter.{discover, parse, load_meta}` | `agentprof-adapters` | `debug_span!` |
| 3 | `analyzer.{derive_episodes, analyze}`, `aggregator.group_by{tool,mcp,day,model}` | `agentprof-core` | `debug_span!` |
| 4 | event-level `tracing::{trace, debug, info, warn, error}!` | anywhere | varies |

TUI 模式（`analyze --export tui` / `watch` / `watch aggregate`）自动把 writer
切到 `$XDG_STATE_HOME/agentprof/agentprof.log`（rolling daily，via `tracing-appender`），
干净退出时 stdout 打印 log 路径。`--log-file -` 强制 stderr（用户自负
alt-screen 污染风险）。

**Soft-fall policy**：任何 init 失败（文件权限 / XDG path 不可写 / env-filter
syntax error 等）软降级到默认 stderr — tracing 永远不阻塞 CLI 启动。

**PII**：session 路径默认 sha256[..8] hex hash（由
`agentprof_core::observability::pii::{hash_path, hash_short}` 提供）；
`AGENTPROF_LOG_FULL_PATHS=1` **系统级 opt-out** —— `hash_path` 自身在每次调用
时读环境变量，故 cli (L1 `cmd.*`) / adapters (L2 `adapter.*`) / core (L3
`analyzer.*` / `aggregator.*`) **全部 4 层** span 都会同步切换为原始路径
emission，无需各层重复实现（M1.6.4 final-review follow-up 修复）。Trade-off：
8-hex 字符存在理论 collision 风险但 PII safety 优先（见
[ADR-0010 D-5](internals/adr-0010-tracing-infrastructure.md)）。

完整设计见 [ADR-0010](internals/adr-0010-tracing-infrastructure.md)
+ [spec](superpowers/specs/2026-06-02-tracing-design.md)
+ [plan](superpowers/plans/2026-06-02-m1.6.4-tracing.md)。

### 15.6 Release process

v0.1.0+ releases are built by `cargo-dist` and published as GitHub Releases
with multi-platform binaries (Linux x86_64/aarch64, macOS x86_64/aarch64
— see [ADR-0014](internals/adr-0014-v0.1.0-release-strategy.md)). The
release workflow `.github/workflows/release.yml` is generated by
`cargo dist generate-ci github` and consumes the config in
`dist-workspace.toml` at the repo root.

The two-stage release flow (local prep + tag push trigger), the abort
path, and the cargo-dist upgrade procedure are documented in
[`CONTRIBUTING.md`](../CONTRIBUTING.md#release-process-maintainers-only)
"Release process" section.

The 4 internal lib crates (`agentprof-core` / `-adapters` / `-storage` /
`-tui`) carry `publish = false` per [ADR-0014 D-3](internals/adr-0014-v0.1.0-release-strategy.md);
only `agentprof-cli` (the CLI binary entry point) is eligible for future
crates.io publishing (currently disabled by [D-1](internals/adr-0014-v0.1.0-release-strategy.md)).

---

## 16. 编码规约

1. **Lib 用 `thiserror`，bin 用 `anyhow`**。lib 出现 anyhow → CI 失败。
2. **禁止 `unwrap()`**；`expect()` 仅限 `main.rs` 和 tests。
3. **公开 API 必带 doc + 至少一个 `# Examples` 段**（`missing_docs` warn 强制；缺失 → CI fail）。这是 L3 文档的硬要求。
4. **边写代码边写文档**：每个 PR 内**同步**更新对应等级文档（L1/L2/L3）；改动语义而文档不变 → CI fail。详见 §14。
5. **每个 crate 必须有 `README.md`**（L2），与 `lib.rs` 顶部 `//!` 内容一致。
6. **新增/修改公开 trait 或破坏性变更** → 同 PR 更新 `CHANGELOG.md`（破坏性用 `BREAKING:`）。
7. **新增 adapter** 走 `agentprof-adapters/src/{name}.rs` + `registry.rs` 注册 + ≥1 fixture + ≥1 `assert_cmd` 集成测试 + 更新 `docs/adapters.md`（L2）。
8. **CLI 子命令逻辑只能放在 `agentprof-cli`**，不允许塞进 lib crate。
9. **错误消息面向用户**：包含 session id、文件路径、可执行的修复建议。
10. **公共结构体优先 `#[non_exhaustive]`**，便于无破坏性扩展字段。
11. **TUI 内绝不 panic**：可能 panic 的调用全部 `Result`，main 装 `set_hook` 还原 raw mode。
12. **Commits 用 Conventional Commits**（`feat:` / `fix:` / `refactor:` / `docs:` / `test:` / `chore:`），自动生成 CHANGELOG。
13. **TDD 默认**：新功能与 bug fix 先有 failing test 再写实现。
14. **依赖图无环**：lib crate 之间不允许有 cycle，CI 校验。

---

## 17. 路线图与本架构的关系

本架构一次性覆盖了 plan.md 中的 Phase 1 + 2 + 3。**实际实施顺序**仍按 plan.md 的 Phase 推进，每个 Phase 启动前在 `docs/superpowers/specs/` 写一份 spec：

| Phase | 启用的 crate / feature | 里程碑 | 状态 |
|---|---|---|---|
| **Phase 0** prototype | `agentprof-core` + `agentprof-adapters::copilot` + `agentprof-cli`（只 `analyze --export md\|json`） | events.jsonl → markdown 报告跑通 | ✅ M1.1–M1.4 已交付 |
| **Phase 1** MVP | + `agentprof-tui`（火焰图 + ROI 表）+ `analyze --export tui` + `list` / `aggregate` 子命令 + 多种 `--export` 格式 + `watch` 子命令 + 全工程结构化 tracing | TUI 可交互 + 跨 session 聚合 + 可分享报告 + 实时刷新 + 可观测性 | 🟡 M1.5 ✅ shipped（[ADR-0006](internals/adr-0006-panic-safe-tui.md)）；**M1.6.1 ✅ shipped**（`list` 子命令 + 8 polish）；**M1.6.2 ✅ shipped**（`aggregate` 子命令，[ADR-0008](internals/adr-0008-aggregate-report-and-utilization.md)）；**M1.6.3 ✅ shipped 2026-06-01**（`watch` 子命令 + `aggregate --export tui` 激活，[ADR-0009](internals/adr-0009-watch-runner-and-notify.md)）；**M1.6.4 ✅ shipped 2026-06-02**（先 `--export speedscope\|html` ✅ shipped 2026-05-31, [ADR-0007](internals/adr-0007-speedscope-export.md)；再 tracing 基础设施 ✅ shipped 2026-06-02 — canonical observability across all 5 crates, 13 `eprintln!` → `tracing`, 全局 `--log-level` / `--log-file` + XDG state log + PII hash, 4-layer span topology, [ADR-0010](internals/adr-0010-tracing-infrastructure.md)）；M1.6.5 (MCP waste) / M1.7 (v0.1.0 release) 进行中 |
| **Phase 2** 工程化 | + `agentprof-storage`（SQLite + 持久化）+ `ingest-otlp` 子命令（启用 `otlp` feature）+ tokenizer + ROI + waste estimation | 跨 session 数据库 + 实时 OTLP + 精确 token 成本 | ❌ 未开始 |
| **Phase 3** 多 agent | + `agentprof-adapters::claude` + `agentprof-adapters::codex` (+ 可选 Gemini) | 三 agent 全支持 | ❌ 未开始 |

> **Adapter 顺序的 events-first pivot**（ADR-0001）：原 Phase 0 计划用 `agentprof-adapters::claude`（因为 Claude session 最常见）；实际 M1.2 改做 Copilot 因为 Copilot CLI 的 events.jsonl 是事件流，比 Claude 的"最终对话日志 + 重做 tokenize"更适合 MVP 快速验证。Claude / Codex adapter 推迟到 Phase 3。

---

## 18. 待回答 / 后续 spec 化的问题

> Phase 0 已回答的问题（M1.2 + M1.3 闭环）：~~Copilot CLI 日志真实 schema~~（见 ADR-0002 Update）；剩下的是 Phase 2/3 的问题。

- [ ] Speedscope JSON 用 "evented" 还是 "sampled" 格式（取决于 turn-level 还是 token-level 粒度）
- [ ] HTML 报告是否走 single-file（base64 内嵌 d3.js）还是多文件
- [ ] OTLP receiver 是否对外暴露作为独立 binary（如 `agentprof-otlp-collector`）
- [ ] 是否要做 prompt caching 优化建议（识别"可缓存"的稳定 schema 前缀）
