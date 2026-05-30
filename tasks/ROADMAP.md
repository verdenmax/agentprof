# agentprof Roadmap

> **本文件是项目总入口。** 如果你是第一次进入本仓库（或时隔一段时间回来），**先读这里**，再去任何其他文档。
>
> **文件名**：`tasks/ROADMAP.md`
> **版本**：1.1
> **最后更新**：2026-05-30
> **当前 commit**：`9abd694`（最新；merge of `fix/post-output-audit`）
> **当前阶段**：**Phase 0 + 1 (MVP)** — M1.1 / M1.2 / M1.3 / M1.4 ✅ 完成（含 4 轮 M1.4 followups），M1.5–M1.7 待开始
> **下一步入口**：`tasks/001-mvp-agent-token-profiler.md` §10 Milestone 1.5（TUI 火焰图 + ROI 表），走 Stage 1 brainstorming
>
> **重大 pivot**（ADR-0001 events-first，详见 §4.1 / §4.2）：M1.2 不再做 ClaudeAdapter，改做 **CopilotAdapter**（real wire data 直接可得）；tokenizer / ROI / waste / aggregate 全部从 M1.3 推迟到 M1.5+。Claude / Codex / Gemini 适配器推迟到 Phase 2 / 3。

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
| [`docs/architecture.md`](../docs/architecture.md) | 代码架构权威（18 节，757 行） | 想动代码 / 想了解"怎么做" |
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
| Spec / Plan | `docs/superpowers/specs/YYYY-MM-DD-<topic>-*.md` | brainstorming / writing-plans 产物 |

### 1.4 AI Agent 指南

| 文件 | 角色 |
|---|---|
| [`.github/copilot-instructions.md`](../.github/copilot-instructions.md) | AI 助手 entry point（含 9 阶段 pipeline + 19 个 skill 清单） |
| [`.github/skills/<name>/SKILL.md`](../.github/skills/) | 5 个项目级 skill（入 git，跟随 clone） |
| [`.github/instructions/*.instructions.md`](../.github/instructions/) | 2 个常驻 always-on 规则 |
| obra/superpowers plugin（全局） | 14 个全局 skill（`~/.copilot/installed-plugins/_direct/obra--superpowers/`） |

### 1.5 Task 文件目录（详见 §3）

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

Phase 3   扩展适配：Codex CLI / Copilot CLI / Gemini / Cursor
          pricing 自动同步 + 三 agent 全支持
                                                                  003-phase3-multi-agent.md (TBD)
─────────────────────────────────────────────────  v1.0.0 ────
```

> **说明**：Phase 0 + Phase 1 合并为 MVP，由单个 task 文件 (`001-mvp-agent-token-profiler.md`) 覆盖。Phase 2 / Phase 3 各占一个 task 文件，分别对应 v0.2.0 / v1.0.0 释放。

### 2.2 当前位置

| 维度 | 当前状态 |
|---|---|
| **Git** | `main` 分支，commit `9abd694`（含 4 个 已 merge feature branch：M1.4 audit followups + turn-metadata-extraction + mode-vocabulary-alignment + post-output-audit） |
| **Crate** | 5 lib/bin + 1 xtask。`agentprof-core` / `agentprof-adapters` / `agentprof-cli` 已实现到 M1.4；`agentprof-tui` / `agentprof-storage` 仍是 `//!` 骨架（M1.5 / Phase 2） |
| **Phase** | Phase 0 / Phase 1（MVP），**M1.1 / M1.2 / M1.3 / M1.4 ✅ 完成**，M1.5（TUI）/ M1.6（list+aggregate+export）/ M1.7（release）❌ 未开始 |
| **测试** | ~230+ tests pass，含 ~70 个 insta 快照（episode_derive / analyzer_on_fixtures / CLI 集成 / 单元）+ 11 个 fixture（含 `with-post-tool-use-hooks` 锁定 Copilot CLI 1.0.x 三个 Optional schema 字段的 parser fix） |
| **CI** | 已配（lint + test matrix + deny + docs + docs-sync + nightly-msrv + release skeleton），未在 GitHub 上运行（remote 未配） |
| **远端** | 未推（本地 `main` only） |
| **Release** | 未发，下次 release = v0.1.0（M1.7 出口） |

### 2.3 Phase 完成度仪表盘

| Phase | 任务文件 | Milestone | 完成度 | Release | 状态 |
|---|---|---|---|---|---|
| **0+1 MVP** | 001 | M1.1–M1.7 | 4/7（M1.1 / M1.2 / M1.3 / M1.4 ✅）= **57%** | v0.1.0 | 🟡 In progress |
| **2** | 002 (TBD) | M2.1–M2.x | 0% | v0.2.0 | ⚪ Planned |
| **3** | 003 (TBD) | M3.1–M3.x | 0% | v1.0.0 | ⚪ Planned |
| **Beyond** | 004+ (TBD) | — | — | post-1.0 | 💭 Vision |

> **注意 events-first pivot 的范围调整**（ADR-0001）：原 PRD 把 tokenizer / ROI 矩阵 / waste 估算 / 跨 session aggregate 全部塞进 M1.3；pivot 后这些**全部推迟到 M1.5+ 或 Phase 2**，M1.3 实际只做 schema-audit + Episode 聚合层。M1.4 实际交付的 `agentprof analyze` 输出是 turn / tool / hook 三表 + 14 类 warnings（parse-stage + derive-stage），不含 ROI / waste。

---

## 3. Task File Index（任务文件目录）

> **命名规则**：`NNN-<scope>.md`，编号单调递增（合并冲突时取已合 PR 最大值 +1）。每个 task 文件是一份独立的 PRD + 实施计划，参考 `proteinCopilot/tasks/00X-*.md` 格式。

### 3.1 已存在的 task 文件

| # | 文件 | 范围 | 状态 | Milestone 完成度 | 计划 release |
|---|---|---|---|---|---|
| **001** | [`001-mvp-agent-token-profiler.md`](./001-mvp-agent-token-profiler.md) | **Phase 0 + 1 MVP**：Copilot adapter（pivot from Claude）+ Episode aggregation + CLI `analyze` (md/json) + TUI flamegraph + list/aggregate/export | 🟡 In-Progress | 4/7（M1.1 / M1.2 / M1.3 / M1.4 ✅） | **v0.1.0** |

### 3.2 计划中的 task 文件（占位）

| # | 文件 | 范围（暂定） | 触发条件 |
|---|---|---|---|
| **002** | `002-phase2-engineering.md` (TBD) | Phase 2 工程化：SQLite 持久化、OTLP receiver、watch 实时刷新、pricing 自动同步 | 001 → v0.1.0 释放后启动 |
| **003** | `003-phase3-multi-agent.md` (TBD) | Phase 3 多 agent：Codex / Copilot / Gemini / Cursor 适配器、三 agent 全支持 | 002 完成后启动 |
| **004+** | 待规划 | post-1.0 feature（如自造 ratatui-snapshot-testing skill、library-mode API、Web dashboard 等） | v1.0.0 后由社区 / 实际需求驱动 |

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
       │  ADR-0001)   │ │  aggregation │ │  ❌        │
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
              │ ❌ [Phase 1 完成]                    │
              └─────────┬────────────────────────────┘
                        │
                        ▼
              ┌──────────────────────────────────────┐
              │ M1.7 E2E 集成 + 文档 + v0.1.0 release│
              │ ❌ [MVP 出口]                        │
              └──────────────────────────────────────┘
```

### 4.2 跨 task 依赖（高层）

```
task 001 (MVP)                                 task 002 (Phase 2)
─────────────────────                          ─────────────────────
M1.1 ✅ skeleton                               M2.1 ❌ SQLite persistence
M1.2 ✅ copilot adapter      ┌───────────►    M2.2 ❌ OTLP receiver
M1.3 ✅ episode aggregation  │                M2.3 ❌ watch 实时刷新
M1.4 ✅ CLI analyze + md     │                M2.4 ❌ pricing 自动同步
M1.5 ❌ TUI views            │                M2.5 ❌ tokenizer + ROI + waste (events-first pivot 推迟)
M1.6 ❌ list/agg/export      │                       │
M1.7 ❌ v0.1.0 release ──────┘                       │ release
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
| **v0.1.0** | 001 M1.7 | MVP：Claude adapter + TUI + 5 种导出 + CLI 6 个子命令 | 🟡 Planned |
| **v0.1.x** | 001 M1.7 后 | bug fix 滚动 release | — |
| **v0.2.0** | 002 (TBD) | SQLite 持久化 + OTLP + watch + pricing sync | ⚪ Planned |
| **v0.3.x** | 002 / 003 | Phase 2 → 3 过渡 | ⚪ Planned |
| **v1.0.0** | 003 M3.4 | 三 agent 全支持 + 公开 API 冻结 + crates.io publish | 💭 Vision |

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

## 6. How to Use This Roadmap（怎么用本文件）

### 6.1 我是新人 / AI 第一次进项目

1. 读完本文件 §0 TL;DR
2. 看 §1 文档地图，按需读 L1 文档（plan.md → architecture.md）
3. 看 §2 知道当前位置
4. 看 §3 task 索引找到对应任务文件

### 6.2 我要继续开发（推进当前 task）

1. 看 §2.2 当前位置 → 知道在哪个 milestone
2. 进 `tasks/001-mvp-agent-token-profiler.md` 找该 milestone 的 Task / Sub-task
3. 按 `.github/copilot-instructions.md` §5 的 **9 阶段 pipeline** 推进：
   - Stage 1 brainstorming → spec
   - Stage 2 ADR（若有候选方案）→ adr-NNNN
   - Stage 3 writing-plans → plan
   - Stage 4 TDD 实现
   - Stage 7 verification → PR
   - Stage 8（release 时）github-release

### 6.3 我要新增一个 feature

1. 判断是 Phase X 内的新功能 还是 整个新方向：
   - **Phase X 内**：进当前 task 文件加 Milestone / Task / Sub-task
   - **新方向**：在 `tasks/` 下新建 `NNN-<scope>.md`（编号 +1），按 §3.3 标准结构填充，并回到本文件 §3.1 加一行
2. 走 9 阶段 pipeline（brainstorming → planning → ...）

### 6.4 我要 release

1. 看 §5.2 当前应该 release 哪个版本号
2. 走 §5.3 release 流程
3. release 后回本文件更新 §2.2 当前位置 + §3 task 状态 + §5.2 状态列

### 6.5 我要做 code review / PR review

1. 看 [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md) 的 L1/L2/L3 checklist
2. 按 [`CONTRIBUTING.md`](../CONTRIBUTING.md) 的 4 大规则核对
3. 必跑本地 gate（`cargo fmt --check`、`cargo clippy`、`cargo test`、`cargo doc`、`cargo deny check`）

### 6.6 我维护本文件

- 每次 milestone 状态变化 → 更新 §2.2 + §2.3 + §3
- 每次新增 task 文件 → §3.1 加一行
- 每次 release → §5.2 状态列更新
- 每次 commit 在 `main` → §2.2 "当前 commit" 字段
- 重要里程碑 → §8 变更记录

---

## 7. Long-term Vision（北极星）

### 7.1 北极星指标

> 用 agentprof 的用户**第一次跑 `agentprof analyze`** 时，看到自己的 `schema_utilization` 数字后，**会发推**说"我居然 80% 的 context 在 schema 上"。

衡量方式：
- 4-week 内 GitHub Star ≥ 500
- 4-week 内有 ≥ 10 个非作者的 PR（typo / 新 adapter / 新 feature）
- HN front page 一次
- 三家 agent CLI（Claude / Codex / Copilot）官方文档或 README 引用本工具

### 7.2 三年愿景

| 时间 | 状态 |
|---|---|
| **0–6 月** | v0.1.0 → v1.0.0，三 agent 全支持，binary distribution 稳定 |
| **6–12 月** | 实时 OTLP 替代事后 jsonl 扫描；MCP server 厂商集成（如 Anthropic 官方 settings 一键 install 本工具） |
| **12–24 月** | Web dashboard（团队 dashboard），跨用户脱敏聚合，最佳实践榜单 |
| **24–36 月** | 成为 LLM agent 性能分析的事实标准；类比 `perf` 之于 Linux profiling |

### 7.3 永远不做的事（明确边界）

| 不做 | 原因 |
|---|---|
| 修改 agent session 文件本身 | 我们是 reader，不是 writer。修改会破坏 agent 自己的恢复机制。 |
| 自动改 `.mcp.json` 删 server | 决策权留给用户。我们只给数据。 |
| 把 session 内容发到第三方分析服务 | 隐私优先，本地优先；联网功能要显式 opt-in |
| 帮用户写 prompt | 不是 prompt engineering 工具 |
| 实时 hook / 拦截 LLM API call | 这是 LiteLLM / Helicone 的领域，我们做事后分析 |

---

## 8. Change Log（本文件自身变更）

| 日期 | 变更内容 | Commit |
|---|---|---|
| 2026-05-26 | 初版：项目总入口 + 文档地图 + Phase 时间线 + task 索引 + 依赖图 + release cadence + 长期愿景 | TBD |

---

## 9. 附录：当前 Git 历史（前 10 个 commit）

```
ae2045a  docs(tasks): add full MVP task file (PRD + implementation plan)
472ac31  docs: tighten pipeline cohesion (three-layer flow + Stage 2 gate + orphan skills)
201ae46  fix(skills): relocate skills from global plugin to repo .github/skills/
dc838fc  docs: install agentprof-extras plugin + unify into 9-stage skill pipeline
1a7a7f6  docs: integrate superpowers skills matrix into project guides
b47aeb5  chore: initial workspace skeleton
```

---

> **更新本文件的纪律**：任何改动 §2.2 当前位置 / §3 task 索引 / §5 release 状态 → **同 commit 更新**本文件 + 在 §8 记一行。这是 `docs/architecture.md` §14 "文档同步" 规则在本文件上的具体体现。
