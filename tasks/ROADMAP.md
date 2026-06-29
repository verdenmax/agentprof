# agentprof Roadmap

> **本文件是项目总入口。** 如果你是第一次进入本仓库（或时隔一段时间回来），**先读这里**，再去任何其他文档。
>
> **文件名**：`tasks/ROADMAP.md`
> **版本**：1.9
> **最后更新**：2026-06-28
> **当前 commit**：`main` HEAD `9df9573`（origin/main 已推送；v0.3.3 已发布 + [Unreleased] M2.3.x visual-guide）
> **当前阶段**：**Phase 2 工程化基本完成**。已发 7 个 tag（v0.1.0–v0.3.3）：v0.2.0 = M2.1 SQLite 持久化 + M2.1.1 aggregate dual-path（[ADR-0017](../docs/internals/adr-0017-unify-session-id-namespace.md)–[ADR-0020](../docs/internals/adr-0020-aggregate-dualpath.md)）；v0.2.1 = M2.2 OTLP receiver（[ADR-0021](../docs/internals/adr-0021-otlp-receiver-architecture.md)）；v0.3.0 = M2.4 OTLP 安全加固（[ADR-0022](../docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md)）；v0.3.1 = M2.5 cache analytics（[ADR-0023](../docs/internals/adr-0023-cache-metrics.md)）；v0.3.3 = M2.3 web dashboard `serve`（[ADR-0024](../docs/internals/adr-0024-web-dashboard-architecture.md)）。最新 **[Unreleased]**：M2.3.x visual-guide HTML 教程（[ADR-0025](../docs/internals/adr-0025-visual-guide.md)）。
> **下一步入口**：**Phase 3 v0.4.0 multi-agent** —— M3.1 ClaudeAdapter（Claude wire 含 tools array，可解锁 schema_utilization）+ M3.2 CodexAdapter（详见 [`docs/plan.md`](../docs/plan.md) §8）
>
> **重大 pivot**（ADR-0001 events-first，详见 §4.1 / §4.2）：M1.2 不再做 ClaudeAdapter，改做 **CopilotAdapter**（real wire data 直接可得）；tokenizer / ROI / waste / aggregate 全部从 M1.3 推迟到 M1.5+。Claude / Codex / Gemini 适配器推迟到 Phase 2 / 3。

参见 [`docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md`](../docs/superpowers/specs/2026-06-09-m2.1-sqlite-persistence-design.md) 与 [`docs/plan.md`](../docs/plan.md) §6 Phase 2 / §8 next steps。

---

## 0. TL;DR

**agentprof** = 给 AI agent 用的 perf flamegraph + ROI 报告器。读 Claude / Codex / Copilot CLI 留下的 session 日志，把 context window 里 `system / tools_schema / history / user / tool_result / output` 各类 token 占比算清楚，标出"加载了但从没被调用"的 tool，导出 TUI / Speedscope / HTML / Markdown / CSV。

差异化：市面同类工具（ccusage 65k⭐ / tokscale 3.2k⭐ / splitrail）都在做 "花了多少 token"，本项目做 "**花得值不值**"。

技术栈：Rust 2021，MSRV 1.78，Cargo workspace（5 crate + xtask），ratatui TUI，tiktoken-rs tokenizer，rusqlite 持久化，双协议 MIT OR Apache-2.0。

---

## 1. Document Map（文档导航）

本项目实行 **L1/L2/L3 三级文档体系**（详见 [`docs/architecture.md`](../docs/architecture.md) §14）。所有文档按下表分类：

### 1.1 L1 — 项目级权威文档

| 文档 | 角色 | 何时读 |
|---|---|---|
| [`tasks/ROADMAP.md`](./ROADMAP.md) | **项目总入口**（本文件） | **任何人，第一次进项目** |
| [`docs/plan.md`](../docs/plan.md) | 产品愿景 / 市场现状 / 路线图 | 想了解"为什么做" |
| [`docs/architecture.md`](../docs/architecture.md) | 代码架构权威（18 节，1322 行） | 想动代码 / 想了解"怎么做" |
| [`tasks/001-mvp-agent-token-profiler.md`](./001-mvp-agent-token-profiler.md) | MVP PRD：US / FR / Milestone / Sub-task 三级粒度（Phase 1 MVP，已发 v0.1.0） | 回顾 MVP 范围；Phase 2/3 进度见 plan.md §6 / §8 |
| [`.github/copilot-instructions.md`](../.github/copilot-instructions.md) | AI 助手必读规则（9 阶段 pipeline） | AI 助手 / 想了解开发流程 |
| [`README.md`](../README.md) | 用户向（安装 + Quick Start） | 想使用工具的最终用户 |
| [`CHANGELOG.md`](../CHANGELOG.md) | Keep-a-Changelog 格式 | 想看历史变更 |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | 贡献规则（4 大铁律） | 想提 PR |

### 1.2 L2 — Crate / Feature / Adapter 级文档

| 类别 | 路径模式 | 内容 |
|---|---|---|
| Crate 文档 | `crates/<name>/README.md`（每个 crate 必有） | 该 crate 做什么、对外接口、模块表、依赖、本地命令 |
| Adapter 贡献指南 | [`docs/adapters.md`](../docs/adapters.md) | 怎么加新 agent 的 adapter |
| 跨 crate feature | `docs/features/<feature>.md` | 跨多个 crate 的功能（HTML 报告、OTLP 等） |
| VS Code Copilot 指令 | `.github/instructions/*.instructions.md` | Stage 0 always-on rules（rust + docs-on-code-change） |

### 1.3 L3 — 实现细节 / ADR

| 类别 | 路径模式 | 内容 |
|---|---|---|
| 公开 API 文档 | rustdoc `///` + `# Examples` | 每个 `pub fn` / `pub struct` 必有，缺 `# Examples` = CI fail |
| 算法 / 决策记录 | `docs/internals/<topic>.md`（ADR 风格） | 算法推导、为什么这么做、被否决的方案 |

### 1.4 Process artifacts（pipeline 产物，不属于 L1/L2/L3 任一）

> 这些是 `brainstorming` / `writing-plans` skill 的产物 —— "**permanent record of original intent**"，原则上 merge 后不再编辑。决策结果会反向落进 ADR (L3) 或 architecture.md (L1)，但 spec / plan 本身保留作历史快照。

| 类别 | 路径 | 内容 |
|---|---|---|
| Brainstorming 设计 | `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md` | Stage 1 产物：方案对比、决策理由 |
| 实施计划 | `docs/superpowers/plans/YYYY-MM-DD-<topic>.md` | Stage 3 产物：multi-step plan + review checkpoints |
| 隐私 / PII 详情 | [`docs/features/privacy.md`](../docs/features/privacy.md) | 列在 L2 features 但 §6.1 也作为 L-1 限制的详情入口 |

### 1.5 AI Agent 指南

| 文件 | 角色 |
|---|---|
| [`.github/copilot-instructions.md`](../.github/copilot-instructions.md) | AI 助手 entry point（含 9 阶段 pipeline + 19 个 skill 清单） |
| [`.github/skills/<name>/SKILL.md`](../.github/skills/) | 5 个项目级 skill（入 git，跟随 clone） |
| [`.github/instructions/*.instructions.md`](../.github/instructions/) | 2 个常驻 always-on 规则 |
| obra/superpowers plugin（全局） | 14 个全局 skill（`~/.copilot/installed-plugins/_direct/obra--superpowers/`） |

### 1.6 Task 文件目录（详见 §3）

按 `NNN-<scope>.md` 编号，每个文件 = 一个完整的 PRD + 实施计划。

---

## 2. Project Phases（时间线 + 当前位置）

### 2.1 时间线总览

```
Phase 0   "100 行 Python 脚本"，验证数据可得性             ───┐
                                                              │ MVP
Phase 1   CLI + TUI 火焰图 + ROI 表 + 跨 session 聚合     ───┤   001-mvp-agent-token-profiler.md
                                                              │
─────────────────────────────────────────────────  v0.1.0 ────┘

Phase 2   工程化：SQLite 持久化 + OTLP receiver + watch
                                                                  002-phase2-engineering.md (TBD)
─────────────────────────────────────────────────  v0.2.0 ────

Phase 3   扩展适配：Claude / Codex / Gemini（Copilot 已在 M1.2 提前交付）
          pricing 自动同步 + 三 agent 全支持
                                                                  003-phase3-multi-agent.md (TBD)
─────────────────────────────────────────────────  v1.0.0 ────
```

> **说明**：上图为**原始规划**时间线。Phase 0 + Phase 1 合并为 MVP（`001-mvp-agent-token-profiler.md` 覆盖，**✅ v0.1.0 已发**）；Phase 2 工程化 **✅ 基本完成**（跨 v0.2.0–v0.3.3）；Phase 3 multi-agent 从 **v0.4.0** 起（未启动）。实际进度以 §2.2 / §2.3 为准。

### 2.2 当前位置

| 维度 | 当前状态 |
|---|---|
| **Git** | `main` 分支 HEAD `9df9573`（origin/main 已推送；运行 `git log -1 --oneline` 查看最新）；最近 milestone：v0.2.0 M2.1 SQLite + M2.1.1 dual-path → v0.2.1 M2.2 OTLP receiver → v0.3.0 M2.4 OTLP 加固 → v0.3.1 M2.5 cache analytics → v0.3.3 M2.3 web dashboard → [Unreleased] M2.3.x visual-guide |
| **Crate** | 5 lib/bin + 1 xtask 全部已实现。`agentprof-core` / `agentprof-adapters` / `agentprof-cli` / **`agentprof-tui`**（M1.5，flamegraph/roi/aggregate/models/turn_detail/mcp_waste 视图 + panic-safe lifecycle，[ADR-0006](../docs/internals/adr-0006-panic-safe-tui.md)）/ **`agentprof-storage` ✅ 已激活**（M2.1 起：SQLite hybrid cache/store + migrations + OTLP receiver，**非骨架**） |
| **Phase** | Phase 1 MVP **✅ 全部完成**（M1.1–M1.7，v0.1.0 已发；M1.6.5/.6 mcp-waste 虽属 Phase 1 milestone 但随 v0.2.0 发布）；Phase 2 工程化 **✅ 基本完成**（M2.1 SQLite / M2.1.1 dual-path / M2.2 OTLP / M2.4 加固 / M2.5 cache / M2.3 web，跨 v0.2.0–v0.3.3）；Phase 3 multi-agent ❌ 未开始；`export` 子命令已取消 |
| **测试** | **1328 tests pass**（`cargo test --workspace --all-features` 验证），含 insta 快照（episode_derive / analyzer_on_fixtures / CLI 集成 / 单元）+ Copilot fixtures（持续随发现的 schema 漏洞增补） |
| **CI** | 已配并在 GitHub 运行（lint + test matrix + deny + docs + docs-sync + nightly-msrv + release）；remote 已配 |
| **远端** | origin/main 已推送（HEAD `9df9573`） |
| **Release** | 已发 7 个 tag（v0.1.0–v0.3.3），最新 **v0.3.3**；下次 = **v0.4.0**（Phase 3 multi-agent 起点） |

### 2.3 Phase 完成度仪表盘

| Phase | 任务文件 | Milestone | 完成度 | Release | 状态 |
|---|---|---|---|---|---|
| **0+1 MVP** | 001 | M1.1–M1.7 | **100%**（全部 ✅；speedscope / html / watch / tracing 随 v0.1.0，mcp-waste M1.6.5/.6 随 v0.2.0） | v0.1.0 | 🟢 Done |
| **2** | 002 (TBD) | M2.1–M2.5 + M2.3 web | **~90%**（SQLite / OTLP receiver / 安全加固 / cache analytics / web dashboard ✅；pricing sync 仍未做） | v0.2.0–v0.3.3 | 🟢 基本完成 |
| **3** | 003 (TBD) | M3.1–M3.x | **0%**（multi-agent 未开始） | v0.4.0 → v1.0.0 | ⚪ Planned |
| **Beyond** | 004+ (TBD) | — | — | post-1.0 | 💭 Vision |

> **注意 events-first pivot 的范围调整**（ADR-0001）：原 PRD 把 tokenizer / ROI 矩阵 / waste 估算 / 跨 session aggregate 全部塞进 M1.3；pivot 后这些**全部推迟到 M1.5+ 或 Phase 2**，M1.3 实际只做 schema-audit + Episode 聚合层。M1.4 实际交付的 `agentprof analyze` 输出是 turn / tool / hook 三表 + 14 类 warnings（parse-stage + derive-stage），不含 ROI / waste。

---

## 3. Task File Index（任务文件目录）

> **命名规则**：`NNN-<scope>.md`，编号单调递增（合并冲突时取已合 PR 最大值 +1）。每个 task 文件是一份独立的 PRD + 实施计划，参考 `proteinCopilot/tasks/00X-*.md` 格式。

### 3.1 已存在的 task 文件

| # | 文件 | 范围 | 状态 | Milestone 完成度 | 计划 release |
|---|---|---|---|---|---|
| **001** | [`001-mvp-agent-token-profiler.md`](./001-mvp-agent-token-profiler.md) | **Phase 0 + 1 MVP**：Copilot adapter（pivot from Claude）+ Episode aggregation + CLI `analyze` (md/json) + TUI flamegraph + list/aggregate/watch + speedscope/html 导出 + mcp-waste（M1.6.5/.6，随 v0.2.0 ship） | ✅ Done | **100%**（M1.1–M1.7 全部 ✅，v0.1.0 已发布） | **v0.1.0** |

### 3.2 计划中的 task 文件（占位）

| # | 文件 | 范围（暂定） | 触发条件 |
|---|---|---|---|
| **002** | `002-phase2-engineering.md` (TBD) | Phase 2 工程化：SQLite 持久化、OTLP receiver、cache analytics、web dashboard、pricing 自动同步 | **已实质执行完毕**（M2.1–M2.5 + M2.3 web ship 于 v0.2.0–v0.3.3；task 文件仍 TBD；pricing sync 未做） |
| **003** | `003-phase3-multi-agent.md` (TBD) | Phase 3 多 agent：Codex / Copilot / Gemini / Cursor 适配器、三 agent 全支持 | 002 完成后启动 |
| **004+** | 待规划 | post-1.0 feature（如自造 ratatui-snapshot-testing skill、library-mode API 等；Web dashboard 已于 v0.3.3 提前实现） | v1.0.0 后由社区 / 实际需求驱动 |

### 3.3 每个 task 文件的标准结构

参考 `001-mvp-agent-token-profiler.md`（也参考了 `proteinCopilot/tasks/001-mvp-proteomics-search-platform.md`）：

```
§1 Introduction          项目简介 / 为什么做 / 技术架构概要
§2 Goals                 G1–G5 主要目标 + 商业价值
§3 User Stories          US-1..US-N 含 AC 验收标准
§4 Functional Reqs       FR-1..FR-N 含 P0/P1/P2 优先级 + 完成情况总览
§5 Non-Goals             NG-1..NG-N 明确排除（推迟到 Phase 几）
§6 Design Considerations 交互 + 典型流程 + 数据流约束
§7 Technical Considerations 架构 / 性能 / 错误 / 可复现 / 测试 / 依赖
§8 Success Metrics       SM-1..SM-N 验收 + 质量门
§9 Open Questions        OQ-1..OQ-N + 当前假设
§10 Implementation       Milestone → Task → Sub-task 三级粒度
§11 (可选) 后续大纲      下一 Phase 的 Milestone 雏形
§12 变更记录              单行日期表
```

---

## 4. Milestone Dependency Graph（里程碑依赖图）

### 4.1 当前 MVP（task 001）内部依赖

```
            ┌───────────────────────────────────────────┐
            │ M1.1 项目骨架与 core crate          ✅      │
            │  - Cargo workspace + 5 crate + xtask      │
            │  - 文档体系 L1/L2/L3                       │
            │  - 9 阶段 skill pipeline                   │
            │  - CI 骨架（fmt + clippy + test + docs）   │
            └─────────────────┬─────────────────────────┘
                              │
                ┌─────────────┼─────────────┐
                ▼             ▼             ▼
       ┌──────────────┐ ┌──────────────┐ ┌────────────┐
       │ M1.2 Copilot │ │ M1.3         │ │ M1.5 TUI   │
       │  adapter ✅  │ │ schema-audit │ │  (depends  │
       │ (pivot per   │ │ + Episode    │ │   on M1.3) │
       │  ADR-0001)   │ │  aggregation │ │  ✅        │
       │              │ │  ✅          │ │            │
       └──────┬───────┘ └─────┬────────┘ └─────┬──────┘
              │               │                │
              └───────┬───────┘                │
                      ▼                        │
              ┌──────────────────────────┐     │
              │ M1.4 CLI `analyze`       │     │
              │ + md/json renderer ✅    │     │
              │ + 4 轮 followups merged  │     │
              │   (audit / turn-meta /   │     │
              │    mode / post-output)   │     │
              │ [Phase 0 出口] ✅        │ ◄───┘ (M1.5 内嵌进 analyze 的 TUI 分支)
              └─────────┬────────────────┘
                        │
                        ▼
              ┌──────────────────────────────────────┐
              │ M1.6 list / aggregate / export       │
              │      / watch + speedscope + html     │
              │ ✅ [Phase 1 完成]                    │
              │   (M1.6.1 / .2 / .3 / .4 全部 ship;  │
              │    .5 MCP waste 推到 0.2.0)           │
              └─────────┬────────────────────────────┘
                        │
                        ▼
              ┌──────────────────────────────────────┐
              │ M1.7 E2E 集成 + 文档 + v0.1.0 release│
              │ ✅ [MVP 出口]                        │
              └──────────────────────────────────────┘
```

### 4.2 跨 task 依赖（高层）

```
task 001 (MVP)                                 task 002 (Phase 2)
─────────────────────                          ─────────────────────
M1.1 ✅ skeleton                               M2.1 ✅ SQLite persistence (v0.2.0)
M1.2 ✅ copilot adapter      ┌───────────►    M2.2 ✅ OTLP receiver (v0.2.1)
M1.3 ✅ episode aggregation  │                M2.3 ✅ web dashboard serve (v0.3.3)
M1.4 ✅ CLI analyze + md     │                M2.4 ✅ OTLP 安全加固 (v0.3.0)
M1.5 ✅ TUI views            │                M2.5 ✅ cache analytics (v0.3.1)
M1.6.1 ✅ list 子命令         │                       │
M1.6.2 ✅ aggregate 子命令    │
M1.6.3 ✅ watch 子命令        │
M1.6.4 ✅ Speedscope + HTML 导出 (2026-05-31) + tracing 基础设施 (2026-06-02, ADR-0010)
M1.6.5 ✅ MCP waste analysis (v0.2.0)
M1.7 ✅ v0.1.0 release ──────┘                       │ release
                                                     ▼
                                              v0.2.0
                                                     │
                                                     ▼
                                              task 003 (Phase 3)
                                              ─────────────────────
                                              M3.1 ❌ Claude adapter
                                              M3.2 ❌ Codex adapter
                                              M3.3 ❌ Gemini adapter (?)
                                              M3.4 ❌ v1.0.0 release
```

> Copilot adapter 已在 M1.2 交付（ADR-0001 events-first pivot），不再列入 Phase 3。Phase 3 现在只剩 Claude / Codex / Gemini。

### 4.3 出口判据（每个 task 推进到下一个 task 的条件）

| 当前 task | 出口判据 | 触发下一 task |
|---|---|---|
| **001 (MVP)** | ✅ M1.7 v0.1.0 tag 推送 + release.yml 绿 + 三平台 binary 上传 | 启动 **002** |
| **002 (Phase 2)** | ✅ M2.x SQLite + OTLP + watch + pricing 全部稳定 + v0.2.0 释放 | 启动 **003** |
| **003 (Phase 3)** | ✅ 三 agent 全支持 + 公开 API 冻结 + v1.0.0 释放 | 进入维护模式 / 启动 004+ |

---

## 5. Release Cadence（发布节奏）

### 5.1 SemVer 规则

遵循 [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html)：

| 版本号 | 含义 |
|---|---|
| `MAJOR` (X.0.0) | 不兼容的公开 API 变化 |
| `MINOR` (0.X.0) | 向后兼容的功能新增（Pre-1.0 期允许小范围 breaking） |
| `PATCH` (0.0.X) | 向后兼容的 bug fix |

Pre-1.0（即 `0.X.Y`）期间，允许 minor bump 包含 breaking change（但需在 CHANGELOG 用 `BREAKING:` 显式标注）。

### 5.2 计划版本

| 版本 | 关联 task | 计划 release 内容 | 状态 |
|---|---|---|---|
| **v0.1.0** | 001 M1.7 | MVP：**Copilot** adapter + TUI + 5 种导出 + CLI **4** 个子命令（analyze/list/aggregate/watch） | ✅ Released (2026-06-06) |
| **v0.1.x** | 001 M1.7 后 | bug fix 滚动 release | — |
| **v0.2.0** | M2.1 + M2.1.1 | SQLite 持久化 + aggregate dual-path | ✅ Released |
| **v0.2.1** | M2.2 | OTLP receiver（Claude Code telemetry） | ✅ Released |
| **v0.3.0** | M2.4 | OTLP 安全加固（supersedes v0.2.1） | ✅ Released |
| **v0.3.1** | M2.5 | cache analytics（Cache% / saved-tokens） | ✅ Released |
| **v0.3.3** | M2.3 | web dashboard `serve` | ✅ Released |
| **v0.4.0** | 003 M3.1+ | Phase 3 multi-agent（Claude / Codex adapter）起点 | ⚪ Planned |
| **v1.0.0** | 003 M3.x | 多 agent 全支持 + 公开 API 冻结 + crates.io publish | 💭 Vision |

### 5.3 Release 流程（Stage 8）

按 `.github/copilot-instructions.md` §5 Stage 8 走，invoke `github-release` skill：

```
1. 决定 SemVer bump（major/minor/patch）  ─┐
2. 整理 [Unreleased] → [vX.Y.Z]            │
3. 改 Cargo.toml workspace.package.version │ github-release skill
4. commit "chore(release): vX.Y.Z"         │ 自动化
5. tag vX.Y.Z + push                       │
6. release.yml workflow 触发 cargo-dist    │
7. GitHub Release 描述 + assets 上传      ─┘
```

---

## 6. Known Limitations & Future Work（已知缺陷 + 未来工作清单）

> 本节是 **当前状态的"诚实告白" + 所有 deferred 工作的单一索引**。每条都指向更详细的 doc / ADR / spec / fixture，方便新人 / AI 一站式查找"为什么这里现在长这样、什么时候会改"。

### 6.1 用户可见的已知限制（current limitations）

| # | 限制 | 严重度 | 详细文档 | 计划修复 |
|---|---|---|---|---|
| L-1 | ~~隐私字段默认裸露~~ → **✅ `--privacy redact\|anonymize`** (analyze+aggregate; md/json/csv full, html/speedscope flamegraph deferred) | ✅ FIXED | [ADR-0026](../docs/internals/adr-0026-report-redaction.md) + [privacy.md §4](../docs/features/privacy.md) | — |
| L-2 | **Subagent token over-attribution**：subagent message（`parentToolCallId` 携带，无 `turnId`）的 `output_tokens` 被算到父 turn — 总数对、per-turn 数偏高 | 🟡 MEDIUM | [ADR-0005 §6 "Side effect"](../docs/internals/adr-0005-analyzer-and-payload-name.md#update-6-post-output-audit-fixes-parse-warning-visibility-schema-mismatches-user-blocking-split) + `crates/agentprof-adapters/tests/fixtures/copilot/with-post-tool-use-hooks/README.md` | M1.5+ 增加 `Turn.subagent_output_tokens` 字段拆分 |
| L-3 | **Turn Summary 无分页**：长 session（745+ turns）一次性吐表，终端 / 富文本编辑器 / GitHub 渲染都比较吃力 | 🟡 MEDIUM | [`docs/superpowers/specs/2026-05-29-post-output-audit-design.md`](../docs/superpowers/specs/2026-05-29-post-output-audit-design.md) §3 "Deferred" | M1.5+（与 TUI 一起；TUI 天然分页） |
| L-4 | **CLI 子命令仍少**：`analyze` (M1.4) + `--export speedscope\|html` (M1.6.4 2026-05-31) + `list` (M1.6.1) + `aggregate` (M1.6.2 + `--export tui` M1.6.3) + `watch` (M1.6.3) + global `--log-level` / `--log-file` (M1.6.4 2026-06-02 tracing infra) ✅；`config` ✅ L-4（`config path\|show\|edit\|init`，统一 `resolve_config_path`，[ADR-0027](../docs/internals/adr-0027-config-subcommand.md)）（`ingest-otlp` ✅ v0.2.1 / `db` ✅ v0.2.0 / `serve` ✅ v0.3.3 / `mcp-waste` ✅ 均已加）；`export` 已取消（与 `analyze --export` 重复） | 🟢 DONE | [`crates/agentprof-cli/README.md`](../crates/agentprof-cli/README.md) + [M1.6.1 spec](../docs/superpowers/specs/2026-05-30-m1.6.1-list-and-polish-design.md) + [M1.6.2 spec](../docs/superpowers/specs/2026-06-01-m1.6.2-aggregate-design.md) + [M1.6.3 spec](../docs/superpowers/specs/2026-06-01-m1.6.3-watch-and-aggregate-tui-design.md) + [M1.6.4 Speedscope spec](../docs/superpowers/specs/2026-05-31-m1.6.4-speedscope-and-html-export-design.md) + [M1.6.4 tracing spec](../docs/superpowers/specs/2026-06-02-tracing-design.md) + [config spec](../docs/superpowers/specs/2026-06-28-config-subcommand-design.md) + [ADR-0007](../docs/internals/adr-0007-speedscope-export.md) + [ADR-0008](../docs/internals/adr-0008-aggregate-report-and-utilization.md) + [ADR-0009](../docs/internals/adr-0009-watch-runner-and-notify.md) + [ADR-0010](../docs/internals/adr-0010-tracing-infrastructure.md) + [ADR-0027](../docs/internals/adr-0027-config-subcommand.md) | ✅ DONE（`config` 已 ship，Phase 2 CLI surface 全部完成） |
| L-5 | **无 tokenizer → 无法精确算 token cost / waste**：当前 `output_tokens` 直接读 wire 字段；ROI / 浪费金额、schema_utilization 等 PRD 原 §5.2 卖点全部依赖 tokenizer | 🟡 MEDIUM | [`docs/plan.md`](../docs/plan.md) §6 pivot 备注 + [`tasks/001-mvp-agent-token-profiler.md`](./001-mvp-agent-token-profiler.md) FR-2 表 | M1.5+ 或 Phase 2 |
| L-6 | ~~**TUI 完全未实现**~~ → **✅ 已交付 M1.5**（3 视图：FlamegraphView / RoiView / AggregateView，panic-safe lifecycle，3 insta snapshots + 2 CLI tests） | ✅ FIXED | [`crates/agentprof-tui/README.md`](../crates/agentprof-tui/README.md) + [ADR-0006](../docs/internals/adr-0006-panic-safe-tui.md) + [spec](../docs/superpowers/specs/2026-05-30-m1.5-tui-design.md) + [plan](../docs/superpowers/plans/2026-05-30-m1.5-tui.md) | — |
| L-7 | ~~**无 SQLite 持久化**~~ → **✅ 已实现 M2.1**（v0.2.0：hybrid cache/store + dual-path read + `db` 子命令家族） | ✅ FIXED | [`crates/agentprof-storage/README.md`](../crates/agentprof-storage/README.md) | — |
| L-8 | **只支持 Copilot CLI**：Claude / Codex / Gemini adapter 未实现 | 🟢 EXPECTED | [`crates/agentprof-adapters/README.md`](../crates/agentprof-adapters/README.md) "Supported agents" | Phase 3 (M3.1 Claude / M3.2 Codex) |
| L-9 | **schema 兼容性只在 1 个 frozen session 验证过**：post-output-audit 在 11 806 行 session 上验证了 17 % → 0 % drop rate，但其它 Copilot CLI 版本、其它 session 风格（如纯 sub-agent / 纯交互式 / 长 plan 模式）可能仍有未发现的 schema 漏洞 | 🟡 MEDIUM | [ADR-0005 §6 "Tests"](../docs/internals/adr-0005-analyzer-and-payload-name.md#update-6-post-output-audit-fixes-parse-warning-visibility-schema-mismatches-user-blocking-split) + 现有 20 个 fixture（含 2026-06-03 M1.6.4 follow-up wave B-6 加的 3 个 combination fixtures：`tool-and-skill-same-turn` / `two-skills-one-turn` / `orphan-skill-mix`；以及 B-7 加的 `with-ask-user-mid-session` 锁 `b5c1429` FlamegraphView 修复） | 每发现新 schema 漏洞时增加 fixture（持续工作） |
| L-10 | **`ParseWarning::OutOfOrder` 不带 line_no**：用户看到 "Parse warnings: 1 / OutOfOrder: 1" 后无法快速定位是哪两行时间戳倒置 | 🟢 LOW | `crates/agentprof-core/src/error.rs` `ParseWarning::OutOfOrder` 变体定义 | 视用户反馈，可能 M1.5+ 加 detail |
| L-11 | **`xtask anonymize` / `xtask audit-pii` 不存在**：fixture / report 的脱敏目前全靠人工 `sed`，没有自动化保护 | 🟡 MEDIUM | [`docs/features/privacy.md`](../docs/features/privacy.md) §5 "Future automation" | 待定（可能 Phase 2 与隐私 flag 一起） |
| L-12 | **CI 无 `/home/<user>/` grep guard**：意外 commit 真实路径不会被自动拦截 | 🟢 LOW | 同上 §5 | 待定 |
| L-13 | **Copilot wire 不广播 tool schema 定义**：`session.start` 不含 tools 列表；`tool.execution_start` 只携带 `toolCallId/toolName/arguments/turnId`，没有 `input_schema` / `parameters` 字段 → `schema_utilization` / `waste_usd` / `tokens_per_call` 等 token-cost ROI 指标在 Copilot adapter 上**结构性不可行**，不是 tokenizer 工作量问题 | 🟢 EXPECTED | [`docs/superpowers/specs/2026-05-30-m1.5-tui-design.md`](../docs/superpowers/specs/2026-05-30-m1.5-tui-design.md) §1 ("Empirical Copilot wire limit") | Phase 3 ClaudeAdapter（Claude wire 含 tools array） |

### 6.2 已确认但未列入 task 文件的未来增强（roadmap-adjacent ideas）

| # | 想法 | 触发来源 | 何时考虑 |
|---|---|---|---|
| F-1 | `Turn.subagent_output_tokens` 字段拆分主 / sub-agent 贡献 | L-2 后续 | M1.5+ 与 ROI 一起 |
| F-2 | ~~`--redact` / `--anonymize` CLI flag~~ → **✅ 已实现** as `--privacy <none\|redact\|anonymize>` (含 stable per-session UUID mapping) | L-1 后续 | ✅ L-1 ([ADR-0026](../docs/internals/adr-0026-report-redaction.md)) |
| F-10 | ~~`Episodes::redact`（修 html/speedscope flamegraph 仍漏 turn-ids / MCP server names）+ `list --privacy`（per-session 行的小 PII 面）~~ → **✅ 已实现**（shared `RedactionContext` + `Episodes::redact_with`，analyze html/speedscope 全脱敏，`list --privacy`） | L-1 deferred scope | ✅ F-10 ([ADR-0028](../docs/internals/adr-0028-episodes-redaction.md))，关闭 ADR-0026 deferred |
| F-3 | `Mode` 词汇扩展（更多 Copilot CLI 模式落地后补 variant）| `Mode::Unknown(String)` fallback 设计 | 持续，发现新 mode 就补 |
| F-4 | 通用 OpenAI-compatible 代理拦截模式（不需要每家 adapter） | [`docs/plan.md`](../docs/plan.md) §6 Phase 3 | Phase 3+ |
| F-5 | ~~Speedscope / HTML / CSV 导出~~ → **✅ 已实现** | FR-6 原始设计 | M1.6.2 / M1.6.4 ✅（`analyze --export speedscope\|html`、`aggregate --export csv`） |
| F-6 | ~~OTLP receiver（接 Claude Code telemetry endpoint）~~ → **✅ 已实现** | [`docs/plan.md`](../docs/plan.md) §6 Phase 2 | M2.2 ✅ v0.2.1 |
| F-7 | 价格表自动同步（`xtask sync-pricing`） | [`tasks/001-mvp-agent-token-profiler.md`](./001-mvp-agent-token-profiler.md) §11 Milestone 3.3 | Phase 3+ |
| F-8 | ~~Web dashboard~~ → **✅ 已实现** | [`docs/plan.md`](../docs/plan.md) §6 Phase 2 | M2.3 ✅ v0.3.3（`serve`） |
| F-9 | OpenTelemetry trace 联动（agentprof report → Tempo / Jaeger） | 由 §7 长期愿景导出 | post-1.0 |

### 6.3 设计决策（user 明确要求保留当前行为，不改）

| # | 行为 | 决策理由 | 来源 |
|---|---|---|---|
| D-1 | Turn ID 输出全 UUID（不截断成 8 字符 + ……） | 用户要求保留全长；某些工作流需要拷贝 UUID 精确匹配 | 2026-05-30 对话 |
| D-2 | `ask_user` 单独建 `## User-blocking tools` 区而非完全隐藏 | 用户思考时间也是 session 的真实成本，不能藏；但要拆出来避免冲乱 Tool Rank | ADR-0005 §6 |
| D-3 | Subagent token 计入父 turn `output_tokens`（不忽略） | 主 turn 调用了 subagent，token 是它"间接花的钱"；忽略会让总数对不上 | ADR-0005 §6 "Side effect" |

> 添加新条目规则：每新发现一个限制就在 6.1 加一行；每新提出一个想法就在 6.2 加一行；每被用户明确要求保留的现状就在 6.3 加一行。**不要让限制"沉默"在 commit message 里**。

---

## 7. How to Use This Roadmap（怎么用本文件）

### 7.1 我是新人 / AI 第一次进项目

1. 读完本文件 §0 TL;DR
2. 看 §1 文档地图，按需读 L1 文档（plan.md → architecture.md）
3. 看 §2 知道当前位置
4. 看 §3 task 索引找到对应任务文件
5. 看 §6 知道当前缺陷 + 未来工作（避免重复提出已知问题）

### 7.2 我要继续开发（推进当前 task）

1. 看 §2.2 当前位置 → 知道在哪个 milestone
2. 进 `tasks/001-mvp-agent-token-profiler.md` 找该 milestone 的 Task / Sub-task
3. 按 `.github/copilot-instructions.md` §5 的 **9 阶段 pipeline** 推进：
   - Stage 1 brainstorming → spec
   - Stage 2 ADR（若有候选方案）→ adr-NNNN
   - Stage 3 writing-plans → plan
   - Stage 4 TDD 实现
   - Stage 7 verification → PR
   - Stage 8（release 时）github-release

### 7.3 我要新增一个 feature

1. 判断是 Phase X 内的新功能 还是 整个新方向：
   - **Phase X 内**：进当前 task 文件加 Milestone / Task / Sub-task
   - **新方向**：在 `tasks/` 下新建 `NNN-<scope>.md`（编号 +1），按 §3.3 标准结构填充，并回到本文件 §3.1 加一行
2. 走 9 阶段 pipeline（brainstorming → planning → ...）

### 7.4 我要 release

1. 看 §5.2 当前应该 release 哪个版本号
2. 走 §5.3 release 流程
3. release 后回本文件更新 §2.2 当前位置 + §3 task 状态 + §5.2 状态列

### 7.5 我要做 code review / PR review

1. 看 [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md) 的 L1/L2/L3 checklist
2. 按 [`CONTRIBUTING.md`](../CONTRIBUTING.md) 的 4 大规则核对
3. 必跑本地 gate（`cargo fmt --check`、`cargo clippy`、`cargo test`、`cargo doc`、`cargo deny check`）

### 7.6 我维护本文件

- 每次 milestone 状态变化 → 更新 §2.2 + §2.3 + §3
- 每次新增 task 文件 → §3.1 加一行
- 每次 release → §5.2 状态列更新
- 每次 commit 在 `main` → §2.2 "当前 commit" 字段
- 每次发现新缺陷 / 提出未来想法 → §6.1 / §6.2 加一行
- 重要里程碑 → §9 变更记录

---

## 8. Long-term Vision（北极星）

### 8.1 北极星指标

> 用 agentprof 的用户**第一次跑 `agentprof analyze`** 时，看到自己的 `schema_utilization` 数字后，**会发推**说"我居然 80% 的 context 在 schema 上"。

衡量方式：
- 4-week 内 GitHub Star ≥ 500
- 4-week 内有 ≥ 10 个非作者的 PR（typo / 新 adapter / 新 feature）
- HN front page 一次
- 三家 agent CLI（Claude / Codex / Copilot）官方文档或 README 引用本工具

### 8.2 三年愿景

| 时间 | 状态 |
|---|---|
| **0–6 月** | v0.1.0 → v1.0.0，三 agent 全支持，binary distribution 稳定 |
| **6–12 月** | 实时 OTLP 替代事后 jsonl 扫描；MCP server 厂商集成（如 Anthropic 官方 settings 一键 install 本工具） |
| **12–24 月** | Web dashboard（团队 dashboard），跨用户脱敏聚合，最佳实践榜单 |
| **24–36 月** | 成为 LLM agent 性能分析的事实标准；类比 `perf` 之于 Linux profiling |

### 8.3 永远不做的事（明确边界）

| 不做 | 原因 |
|---|---|
| 修改 agent session 文件本身 | 我们是 reader，不是 writer。修改会破坏 agent 自己的恢复机制。 |
| 自动改 `.mcp.json` 删 server | 决策权留给用户。我们只给数据。 |
| 把 session 内容发到第三方分析服务 | 隐私优先，本地优先；联网功能要显式 opt-in |
| 帮用户写 prompt | 不是 prompt engineering 工具 |
| 实时 hook / 拦截 LLM API call | 这是 LiteLLM / Helicone 的领域，我们做事后分析 |

---

## 9. Change Log（本文件自身变更）

| 日期 | 变更内容 | Commit |
|---|---|---|
| 2026-05-26 | 初版：项目总入口 + 文档地图 + Phase 时间线 + task 索引 + 依赖图 + release cadence + 长期愿景 | TBD |
| 2026-05-30 | v1.1：同步至 M1.4 + 4 轮 followups 后实状（4/7 = 57% MVP），承认 events-first pivot；**新增 §6 Known Limitations & Future Work 集中索引**（12 条已知缺陷 + 9 条未来工作 + 3 条用户明确保留的设计决策）；旧 §6/7/8/9 顺延为 §7/8/9/10 | `6c26972` + 本 commit |
| 2026-06-02 | v1.2：M1.6.4 ✅ 完成（追加 tracing 基础设施 ship 2026-06-02 + ADR-0010）— 同步 header / §2.2 / §2.3 / §3.1 / §4.2 / L-4 | 本 commit |
| 2026-06-02 | v1.3：M1.6.4 ✅ merged（`8abc590`）— Speedscope+HTML (2026-05-31, ADR-0007) + tracing 基础设施 (2026-06-02, ADR-0010)。8 milestone surface 全部 ship。 | `8abc590` |
| 2026-06-03 | v1.4：**M1.6.4 follow-up wave**（8 cleanup commits：`d87adec` → `766b8f0`）—— post-merge 文档审计（`d87adec`）；cleanup batch 1 8 review nits（`4301125`）；`hash_path` 在 L2/L3 span 也 honor `AGENTPROF_LOG_FULL_PATHS`（Critical L1-only gap 修复，`83d2ed0`）；crate-boundary 规则澄清允许 dev-deps（`95fd059`）；B-3 / B-4 / B-5 / B-6 speedscope+HTML follow-ups（EmitCtx refactor `b376d18`；3 new `ExportWarning` variants `c54a1af`；`Display` impls + defensive html escape `afae0e8`；3 new combination fixtures `766b8f0`）。同步 header / §2.2 / §2.3 / §4.1 graph / §6.1 L-9 fixture count / §10 anchors。**注**：commit `4301125 chore(m1.6.5): cleanup batch 1` 主题用了 `m1.6.5` token 是误用 —— 实际属于本次 follow-up wave，**不是** M1.6.5 milestone（reserved for MCP server waste analysis at §6.1 L-4，deferred to 0.2.0 per `docs/plan.md §8`）。 | `766b8f0` |
| 2026-06-28 | v1.9：**全量同步至 v0.3.3 现实** —— header / §1.1 / §2.1 时间线 / §2.2 当前位置 / §2.3 仪表盘 / §3 task 索引 / §4 依赖图 / §5.2 版本表 / §6.1 L-4·L-7 / §6.2 F-5·F-6·F-8 全部刷新。Phase 1 MVP ✅ 100%（v0.1.0）+ Phase 2 ✅ 基本完成（M2.1 SQLite / M2.2 OTLP / M2.4 加固 / M2.5 cache / M2.3 web，v0.2.0–v0.3.3）。 | `9df9573` |

---

## 10. 附录：当前 Git 历史

获取最新 commit 列表（任何时候都准确）：

```bash
git log main --oneline -20
```

最近的 milestone merges（不会变；写死作为锚点）：

```
# Release tags（v0.1.0 → v0.3.3，真实 commit hash — 由 `git rev-list -n1 <tag>` 得）
9df9573  HEAD — [Unreleased] M2.3.x visual-guide + list 测试修复
34aad50  v0.3.3 — M2.3 web dashboard (serve)
66967b7  v0.3.2 — rustls CryptoProvider fix
8be7803  v0.3.1 — M2.5 cache analytics
cf33b91  v0.3.0 — M2.4 OTLP 安全加固
c28f53e  v0.2.1 — M2.2 OTLP receiver
ec2a64a  v0.2.0 — M2.1 SQLite + M2.1.1 dual-path
7e29d97  v0.1.0 — MVP release

# 更早的 M1.6.4 时代锚点（保留作历史参考）:
766b8f0  test(fixtures): B-6 add tool+skill / multi-skill / orphan+skill (M1.6.4 follow-up wave)
afae0e8  fix(html,speedscope): B-5 render robustness (M1.6.4 follow-up wave)
c54a1af  fix(speedscope): B-4 timestamp robustness (M1.6.4 follow-up wave)
b376d18  chore(speedscope): B-3 cleanup — debug_assert + EmitCtx refactor (M1.6.4 follow-up wave)
95fd059  docs(arch): clarify crate-boundary rule allows dev-deps (M1.6.4 follow-up wave)
83d2ed0  fix(core): hash_path honors AGENTPROF_LOG_FULL_PATHS at all layers (M1.6.4 follow-up wave)
4301125  chore(m1.6.5): cleanup batch 1 — 8 M1.6.4 review nits  ← 主题 `m1.6.5` token 是误用；实属 follow-up wave
d87adec  docs(m1.6.4): post-merge audit (M1.6.4 follow-up wave)
8abc590  Merge branch 'feat/m1.6.4-tracing' into main          ← M1.6.4 milestone merge
9abd694  Merge fix/post-output-audit: close 3 audit findings + privacy doc
e0318ed  fix(mode-vocabulary): align Mode enum to real Copilot wire
010c9af  feat(turn-metadata): populate Turn.model / .mode / .output_tokens
8399bdd  fix(m1.4-audit-followups): 10 findings closed
(M1.4 initial: feat/m1.4-cli-and-analyzer)
(M1.3 final:  feat/m1.3-episode-and-schema-fix)
(M1.2 final:  feat/m1.2-copilot-adapter)
(M1.1 skeleton: chore: initial workspace skeleton + skills matrix + pipeline)
```

实时 doc-sync commits（每隔几个 milestone 同步一次）紧跟在最新 milestone 之后。

---

> **更新本文件的纪律**：任何改动 §2.2 当前位置 / §3 task 索引 / §5 release 状态 → **同 commit 更新**本文件 + 在 §9 记一行。这是 `docs/architecture.md` §14 "文档同步" 规则在本文件上的具体体现。
