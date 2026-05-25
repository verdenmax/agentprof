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
- Claude Code: `~/.claude/projects/**/*.jsonl`
- Codex CLI: `~/.codex/sessions/...`
- Copilot CLI: TBD（待调研）

---

## 5. 可视化（四个关键视图）

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

---

## 6. 实施路径

### Phase 0：Prototype（1 个下午）
- [ ] 100 行 Python 脚本
- [ ] 解析 `~/.claude/projects/**/*.jsonl`
- [ ] 每个 session 算出"加载了哪些 tool、调用了哪些"
- [ ] 差集 = 浪费的 tools
- [ ] 输出 Markdown / TSV 报告
- **目的**：验证数据可得性 + 看自己真实的利用率

### Phase 1：MVP（1–2 周，单人）
- [ ] CLI 工具 `agentprof analyze <session-id>`
- [ ] 单 session 火焰图（terminal 用 textual TUI，或 HTML 输出）
- [ ] Tool ROI 表
- [ ] 跨 session 聚合视图

### Phase 2：工程化（再 1 周）
- [ ] 接入 OTLP（订阅 Claude Code 的 telemetry endpoint）
- [ ] 持久化数据库（SQLite）
- [ ] Web dashboard（可选）

### Phase 3：扩展适配（每个 +3 天）
- [ ] Codex CLI 日志解析
- [ ] Copilot CLI 日志解析
- [ ] 通用 OpenAI-compatible 代理拦截模式

---

## 7. 决策点 / 待回答的问题

- [ ] **要不要做产品**？还是只为自己用一次就够？
- [ ] 包名 / 项目名定？候选：`agentprof`、`tool-roi`、`ctxprof`
- [ ] 技术栈：Python（数据生态好）还是 Node/TS（和 ccusage 一致）？
- [ ] 火焰图渲染：terminal TUI、HTML、还是直接接 Perfetto / Speedscope？
- [ ] 是否做成 OTLP receiver，让 Claude Code 直接推数据进来？

---

## 8. 下一步行动

**立即可做**：写 Phase 0 的 prototype，用自己机器上的真实 session 数据验证：
1. JSONL 格式是否稳定可解析
2. tool schema 文本是否容易拿到
3. 自己的 schema 利用率到底是多少（如果 >80%，问题不大；<30% 就是强信号）

跑通后再决定是否进入 Phase 1。
