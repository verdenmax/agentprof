# AI Agent Token Profiler —— 初步计划

> 工作代号：`agentprof` / `tool-roi`（待定）
> 创建时间：2026-05-25
> 状态：构思阶段，未启动开发

---

## 1. 问题陈述

现有 Claude Code / Codex CLI / Copilot CLI 等 agent 工具，对 token 的"流转"只提供粗粒度可视化：

- 内建 `/context`、`/cost` —— 只能看**当前百分比和总数**
- `ccusage`、`claude-monitor` —— 只统计**花了多少钱、用了多少 token**
- Langfuse / Phoenix / Helicone —— 提供**通用 LLM trace 火焰图**，但不知道 client 加载了哪些 tools

**缺失的能力**：
1. 火焰图 / Gantt 风格地看清"system / tools_schema / history / user / tool_result / output"在 context window 里各占多少
2. 一次 session 中：**加载了 N 个 tool，schema 共 X tokens，实际调用了 K 个，未用 N-K 个**
3. **Tool schema 利用率** = 被调用 tools 的 schema 占比 / 总 schema 占比
4. 跨 session 的 ROI 视角：**哪个 MCP server 长期占 token 但从不被用**

本质上是给 AI agent 做的 **profiler + trace 工具**，类比传统 perf flamegraph。

---

## 2. 现状盘点

| 能力 | 现状 | 谁做了 |
|---|---|---|
| 火焰图 / Gantt 风格 trace | ✅ 成熟 | Langfuse、Arize Phoenix、Helicone、LangSmith |
| 每次 API call 的 token 总数分解 | ⚠️ 部分 | Anthropic API 只回 input/output/cache_creation/cache_read，不分类 |
| 本次共加载 N 个 tools，schema 共 X tokens | ⚠️ 自己算 | 无现成工具，需 hook MCP listTools |
| 本次实际调用了 K 个 tools | ✅ | trace 日志可见 |
| 未被调用的 tools（差集） | ❌ | 完全空白 |
| Tool schema 利用率指标 | ❌ | 完全空白 |
| 跨 session 的 MCP server ROI | ❌ | 完全空白 |

**结论**：核心差异化 = 把"加载了什么"和"调用了什么"做差集，给出**ROI 视角**。

---

## 3. 为什么值得做

1. **痛点真实且加剧**：MCP 普及后，常见挂 5+ servers，tool schemas 占用 10k–30k tokens。Anthropic 推 Skills 就是承认这是问题。
2. **目前优化全凭直觉**：用户砍 MCP server 全靠"感觉没用"，缺数据支撑。
3. **可量化为美元**：`未利用 schema tokens × 调用次数 × 单价 = 浪费成本/月` —— 工程团队会买账。
4. **正面无竞争**：`ccusage` 做"花了多少钱"，本项目做"花得值不值"。差异化清晰。

**反向风险**：
- 受众偏窄（重度 agent 用户）
- prompt caching 普及后，重复 schema 的边际成本降到 1/10
- Anthropic 可能官方推出类似功能

---

## 4. 数据采集设计（三条腿）

```
┌──────────────────────────────────────────────────────────────┐
│ 1. Schema snapshot       每个 session 启动时抓一次             │
│    - 解析 CLI 启动日志 / MCP listTools 响应                    │
│    - 用 tiktoken 或 Anthropic count_tokens API                 │
│      算每个 tool 的 token 数                                    │
│                                                               │
│ 2. Call trace           从 JSONL session log 增量读           │
│    - tool_use blocks → 实际被调用的 tool name + 次数            │
│    - usage 字段 → 每次 API call 的 token 明细                  │
│                                                               │
│ 3. Context decomposition 订阅 Claude Code 的 OTLP 流          │
│    - claude_code.token.usage (按 type 分桶)                    │
│    - claude_code.tool_decision 事件                            │
└──────────────────────────────────────────────────────────────┘
                              ↓
        三路数据 JOIN，按 session_id 聚合
                              ↓
        计算 metrics + 渲染火焰图
```

**数据源位置**：
- Claude Code: `~/.claude/projects/**/*.jsonl`（Phase 3）
- Codex CLI: `~/.codex/sessions/...`（Phase 3）
- Copilot CLI: `~/.copilot/session-state/<uuid>/events.jsonl`（M1.2 已实现）

---

## 5. 可视化（五个关键视图）

### 5.1 单 session 火焰图
- x 轴 = turn 序号
- y 轴 = 层级（agent → tool call → sub-call）
- 矩形宽度 = token 数
- 颜色按类别：system / tools_schema / history / user / tool_result / output
- 一眼判断"tools_schema 是不是基线噪音"

### 5.2 Tool ROI 矩阵（核心卖点）

```
tool name          | schema_tokens | calls | tokens_per_call | ROI
─────────────────────────────────────────────────────────────────
filesystem.read    |    420        |  87   |     4.83        | ★★★★★
github.create_pr   |    680        |   2   |   340           | ★★
playwright.click   |   1240        |   0   |     ∞ (waste)   | ✗ kill
```

### 5.3 MCP server 维度聚合
- 哪个 MCP 是"贵且没用"的
- 输出建议：`从 settings 移除 mcp.playwright 可节省 ~3.2k tokens/session`

### 5.4 Schema 利用率时间序列
- `called_tool_schema_tokens / total_schema_tokens` 按天/周
- 指标降到 <20% 时告警："考虑精简 MCP"

### 5.5 MCP Waste split-pane（✅ M1.6.5 + M1.6.6 ship）
- TUI `[5] McpWaste` 视图（按 `5` 切换）：左 40% server 列表（loaded / unused 比 + Tokens 列 + `!` 标记 fully-unused server），右 60% 选中 server 的 per-tool 明细（call_count / Tokens / source）
- Banner 2 行：第 1 行汇总 `Source: <data-source>   Loaded: <count>/<≈?><tokens>   Unused: <count>/<≈?><tokens>   Fully-unused servers: <n>`；第 2 行展示 `Tokenizer: <kind>   Token source: <provenance>`（启发式或 sidecar 精确，`≈` 前缀标识非精确）
- 同时提供独立子命令 `agentprof mcp-waste`（md/json/html）+ `agentprof analyze --section mcp-waste` 嵌入到单 session 报告 + `agentprof aggregate --by mcp-server` 加 `Unused tools` / `Sessions w/0 calls` / `Wasted tokens` 列
- M1.6.6 token-cost：`--tokens-per-tool <N>`（默认 200 启发式）+ `--tool-descriptions <path>`（sidecar 文件或目录，命中时走 tiktoken 精确计数），tokenizer 按 session 主导 model 自动推断（`gpt-5*` / `gpt-4o*` / `o1*` / `o3*` → `o200k_base`，否则 `cl100k_base`）
- 设计文档：[ADR-0015](internals/adr-0015-mcp-waste-architecture.md)（架构）+ [ADR-0016](internals/adr-0016-mcp-token-cost-architecture.md)（token cost）

---

## 6. 实施路径

> **进度同步（2026-06-03）**：MVP **8/8 shippable surface ≈ 98% (M1.1–M1.6.4 ✅; 剩 M1.7 v0.1.0 release)** — M1.1 ✅ skeleton / M1.2 ✅ Copilot adapter / M1.3 ✅ Episode aggregation / M1.4 ✅ CLI `analyze` 含 4 轮 followups / M1.5 ✅ TUI + ADR-0006 panic-safe / **M1.6.1 ✅ `list` 子命令 + 8 audit polish** / **M1.6.2 ✅ `aggregate` 子命令 + ADR-0008** / **M1.6.3 ✅ `watch` 子命令 + `aggregate --export tui` 激活 + ADR-0009** / **M1.6.4 ✅ `--export speedscope|html` + ADR-0007（2026-05-31）+ tracing 基础设施 + ADR-0010（2026-06-02）** / 2026-06-03 **M1.6.4 follow-up wave** ✅（8 cleanup commits `d87adec` → `766b8f0`：post-merge audit / `hash_path` env-var L1-only gap fix / crate-boundary 澄清 / B-3 EmitCtx + B-4 ExportWarning + B-5 Display impls + B-6 combination fixtures）。MVP feature work 全部完成，剩 M1.7 v0.1.0 release。详见 [`tasks/ROADMAP.md`](../tasks/ROADMAP.md) 和 [`CHANGELOG.md`](../CHANGELOG.md)。
>
> **events-first pivot（ADR-0001）**：原 Phase 0 / 1 计划见下；实际路径有以下重大调整：
>
> - **M1.2 改做 Copilot adapter**（不是 Claude） — Copilot CLI 的 `events.jsonl` 是事件流，直接含 tool/hook/turn 元数据，比 Claude 的"最终对话日志 + 重做 tokenize"更适合 MVP 快速验证。Claude / Codex adapter 推迟到 Phase 3。
> - **Tokenizer / ROI / waste / aggregate 全部从 Phase 1 推迟到 M1.5+ 或 Phase 2** — events 模型下 `outputTokens` 字段已经能算总账，先把可视化跑通再补 ROI 评分。
> - **TUI 已 ship**（M1.5 ✅）：三视图（Flamegraph / Roi / Aggregate）+ panic-safe lifecycle（ADR-0006）。
> - **M1.6 拆分**（2026-05-30 decomposition）：原 8 子任务的 M1.6 拆为 M1.6.1 (`list` ✅) + **M1.6.2 (`aggregate`) ✅** + **M1.6.3 (`watch` + `aggregate --export tui`) ✅** + **M1.6.4 (Speedscope + HTML) ✅** + M1.6.5 (MCP waste analysis, 新建)；`export` 子命令已取消（与 `analyze --export` 100% 重复）。
>
> 下面的清单**保留原始 Phase 0–3 设计**作为产品愿景；具体里程碑实际进度参见 ROADMAP。

### Phase 0：Prototype（1 个下午）
- [ ] 100 行 Python 脚本
- [ ] 解析 `~/.claude/projects/**/*.jsonl`
- [ ] 每个 session 算出"加载了哪些 tool、调用了哪些"
- [ ] 差集 = 浪费的 tools
- [ ] 输出 Markdown / TSV 报告
- **目的**：验证数据可得性 + 看自己真实的利用率
- **实际**：跳过 Python prototype，直接 Rust workspace；用 Copilot CLI 的 events.jsonl 而非 Claude 的对话日志。

### Phase 1：MVP（1–2 周，单人）
- [x] CLI 工具 `agentprof analyze <session-id>` ✅ M1.4
- [x] 单 session 火焰图（terminal `ratatui` TUI）✅ M1.5（`analyze --export tui`）
- [x] Tool ROI 表 ✅ M1.5（TUI Roi 视图，按 calls/output_tokens/duration 排序）
- [x] CLI 列表子命令 ✅ M1.6.1（`agentprof list` 7 列紧凑表格）
- [x] 跨 session 聚合视图 ✅ M1.6.2（`aggregate --by tool|mcp-server|day|model --export md|json|csv|html`，[ADR-0008](internals/adr-0008-aggregate-report-and-utilization.md)）
- [x] Speedscope JSON + HTML 报告导出 ✅ M1.6.4（`analyze --export speedscope|html`，[ADR-0007](internals/adr-0007-speedscope-export.md)）
- [x] 实时刷新 TUI ✅ M1.6.3（`agentprof watch` 单 session + `watch aggregate ...` 跨 session；`aggregate --export tui` 也一并激活，[ADR-0009](internals/adr-0009-watch-runner-and-notify.md)）
- [x] 全工程结构化 tracing ✅ M1.6.4 (2026-06-02)（canonical observability across all 5 crates；13 `eprintln!` → `tracing::*!`；全局 `--log-level` / `--log-file` flags + `AGENTPROF_LOG_FULL_PATHS` env；TUI 模式自动重定向到 `$XDG_STATE_HOME/agentprof/agentprof.log`；4 层 span 拓扑 `cmd` → `adapter` → `analyzer`/`aggregator` → events；PII：session 路径默认 sha256[..8] hash；[ADR-0010](internals/adr-0010-tracing-infrastructure.md)）
- [x] MCP server waste analysis ✅ M1.6.5（`agentprof mcp-waste`, `analyze --section mcp-waste`, `aggregate --by mcp-server` 加 waste 列, TUI 5th view key `5`，[ADR-0015](internals/adr-0015-mcp-waste-architecture.md)）
- [x] MCP waste token-cost view ✅ M1.6.6（`--tokens-per-tool` heuristic + `--tool-descriptions` sidecar 走 tiktoken 精确计数；3 个子命令统一接入；TUI 5th view banner 2 行 + Tokens 列；aggregate `--by mcp-server` 加 Wasted-tokens 列；[ADR-0016](internals/adr-0016-mcp-token-cost-architecture.md)）

### Phase 2：工程化（再 1 周）
- [~] **M2.1 SQLite 持久化** — 🟡 nearly complete on `feat/m2.1-sqlite-persistence`：hybrid cache/store mode ([ADR-0019](internals/adr-0019-hybrid-storage-mode.md))，`SessionDataSource` trait + dual-path read ([ADR-0018](internals/adr-0018-session-datasource-trait.md))，id-namespace 统一 hotfix ([ADR-0017](internals/adr-0017-unify-session-id-namespace.md))，`agentprof db {init,stats,ingest,prune,vacuum,export}` 子命令家族，3 个全局 flag（`--no-cache` / `--storage-path` / `--quiet`）。`analyze` + `list` + `mcp-waste` 三个 surface 已接入；**aggregate dual-path 推迟到 M2.1.1**（需 Episodes hoist 进 AnalysisReport）。
- [ ] **M2.2 OTLP receiver** — 接入 Claude Code 的 telemetry endpoint。**M2.1 完成后下一站**。`SessionDataSource` trait 已经为 OTLP impl 预留 slot（[ADR-0018](internals/adr-0018-session-datasource-trait.md) "OTLP-ready"）。监听拓扑（gRPC 4317 only vs 也开 HTTP 4318）+ 认证策略需在 M2.2 brainstorming 决定。
- [ ] Web dashboard（可选，M2.3 候选）

### Phase 3：扩展适配（每个 +3 天）
- [ ] Claude CLI 日志解析（原 M1.2，pivot 推后；M3.1）
- [ ] Codex CLI 日志解析（M3.2）
- [x] Copilot CLI 日志解析 ✅ **已在 M1.2 交付，从 Phase 3 提前**
- [ ] 通用 OpenAI-compatible 代理拦截模式

---

## 7. 决策点 / 已解答 + 待回答

### 7.1 已解答（v0.1.x 实际答案 — 2026-06-09 整理）

- [x] **要不要做产品**？→ **做**。v0.1.0 已 release（M1.7，2026-06-08），cargo-dist 多平台 binary + GitHub Release 流程跑通；v0.1.x 持续迭代（M1.6.5 MCP waste + M1.6.6 token-cost + 性能/正确性 audit followup）。
- [x] **包名 / 项目名**？→ **`agentprof`**（v0.1.0 release 时已定，cargo workspace 5 个 crate 全部统一前缀）。
- [x] **技术栈**？→ **Rust 2021，MSRV 1.78**（详见 [`architecture.md`](architecture.md) §2）。`ccusage` 路线（Node/TS）和 Python 路线均放弃 —— Rust 更适合本地 CLI + binary 分发 + TUI 性能 + tokenizer 接入。
- [x] **火焰图渲染**？→ **TUI + HTML + Speedscope 三栖**：terminal `ratatui` TUI（M1.5）+ HTML 静态报告（M1.6.4，`askama` 模板）+ Speedscope JSON（M1.6.4，`analyze --export speedscope` 可直接拖 speedscope.app）。Perfetto 暂不做（Speedscope 对 flame 场景更合适）。
- [x] **是否做 OTLP receiver**？→ **是**，作为 **M2.2** 列入 Phase 2 工程化（详见 §6 Phase 2 + [`tasks/001-mvp-agent-token-profiler.md`](../tasks/001-mvp-agent-token-profiler.md) §11.2）。

### 7.2 待回答（v0.2.0+ 范围）

- [ ] SQLite schema 演进策略：单表 `analysis_reports` JSONB 还是规范化的 sessions/turns/tools 多表？（M2.1 brainstorming 入口决定）
- [ ] OTLP receiver 监听拓扑：gRPC only（4317）还是 HTTP/protobuf 也开（4318）？认证策略？（M2.2 brainstorming 入口决定）
- [ ] Web dashboard：纯静态 HTML 报告（已有）够用，还是要做 server 模式（实时刷新 + SQLite 后端）？（如做则 M2.3）
- [ ] 是否要内置定价表，把 token-cost 翻译成 actual $？（M3.3 在 Phase 3 已列出，需先确定 SLA：每月手动 sync 还是 cron 自动）
- [ ] `crates.io` 公开发布时机：等 v1.0.0 API 冻结，还是 v0.2.0 / v0.3.0 就开始 publish 占坑？

---

## 8. 下一步行动

> **2026-06-10 更新**：v0.1.0 已 release，v0.1.x 增量（M1.6.5 + M1.6.6 + audit + docs sweep）累积 73 commits on main；**M2.1 SQLite 持久化在 `feat/m2.1-sqlite-persistence` 分支 nearly complete**（hybrid mode / `SessionDataSource` trait / `db` 子命令家族 / 3 全局 flag / id-namespace hotfix 全部 ship；详见 [ADR-0017](internals/adr-0017-unify-session-id-namespace.md) / [ADR-0018](internals/adr-0018-session-datasource-trait.md) / [ADR-0019](internals/adr-0019-hybrid-storage-mode.md)）。**当前位置**：M2.1 进入 T8 文档同步阶段，下一站 **M2.2 OTLP receiver**（Phase 2 第二条腿）。

**当前位置**：
- ✅ M1.7 v0.1.0 release（2026-06-08，cargo-dist 多平台 binary）
- ✅ M1.6.5 MCP server waste analysis（[ADR-0015](internals/adr-0015-mcp-waste-architecture.md)）
- ✅ M1.6.6 MCP waste token-cost view + tiktoken-rs 接入（[ADR-0016](internals/adr-0016-mcp-token-cost-architecture.md)）
- ✅ Audit followup（A1 `WasteComputeContext::with_bpe` 性能 + B1-B4 正确性 + Windows CI cfg(unix)）
- ✅ v0.1.x 文档全量同步（2026-06-09 doc sweep wave）
- 🟡 **M2.1 SQLite 持久化** — nearly complete（branch `feat/m2.1-sqlite-persistence`）

**下一步推荐**（按 ROI 排序）：

1. **M2.1 合并 + v0.2.0 tag**（小事，merge + 1 tag）：M2.1 文档同步完成 (T8.2) 后合 main，CHANGELOG `[Unreleased]` → `[0.2.0] - 2026-MM-DD`，cargo-dist 自动出 binary，走 [`github-release`](../.github/skills/github-release/SKILL.md) skill
2. **M2.1.1 follow-up** — aggregate dual-path 接入（hoist `Episodes` into `AnalysisReport`，让 `aggregate` 也走 SQLite 缓存）。是 M2.1 已知的 known limitation：当前 `agentprof aggregate ...` 不 benefit from cache，因为聚合需要 per-call durations / per-event timestamps 等 `Episodes` 数据，而 `AnalysisReport` 不携带。详见 [ADR-0018](internals/adr-0018-session-datasource-trait.md) "Consequences › Neutral"。
3. **M2.2 OTLP receiver**（中期，~1 周，Phase 2 第二腿）：订阅 Claude Code telemetry endpoint。**已被 M2.1 trait 设计预留** —— `SessionDataSource` trait 在 [ADR-0018](internals/adr-0018-session-datasource-trait.md) 明确为 OTLP impl 留 slot，只需新增一个 trait impl 即可接入 cli。监听拓扑 + 认证策略走 brainstorming。
4. **Phase 3 扩展适配**（每个 +3 天）：M3.1 ClaudeAdapter（覆盖最大用户群）+ M3.2 CodexAdapter；骨架已在，缺接入

**已知限制（M2.1 → M2.1.1 follow-up 待解）**：
- `aggregate` （全部 4 个 `--by` 子模式）仍走单路径 adapter 读取，不 benefit from SQLite 缓存。原因：跨 session 聚合需要 per-call durations 等 `Episodes` 数据，而 `AnalysisReport` 不携带。Fix 路径：把必要 Episodes 字段 hoist 进 `AnalysisReport`（参考 M2.1 T5.2.5 已经 hoist 的 `loaded_mcp_tools`），再让 `cmd::aggregate` 走 `build_data_source(...)`。落点：**M2.1.1**。

**进入下一个 milestone 入口**：走 9 阶段 pipeline 的 Stage 1（brainstorming）。在 `docs/superpowers/specs/` 写 `2026-XX-XX-m2.x-<topic>-design.md` 或 `2026-XX-XX-m3.x-<topic>-design.md`。


**已 ship 里程碑的关键问题答复（历史档案）**：
- ratatui 火焰图组件 → M1.5 选择手写 `Block + Paragraph + Constraint` 组合（ADR-0006）。
- ROI 评分公式 → M1.5 在没 tokenizer 时退化为 `calls × output_tokens × duration` 复合排序。
- TUI 数据源 → M1.5 复用 `AnalysisReport`，新增 `Episode` 派生层（M1.3）。
- 交互 → M1.5 实现三视图（Flamegraph / Roi / Aggregate）+ 1/2/3 切视图 + 排序键 + ↑↓ 选 turn + Enter drill-down + ? 帮助 overlay。

**历史背景（原 §8 文字保留作记录）**：当时的"立即可做"是写 Phase 0 prototype 验证 JSONL 可解析性 + tool schema 提取 + 自身利用率基线。这三个问题已被 M1.2 + M1.3 完整回答（Copilot CLI events.jsonl 100% 可解析；events 模型下 tool 调用直接可读，无需 schema 提取）。
