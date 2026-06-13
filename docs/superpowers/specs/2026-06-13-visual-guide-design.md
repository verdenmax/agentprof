# agentprof Visual Guide — 设计 spec

> 仿 [`langchain-visual-guide`](https://github.com/verdenmax/langchain-visual-guide) 风格的可视化 HTML 教程，单站点 + 两个章节（用法 + Wiki），用 Rust xtask 构建，部署到 GitHub Pages。

| 字段 | 值 |
|---|---|
| **作者** | agentprof maintainers（brainstorming with GitHub Copilot CLI） |
| **创建** | 2026-06-13 |
| **状态** | Approved（待 spec self-review + user review） |
| **目标版本** | v0.3.4（pure docs 增量；或如无 SemVer bump 需求，直接合 main） |
| **依赖** | 当前 main HEAD（v0.3.3 ship 之后） |
| **下一步** | writing-plans skill → 生成实现计划 |

---

## 1. 问题陈述

agentprof 当前文档分散在 4 个独立通道：

| 通道 | 受众 | 形式 | 问题 |
|---|---|---|---|
| **`README.md`**（根 / 各 crate） | 用户 + 浏览源码者 | 长篇 markdown | 完整但缺图、入门门槛高 |
| **`docs/architecture.md`** | 开发者 / AI 协作者 | 一长篇 L1 设计文档 | 1400 行无目录无图、不可扫读 |
| **`docs/internals/adr-*.md`** | 设计决策追溯 | 24 份 ADR | 决策记录而非教程 |
| **rustdoc** | API 使用者 | 自动生成 | 缺端到端流程、无场景化叙事 |

**缺口**：
1. **新用户 onboarding 慢** — 从 "知道这是个 token profiler" 到 "跑起来看到第一张火焰图" 没有平滑路径。
2. **开发者无源码导览** — 想阅读 5 crate / 写 adapter / 加 feature 时，没有"从架构图到具体文件"的 hand-hold。
3. **没有视觉资产** — 火焰图、ROI 表、dashboard 截图等是 agentprof 卖点，从来没在文档里出现过。
4. **市场宣传不友好** — 推荐给同事时，没有一个 "30 秒看完就懂" 的入口（GitHub README 太长 + 缺图）。

**Visual guide 解决的是**：给上述 4 个缺口提供一个**单一 entry**，新手有 6 节用法课，开发者有 8 节 Wiki，全部带图 + 真实代码片段。

---

## 2. 范围

### In scope（MVP v0.3.4）

1. **目录布局** `docs/visual-guide/`，子目录 `usage/`、`wiki/`、`assets/`。
2. **14 课内容**：6 节用法 + 8 节 Wiki。每课中文 ≥ 600 字。
3. **xtask 子命令** `cargo xtask visual-guide [--clean] [--check]`。
4. **共享 shell**（CSS 设计系统 + 顶部 nav + 进度条 + footer + favicon）。
5. **组件库**：accordion、code_block（4 语言高亮）、flow_diagram（SVG）、comparison_table、prev/next nav、source_ref。
6. **真实资产**：≥ 3 张 agentprof 真实输出（火焰图 SVG + dashboard 截图 + HTML 报告截图）。
7. **GH Pages CI workflow**（`.github/workflows/visual-guide.yml`），main push 自动部署，PR 仅 `--check`。
8. **L1/L2 文档同步**：`docs/architecture.md` §15.1 仓库结构 + L2 README.md 加章节、根 README.md 加在线阅读徽章、CHANGELOG.md `[Unreleased]` 加 entry。
9. **ADR-0025** 记录本设计的 7 个决策。

### Out of scope（明示推迟）

1. **搜索框 / 左侧 sticky ToC**（参考项目也没有，YAGNI）。
2. **课末 quiz**（参考项目有，但 agentprof 用户更需要的是源码导航而非测验）。
3. **PDF 导出**（参考项目有 PDF，但 agentprof 暂不发 PDF；GH Pages 在线版已足够）。
4. **i18n / 英文版**（中文一版，与项目主文档语言一致；后续如有海外用户需求再扩）。
5. **截图自动刷新工具** `xtask visual-guide --refresh-screenshots`（不在 MVP；首次截图手工生成）。
6. **代码高亮的语言扩展**（仅 Rust / bash / toml / sql 4 种；不引 syntect / tree-sitter）。
7. **Phase 3 内容**（ClaudeAdapter / CodexAdapter 写法）— 仅在 Wiki §3 的"如何写新 adapter"段给出纲要，不出完整实现；待 M3.1 / M3.2 真正交付后再补完。
8. **观测 / pricing 视图**（Q4b 推荐引擎）— 不在 MVP，路线图表里提及。

---

## 3. 设计决策

### D-1: 单站点 + 两个章节（共享 shell）

复用 `langchain-visual-guide` 单站点模式：所有页面共享同一 CSS / 同一顶部 nav / 同一 footer。「用法」和「Wiki」只是 `PAGES` 数组里的 `section` 字段不同，文件分别落在 `usage/` 和 `wiki/` 子目录。

**为什么**：（a）维护一套样式；（b）跨章节互链方便（用法 §5 serve → Wiki §7 web-dashboard 架构）；（c）GH Pages 单部署单 domain；（d）参考项目 27 课验证过此模式可扩展。

**Alternatives rejected**：
- 两个独立站点（双部署、双 CSS 维护、互链复杂）。
- 单站点 + tab 切换（JS 复杂度、SEO 差、`file://` 不友好）。

### D-2: 文件布局 `docs/visual-guide/{usage,wiki,assets}/`

- `index.html`：入口，14 课目录卡片。
- `usage/NN-<slug>.html`：6 节用法。
- `wiki/NN-<slug>.html`：8 节 Wiki。
- `assets/`：手工维护的真实截图 + SVG（**入 git**）。
- 生成的 `.html` **不入 git**（参考 langchain-visual-guide 把 HTML 入 git；本项目反向选择以避免每次 commit diff 噪音 + 源文件 + 编译产物的双重 review 负担）。

**资产相对路径**：lesson 用 `../assets/<file>`，index 用 `assets/<file>`。`file://` 与 GH Pages 服务器双兼容。

### D-3: Rust xtask + askama 构建（不用 Python）

`xtask` 已存在（M2.1 后用于 anonymize 日志 / release 校验），新增 `visual_guide` 子模块 + `cargo xtask visual-guide` 子命令。

**xtask 依赖增量**：仅 `askama`（workspace 已有，cli 已用）+ `chrono`（workspace 已有）。**不引入** syntect / tree-sitter / pulldown-cmark / handlebars。

**为什么**：（a）项目语言一致（无 Python crate 引入）；（b）askama 已在 workspace 里（cli 的 dashboard 用），xtask 引入此 dep 不额外扩 workspace 依赖图（**注意**：xtask 与 cli 各自用自己的 askama 模板，二者不共享模板文件，只是共享 askama crate）；（c）xtask 是隔离 crate，主构建图不变；（d）`cargo deny` 可控（不引入 PyO3 / npm 等异质依赖）。

**Alternatives rejected**：
- 复用 langchain-visual-guide 的 Python build.py — 引入 Python runtime 依赖、CI 复杂。
- mdBook — 风格固化、难达到 langchain-visual-guide 的设计感。
- 手写 HTML — 14 课重复 chrome 太多、维护成本高。

### D-4: 内容范围 MVP（14 课）

「用法」6 课覆盖：what + install + analyze + list/aggregate + serve + db/ingest-otlp。

「Wiki」8 课覆盖：architecture + data-model + adapter + analyzer + storage + otlp-receiver + web-dashboard + contributing。

**为什么这个边界**：（a）覆盖当前 ship 的所有用户向功能（M1/M2 + M2.3）；（b）单 milestone 可交付；（c）排除未 ship 的（Phase 3 多 agent / Q4b 推荐引擎）以避免文档与代码不同步。

**每课字数下限**：~600 字。**目标**：800-1200 字 + 1-3 张图 / 表 + 2-4 个折叠卡片。

### D-5: 中文 + 反向类比 + 折叠卡片课件风格

每课统一 4 块结构：
1. **顶部 lead** — 一句话 + 反向类比（参考 LangChain 的"标准化传动系统"类比）。
2. **痛点对比表 / 概念 SVG 图** — 三栏：痛点 → 没工具时 → agentprof 的做法。
3. **折叠卡片 ×2-4** — 每张含：示例代码 / 为什么必要 / agentprof 怎么做 / 其他选择。
4. **底部** — 上一课 + 下一课 + 相关 ADR + 相关源码（GitHub blob URL，不写死行号）。

**代码高亮**：手写 lexer 覆盖 Rust / bash / toml / sql，输出 `<span class="kw">` 等。不依赖外部 JS / 库。

### D-6: GitHub Pages CI 联动

`.github/workflows/visual-guide.yml` 触发条件：
- `push` 到 main 且改动了 `xtask/src/visual_guide/**` 或 `docs/visual-guide/assets/**` 或 workflow 自身 → 重生成 HTML + 部署 GH Pages。
- `pull_request` 改动同上路径 → 仅 `cargo xtask visual-guide --check`（不部署）。
- `workflow_dispatch` 手动触发。

权限：`pages: write` + `id-token: write`。Concurrency group `pages` 防并发部署冲突。

**为什么**：用户明确选项「公开发布 + 在线阅读」；GH Pages 是 GitHub 原生 + 免费 + agentprof 当前没有任何 GH Pages 站点（无冲突）。

### D-7: 不发 SemVer 新版本（或仅 v0.3.4 docs-only patch）

可视化指南是文档增量，不影响公开 API、不改 CHANGELOG 任何 Added / Changed / Removed 行（仅 Documentation 段）。

**选项 A**（推荐）：标 v0.3.4，CHANGELOG 单段 "## [0.3.4] - YYYY-MM-DD ### Documentation"。优势：可以从 GitHub Release 直链到上线时间点。

**选项 B**：仅 commit 到 main，不打 tag。优势：避免空 tag 噪音。

**决策**：deferred 到 plan 阶段，由 user 在 T21 之前选。

---

## 4. 课程大纲（14 课）

### 用法 章节（面向完全新手）

| # | 文件名 | 标题 | 关键讨论 |
|---|---|---|---|
| 1 | `usage/01-what-is-agentprof.html` | agentprof 是什么 | 一句话定位 + 反向类比 + 3 类痛点（黑盒 / 无 ROI / MCP 浪费）三栏表 |
| 2 | `usage/02-install.html` | 5 分钟上手 | one-line installer（curl） / `cargo install` / from-source 三种 + 第一次 `analyze --agent copilot` |
| 3 | `usage/03-analyze.html` | analyze：看懂一次 session | md / tui / html 三种导出（**嵌入真实截图**） + Turn Summary / Tool Rank / Cache 段读法 |
| 4 | `usage/04-list-aggregate.html` | list / aggregate：跨 session 视角 | `list --since` + `aggregate --by {model,tool,day,mcp-server}` + 「何时用哪个 by」决策树 |
| 5 | `usage/05-serve.html` | serve：浏览器实时看板 | `agentprof serve` 5 视图截图 + `[serve]` config + 「serve vs 静态 HTML」决策表 |
| 6 | `usage/06-db-otlp.html` | db + ingest-otlp：存数据库 + 接入 OTLP | hybrid cache/store 概念图 + `db init/ingest/stats` + `ingest-otlp` 接入 Claude Code / Codex 路径图 |

### Wiki 章节（面向中阶 + 开发者）

| # | 文件名 | 标题 | 关键讨论 |
|---|---|---|---|
| 1 | `wiki/01-architecture.html` | 架构全景 | 5 crate 依赖图 SVG + L1/L2/L3 文档体系 + 9 阶段 pipeline + ADR 索引 |
| 2 | `wiki/02-data-model.html` | 数据模型 | `Event` → `Episode` → `AnalysisReport` 三层关系图 + 关键 struct 字段 |
| 3 | `wiki/03-adapter.html` | Adapter trait | trait 接口 + `AgentKind` + CopilotAdapter 案例 + **怎么写新 adapter**（M3.1 入门纲要） |
| 4 | `wiki/04-analyzer.html` | 分析层 rollups | `compute_analysis` 流水线 + turn_summary / tool_rank / hook_rank / cache_metrics 公式 |
| 5 | `wiki/05-storage.html` | 存储层 hybrid mode | ADR-0019 + SQLite schema（sessions / model_metrics / episodes_json） + dual-path 取舍 |
| 6 | `wiki/06-otlp-receiver.html` | OTLP receiver | ADR-0021/0022 + gRPC + HTTP + Bearer/mTLS + LRU cap + 256-byte session.id 防御层 |
| 7 | `wiki/07-web-dashboard.html` | Web dashboard 架构 | ADR-0024 + axum + askama + vanilla JS poller + chunk-endpoint + 5 视图源码 walk-through |
| 8 | `wiki/08-contributing.html` | 贡献指南 | Conventional Commits + brainstorming → spec → plan → TDD → review pipeline + 怎么开 PR / 加 ADR |

---

## 5. 文件 / 模块布局

### 5.1 `docs/visual-guide/` 输出

```
docs/visual-guide/
├── README.md                       手工：本地构建说明 + 在线阅读链接
├── index.html                      ⚙ xtask 生成
├── usage/                          ⚙ xtask 生成（6 文件）
├── wiki/                           ⚙ xtask 生成（8 文件）
└── assets/                         手工资产（入 git）
    ├── flamegraph-sample.svg       agentprof analyze --export speedscope 真实样本
    ├── dashboard-overview.png      agentprof serve 主页截图
    ├── dashboard-aggregate.png     agentprof serve aggregate 截图
    ├── report-html-sample.png      agentprof analyze --export html 截图
    └── architecture-deps.svg       手绘 5-crate 依赖图（与 architecture.md §3 一致）
```

### 5.2 `xtask/src/visual_guide/` 源码

```
xtask/src/
├── lib.rs                          既有，加 pub mod visual_guide
└── visual_guide/
    ├── mod.rs                      CLI 入口 + run() 主流程 + --clean / --check 处理
    ├── shell.rs                    head_meta / nav / footer / progress_bar / index_page / page wrapper
    ├── css.rs                      CSS 设计 tokens + dark mode + responsive 规则
    ├── pages.rs                    PAGES: &[(file, title, section)] + index 元数据
    ├── components.rs               accordion / code_block / svg_diagram / comparison_table / source_ref
    ├── highlight.rs                手写 lexer for rust/bash/toml/sql
    ├── usage_01.rs ... usage_06.rs 6 节内容（每个 ~300 行 askama 模板调用 + 中文文本）
    ├── wiki_01.rs ... wiki_08.rs   8 节内容
    └── templates/                  askama 模板（page.html 壳 + index.html 模板）
        ├── page.html
        └── index.html
```

### 5.3 `xtask/tests/visual_guide.rs` 测试

5 个集成测试：
1. `render_all_succeeds`：完整生成 15 个文件，无 askama 错误。
2. `html_is_well_formed`：所有输出 HTML 用 `quick-xml` 解析无错。
3. `prev_next_links_valid`：每页的 prev/next 指向 PAGES 真实存在的 entry。
4. `source_refs_exist`：所有 `source_ref!("crate", "module::symbol")` 中的 crate 在 workspace 实际存在（grep `crates/` 子目录）。
5. `asset_refs_exist`：所有 `<img src="../assets/X">` 中的 X 在 `docs/visual-guide/assets/` 实际存在。

---

## 6. 关键技术决定

### 6.1 代码高亮

手写 lexer（`xtask/src/visual_guide/highlight.rs`），约 200 LOC。识别：
- Rust：`fn` / `let` / `mut` / `pub` / `mod` / `use` / `struct` / `enum` / `impl` / `trait` / `match` / `if` / `else` / `for` / `while` / `loop` / `return` / `async` / `await` / `Self` / `self` 关键字；`"..."` 字符串；`//` 单行注释；`/* */` 块注释；数字常量。
- Bash：`$` 变量 + 关键字 `if/then/fi/for/do/done/while`；`#` 注释；`"..."` / `'...'` 字符串。
- TOML：键 `=`；section header `[...]`；`#` 注释；字符串。
- SQL：关键字大写匹配（`SELECT/FROM/WHERE/JOIN/CREATE TABLE/INDEX/UPDATE/DELETE/INSERT/VALUES/PRAGMA`）；`--` 注释。

不识别 macro hygiene / nested generics 等复杂语法 — 容错优先。

### 6.2 SVG 图绘制

`svg_diagram(nodes, edges)` 输出 inline `<svg>`，使用：
- 固定 viewBox `0 0 800 400`
- 节点 = `<rect>` + `<text>`（按 grid 自动布局）
- 边 = `<line>` + arrow `<marker>`
- 跟 dark mode：CSS 通过 `currentColor` 继承

复杂图（5-crate 依赖、Event → Episode → AnalysisReport 三层）手工预绘存 `assets/*.svg` + `<img>` 引用；简单图（流程箭头）用 `svg_diagram()` 生成。

### 6.3 真实截图准备

T17 的工作：
1. 准备一个 anonymized session fixture（已有 `crates/agentprof-adapters/tests/fixtures/copilot/`）。
2. 跑 `agentprof analyze --export html --output /tmp/sample.html`，浏览器截图 → `assets/report-html-sample.png`。
3. 跑 `agentprof analyze --export speedscope` 得到 svg → `assets/flamegraph-sample.svg`。
4. 跑 `agentprof serve --storage-path /tmp/sample.db`，浏览器访问 5 个视图各截一张 → `assets/dashboard-*.png`。

截图分辨率 1280×800，加 v0.3.3 / v0.3.4 水印（小字在右下角，避免文档过时混淆）。

### 6.4 GitHub blob URL 模板

`source_ref!(crate, module, symbol)` 展开为：
```
https://github.com/verdenmax/agentprof/blob/main/crates/agentprof-<crate>/src/<module>.rs#L<不写>
```
**不带行号** — 参考 langchain-visual-guide 的做法，避免升级失效。读者点开后用浏览器 Ctrl+F 找 `symbol`。

---

## 7. 测试 & 验收

### 7.1 自动化测试

- **`cargo test -p xtask`**：5 个集成测试（见 §5.3）。
- **`cargo xtask visual-guide --check`**：PR CI 路径，<30s。
- **`cargo xtask visual-guide`**：main push CI 路径，重生成 + 上传 Pages artifact。

### 7.2 手工验收（14 项）

| # | 验收项 | 如何验 |
|---|---|---|
| 1 | `cargo xtask visual-guide` 成功生成 15 个 HTML（1 index + 6 usage + 8 wiki） | 输出文件计数 |
| 2 | 14 课 + index 全部能在 Chrome / Firefox 用 `file://` 打开 | 手工 |
| 3 | 每课的 prev/next 导航指向正确 | xtask test #3 |
| 4 | 每课底部「相关源码」链接到 GitHub blob URL，URL 中带 `main` 分支（不带行号） | 手工核对 |
| 5 | 真实截图 / 火焰图 SVG 在 mobile + desktop 都能正常显示 | 手工 |
| 6 | dark mode 自动跟随系统 | 手工切换 |
| 7 | 「Wiki §1 架构全景」的 5-crate 依赖图与 `docs/architecture.md` §3 一致 | 手工核对 |
| 8 | 「Wiki §3 Adapter」给出可执行的 ClaudeAdapter 写法纲要（不写实现，但给文件路径 + 步骤） | 手工核对 |
| 9 | 「用法 §5 serve」的截图是 `agentprof serve` 真实输出（不是 mockup） | 手工生成 |
| 10 | `cargo xtask visual-guide --check` 在 PR 上 < 30s 完成 | CI 实测 |
| 11 | GH Pages 部署后访问 `https://verdenmax.github.io/agentprof/usage/01-...html` 可达 | 部署后测 |
| 12 | 全部 14 课中文字数 ≥ 8000 字（每课均 ~600 字最低线） | `wc -m` |
| 13 | 5 个 xtask 集成测试全绿 | `cargo test -p xtask` |
| 14 | README.md 加「在线阅读」徽章 | 手工核对 |

### 7.3 性能

不是性能敏感场景。`xtask visual-guide` 全量生成在开发机 < 5s，CI < 30s（含 cargo build 增量）。

---

## 8. 风险 & 缓解

| # | 风险 | 严重度 | 缓解 |
|---|---|---|---|
| 1 | askama 在 xtask 增加编译时间 | 低 | xtask 是独立 crate，主 workspace build 不受影响；只在 `cargo build -p xtask` 时编译 |
| 2 | 手写 lexer 漏 token 类型 | 低 | 4 语言固定 + fixture 验证；漏判降级为普通文本，不影响可读性 |
| 3 | 真实截图随版本失效 | 中 | 截图加版本水印；CHANGELOG 记录截图所属版本；T17 工作单列 |
| 4 | GH Pages 与未来 rustdoc 部署冲突 | 低 | 当前未部署 rustdoc；本指南只占 `/usage/` `/wiki/` `/assets/` `/index.html`，未来 rustdoc 可走 `/api/` |
| 5 | HTML 不入 git 导致 reviewer 看不到效果 | 低 | CI 在 PR 也跑 `--check`；本地 reviewer 跑 `cargo xtask visual-guide` 即可预览；GH Pages 部署后 PR 可加 preview link（暂不做） |
| 6 | 中文内容写作工作量大 | 中 | subagent-driven：每课独立 subagent 写作，并行化；reviewer 校对 |
| 7 | Wiki §3 "如何写新 adapter" 与未来 M3.1 实际实现冲突 | 低 | 只给纲要 + ADR 引用，不给完整代码；M3.1 ship 后回填 |

---

## 9. 路线图 / 后续

| 后续工作 | 触发条件 | ETA |
|---|---|---|
| 截图自动刷新 `xtask visual-guide --refresh-screenshots` | agentprof 出现一次 UI 大改 | M3.x |
| 英文版（i18n） | 收到 ≥ 1 个国际用户请求 | TBD |
| 添加课末 quiz（quizzes.rs） | 用户反馈"想自测"  | TBD |
| Wiki §3 补 ClaudeAdapter 实战源码 | M3.1 ship 后 | M3.1 wave |
| 添加 mcp-waste / watch / pricing 专课 | 用户反馈缺失 | TBD |
| PDF 导出（cargo xtask visual-guide --pdf） | 收到打印需求 | TBD |

---

## 10. 引用

- 参考项目：[`verdenmax/langchain-visual-guide`](https://github.com/verdenmax/langchain-visual-guide)
- 项目主架构：`docs/architecture.md`（L1）
- 既有 ADR：`docs/internals/adr-*.md`（24 份）
- 既有路线图：`docs/plan.md` §6 Phase 0/1/2/3
- 既有 features：`docs/features/*.md`
- 既有 xtask 范例：`xtask/src/anonymize.rs`、`xtask/src/release.rs`

---

## 11. 决策点（待确认）

- [ ] D-7 选项 A（v0.3.4 tag）vs 选项 B（仅 main commit）— 由 user 在 plan T21 之前选。
- [ ] CHANGELOG.md `[0.3.4]` 还是 `[Unreleased]` — 与上一项绑定。

其他设计点均已 user-approved（5 轮 ask_user 全部完成）。

---

**End of design spec.** 下一步：spec self-review + user review → writing-plans skill。
