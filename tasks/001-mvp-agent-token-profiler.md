# PRD: agentprof MVP —— AI Agent Token Profiler

> **文件名**：`tasks/001-mvp-agent-token-profiler.md`
> **版本**：1.2
> **创建日期**：2026-05-25 · **最后更新**：2026-05-31
> **状态**：**In-Progress — M1.1 / M1.2 / M1.3 / M1.4 / M1.5 ✅ + M1.6.1 ✅ + M1.6.4 ✅ 已交付**（6/7 ≈ 85 %）；M1.6.2 / M1.6.3 / M1.7 ❌ 未开始
> **当前 commit**：`main` HEAD（`git log -1 --oneline`）；最近一个重大 milestone merge = `9abd694` (post-output-audit)
>
> **重大 pivot（ADR-0001 events-first）**：M1.2 不做 ClaudeAdapter，改做 **CopilotAdapter**（real wire data 直接可得）。Tokenizer / ROI / waste / aggregate 全部从 M1.3 推迟到 M1.5+ 或 Phase 2。FR-2（Tokenizer）/ FR-6（Speedscope/HTML/CSV）/ FR-7（Config + Storage）目前完成度 0%，**这是 pivot 的预期行为**，不是落后。
>
> **所属阶段**：MVP = `docs/plan.md` 的 Phase 0（验证可得性）+ Phase 1（CLI + TUI 火焰图 + ROI 表 + 跨 session 聚合）
> **权威文档**：[`tasks/ROADMAP.md`](./ROADMAP.md)（项目总入口） / [`docs/architecture.md`](../docs/architecture.md) §3–§17（L1 架构定稿）/ [`docs/plan.md`](../docs/plan.md)（产品/路线图）/ [`.github/copilot-instructions.md`](../.github/copilot-instructions.md) §5 9 阶段 pipeline

---

## 1. Introduction / Overview

### 1.1 项目简介

**agentprof** 是一个 **AI agent 用的 perf flamegraph + ROI 报告器**。读取 Claude / Codex / Copilot CLI 留下的本地 session 日志（JSONL），把 context window 里 `system / tools_schema / history / user / tool_result / assistant_output` 各类 token 的占比算清楚，标出"加载了但从没被调用"的 tool，导出 **TUI（ratatui 交互式）/ Speedscope JSON / HTML / Markdown / CSV** 五种视图。

本 MVP（Phase 0 + Phase 1）聚焦在 **Claude Code** 一家 agent 上，跑通"读 JSONL → tokenize → 算 ROI → 渲染 TUI/导出"的端到端链路。Codex/Copilot 适配器与 OTLP receiver 留到 Phase 2/3（见 §11 大纲）。

### 1.2 为什么需要这个项目

现有同类工具（`ccusage` 65k⭐ / `tokscale` 3.2k⭐ / `splitrail` / `toktrack` / `claude-usage`）的共同盲区：

| 痛点 | 现状 |
|---|---|
| **粗粒度可视化** | `/context`、`/cost` 只看当前百分比和总数 |
| **只算花费** | `ccusage` 等只统计"花了多少 token、多少钱"，不分类 |
| **通用 LLM trace 不懂 tools** | Langfuse / Phoenix / Helicone 给火焰图但不知道 client 加载了哪些 tool |
| **schema 利用率盲区** | 加载了 N 个 tool（schema X tokens），实际调用了 K 个，未用 N-K 个 —— **没人量化** |
| **MCP server ROI 盲区** | MCP 普及后常挂 5+ servers，schema 占用 10k–30k tokens；用户砍 server 全靠"感觉" |

agentprof 的差异化：**把"加载了什么"和"调用了什么"做差集，量化为"浪费的 tokens × 调用次数 × 单价 = 美元/月"**。市场上 ccusage 做"花了多少钱"，本项目做"**花得值不值**"。

### 1.3 技术架构概要

```text
用户 ──→ agentprof <COMMAND> ──→ agentprof-cli
                                       │
       ┌───────────────────────────────┼───────────────────────────────┐
       ▼                               ▼                               ▼
agentprof-adapters             agentprof-tui                   agentprof-storage
 (claude/...)                  (flamegraph / ROI / aggregate)  (SQLite, Phase 2)
       │                               │                               │
       └───────────────────────────────┼───────────────────────────────┘
                                       ▼
                              agentprof-core
                (model + tokenizer + analyzer + export)
```

- **Rust 2021 Workspace**：1 个 bin (`agentprof-cli`) + 4 个 lib（`core` / `adapters` / `storage` / `tui`）+ 1 个 `xtask`
- **依赖图**：`cli → tui → core` + `cli → adapters → core` + `cli → storage → core`（无环）
- **Tokenizer**：`tiktoken-rs` 离线优先（cl100k_base 近似 Anthropic 系），feature flag `anthropic-api` 可启用真实 `count_tokens` API
- **TUI**：`ratatui` + `crossterm`，火焰图 / ROI 表 / 聚合视图三种 view
- **存储**：`rusqlite` (bundled) 持久化分析结果，feature flag `otlp` 预留 OTLP receiver（Phase 2）
- **导出**：`askama` HTML 模板 + Speedscope JSON + Markdown/CSV

---

## 2. Goals

### 2.1 主要目标

| # | 目标 | 说明 |
|---|---|---|
| G1 | **端到端自动化** | 单条 `agentprof analyze` 命令即可从 `~/.claude/projects/**/*.jsonl` 输出火焰图 / ROI / 浪费估算 |
| G2 | **schema 利用率量化** | `schema_utilization = Σ schema_tokens(called) / Σ schema_tokens(loaded)`，单 session 与跨 session 都可看 |
| G3 | **跨 session ROI 聚合** | `aggregate --by mcp-server --since 30d` 输出"哪个 MCP server 长期占 token 但从不被用"的浪费榜 |
| G4 | **离线优先** | 默认不联网（`cl100k_base` 近似 Anthropic），可选 `--use-anthropic-api` 精确化 |
| G5 | **多视图导出** | TUI 交互 + Speedscope JSON / HTML / Markdown / CSV 四种文件格式，覆盖"看一眼"和"归档分享"两种场景 |

### 2.2 商业价值

- **降低决策门槛**：把"砍 MCP server"从凭直觉变成有数据依据
- **可量化为美元**：`unused_schema_tokens × turn_count × input_price = waste_estimate_usd`，工程团队会买单
- **可拓展到三 agent**：Phase 3 同样的架构适配 Codex / Copilot CLI，零代码改动
- **正面无竞争**：搜过 50+ 个 token tracker 仓库，没人做 ROI / 利用率分析

---

## 3. User Stories

### US-1：单 session 利用率探查

> **作为** Claude Code 重度用户，
> **我希望** 一条命令查看最近一次 session 的 schema 利用率，
> **以便** 判断我的 MCP 配置是不是基线噪音。

**验收标准**：
- AC-1.1：`agentprof analyze --agent claude` 默认挑最近修改时间的 session
- AC-1.2：输出包含 `schema_utilization`（0.0–1.0 浮点）+ 总 token 数 + 加载/调用的 tool 数量
- AC-1.3：可选 `--export md` 把结果写到 stdout 或 `--out path.md`
- AC-1.4：第一次跑 < 5s（含 tokenize），缓存 token 数后 < 1s

### US-2：找出从未被调用的 tool

> **作为** 用户，
> **我希望** 看到一份"加载了但从未调用"的 tool 列表，
> **以便** 知道哪些 MCP server 可以裁掉。

**验收标准**：
- AC-2.1：Tool ROI 表按 `roi_score` 排序，未调用 tool 标为 `RoiScore::Wasted`（✗ kill 建议）
- AC-2.2：每行显示 `schema_tokens / call_count / avg_result_tokens / tokens_per_call / 建议`
- AC-2.3：未调用 tool 的浪费估算用美元显示（基于 `pricing` 配置表）

### US-3：单 session 火焰图（TUI）

> **作为** 用户，
> **我希望** 用类似 perf flamegraph 的方式可视化每个 turn 的 token 分布，
> **以便** 一眼看出 `tools_schema` 是不是基线噪音、`history` 在第几个 turn 爆炸。

**验收标准**：
- AC-3.1：`agentprof analyze --export tui`（默认）打开 ratatui 交互界面
- AC-3.2：x 轴 = turn 序号，y 轴 = 层级（agent → tool_call → sub-call），矩形宽度 = token 数
- AC-3.3：颜色按类别（system / tools_schema / history / user / tool_result / output / cache_read / cache_creation）
- AC-3.4：方向键导航、`q` 退出、`r` 刷新、`t` 切换 ROI 表视图、`a` 切换聚合视图
- AC-3.5：TUI 内部任何异常都不能让终端卡在 raw mode（panic hook 恢复终端）

### US-4：跨 session 聚合视图

> **作为** 用户，
> **我希望** 跨多个 session 看 tool 的累计 ROI，
> **以便** 发现"偶尔有用"和"长期没用"的差别。

**验收标准**：
- AC-4.1：`agentprof aggregate --by tool --since 30d` 输出"按 tool 名"的聚合表
- AC-4.2：`--by mcp-server` 聚合到 MCP server 维度，给出"砍掉 X 节省 Y tokens/session"建议
- AC-4.3：`--by day` 输出利用率时间序列（当 `<20%` 标红警告）
- AC-4.4：`--by model` 按 `claude-sonnet-4.5` / `gpt-5.x` 等模型维度聚合

### US-5：导出与分享

> **作为** 用户，
> **我希望** 把分析结果导出成可分享的文件，
> **以便** 给团队展示或归档审计。

**验收标准**：
- AC-5.1：`--export speedscope` 输出 [speedscope.app](https://www.speedscope.app/) 兼容 JSON
- AC-5.2：`--export html` 输出 single-file HTML（内嵌 d3.js，可直接发邮件）
- AC-5.3：`--export md` 输出 Markdown（GFM 表格），适合 GitHub Issue / PR
- AC-5.4：`--export csv` 输出原始数据表，适合 Excel / 二次脚本
- AC-5.5：四种导出格式的关键字段（schema_utilization / waste_estimate_usd / tool 列表）数值一致

### US-6：列出可用 session

> **作为** 用户，
> **我希望** 列出最近的 session 而不只是看到一个 id，
> **以便** 挑选要分析的目标。

**验收标准**：
- AC-6.1：`agentprof list --agent claude --since 7d` 输出最多 50 行
- AC-6.2：每行包含 `session_id / 时间 / 模型 / turn 数 / 总 token / utilization`
- AC-6.3：按 `started_at` 倒序，最近的在最上面
- AC-6.4：单个 session 解析失败时**只跳过这一行**，不让整个命令崩溃，末尾 stderr 汇总失败计数

### US-7：配置数据源路径

> **作为** 用户，
> **我希望** 系统能自动找到 Claude session 目录，也能配置自定义路径，
> **以便** 跨机器、跨用户复用同一份工具。

**验收标准**：
- AC-7.1：默认按 `docs/architecture.md` §6 表（`~/.claude/projects/**/*.jsonl`）查找
- AC-7.2：配置文件 `~/.config/agentprof/config.toml` 可覆盖（参见 §10）
- AC-7.3：`agentprof config show` 打印当前生效配置 + 来源（默认 / 配置文件 / 环境变量）
- AC-7.4：`agentprof config path` 输出配置文件路径，不存在时给"创建命令"建议

---

## 4. Functional Requirements

> **完成情况总览**（更新于 2026-05-30；events-first pivot 已生效）：
>
> | 模块 | P0 需求 | P1 需求 | P2 需求 | 完成率 | 备注 |
> |------|---------|---------|---------|--------|------|
> | FR-1 适配器 | 6/6 (Copilot) | 2/2 | — | **100%** (Copilot) | ClaudeAdapter pivot 到 Phase 2 / 3 |
> | FR-2 Tokenizer | 0/5 | 0/1 | — | **0%** | events-first pivot → 推迟到 M1.5+ |
> | FR-3 Analyzer | 部分（turn / tool / hook 三表 + warnings 已交付） | 部分 | — | **~50%** | ROI / utilization / waste 推迟到 M1.5+ |
> | FR-4 TUI | 0/5 | 0/2 | — | **0%** | 计划 M1.5 |
> | FR-5 CLI | 1/7 (`analyze` ✅) | 0/1 | — | **~14%** | `list`/`aggregate`/`export`/`config`/`tui` 计划 M1.6 |
> | FR-6 导出 | 2/4 (md + json) | 0/1 | — | **50%** | speedscope / html / csv 计划 M1.6 |
> | FR-7 配置 + 存储 | 0/3 | 0/2 | 0/1 | **0%** | 计划 Phase 2 (M2.1 SQLite) |

> **FR-1 已交付清单（Copilot adapter）**：`Adapter` trait + `CopilotAdapter` + `agentprof_adapters::copilot::*`（28 named CopilotEvent variants + WithEnvelope + Unknown）+ 4 个 payload-* trait method（name / model / output_tokens / mode）+ 11 个 fixture + 60+ unit/round-trip/path tests。详见 `CHANGELOG.md [Unreleased]` 中各 sub-section（M1.2 / M1.3 / M1.4 / audit followups / turn-metadata / mode-vocab / post-output-audit）。

### FR-1：适配器（agentprof-adapters）

| ID | 需求 | 优先级 |
|---|---|---|
| FR-1.1 | 实现 `Adapter` trait（`agent_kind` / `default_session_root` / `discover_sessions` / `load_session`）| P0 |
| FR-1.2 | 实现 `ClaudeAdapter`：解析 `~/.claude/projects/**/*.jsonl` | P0 |
| FR-1.3 | 自动 discover：递归 walk + glob 过滤 + 按 mtime 倒序 | P0 |
| FR-1.4 | 解析 `tool_use` blocks 提取实际调用的 tool name + arguments | P0 |
| FR-1.5 | 解析 `tools` 配置块提取**加载**的 tool schema 全文（用于 tokenize） | P0 |
| FR-1.6 | 解析 `usage` 字段（input/output/cache_creation/cache_read） | P0 |
| FR-1.7 | 单文件解析失败不影响其他文件（`Vec<Result<…>>`） | P1 |
| FR-1.8 | 注册表 `registry.rs`：`AgentKind → Box<dyn Adapter>` + `auto` 选择最近的 | P1 |

> **Pivot 备注（FR-1）**：FR-1.2 的 `ClaudeAdapter` 被推迟到 Phase 2 / 3，M1.2 实际交付的是等价但更直接的 `CopilotAdapter`（read `~/.copilot/session-state/<uuid>/events.jsonl`，事件流直接含 tool/hook/turn 元数据，无需 tokenize tools_schema）。FR-1.4 / FR-1.5 在 events-first 模型下被分解为 `tool.execution_start` / `tool.execution_complete` 等具体事件解析。FR-1.6 推迟到 M1.5+ tokenizer 工作；M1.4 直接读 `assistant.message.outputTokens` 字段。

**适配器扩展指南**：[`docs/adapters.md`](../docs/adapters.md)

### FR-2：Tokenizer（agentprof-core::tokenizer）

| ID | 需求 | 优先级 |
|---|---|---|
| FR-2.1 | `count_tokens(model: &ModelId, text: &str) -> Result<u32, CoreError>` 统一入口 | P0 |
| FR-2.2 | OpenAI 系（gpt-*）使用 `tiktoken_rs::o200k_base` / `cl100k_base` | P0 |
| FR-2.3 | Anthropic 系（claude-*）默认使用 `cl100k_base` 近似估算（误差 ±5–10%） | P0 |
| FR-2.4 | 内存缓存按 `(model, blake3_hash(text))` → `u32` | P0 |
| FR-2.5 | `tokenize_tool_def(tool: &ToolDef) -> u32`：还原 wire format JSON 后再算 | P0 |
| FR-2.6 | feature `anthropic-api`：调 Anthropic `/v1/messages/count_tokens`，需 `ANTHROPIC_API_KEY` 环境变量 | P1 |

### FR-3：Analyzer（agentprof-core::analyzer）

| ID | 需求 | 优先级 |
|---|---|---|
| FR-3.1 | `compute_token_buckets(session) -> Vec<TokenBucket>`：每个 assistant turn 一份 | P0 |
| FR-3.2 | `compute_roi(session) -> Vec<RoiRow>`：每个 tool 一行 | P0 |
| FR-3.3 | `schema_utilization(session) -> f32`：`Σ called_schema / Σ loaded_schema` | P0 |
| FR-3.4 | `waste_estimate_usd(session, pricing) -> f32`：`Σ unused_schema × turn_count × input_price` | P0 |
| FR-3.5 | `RoiScore` 分位数打分：未调用 → `Wasted`；其余按 `schema_tokens / call_count` 分 5 档 | P0 |
| FR-3.6 | `aggregate(sessions, by: AggregateKey) -> AggregateReport`：跨 session 聚合（tool / mcp-server / day / model） | P0 |
| FR-3.7 | 所有算法**纯确定性**（相同输入 → 相同输出），不依赖时钟/随机数 | P1 |

### FR-4：TUI 视图（agentprof-tui）

| ID | 需求 | 优先级 |
|---|---|---|
| FR-4.1 | `AppRunner`：事件循环 + 视图切换 + panic-safe terminal lifecycle | P0 |
| FR-4.2 | `views::flamegraph`：每 turn 堆叠柱状图（颜色按类别） | P0 |
| FR-4.3 | `views::roi`：Tool ROI 表（排序 / 过滤 / 跳转） | P0 |
| FR-4.4 | `views::aggregate`：跨 session 聚合表（MCP server 浪费榜、利用率时间序列） | P0 |
| FR-4.5 | `theme`：调色板（深色优先，遵循 256 色终端） | P0 |
| FR-4.6 | 键位：`q` 退出 / `t` ROI / `f` flamegraph / `a` aggregate / `r` refresh / `?` help | P1 |
| FR-4.7 | snapshot test：`ratatui::backend::TestBackend` + `insta` | P1 |

### FR-5：CLI 子命令（agentprof-cli）

| ID | 需求 | 优先级 |
|---|---|---|
| FR-5.1 | `agentprof analyze [--agent ...] [--session ...] [--path ...] [--export ...] [--out ...]` | P0 |
| FR-5.2 | `agentprof list [--agent ...] [--since 7d] [--limit 50]` | P0 |
| FR-5.3 | `agentprof aggregate [--by tool|mcp-server|day|model] [--since 30d] [--export ...]` | P0 |
| FR-5.4 | `agentprof export <session> --format speedscope|html|md|csv [--out ...]` | P0 |
| FR-5.5 | `agentprof config [show|edit|path]` | P0 |
| FR-5.6 | `agentprof watch [--agent ...]`：监听目录变化实时刷新 TUI | P0 |
| FR-5.7 | clap derive + env 默认值；退出码遵循 `architecture.md §8.1`（0/1/2/3/130） | P0 |
| FR-5.8 | `tracing_subscriber::EnvFilter`：`RUST_LOG=agentprof=info` 默认 | P1 |

### FR-6：导出器（agentprof-core::export + agentprof-cli::report_html）

| ID | 需求 | 优先级 |
|---|---|---|
| FR-6.1 | Markdown：GFM 表格，含 schema_utilization、Tool ROI、Waste estimate | P0 |
| FR-6.2 | CSV：每个 tool 一行，列与 `RoiRow` 字段对齐 | P0 |
| FR-6.3 | Speedscope JSON：`evented` 格式，每 turn 一组 frame | P0 |
| FR-6.4 | HTML：`askama` 编译期模板 + 内嵌 d3.js（base64），single-file 可直接邮件分享 | P0 |
| FR-6.5 | 四种格式的关键字段数值一致（snapshot diff 测试） | P1 |

### FR-7：配置 + 存储（agentprof-cli::config + agentprof-storage）

| ID | 需求 | 优先级 |
|---|---|---|
| FR-7.1 | `~/.config/agentprof/config.toml`（XDG 路径优先，`directories` crate） | P0 |
| FR-7.2 | 配置项：`paths.{claude,codex,copilot,db}` + `tokenizer.*` + `pricing.<model>` | P0 |
| FR-7.3 | 配置优先级：CLI flag > env (`AGENTPROF_*`) > config 文件 > 内置默认 | P0 |
| FR-7.4 | SQLite schema（`docs/architecture.md` §9）：`sessions / tools_loaded / turn_buckets` 三表 | P1 |
| FR-7.5 | migrations：`NNN_<name>.sql` 启动时 idempotent 执行 | P1 |
| FR-7.6 | feature `otlp`（Phase 2 预留）：编译开关存在，但 binary 不强制启用 | P2 |

---

## 5. Non-Goals（MVP 明确排除）

| # | 排除项 | 未来阶段 |
|---|---|---|
| NG-1 | `agentprof ingest-otlp`：OTLP receiver 实时订阅 Claude Code telemetry | Phase 2 |
| NG-2 | `agentprof-adapters::codex` / `agentprof-adapters::copilot`：另外两家 agent | Phase 3 |
| NG-3 | Web dashboard / 持久服务 / 多用户协作 | 未规划 |
| NG-4 | 利用率趋势告警 / 邮件通知 / Slack 集成 | Phase 3 |
| NG-5 | 自动 PR 优化建议（如自动改 `.mcp.json` 删 server） | 未规划 |
| NG-6 | 团队 dashboard / 多用户视图 | 未规划 |
| NG-7 | 定价表自动同步（Anthropic / OpenAI 官方价格变化） | Phase 3 |
| NG-8 | 谱图编辑能力（修改 session 文件本身） | 未规划 |
| NG-9 | 实时通知 / 推送 / 邮件 | 未规划 |
| NG-10 | Hook MCP `listTools` 主动拦截（替代 JSONL 事后分析） | Phase 3 |

---

## 6. Design Considerations

### 6.1 交互设计

- **零配置启动**：首次跑 `agentprof analyze` 应当**直接出结果**（XDG 默认路径），不需要先 `agentprof config`
- **不联网默认**：默认 tokenize 全本地，避免泄露 session 内容；`--use-anthropic-api` 显式 opt-in
- **错误消息面向用户**：必含 session id + 文件路径 + 修复建议（`docs/architecture.md` §12.3）
  ```text
  Error: failed to parse session abc-123 at /home/me/.claude/projects/foo/session.jsonl
    cause: unexpected EOF at line 142
    suggestion: this file may be truncated; try `agentprof list` to find a complete session
  ```
- **TUI 渐进式披露**：默认看火焰图概览，按 `t` 切到 ROI 表看明细，按 `a` 切到聚合看跨 session
- **结果可复现**：相同输入 + 相同 model + 相同 pricing → 完全相同输出（含 waste_estimate_usd）

### 6.2 典型交互流程

```text
用户：agentprof analyze --agent claude --export tui
  │
  ├─ adapter::ClaudeAdapter 自动 discover 最近 session
  ├─ tokenizer 算每个 tool schema + 每条 turn 的 token 数
  ├─ analyzer 算 ROI / utilization / waste
  ├─ tui::AppRunner 启动 ratatui，渲染火焰图
  │
  │  火焰图视图：
  │    Turn 1: [system 200][tools_schema 18,432  ────────────][user 50][output 120]
  │    Turn 2: [system 200][tools_schema 18,432  ────────────][history 250][...]
  │    ...
  │
  │  [q]uit  [t]ool ROI  [a]ggregate  [r]efresh  [?]help
  │
  ├─ 用户按 t → 切到 Tool ROI 表
  │  ┌──────────────────┬────────┬───────┬──────────────┬─────────┐
  │  │ tool             │ schema │ calls │ tokens/call  │ ROI     │
  │  ├──────────────────┼────────┼───────┼──────────────┼─────────┤
  │  │ filesystem.read  │   420  │  87   │     4.83     │ ★★★★★   │
  │  │ github.create_pr │   680  │   2   │   340        │ ★★      │
  │  │ playwright.click │  1240  │   0   │     ∞        │ ✗ kill  │
  │  └──────────────────┴────────┴───────┴──────────────┴─────────┘
  │
  └─ 用户按 q 退出 → 终端正常恢复
```

### 6.3 数据流约束（与 `architecture.md` §7 一致）

```
Adapter::discover_sessions → Vec<SessionRef>
                              ↓
Adapter::load_session       → RawSession { turns, tool_defs }
                              ↓
tokenizer::tokenize_*       → TokenizedSession (token_buckets)
                              ↓
analyzer::compute_roi etc.  → AnalysisReport
                              ↓
                ┌─────────────┼─────────────────────────────┐
                ▼             ▼                             ▼
            TUI render     storage::persist          export::{md,csv,speedscope,html}
```

**纪律**：lib crate 之间**只能通过 `agentprof-core` 中定义的 trait/struct 通信**，禁止 `agentprof-adapters` 依赖 `agentprof-tui` 或类似的横向依赖（见 `architecture.md` §3.1 依赖图）。

---

## 7. Technical Considerations

### 7.1 架构（详见 `docs/architecture.md`）

- **Rust 2021 Workspace**，MSRV 1.78（CI weekly 校验）
- 5 个 lib/bin crate + 1 个 `xtask`：`agentprof-core`（叶子）/ `agentprof-adapters` / `agentprof-storage` / `agentprof-tui` / `agentprof-cli`（bin）
- 共享 lints + `[workspace.dependencies]` 集中管理（见 `Cargo.toml`）
- **错误模型分层**：lib 用 `thiserror`（`CoreError` / `AdapterError` / `StorageError` / `TuiError`）；bin 用 `anyhow`；**lib 出现 anyhow → CI fail**

### 7.2 性能

| 场景 | 要求 | 设计 |
|---|---|---|
| 单 session（100 turn / 10 个 tool） | analyze 首次 < 5s | tokenize 缓存 + adapter streaming |
| 单 session 复跑 | < 1s | tokenize 内存缓存 |
| `list --since 7d`（~50 session） | < 3s | 只读 metadata，不 tokenize |
| `aggregate --since 30d`（~200 session） | < 10s | 并发解析 + SQLite cache |
| TUI 启动 | < 500ms | 复用 `analyze` 已 tokenize 的结果 |

### 7.3 错误处理（`architecture.md` §12）

- 解析单个 session 失败不能拖垮整个命令：`Vec<Result<…>>` + `tracing::warn!`，末尾 stderr 汇总失败计数
- TUI 内**绝不 panic**：`main()` 装 `std::panic::set_hook` 还原 raw mode 再 abort（`architecture.md` §16.11）
- 错误消息含 session id + 文件路径 + 可执行的修复建议
- CLI 退出码：`0` 成功 / `1` 用户错误 / `2` 数据错误 / `3` 外部服务错误 / `130` SIGINT

### 7.4 可复现性

- 每次 analyze 生成 `report.report_hash`（blake3）—— 相同输入 + 配置 → 相同 hash
- Markdown / CSV / JSON 导出文件头部都带 `# generated by agentprof v0.1.0 at <iso8601>` + `# report_hash: <hex>`
- SQLite 持久化的 `sessions` 表带 `report_hash` 列，便于校验

### 7.5 可测试性

| 层 | 测试策略 |
|---|---|
| `agentprof-core` | 纯函数单元测试：tokenize 计数、ROI 打分、waste 公式、schema utilization 边界值 |
| `agentprof-adapters` | fixture 文件 + `assert_cmd` 集成：`tests/fixtures/claude/*.jsonl` 至少 1 个含未调用 tool + 1 个高频 tool + 1 个失败 tool_result |
| `agentprof-storage` | 内存 SQLite + migration idempotency + 并发写入测试 |
| `agentprof-tui` | `ratatui::backend::TestBackend` + `insta` snapshot |
| `agentprof-cli` | `assert_cmd` + `predicates`：每个子命令正负两条 case + 退出码校验 |

### 7.6 依赖管理

核心 Rust 依赖（在 `Cargo.toml` `[workspace.dependencies]` 中已声明）：

| Crate | 用途 | 范围 |
|---|---|---|
| `serde` + `serde_json` | 序列化 + JSONL 解析 | core / adapters / storage / cli |
| `thiserror` | lib 错误定义 | 所有 lib |
| `anyhow` | bin 顶层错误 | cli |
| `tracing` + `tracing-subscriber` | 日志 + EnvFilter | cli / 所有 lib |
| `chrono` | 时间戳 + ISO8601 | core / adapters / storage |
| `tiktoken-rs` | tokenize | core |
| `ratatui` + `crossterm` | TUI | tui |
| `rusqlite` (bundled) | SQLite | storage |
| `clap` (derive + env) | CLI 解析 | cli |
| `askama` | HTML 模板 | cli |
| `directories` | XDG 路径 | cli |
| `walkdir` + `globset` | adapter 文件发现 | adapters |
| `reqwest` (opt) | Anthropic API | core, feature `anthropic-api` |
| `tokio` (opt) | async（仅 storage/OTLP/api） | core / storage |
| `assert_cmd` + `predicates` + `insta` + `tempfile` | 测试 | dev-dep |

---

## 8. Success Metrics

### 8.1 MVP 验收指标

| # | 指标 | 目标 | 验证方式 |
|---|---|---|---|
| SM-1 | 端到端可跑通 | 真实 `~/.claude/projects/**/*.jsonl` 上 `agentprof analyze --export tui` 一次成功 | 手动测试 |
| SM-2 | Tokenize 一致性 | OpenAI 模型与 `tiktoken` Python 参考实现差异 ≤ 0 token | 对比测试 |
| SM-3 | Claude 估算误差 | 与 Anthropic API（`anthropic-api` feature）差异 ≤ ±10% | feature 集成测试 |
| SM-4 | ROI 排序正确性 | 已知 fixture：未调用 tool 排末位（Wasted），高频低 schema tool 排首位（★★★★★） | 单元测试 |
| SM-5 | 跨 session 聚合一致性 | `aggregate --by tool` 各 tool 累计数 = 各 session 单独算的总和 | 单元测试 |
| SM-6 | 导出格式一致 | md / csv / speedscope / html 四种格式中的 `schema_utilization` / `waste_estimate_usd` 完全相同 | snapshot 测试 |
| SM-7 | TUI panic 安全 | 注入异常 → 终端恢复正常（非 raw mode） | 手动测试 |
| SM-8 | 全 workspace 编译通过 | `cargo check --workspace --all-features` 无 error/warning | CI |
| SM-9 | clippy 零 warning | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | CI |
| SM-10 | rustdoc 零 warning | `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace` | CI |

### 8.2 质量指标

- `cargo fmt --all -- --check` 通过
- `cargo deny check` 通过（license allowlist + advisories）
- 所有公开 API 带 rustdoc + `# Examples`（`missing_docs = warn` 升 error）
- 每个 crate 有 `README.md`（L2，与 `lib.rs` `//!` 一致）
- `docs-sync` CI job 通过（见 `architecture.md` §14.5）

---

## 9. Open Questions

| # | 问题 | 影响 | 当前假设 / 行动 |
|---|---|---|---|
| OQ-1 | Claude Code JSONL 的具体 schema 是否稳定？字段是否随版本变化？ | adapter parse 实现 | Phase 0 Task 1.2.1 抓真实样本，写 fixture 锁定 schema |
| OQ-2 | Claude Code 是否在 prompt 中重复发 tools_schema（每个 turn）？还是只发一次？ | waste_estimate 公式 | 假设每个 assistant turn 重复发；通过实测核对 |
| OQ-3 | `tiktoken` `cl100k_base` 估算 Claude token 的误差有多大？±5% 还是 ±15%？ | 默认 tokenizer 是否够用 | Task 1.3.5 跑对比实验，决定是否在 Phase 1 就上 anthropic-api feature |
| OQ-4 | Speedscope JSON `evented` vs `sampled` 哪个适合 turn-level 粒度？ | export 实现 | 假设 `evented`，每 turn 一组 push/pop frame |
| OQ-5 | HTML 报告是否真要 single-file？base64 嵌 d3.js 后体积会不会爆？ | export html 实现 | Task 1.6.5 实测 d3.js minified 约 280KB，可接受 |
| OQ-6 | pricing 表是放配置文件还是内置默认？后者怎么跟 Anthropic 价格调整保持同步？ | 配置实现 | MVP：内置默认 + 用户可覆盖；自动同步推迟到 Phase 3 |
| OQ-7 | `agentprof watch` 用 `notify` crate（inotify）还是轮询 mtime？ | watch 实现 | 默认 `notify`，降级到 1s 轮询；watch 在 MVP 标记为 P1 |
| OQ-8 | TUI snapshot 测试在 CI 上跨平台稳定吗？（Linux/macOS 字体宽度差异） | tui 测试 | 用 `TestBackend` 固定 80×24，应跨平台稳定；macOS 上有 emoji 宽度问题就退避 |

---

## 10. Implementation Milestones

### 关键路径

```text
M1.1 (skeleton ✅) ─→ M1.2 (claude adapter) ─→ M1.3 (tokenizer + analyzer) ─→ M1.4 (CLI analyze + md export) ─→ M1.5 (TUI flamegraph + ROI) ─→ M1.6 (list/aggregate/export) ─→ M1.7 (集成验证)
```

### 依赖关系

```text
M1.1 (skeleton, ✅ done) ──┬──→ M1.2 (claude adapter)
                            ├──→ M1.3 (tokenizer + analyzer)  ← 需要 M1.2 的 RawSession
                            ├──→ M1.4 (CLI analyze)           ← 需要 M1.2 + M1.3
                            ├──→ M1.5 (TUI)                   ← 需要 M1.3 的 AnalysisReport
                            ├──→ M1.6 (list/aggregate/export) ← 需要 M1.4 + M1.5
                            └──→ M1.7 (集成验证)              ← 需要全部
```

### 每个 Milestone 走完整 9 阶段 pipeline

参见 [`.github/copilot-instructions.md`](../.github/copilot-instructions.md) §5。每个 Milestone 的实际推进：

1. **Stage 1** `brainstorming` → `docs/superpowers/specs/YYYY-MM-DD-<milestone>-design.md`
2. **Stage 2** `create-architectural-decision-record`（若有 ≥2 个候选方案）→ `docs/internals/adr-NNNN-*.md`
3. **Stage 3** `writing-plans` → `docs/superpowers/specs/YYYY-MM-DD-<milestone>-plan.md`
4. **Stage 4** `test-driven-development` + 条件辅助 skill → 代码 + 测试 + L3 rustdoc + L2 README
5. **Stage 5（横切）**：如改了 `.github/workflows/*.yml` → `create-github-action-workflow-specification`
6. **Stage 6（横切）**：撞到 bug → `systematic-debugging` → 回原 stage
7. **Stage 7** `verification-before-completion` → 本地 gate 输出 + PR 描述
8. **Stage 8**（仅 release 时）`github-release` → CHANGELOG + tag

---

### Milestone 1.1：项目骨架与 `core` crate

> **状态**：✅ 已完成（5 commit `b47aeb5`/`1a7a7f6`/`dc838fc`/`201ae46`/`472ac31`，`cargo check --workspace --all-features` 通过，0 warning）

> 建立 Rust Workspace 结构 + 文档体系 + skill pipeline + CI 骨架。本 milestone 不包含任何业务代码，仅为后续 milestone 提供基础。
> 关联 FR：无业务 FR（基础工程）

#### Task 1.1.1：Cargo Workspace 与共享配置 ✅

- **Sub-task 1.1.1.1**：根 `Cargo.toml` workspace + 5 crate + xtask + 共享 `[workspace.dependencies]` + lints ✅
- **Sub-task 1.1.1.2**：`rust-toolchain.toml`（stable + rustfmt + clippy + rust-src） ✅
- **Sub-task 1.1.1.3**：`rustfmt.toml` / `clippy.toml`（MSRV 1.78）/ `deny.toml`（license allowlist） ✅
- **Sub-task 1.1.1.4**：`.editorconfig` / `.gitignore`（不忽略 `Cargo.lock`） ✅
- **Sub-task 1.1.1.5**：双协议 LICENSE-MIT + LICENSE-APACHE ✅

#### Task 1.1.2：5 个 crate 空壳 ✅

- **Sub-task 1.1.2.1**：`agentprof-core/{Cargo.toml,README.md,src/lib.rs}` —— 顶部 `//!` + features `anthropic-api` ✅
- **Sub-task 1.1.2.2**：`agentprof-adapters/{Cargo.toml,README.md,src/lib.rs}` ✅
- **Sub-task 1.1.2.3**：`agentprof-storage/{Cargo.toml,README.md,src/lib.rs}` —— features `otlp` ✅
- **Sub-task 1.1.2.4**：`agentprof-tui/{Cargo.toml,README.md,src/lib.rs}` ✅
- **Sub-task 1.1.2.5**：`agentprof-cli/{Cargo.toml,README.md,src/main.rs}` —— features `full=anthropic-api+otlp` 默认 ✅
- **Sub-task 1.1.2.6**：`xtask/{Cargo.toml,README.md,src/main.rs}`（`publish = false`） ✅

#### Task 1.1.3：用户向文档 + 贡献者文档 ✅

- **Sub-task 1.1.3.1**：根 `README.md`（项目简介 + 链接到 plan/architecture） ✅
- **Sub-task 1.1.3.2**：`CHANGELOG.md`（Keep-a-Changelog + SemVer + `[Unreleased]`） ✅
- **Sub-task 1.1.3.3**：`CONTRIBUTING.md`（含 4 大规则：文档同步 / TDD / Conventional Commits / 本地 gate） ✅
- **Sub-task 1.1.3.4**：`docs/architecture.md`（L1 权威，18 节） ✅
- **Sub-task 1.1.3.5**：`docs/adapters.md`（L2 适配器贡献指南占位） ✅
- **Sub-task 1.1.3.6**：`docs/{features,internals,superpowers/specs}/README.md` 占位 ✅

#### Task 1.1.4：AI 助手指南 + Skill 体系 ✅

- **Sub-task 1.1.4.1**：`.github/copilot-instructions.md`（§0–§12，含 9 阶段 pipeline） ✅
- **Sub-task 1.1.4.2**：`.github/instructions/rust.instructions.md`（vendored from awesome-copilot） ✅
- **Sub-task 1.1.4.3**：`.github/instructions/update-docs-on-code-change.instructions.md`（vendored） ✅
- **Sub-task 1.1.4.4**：`.github/instructions/README.md`（L2 说明） ✅
- **Sub-task 1.1.4.5**：`.github/skills/` 5 个项目级 skill（cli-mastery / copilot-cli-quickstart / github-release / create-github-action-workflow-specification / create-architectural-decision-record） ✅
- **Sub-task 1.1.4.6**：`.github/skills/README.md`（provenance + 同步命令 + license） ✅

#### Task 1.1.5：CI 骨架 ✅

- **Sub-task 1.1.5.1**：`.github/workflows/ci.yml`（lint + test matrix + deny + docs + docs-sync） ✅
- **Sub-task 1.1.5.2**：`.github/workflows/nightly-msrv.yml`（weekly cargo check on 1.78） ✅
- **Sub-task 1.1.5.3**：`.github/workflows/release.yml`（cargo-dist 骨架占位） ✅
- **Sub-task 1.1.5.4**：`.github/PULL_REQUEST_TEMPLATE.md`（L1/L2/L3 checklist + 本地 gate） ✅

#### Task 1.1.6：git init + 验证 ✅

- **Sub-task 1.1.6.1**：`git init -b main` + 5 个有意义的 commit ✅
- **Sub-task 1.1.6.2**：`cargo check --workspace --no-default-features` 通过（39s） ✅
- **Sub-task 1.1.6.3**：`cargo check --workspace --all-features` 通过（24s 增量） ✅
- **Sub-task 1.1.6.4**：`cargo fmt --all -- --check` 通过 ✅

---

### Milestone 1.2：`agentprof-adapters::copilot` — Copilot CLI session 解析（pivot from Claude）

> **状态**：✅ **已完成**（merge commit `feat/m1.2-copilot-adapter`；详见 `CHANGELOG.md [Unreleased]` § "M1.2 — Copilot CLI adapter"）
>
> **Pivot 说明**（ADR-0001 events-first）：原计划做 ClaudeAdapter，因为 Claude 的 JSONL 是「最终对话日志」需要重做 tokenizer 才能算 token；Copilot CLI 的 `~/.copilot/session-state/<uuid>/events.jsonl` 是「事件流」，直接含 tool/hook/turn 元数据，能让 MVP 更快验证产品价值。**ClaudeAdapter 推迟到 Phase 2 / 3**。
>
> 实际交付：`Adapter` trait + `CopilotAdapter` + 28 named `CopilotEvent` variants (+ `Unknown`) + `discover_sessions` (mtime 排序) + `load_session` (含 live-mode 截断容忍 + parse_warnings 收集) + **9 fixture** (synthetic only, ADR-0003；M1.3 / M1.4 后续增长到 **12**) + 23 round-trip tests + 38 单元测试。
>
> 关联 FR：FR-1.1 / FR-1.3 / FR-1.7 / FR-1.8 + (FR-1.4 / FR-1.5 / FR-1.6 用 events 模型变形)| 关联 US：US-1 / US-6
>
> **后续 sub-task 段（Task 1.2.1 ~ 1.2.4）保留作历史记录**，但请注意它们针对 Claude，不反映已交付的 Copilot 实现细节。Copilot adapter 的实际细节见 `crates/agentprof-adapters/src/copilot/*.rs` + `docs/superpowers/specs/2026-05-26-copilot-adapter-event-first-design.md`。

#### Task 1.2.1：调研真实 Claude session 格式

- **Sub-task 1.2.1.1**：抓取本地 `~/.claude/projects/**/*.jsonl` 至少 5 个真实 session（覆盖：仅 builtin tool / 含 MCP server / 含 Skill / 含 tool_use_error / 无 tool 调用）
- **Sub-task 1.2.1.2**：用 `cargo run -p xtask -- anonymize <real-log> > fixtures/claude/<name>.jsonl` 匿名化（替换 paths/emails/tokens），结果 commit 进 `crates/agentprof-adapters/tests/fixtures/claude/`
- **Sub-task 1.2.1.3**：记录 schema 观察笔记到 `docs/internals/claude-jsonl-schema.md`（L3）：每行类型枚举、字段路径、版本信息（user-agent / claude-cli-version 提示）
- **Sub-task 1.2.1.4**：识别**加载的 tools**所在位置（system block 内 `tools` 数组 / 单独的 init 消息），确认每条 assistant turn 是否重复发（关键，影响 OQ-2）
- **Sub-task 1.2.1.5**：识别**调用的 tools** 所在位置（assistant message 的 `content` 数组中 type=`tool_use` 块），提取 `name` / `input`
- **Sub-task 1.2.1.6**：识别 `usage` 字段位置（`input_tokens` / `output_tokens` / `cache_creation_input_tokens` / `cache_read_input_tokens`），与 `RawSession::Turn::usage` 对齐

#### Task 1.2.2：实现 Adapter trait（agentprof-core）

- **Sub-task 1.2.2.1**：在 `agentprof-core/src/model/adapter.rs` 实现 `Adapter` trait（`agent_kind` / `default_session_root` / `discover_sessions` / `load_session`）
- **Sub-task 1.2.2.2**：定义 `SessionRef`（`id` / `agent` / `path: PathBuf` / `modified_at: SystemTime` / `size_bytes: u64`）
- **Sub-task 1.2.2.3**：定义 `AgentKind` 枚举（`Claude` / `Codex` / `Copilot`）+ `Display` + `FromStr`
- **Sub-task 1.2.2.4**：所有公开类型 derive `Serialize` + `Deserialize` + `Debug` + `Clone` + `#[non_exhaustive]`
- **Sub-task 1.2.2.5**：rustdoc + `# Examples` doctest（满足 `missing_docs` 升 error 后通过）
- **Sub-task 1.2.2.6**：单元测试：`AgentKind::from_str("claude")` / round-trip serde

#### Task 1.2.3：ClaudeAdapter 实现

- **Sub-task 1.2.3.1**：在 `agentprof-adapters/src/claude.rs` 实现 `ClaudeAdapter::default()`（无字段 / 零初始化）
- **Sub-task 1.2.3.2**：实现 `default_session_root()`：用 `directories::BaseDirs` 拼 `~/.claude/projects`（macOS / Linux 同；Windows 用 `%USERPROFILE%\.claude\projects`）
- **Sub-task 1.2.3.3**：实现 `discover_sessions`：`walkdir::WalkDir` + `globset` 过滤 `**/*.jsonl` + 按 `modified()` 倒序 + 限制 `max_depth(8)` 防爆
- **Sub-task 1.2.3.4**：实现 `load_session`：逐行 `serde_json` 解析，跳过空行 / 注释行 / 损坏行（用 `tracing::warn!` 记录），最终组装 `RawSession`
- **Sub-task 1.2.3.5**：解析 `tools` 加载块：保留**完整 JSON 字符串**到 `ToolDef::schema_text`（给后续 tokenize 用），并提取 `name` + 推断 `ToolSource`（前缀 `mcp__<server>__` → `ToolSource::Mcp { server }`，前缀 `skill__<name>__` → `ToolSource::Skill { name }`，其余 → `ToolSource::Builtin`）
- **Sub-task 1.2.3.6**：解析 turn payload 五种类型：`System` / `User(text)` / `Assistant { text, tool_calls }` / `ToolResult { tool_name, content, is_error }`；忽略 Claude 特有的 `thinking` 块（不计入 token bucket，记入单独的 `cache_read` 字段如有）
- **Sub-task 1.2.3.7**：解析 `usage` 字段 → `ApiUsage`；缺失字段用 `Option<u32>` 表示
- **Sub-task 1.2.3.8**：模型识别：从 `message.model` 提取 `ModelId`（如 `"claude-sonnet-4-5"` → `ModelId("claude-sonnet-4.5")` 规范化）
- **Sub-task 1.2.3.9**：错误处理：用 `AdapterError`（`thiserror`）；单文件解析失败返回 `Err`，但 caller（`load_all`）应用 `Vec<Result<...>>` 模式

#### Task 1.2.4：注册表 + auto 选择逻辑

- **Sub-task 1.2.4.1**：在 `agentprof-adapters/src/registry.rs` 实现 `register_default_adapters() -> HashMap<AgentKind, Box<dyn Adapter>>`，MVP 只注册 `Claude`
- **Sub-task 1.2.4.2**：实现 `Registry::get_for_auto(roots: &[PathBuf]) -> Result<Box<dyn Adapter>>`：扫所有已注册 adapter 的默认路径，选最近 mtime 那一家
- **Sub-task 1.2.4.3**：实现 `Registry::list_agents() -> Vec<AgentKind>`（CLI 输出可用的 `--agent` 取值用）

#### Task 1.2.5：单元 + 集成测试

- **Sub-task 1.2.5.1**：单元测试（`#[cfg(test)] mod`）：每种 turn 类型解析正确 / `tools` 块识别正确 / `usage` 字段正确 / `ModelId` 规范化
- **Sub-task 1.2.5.2**：fixture 集成测试（`tests/claude.rs`）：每个 fixture 文件至少断言 `turn_count` / `tool_def_count` / `total_input_tokens`
- **Sub-task 1.2.5.3**：fixture 必含：≥1 个未被调用的 tool / ≥1 个高频调用 tool / ≥1 个失败 tool_result（覆盖 ROI 三档）
- **Sub-task 1.2.5.4**：错误场景测试：损坏 JSONL / 截断文件 / 不存在路径 / 权限拒绝
- **Sub-task 1.2.5.5**：snapshot 测试（`insta`）：`RawSession` 的 Debug 输出锁定，未来回归易察觉

#### Task 1.2.6：文档同步（L2 + L3）

- **Sub-task 1.2.6.1**：`crates/agentprof-adapters/README.md` 补"支持的 agent"段，列出 ClaudeAdapter + 输入路径 + 已知限制
- **Sub-task 1.2.6.2**：`crates/agentprof-adapters/src/claude.rs` 顶部 `//!`：data source 路径 / wire-format notes / known quirks
- **Sub-task 1.2.6.3**：补全 `docs/adapters.md`（L2）"claude (Claude Code)"段，含 wire-format 笔记 + ToolSource 推断规则
- **Sub-task 1.2.6.4**：CHANGELOG.md `[Unreleased]` 加 `feat(adapters): add Claude Code adapter`

---

### Milestone 1.3：`agentprof-core::episode` — schema-audit + Episode aggregation（pivot from tokenizer/ROI）

> **状态**：✅ **已完成**（merge commit `feat/m1.3-episode-and-schema-fix`；详见 `CHANGELOG.md [Unreleased]` § "M1.3 Phase A+B" + "M1.3 Phase C"）
>
> **Pivot 说明**（events-first 续）：原计划做 tokenizer + Tool ROI + waste_estimate + cross-session aggregate，因为 Copilot wire data 直接含 `outputTokens` 字段，**tokenizer 推迟到 M1.5+ Phase 2**；ROI / waste / aggregate 也推迟到 M1.5+。M1.3 实际交付的是：
>
> 1. **`xtask schema-audit`** — 扫真实 Copilot session 跑 schema 体检，输出 MissingVariant / MissingField / BadType 等差异报告。Phase A 用它发现并补全 10 个新 `CopilotEvent` variant + 4 处 payload struct 字段调整。
> 2. **`agentprof_core::episode` 模块** — `Episodes { turns, tools, hooks, skills, warnings }` + `derive_episodes(&[Event], &SessionMeta) -> Episodes` 算法。把 events 流聚合成 ToolEpisode / HookEpisode / Turn / SkillInvocation，含 orphan 处理、abort 处理、out-of-order 容忍。`DeriveWarning` 4 个 variant。
>
> 关联 FR：(FR-3.1 / FR-3.2 / FR-3.3 部分覆盖，作 "turn / tool / hook 聚合"；其余 FR-2.x / FR-3.4-3.7 ROI / waste 推迟) | 关联 US：US-1（部分）
>
> **下面 sub-task 段（Task 1.3.1 ~ 1.3.7）保留作历史记录**，但已不是当前实现轨迹。实际算法见 `crates/agentprof-core/src/episode/derive.rs` + `docs/superpowers/specs/2026-05-27-m1.3-episode-and-schema-fix-design.md`。

#### Task 1.3.1：核心数据模型（agentprof-core/src/model）

- **Sub-task 1.3.1.1**：在 `model/session.rs` 定义 `RawSession` / `Turn` / `TurnPayload` / `ToolCall` / `ApiUsage`（与 `architecture.md` §5 对齐）
- **Sub-task 1.3.1.2**：在 `model/tool.rs` 定义 `ToolDef` / `ToolSource`（Builtin / Mcp { server } / Skill { name }）
- **Sub-task 1.3.1.3**：在 `model/bucket.rs` 定义 `TokenBucket`（8 字段 + `#[non_exhaustive]`）
- **Sub-task 1.3.1.4**：在 `model/roi.rs` 定义 `RoiScore`（6 档枚举）/ `RoiRow`
- **Sub-task 1.3.1.5**：在 `model/report.rs` 定义 `AnalysisReport`（session_id / buckets_per_turn / roi_rows / schema_utilization / estimated_waste_usd / report_hash）
- **Sub-task 1.3.1.6**：在 `model/agg.rs` 定义 `AggregateKey`（Tool / McpServer / Day / Model）/ `AggregateReport`
- **Sub-task 1.3.1.7**：所有类型 derive `Serialize` + `Deserialize` + `Debug` + `Clone` + `#[non_exhaustive]`
- **Sub-task 1.3.1.8**：单元测试：每种类型 serde round-trip

#### Task 1.3.2：tokenizer 模块

- **Sub-task 1.3.2.1**：`tokenizer/mod.rs` 定义 `count_tokens(model: &ModelId, text: &str) -> Result<u32, CoreError>` 入口
- **Sub-task 1.3.2.2**：`tokenizer/local.rs`：基于 `tiktoken-rs`，根据 `model` 前缀分发：`gpt-5*` / `gpt-4*` → `o200k_base` / `cl100k_base`；`claude-*` → `cl100k_base`（近似）
- **Sub-task 1.3.2.3**：`tokenizer/cache.rs`：`LruCache<(ModelId, [u8; 32]), u32>`（blake3 hash 文本），默认容量 10_000 entries
- **Sub-task 1.3.2.4**：`tokenizer/wire.rs::tokenize_tool_def(tool: &ToolDef) -> u32`：把 `ToolDef` 重组成 Anthropic wire format JSON（`{name, description, input_schema}`）再 tokenize；Claude 与 OpenAI 的 wire format 不同，按 agent 分支
- **Sub-task 1.3.2.5**：feature `anthropic-api`（`tokenizer/anthropic_api.rs`）：调 `https://api.anthropic.com/v1/messages/count_tokens`，需 `ANTHROPIC_API_KEY` env；用 `reqwest` blocking client（避免在 lib 强引入 tokio）—— 或用 feature 触发 tokio
- **Sub-task 1.3.2.6**：单元测试：缓存命中 / 缓存 miss / 模型分发正确 / unknown model 返回 `CoreError::UnknownModel`
- **Sub-task 1.3.2.7**：integration 测试（feature `anthropic-api` 下，需 env）：与本地估算差异 ≤ ±10%（SM-3）

#### Task 1.3.3：analyzer 模块 —— bucket 计算

- **Sub-task 1.3.3.1**：`analyzer/buckets.rs::compute_token_buckets(session: &RawSession, tokenizer: &Tokenizer) -> Vec<TokenBucket>`：每个 assistant turn 一份 bucket
- **Sub-task 1.3.3.2**：分类规则：
  - `system` = system message 文本的 token 数
  - `tools_schema` = `Σ tokenize_tool_def(tool)` for all loaded tools（每个 assistant turn 都加，反映重复发送的真实成本）
  - `history` = 该 turn 之前所有 user/assistant turn 的 token 数累计（cache_read 部分单独减去）
  - `user` = 当前 user turn 文本
  - `tool_result` = 上一个 tool_result block 的 content tokens
  - `assistant_output` = 当前 assistant turn 的输出 tokens（含 tool_use 块的 args JSON）
  - `cache_read` / `cache_creation`：直接来自 `ApiUsage`
- **Sub-task 1.3.3.3**：边界：第一个 assistant turn 无 `history` / `tool_result`（应为 0）
- **Sub-task 1.3.3.4**：fixture 单元测试：构造已知 session 验证每个 bucket 数值

#### Task 1.3.4：analyzer 模块 —— ROI + utilization + waste

- **Sub-task 1.3.4.1**：`analyzer/utilization.rs::schema_utilization(session, tokenizer) -> f32`：`Σ schema_tokens(called_tools) / Σ schema_tokens(loaded_tools)`，分母为 0 时返回 `0.0`
- **Sub-task 1.3.4.2**：`analyzer/roi.rs::compute_roi(session, tokenizer) -> Vec<RoiRow>`：对每个 `ToolDef` 算 `schema_tokens` + 调用次数 + 平均 result tokens
- **Sub-task 1.3.4.3**：`RoiScore` 打分：`call_count == 0` → `Wasted`；其余按 `tokens_per_call = schema_tokens / max(call_count, 1)` 排序，按四分位 → `Star1..Star5`
- **Sub-task 1.3.4.4**：`analyzer/waste.rs::waste_estimate_usd(session, pricing, tokenizer) -> f32`：
  ```
  Σ over unused_tools (
      schema_tokens(tool) × assistant_turn_count × input_price_per_token(model)
  )
  ```
  `assistant_turn_count` 取 session 中 assistant role turn 的数量
- **Sub-task 1.3.4.5**：`analyzer/pricing.rs::Pricing`：`HashMap<ModelId, ModelPricing { input_per_million_usd, output_per_million_usd }>` + 内置默认（claude-sonnet-4.5 = 3/15, gpt-5 = 1.25/10）
- **Sub-task 1.3.4.6**：单元测试：构造已知 session，验证 utilization 准确到 0.001 + waste 准确到 0.01 USD

#### Task 1.3.5：analyzer 模块 —— aggregate

- **Sub-task 1.3.5.1**：`analyzer/agg.rs::aggregate(sessions: &[RawSession], key: AggregateKey, tokenizer) -> AggregateReport`：把 N 个 session 的 ROI 行按 key 分组累加
- **Sub-task 1.3.5.2**：`AggregateKey::Tool`：按 tool name；`McpServer`：按 ToolSource 提取 server name；`Day`：按 session.started_at 的 yyyy-mm-dd；`Model`：按 session.model
- **Sub-task 1.3.5.3**：输出含每组的 `total_loaded_tokens` / `total_called_tokens` / `utilization` / `aggregate_waste_usd`
- **Sub-task 1.3.5.4**：单元测试：3 个 fixture session 按 4 种 key 聚合，验证累加正确性

#### Task 1.3.6：性能与确定性

- **Sub-task 1.3.6.1**：所有算法**纯函数**（无 IO / 时钟 / 随机数）；用 proptest（feature `proptest-tests`）验证幂等
- **Sub-task 1.3.6.2**：tokenizer 缓存压力测试：10k 不同 (model, text) 调用，cache hit ratio > 80%（命中第二次）
- **Sub-task 1.3.6.3**：单 session（100 turn / 10 tool / 平均 200 token/text）完整 analyze < 500ms（不算 tokenize 首次 warm-up）
- **Sub-task 1.3.6.4**：`AnalysisReport::report_hash`：blake3(`bincode::serialize(&report)`)，相同输入 → 相同 hash

#### Task 1.3.7：文档同步

- **Sub-task 1.3.7.1**：每个公开 fn / struct 带 `///` + `# Examples` + `# Errors`
- **Sub-task 1.3.7.2**：`crates/agentprof-core/README.md` 补"对外接口"段，列出 tokenizer + analyzer + export 模块
- **Sub-task 1.3.7.3**：`docs/internals/tokenizer-strategy.md`（L3 ADR）：cl100k 近似 vs API 的权衡
- **Sub-task 1.3.7.4**：`docs/internals/waste-formula.md`（L3 ADR）：公式推导 + 假设（重复发送 schema 每 turn 一次）
- **Sub-task 1.3.7.5**：`docs/internals/roi-scoring.md`（L3 ADR）：分位数打分规则 + RoiScore 枚举语义
- **Sub-task 1.3.7.6**：CHANGELOG `feat(core): tokenizer + analyzer with ROI / waste / aggregate`

---

### Milestone 1.4：`agentprof-cli` `analyze` 子命令 + Markdown 导出

> **状态**：✅ **已完成**（最后 merge commit `9abd694`；含 4 轮 followups）
>
> 第一个端到端可用的命令。**Phase 0 的"终点"已达成**：跑通 events.jsonl → CopilotEvent parse → derive_episodes → analyze → md/json 报告。
>
> 关联 FR：FR-5.1（`analyze` ✅）/ FR-5.7（退出码 ✅）/ FR-5.8（output 路径 ✅）/ FR-6.1（md ✅）/ FR-6.2（json ✅）| 关联 US：US-1（部分）
>
> **实际交付的 4 个 merge**（按时间顺序）：
>
> | Merge | 内容 |
> |---|---|
> | `feat/m1.4-cli-and-analyzer` | M1.4 初版：`analyze` 子命令 + `AnalysisReport` + `turn_summary` / `tool_rank` / `hook_rank` + md 渲染器（手写非 askama）+ JSON 渲染器 + `--export` / `--section` / `--output` / `--session` / `--root` flag + 4 个 CLI 集成测试。Markdown 通过 `assert_cmd` + insta 锁定。 |
> | `fix/m1.4-audit-followups` (`8399bdd`) | 10 个 audit findings：orphan tool sentinel (`<orphan>`) + `DeriveWarning::PayloadNameMissing` + UUID-typo error + Claude/Codex 不支持时的友好错误 + md cell escape + JSON trailing newline + path-error 不重复 `events.jsonl` + `looks_like_uuid` 严格校验 + `AnalysisReport` JSON round-trip test + ADR-0005 D-1 表修正。 |
> | `feat/turn-metadata-extraction` (`010c9af`) | `Event` trait 加 3 个 method (`payload_model` / `payload_output_tokens` / `payload_mode`)；`Turn` 字段 `model` / `mode` / `output_tokens` 从 `None` 真正填上数据；`derive_episodes` 加 `DeriveState.current_mode` 状态机；14 个 snapshot 重接受 + 1 CLI E2E 测试锁定 `minimal` fixture output_tokens=10。 |
> | `fix/mode-vocabulary-alignment` (`e0318ed`) | `Mode` 词汇对齐真实 Copilot wire：`Interactive / Plan / Autopilot / Unknown(String)`（替换旧的 `Ask / Auto / Expert`）。 |
> | `fix/post-output-audit` (`9abd694`) | 3 个 schema-mismatch parser drops 修复（`HookInput.source` / `UserMessageData.source` / `AssistantMessageData.turn_id` 全部 → `Option<String>`；新增 `parent_tool_call_id`，real-session drop rate 17% → 0%）+ `AnalysisReport.parse_warnings` 让用户能看见 silent drops + `ToolRankRow.is_user_blocking` + md 拆分出 `## User-blocking tools` 区 + `docs/features/privacy.md` 新文档（PII 分级表）+ ADR-0005 §6。 |
>
> **下面 sub-task 段（Task 1.4.1 ~ 1.4.5）保留作历史记录**。实际实现见 `crates/agentprof-cli/src/cmd/analyze.rs` + `crates/agentprof-cli/src/cmd/format/{md,json}.rs` + `docs/superpowers/specs/2026-05-29-m1.4-cli-and-analyzer-design.md` + 4 个 followup spec。

#### Task 1.4.1：CLI 框架 + tracing 初始化

- **Sub-task 1.4.1.1**：`crates/agentprof-cli/src/main.rs` 装 `std::panic::set_hook`（TUI 需要，统一在此装一次）
- **Sub-task 1.4.1.2**：`tracing_subscriber::EnvFilter::from_default_env().or("agentprof=info")` + `fmt::layer().with_ansi(io::stderr().is_terminal())`
- **Sub-task 1.4.1.3**：`clap::Parser` derive `Cli { #[command(subcommand)] cmd: Subcommand }`，placeholder `cmd::analyze::run(...)` 等
- **Sub-task 1.4.1.4**：退出码统一：`fn main() -> ExitCode` → `cmd::*::run` 返回 `Result<ExitCode>`
- **Sub-task 1.4.1.5**：单元测试：`Cli::try_parse_from(["agentprof","--help"])` 成功

#### Task 1.4.2：`config` 子命令（先做这个，analyze 依赖它读路径）

- **Sub-task 1.4.2.1**：`config.rs`：`Config { paths: Paths, tokenizer: TokenizerCfg, pricing: HashMap<ModelId, ModelPricing> }`，全部 `Default` 内置合理值
- **Sub-task 1.4.2.2**：`Config::load()`：优先级 CLI > env (`AGENTPROF_*`) > `~/.config/agentprof/config.toml` > 默认；缺文件不报错
- **Sub-task 1.4.2.3**：`cmd::config::run(action)`：`show` 打印 toml + 来源；`edit` 启动 `$EDITOR`；`path` 输出路径字符串
- **Sub-task 1.4.2.4**：单元测试：合并优先级 / 缺字段用默认值 / 损坏 toml 报错指出位置

#### Task 1.4.3：`analyze` 子命令 ── 解析阶段

- **Sub-task 1.4.3.1**：`cmd/analyze.rs::Args`：`agent: AgentSel`（`auto` / `claude` / `codex` / `copilot`）/ `session: Option<String>` / `path: Option<PathBuf>` / `export: ExportFormat`（默认 `tui`）/ `out: Option<PathBuf>` / `use_anthropic_api: bool`
- **Sub-task 1.4.3.2**：解析 `--agent`：`auto` → `Registry::get_for_auto(&config.paths)`；具名 → `Registry::get(agent)`
- **Sub-task 1.4.3.3**：解析 session 选择：`--session <id>` → 在 discover 结果里按 id 匹配；`--path <p>` → 直接当 SessionRef；无参数 → 最近 mtime
- **Sub-task 1.4.3.4**：错误消息：无可用 session 时建议 "try `agentprof list` to see available sessions"
- **Sub-task 1.4.3.5**：`assert_cmd` 集成测试：fixture 路径 + 各种参数组合 + 退出码 0/1/2

#### Task 1.4.4：`analyze` 子命令 ── tokenize + analyze 阶段

- **Sub-task 1.4.4.1**：构造 `Tokenizer`：根据 `--use-anthropic-api` 或 `config.tokenizer.anthropic_estimator` 选择 local / api
- **Sub-task 1.4.4.2**：调 `analyzer::compute_buckets` + `compute_roi` + `schema_utilization` + `waste_estimate_usd` 拼出 `AnalysisReport`
- **Sub-task 1.4.4.3**：tracing：`info!(session_id, n_turns, n_tools, "analyze begin")` / `info!(utilization, waste_usd, "analyze done in {}ms")`

#### Task 1.4.5：Markdown 导出（FR-6.1）

- **Sub-task 1.4.5.1**：`agentprof-core/src/export/markdown.rs::write_markdown(report, w) -> io::Result<()>`：
  ```markdown
  # agentprof report
  - generated by agentprof v0.1.0 at 2026-MM-DD ...
  - report_hash: <blake3 hex>
  ## Session
  - id: ...
  - model: ...
  - turns: ...
  ## Schema utilization
  utilization = 12.7%  (called 2,340 / loaded 18,432 tokens)
  ## Tool ROI
  | tool | schema | calls | tokens/call | ROI |
  | ...  | ...    | ...   | ...         | ★★ |
  ## Waste estimate
  $3.42/session × N sessions/month = ~$103/month
  ```
- **Sub-task 1.4.5.2**：`export_markdown` 函数支持 writer trait（stdout / 文件 / 内存 buffer）
- **Sub-task 1.4.5.3**：单元测试：构造已知 `AnalysisReport` → 字节一致 snapshot

#### Task 1.4.6：CSV 导出（FR-6.2）

- **Sub-task 1.4.6.1**：`agentprof-core/src/export/csv.rs::write_csv(report, w)`：每个 RoiRow 一行
- **Sub-task 1.4.6.2**：列：`tool,source,schema_tokens,call_count,avg_result_tokens,tokens_per_call,roi_score,estimated_waste_usd`
- **Sub-task 1.4.6.3**：处理 `tool` 名中可能的逗号/引号（用 csv crate 或手写转义）
- **Sub-task 1.4.6.4**：单元测试 + snapshot

#### Task 1.4.7：文档同步

- **Sub-task 1.4.7.1**：`crates/agentprof-cli/README.md` 补 `analyze` / `config` 子命令的用法 + 示例
- **Sub-task 1.4.7.2**：根 `README.md` 加 Phase 0 快速示例：`cargo run -p agentprof-cli -- analyze --agent claude --export md`
- **Sub-task 1.4.7.3**：CHANGELOG `feat(cli): add analyze + config subcommands with md/csv export`

#### M1.4 出口条件 = Phase 0 完成

- ✅ 真实的 `~/.claude/projects/**/*.jsonl` 上跑通 → markdown 报告
- ✅ 用户能看到自己的 schema_utilization 数字（验证假设：如果 >80% 问题不大，<30% 就是强信号）
- ✅ `cargo test --workspace` 全绿

---

### Milestone 1.5：`agentprof-tui` —— 火焰图 + ROI 表（Phase 1 重点）

**Status:** ✅ shipped 2026-05-30 — see [`docs/superpowers/specs/2026-05-30-m1.5-tui-design.md`](../docs/superpowers/specs/2026-05-30-m1.5-tui-design.md) for the events-first refresh that supersedes this section's pre-pivot sub-tasks (tokenizer / RoiScore-5★ / cross-session aggregate are now Phase 3 / M1.6). Panic-safe lifecycle contract: [ADR-0006](../docs/internals/adr-0006-panic-safe-tui.md).

> **状态**：~~❌ 未开始~~ → ✅ shipped 2026-05-30（见上方 **Status** 行）
>
> ratatui 三视图（flamegraph / roi / aggregate），TUI 内绝不 panic（终端 raw mode 恢复）。
> 关联 FR：FR-4.1 ~ FR-4.7 | 关联 US：US-3

#### Task 1.5.1：crate 骨架与 panic-safe 终端生命周期

- **Sub-task 1.5.1.1**：`agentprof-tui/Cargo.toml` 已就绪（依赖 ratatui + crossterm + agentprof-core）
- **Sub-task 1.5.1.2**：`app/mod.rs::AppRunner`：`fn new(report: AnalysisReport) -> Result<Self, TuiError>` / `fn run(self) -> Result<(), TuiError>`
- **Sub-task 1.5.1.3**：`app/terminal.rs::install_panic_hook()`：在进入 raw mode 前调用，覆盖默认 hook 先恢复终端再 abort
- **Sub-task 1.5.1.4**：`app/terminal.rs::enter() / leave()`：成对操作（enable_raw_mode / EnterAlternateScreen + 反操作）
- **Sub-task 1.5.1.5**：单元测试：`leave()` 不 panic 即使从未 `enter()` 过（用 mock backend）
- **Sub-task 1.5.1.6**：手动测试：故意触发 panic → 终端正常恢复（SM-7）

#### Task 1.5.2：事件循环 + 视图切换

- **Sub-task 1.5.2.1**：`app/event.rs::Event`：`Tick` / `Key(KeyEvent)` / `Resize(u16,u16)`
- **Sub-task 1.5.2.2**：`app/state.rs::AppState { current_view: View, scroll: (u16, u16), report: AnalysisReport, aggregate: Option<AggregateReport> }`
- **Sub-task 1.5.2.3**：键位 dispatch：`q` quit / `t` switch to RoiView / `f` FlamegraphView / `a` AggregateView / `r` refresh / `?` Help / 方向键滚动
- **Sub-task 1.5.2.4**：60Hz tick + crossterm `event::poll(Duration::from_millis(16))`
- **Sub-task 1.5.2.5**：单元测试：状态转移正确（视图切换 / 滚动饱和）

#### Task 1.5.3：FlamegraphView

- **Sub-task 1.5.3.1**：`views/flamegraph.rs::render(frame, area, state)`：每 turn 一根堆叠条（horizontal stacking），颜色按 8 种 bucket 类别
- **Sub-task 1.5.3.2**：x 轴 turn 序号 + 总长度（token 数）；y 轴层级（agent → tool_call → sub-call，本 MVP 只做单层 agent）
- **Sub-task 1.5.3.3**：调色板：在 `theme.rs` 定义 8 色（system/tools_schema/history/user/tool_result/output/cache_read/cache_creation），深色优先
- **Sub-task 1.5.3.4**：legend / footer 显示当前 turn 信息（点击切换的 turn 序号）
- **Sub-task 1.5.3.5**：snapshot 测试（`TestBackend 100x30`）：固定 fixture → 像素 hash 锁定

#### Task 1.5.4：RoiView

- **Sub-task 1.5.4.1**：`views/roi.rs::render(...)`：`ratatui::widgets::Table` 渲染 `RoiRow` 列表
- **Sub-task 1.5.4.2**：列：`tool` / `source` / `schema` / `calls` / `tokens/call` / `ROI`（★★★★★ 字符）
- **Sub-task 1.5.4.3**：按 `roi_score` 倒序 + 同分按 `schema_tokens` 倒序
- **Sub-task 1.5.4.4**：高亮当前行（方向键导航），底部状态栏显示选中行的额外信息
- **Sub-task 1.5.4.5**：snapshot 测试

#### Task 1.5.5：AggregateView（依赖 M1.6 的 aggregate 子命令也用同一份 report）

- **Sub-task 1.5.5.1**：`views/aggregate.rs::render(...)`：根据 `AggregateKey` 渲染对应表
- **Sub-task 1.5.5.2**：底部小提示：`utilization < 20%` 标红警告 + 建议（"consider trimming MCP servers"）
- **Sub-task 1.5.5.3**：snapshot 测试

#### Task 1.5.6：CLI 接入 TUI

- **Sub-task 1.5.6.1**：`cmd::analyze::run` 中 `ExportFormat::Tui` 分支：`AppRunner::new(report)?.run()?`
- **Sub-task 1.5.6.2**：CTRL-C 处理（已有 panic hook + signal 安装）
- **Sub-task 1.5.6.3**：`assert_cmd` 集成测试用 `--export md` 跳过 TUI（CI 没 tty）；TUI 单独留手动测试任务

#### Task 1.5.7：文档同步

- **Sub-task 1.5.7.1**：`crates/agentprof-tui/README.md` 补"按键参考"段
- **Sub-task 1.5.7.2**：根 `README.md` 加 TUI 截图（asciinema 或手工绘制）
- **Sub-task 1.5.7.3**：`docs/internals/panic-safe-tui.md`（L3 ADR）：panic hook + leave 顺序的实现细节
- **Sub-task 1.5.7.4**：CHANGELOG `feat(tui): add ratatui flamegraph + ROI + aggregate views`

---

### Milestone 1.6：`list` / `aggregate` / `export` 子命令 + Speedscope/HTML 导出

> **Status (decomposed 2026-05-30):** Original 8-task M1.6 split into smaller milestones:
> - **M1.6.1 ✅ shipped 2026-05-30**: `list` subcommand + 8 M1.5 audit polish items. See [`docs/superpowers/specs/2026-05-30-m1.6.1-list-and-polish-design.md`](../docs/superpowers/specs/2026-05-30-m1.6.1-list-and-polish-design.md).
> - **M1.6.2** (future): `aggregate` subcommand (needs `AggregateReport` type design).
> - **M1.6.3** (future): `watch` subcommand (needs `notify` + concurrency design).
> - **M1.6.4 ✅ shipped 2026-05-31**: Speedscope JSON + HTML report exporters. See [`docs/superpowers/specs/2026-05-31-m1.6.4-speedscope-and-html-export-design.md`](../docs/superpowers/specs/2026-05-31-m1.6.4-speedscope-and-html-export-design.md) + [ADR-0007](../docs/internals/adr-0007-speedscope-export.md).
> - **`export` subcommand** (cancelled): 100% redundant with `analyze --export`; surface removed.
>
> Original sub-task tree preserved below for historical context; task 1.6.1 (list) ✅; task 1.6.4 (Speedscope + HTML) ✅; tasks 1.6.2 / 1.6.5 / 1.6.6 / 1.6.7 in future milestones; task 1.6.3 (export) cancelled.

> **状态**：❌ 未开始
>
> Phase 1 收口：把跨 session 聚合、单独导出命令、Speedscope + HTML 输出全部补齐。
> 关联 FR：FR-5.2 ~ FR-5.6 / FR-6.3 / FR-6.4 | 关联 US：US-4 / US-5 / US-6

#### Task 1.6.1：`list` 子命令

- **Sub-task 1.6.1.1**：`cmd/list.rs::Args { agent, since, limit }`
- **Sub-task 1.6.1.2**：调 `Adapter::discover_sessions` + 过滤 `mtime > now - since` + 排序倒序 + `take(limit)`
- **Sub-task 1.6.1.3**：解析 metadata only（不 tokenize）：读 JSONL 前 N 行抓 model + turn count
- **Sub-task 1.6.1.4**：输出表格（用 `comfy-table` 或手工）：`id / started_at / model / turns / total_tokens / utilization?`
- **Sub-task 1.6.1.5**：`utilization` 列若需精确算需 tokenize，故 list 只显示 `--with-utilization` flag 启用时才算
- **Sub-task 1.6.1.6**：单 session 解析失败 → 跳过 + 末尾 stderr `[!] 3 sessions failed to parse, see RUST_LOG=warn for details`

#### Task 1.6.2：`aggregate` 子命令

- **Sub-task 1.6.2.1**：`cmd/aggregate.rs::Args { agent, by, since, export, out }`
- **Sub-task 1.6.2.2**：discover + parallel parse + tokenize（用 `rayon::par_iter`，控制并发度 = CPU 数）
- **Sub-task 1.6.2.3**：调 `analyzer::aggregate(sessions, key, tokenizer)` → `AggregateReport`
- **Sub-task 1.6.2.4**：根据 `--export` 输出：`tui` → AggregateView；`md` / `csv` / `html` → 对应导出
- **Sub-task 1.6.2.5**：tracing：`info!("aggregated {} sessions in {}ms", n, elapsed)`
- **Sub-task 1.6.2.6**：`assert_cmd` 集成测试：用 ≥3 个 fixture session

#### Task 1.6.3：`export` 子命令

- **Sub-task 1.6.3.1**：`cmd/export.rs::Args { session: String, format, out }`：纯导出，不进 TUI
- **Sub-task 1.6.3.2**：load → analyze → export，复用 `analyze` 子命令的内部逻辑
- **Sub-task 1.6.3.3**：`assert_cmd` 集成测试：每种 format 各一条

#### Task 1.6.4：`watch` 子命令（P1）

- **Sub-task 1.6.4.1**：用 `notify` crate watch agent 默认目录 + 配置覆盖
- **Sub-task 1.6.4.2**：事件 debounce 500ms（防止文件写入分片触发多次）
- **Sub-task 1.6.4.3**：检测到新 jsonl 或 mtime 变化 → 自动 reanalyze 并 push 给 TUI
- **Sub-task 1.6.4.4**：手动测试：跑一个真 Claude 会话，观察 TUI 实时刷新

#### Task 1.6.5：Speedscope JSON 导出（FR-6.3）

- **Sub-task 1.6.5.1**：`agentprof-core/src/export/speedscope.rs`：定义 Rust 结构对应 [Speedscope schema](https://github.com/jlfwong/speedscope/blob/main/src/lib/file-format-spec.ts)
- **Sub-task 1.6.5.2**：选 `evented` 格式（每 turn 一组 OpenFrame / CloseFrame）
- **Sub-task 1.6.5.3**：单位用 `tokens`（不是时间），unit field 设 `"none"`
- **Sub-task 1.6.5.4**：snapshot 测试 + 手动验证：导出后到 [speedscope.app](https://www.speedscope.app/) 拖入能正常显示

#### Task 1.6.6：HTML 报告导出（FR-6.4）

- **Sub-task 1.6.6.1**：`crates/agentprof-cli/templates/report.html`（askama 模板）：HTML 骨架 + CSS + 占位
- **Sub-task 1.6.6.2**：内嵌 d3.js minified（约 280KB，base64 进 `<script>`）
- **Sub-task 1.6.6.3**：JS 端读取嵌入的 JSON（`<script id="data" type="application/json">{{ json_data | safe }}</script>`）渲染火焰图 + 表格
- **Sub-task 1.6.6.4**：`cmd::export::run(format=Html)`：渲染模板 + 写文件
- **Sub-task 1.6.6.5**：snapshot 测试（只比 HTML 关键字段，不比 d3.js bytes）
- **Sub-task 1.6.6.6**：手动测试：浏览器打开 → 火焰图可缩放 + ROI 表可排序

#### Task 1.6.7：四种格式一致性测试（SM-6）

- **Sub-task 1.6.7.1**：`tests/export_consistency.rs`：用同一 fixture 跑四种导出
- **Sub-task 1.6.7.2**：分别 parse 出 `schema_utilization` / `waste_estimate_usd` / 每个 tool 的 `schema_tokens` 数值
- **Sub-task 1.6.7.3**：assert 四种格式数值完全相同（snapshot diff）

#### Task 1.6.8：文档同步

- **Sub-task 1.6.8.1**：`crates/agentprof-cli/README.md` 补 list / aggregate / export / watch 四个子命令
- **Sub-task 1.6.8.2**：根 `README.md` 加 Phase 1 完整示例（4 种导出）
- **Sub-task 1.6.8.3**：`docs/features/html-report.md`（L2）：HTML 报告的依赖、生成、渲染流程
- **Sub-task 1.6.8.4**：CHANGELOG `feat(cli): add list/aggregate/export/watch subcommands` + `feat(core): add speedscope + html exporters`

---

### Milestone 1.7：端到端集成、文档、首次 release

> **状态**：❌ 未开始
>
> 把 MVP 整体跑通、写 user-facing docs、出第一个 release（v0.1.0）。
> 关联 SM：SM-1 / SM-7 / SM-8 / SM-9 / SM-10 | 关联 US：全部

#### Task 1.7.1：端到端真实数据测试

- **Sub-task 1.7.1.1**：用本机真实 `~/.claude/projects/**/*.jsonl` 跑 `agentprof analyze --export tui`，验证 SM-1
- **Sub-task 1.7.1.2**：跑 `agentprof list --since 30d --limit 50`，验证 List 输出正确
- **Sub-task 1.7.1.3**：跑 `agentprof aggregate --by mcp-server --since 30d --export md > my-mcp-roi.md`，验证浪费榜
- **Sub-task 1.7.1.4**：跑 `agentprof export <session> --format speedscope --out s.json` → 拖到 speedscope.app
- **Sub-task 1.7.1.5**：把所有发现的 bug 开 issue（如格式没识别、字段缺失），按 Stage 6 systematic-debugging 流程修

#### Task 1.7.2：CI 收口

- **Sub-task 1.7.2.1**：`ci.yml` 所有 job 全绿（lint / test matrix / deny / docs / docs-sync）
- **Sub-task 1.7.2.2**：`nightly-msrv.yml` 通过（`cargo +1.78 check --workspace --all-features`）
- **Sub-task 1.7.2.3**：在 GitHub 启用 status check required for `main` 分支

#### Task 1.7.3：文档完善

- **Sub-task 1.7.3.1**：根 `README.md`：补 installation（`cargo install agentprof` 或 release binary）+ Quick Start + 截图 + FAQ
- **Sub-task 1.7.3.2**：`docs/cli.md`（L2）：每个子命令的完整参考（参数 + 退出码 + 示例）
- **Sub-task 1.7.3.3**：`docs/configuration.md`（L2）：`~/.config/agentprof/config.toml` 完整字段说明
- **Sub-task 1.7.3.4**：`docs/features/tool-roi-matrix.md`（L2）：ROI 算法的解释 + 怎么看 ROI 表
- **Sub-task 1.7.3.5**：`docs/features/cross-session-aggregate.md`（L2）：跨 session 聚合的使用场景
- **Sub-task 1.7.3.6**：CONTRIBUTING.md 更新（如有新流程项）
- **Sub-task 1.7.3.7**：所有 rustdoc `# Examples` doctest 通过（`cargo test --doc`）

#### Task 1.7.4：v0.1.0 release

- **Sub-task 1.7.4.1**：Stage 8 走 `github-release` skill：决定 SemVer = 0.1.0（首次 release，pre-1.0 ）
- **Sub-task 1.7.4.2**：`CHANGELOG.md` 把 `[Unreleased]` 改为 `[0.1.0] - 2026-MM-DD`，加新空 `[Unreleased]` 段
- **Sub-task 1.7.4.3**：`Cargo.toml` workspace.package.version 改 `0.1.0`
- **Sub-task 1.7.4.4**：commit `chore(release): v0.1.0` + tag `v0.1.0` + push
- **Sub-task 1.7.4.5**：`release.yml` workflow 触发（cargo-dist 多平台 binary）
- **Sub-task 1.7.4.6**：GitHub Release 描述：链接到 README + CHANGELOG + 致谢已用工具（obra/superpowers, github/awesome-copilot, ratatui, tiktoken-rs, ccusage 等）

#### Task 1.7.5：项目宣发（可选）

- **Sub-task 1.7.5.1**：发 r/rust + r/ClaudeAI + r/LocalLLaMA reddit post
- **Sub-task 1.7.5.2**：Tweet 截图 + 卖点（"how much of your context is wasted on MCP schemas?"）
- **Sub-task 1.7.5.3**：在 [Awesome Claude Code](https://github.com/hesreallyhim/awesome-claude-code) / [tonsofskills.com](https://tonsofskills.com) 提 PR 加入索引
- **Sub-task 1.7.5.4**：投 Hacker News（早 8 点 PST 发）

---

## 11. Phase 2 / 3 大纲（后续迭代）

### Milestone 2.1：SQLite 持久化（`agentprof-storage::sqlite`）

> 目标：跨会话/跨命令复用分析结果，避免重复 tokenize。

- 实现 `Db::open_default()`（XDG 路径）+ migration runner（idempotent）
- migrations `001_initial.sql`（sessions / tools_loaded / turn_buckets 三表，schema 与 `architecture.md` §9 一致）
- `Db::upsert_session(&AnalysisReport)`：去重写入（用 `report_hash` 判重）
- `Db::query_sessions_since(duration)`：替代 adapter discover，从 SQLite 取
- `agentprof analyze` 完成后自动持久化（除非 `--no-cache`）
- `agentprof list` / `aggregate` 优先从 SQLite 读，缺失则回 adapter 现算

### Milestone 2.2：OTLP receiver（feature `otlp`）

> 目标：实时订阅 Claude Code OTLP telemetry，替代 jsonl 事后扫描。

- 启用 feature 后多一个子命令 `agentprof ingest-otlp --listen 127.0.0.1:4317`
- 接收 OTLP gRPC（`tonic` + `opentelemetry-otlp` server-side）
- 订阅 `claude_code.token.usage` + `claude_code.tool_decision` 事件
- 实时 push 到 SQLite + TUI (`watch` 模式直接连 OTLP socket)

### Milestone 3.1：ClaudeAdapter（`agentprof-adapters::claude`）

> 原 M1.2 的 ClaudeAdapter 工作。M1.2 events-first pivot 后推迟到 Phase 3。

- 抓 `~/.claude/projects/**/*.jsonl` 真实样本（详见原 §1.2 Task 1.2.1 系列调研）
- 实现 `ClaudeAdapter`：JSONL 是 "最终对话日志"，需要 tokenize tools_schema 才能算 token（依赖 M2.5 tokenizer）
- 注意 schema 与 Copilot 不同：tool_use blocks 嵌在 assistant `content` array 内，`usage` 字段在 message envelope 上
- 重复 M1.2 流程（discover / load / parse / ToolSource 推断）
- 更新 `Registry::register_default_adapters` 加 Claude
- `assert_cmd` 集成测试用 `--agent claude --path <fixture>`

### Milestone 3.2：CodexAdapter（`agentprof-adapters::codex`）

- 抓 `~/.codex/sessions/...` 真实样本
- 重复 M3.1 流程（discover / load / parse / ToolSource 推断）
- 更新 `Registry::register_default_adapters` 加 Codex
- `assert_cmd` 集成测试用 `--agent codex --path <fixture>`

> **Copilot adapter 已在 M1.2 交付（ADR-0001 events-first pivot），不再列入 Phase 3。**

### Milestone 3.3：定价表自动同步

- `xtask sync-pricing`：抓 Anthropic / OpenAI 官方价格页 + Diff 检测 + 写回 `agentprof-core/src/analyzer/pricing.rs`
- 月度 CI cron job 检查

### Milestone 3.4：v1.0.0 release

- 三 agent 全支持 + OTLP receiver + SQLite 持久化全部稳定
- 公开 API 冻结（去掉 `#[non_exhaustive]` 的若干字段确定下来）
- 文档站（mdBook 或 docs.rs landing）
- 在 crates.io 公开发布（`cargo publish` 5 个 crate）

---

## 12. 变更记录

| 日期 | 变更内容 | 原因 |
|---|---|---|
| 2026-05-25 | 初始 PRD + 实施计划版本（覆盖 Phase 0 + Phase 1 MVP） | 项目骨架完成后正式立项 |

---

> **下一步执行入口**：从 Milestone 1.2 开始，按 9 阶段 pipeline 推进（见 `.github/copilot-instructions.md` §5）。
> 推荐第一步：进 Stage 1，invoke `brainstorming` skill 产出 `docs/superpowers/specs/2026-05-26-claude-adapter-design.md`，先决定 Task 1.2.1 的 fixture 数据怎么取、ToolSource 推断规则的细节。
