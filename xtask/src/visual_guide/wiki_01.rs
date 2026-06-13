//! Wiki lesson 1 — 「架构全景」.
//!
//! Target audience: intermediate user + project contributor who needs
//! a single page that names every crate, the dependency rules between
//! them, the L1/L2/L3 doc system, and the ADR index.

use super::components::{accordion, comparison_table, source_ref};

/// Render the HTML body for wiki lesson 1.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_01::render();
/// assert!(html.contains("agentprof-core"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"
<h1>架构全景</h1>

<p class="lead">
agentprof 是一个 <strong>5 crate 的 Rust workspace</strong>（<code>core</code> / <code>adapters</code> / <code>storage</code> / <code>tui</code> / <code>cli</code>，外加构建辅助 <code>xtask</code>）。
<strong><code>agentprof-core</code> 是依赖图的叶子</strong>（零 workspace 依赖），<strong><code>agentprof-cli</code> 是唯一组装层</strong>（依赖所有其他 crate）。
项目级关键决策由 <strong>24 份 ADR</strong> 锁定，连同 L1/L2/L3 三级文档体系一起，构成「读了 30 分钟就能动手贡献」的知识地基。
</p>

<div class="card analogy">
  <div class="tag">🛫 生活类比 — 把 agentprof 想成一座机场</div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong><code>agentprof-core</code> = 飞机引擎</strong> — 核心算法、数据模型、tokenizer、export 格式。没它什么都飞不起来。</li>
    <li><strong><code>agentprof-adapters</code> = 地勤</strong> — 接 Claude / Codex / Copilot 三个不同登机口；每加一家 AI agent CLI 就多一条地勤线。</li>
    <li><strong><code>agentprof-storage</code> = 行李系统</strong> — SQLite 持久化 + OTLP receiver（M2.2 + M2.4），把会话数据稳妥放下、随时取回。</li>
    <li><strong><code>agentprof-tui</code> = 塔台</strong> — 5 个交互视图（Sessions / TurnDetail / ToolRank / HookRank / Models），实时看流量。</li>
    <li><strong><code>agentprof-cli</code> = 航站楼</strong> — 用户唯一入口，所有子命令在这里挂载。</li>
  </ul>
</div>

<h2>5 个 crate 的角色与依赖</h2>

<p>记一句话：<strong><code>core</code> 在最底下，<code>cli</code> 在最上面，中间没有循环</strong>。</p>
"#);

    s.push_str(&comparison_table(
        &["crate", "角色", "依赖的 workspace crate"],
        &[
            (
                "<code>agentprof-core</code>",
                "叶子库：data model + analyzer + tokenizer + export",
                "<strong>零</strong>（必须保持叶子）",
            ),
            (
                "<code>agentprof-adapters</code>",
                "<code>Adapter</code> trait + <code>CopilotAdapter</code>（M3.1 / M3.2 待添 Claude / Codex）",
                "仅依赖 <code>core</code>",
            ),
            (
                "<code>agentprof-storage</code>",
                "SQLite 持久化 + OTLP receiver（M2.2 + M2.4）",
                "依赖 <code>core</code>",
            ),
            (
                "<code>agentprof-tui</code>",
                "5 视图：Sessions / TurnDetail / ToolRank / HookRank / Models",
                "依赖 <code>core</code>",
            ),
            (
                "<code>agentprof-cli</code>",
                "唯一组装层：所有子命令 + <code>main</code>",
                "依赖<strong>所有</strong>其他 crate",
            ),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片分别讲：① 依赖图与无环规则 · ② L1/L2/L3 三级文档体系 · ③ 24 份 ADR 索引。每张都给「为什么 / agentprof 怎么做 / 其他选择」。</p>"#);

    s.push_str(&accordion(
        1,
        "5-crate 依赖图 + 无环规则",
        r#"<div class="qa">
<div class="q">🗺️ 依赖图</div>
<div class="a">
<img class="diagram" src="../assets/architecture-deps.svg" alt="5-crate 依赖图（cli 顶层 → adapters / storage / tui → core 叶子；T19 落实际 SVG）">
<p style="font-size:.88rem;color:var(--muted);margin-top:.3em">图：5 crate 依赖关系。<code>cli</code> 在顶层，依赖所有其他 crate；<code>tui</code> / <code>adapters</code> / <code>storage</code> 在中间层，只依赖 <code>core</code>；<code>core</code> 是叶子。<em>实际 SVG 在 T19 渲染落盘。</em></p>
<pre class="code">agentprof-cli  ──▶  agentprof-tui
       │                │
       ├──────────────▶ agentprof-adapters ──▶ agentprof-core
       │                                          ▲
       └──▶ agentprof-storage ───────────────────┘</pre>
</div>
<div class="q">🤔 为什么这样切</div>
<div class="a">三条硬规则：<strong>① <code>core</code> 必须是叶子</strong> — 它绝不能 <code>use agentprof_adapters::...</code> 或任何 workspace crate，这样核心数据模型可以独立单元测试、独立发版、独立 reuse；<strong>② <code>cli</code> 是唯一组装层</strong> — 所有子命令逻辑只能放 <code>agentprof-cli</code>，不允许 lib crate 依赖它，保持 lib / bin 边界；<strong>③ 依赖图无环</strong> — lib crate 之间不能有 cycle。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">CI 用 <code>cargo metadata</code> + <code>grep</code> 校验无环。workspace <code>[lints]</code> 段统一开 <code>clippy::missing_docs_in_private_items</code>、<code>clippy::unwrap_used = &quot;deny&quot;</code> 等，配合 <code>docs/architecture.md §3</code>「Crate 边界」与 <code>.github/copilot-instructions.md §3</code> 的硬规则。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>mono-crate</strong>（所有代码塞一个 lib）— 上手最简单，但难做单元测试和分层；<strong>10+ 微 crate</strong> — 边界更清晰但编译期、维护成本大幅上升。<strong>5 crate workspace</strong> 是 Rust 生态在中型 CLI 工具上的标准做法（参考 <code>cargo</code> / <code>rust-analyzer</code> 自己的拆分）。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "L1 / L2 / L3 三级文档体系",
        r#"<div class="qa">
<div class="q">📚 三个等级</div>
<div class="a">
<ul style="margin:.3em 0 0 1.2em">
<li><strong>L1（项目级）</strong>：<code>docs/architecture.md</code>（权威架构）/ <code>docs/plan.md</code>（路线图）— 改动跨 crate 或动到分层时必须同 PR 更新。</li>
<li><strong>L2（crate 级）</strong>：每个 crate 一份 <code>crates/&lt;name&gt;/README.md</code>，描述该 crate 的对外接口、模块表、feature flags、典型用法。</li>
<li><strong>L3（API 级）</strong>：rustdoc <code>///</code> + 强制 <code># Examples</code> + <code># Errors</code> + <code># Panics</code> 段；公开 API 必带 doc（<code>missing_docs</code> 已升 error）。</li>
</ul>
</div>
<div class="q">🤔 为什么必要</div>
<div class="a">文档不能落后于代码。AI agent 最常见的反模式是「改了 <code>pub fn</code> 签名但不更 rustdoc / CHANGELOG / L2 README」— 半年后回头看，谁都不知道当时为什么改。三级体系给每种「文档颗粒度」一个固定的家。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a"><code>.github/instructions/update-docs-on-code-change.instructions.md</code> 是 Copilot 常驻规则，applyTo <code>**/*.{md,rs,...}</code>，每次编辑自动加载；CI 跑 <code>docs-sync</code> job 校验 CHANGELOG 是否同步；clippy <code>missing_docs</code> 升 error 让没写 rustdoc 的 PR 直接红。L1 / L2 / L3 的「触发表」写在 <code>.github/copilot-instructions.md §4.2</code>。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>纯 rustdoc</strong>（无 markdown）— API 链接好但跨 crate 叙事跳跃；<strong>纯 markdown wiki</strong>（GitHub Wiki / Notion）— 叙事好但缺 API 自动链接、容易和代码漂移；三级体系是「叙事 + API + 项目级决策」三种需求各得其所。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "24 份 ADR 决策记录索引（最近 6 份）",
        r#"<div class="qa">
<div class="q">📜 什么是 ADR</div>
<div class="a">Architectural Decision Record — 每个「选 A 不选 B」的决策都写一份 markdown，存在 <code>docs/internals/adr-NNNN-&lt;topic&gt;.md</code>。截至 v0.4.x 共 24 份（编号 0001–0024），第 25 份「visual guide」在 T21 落盘。</div>
<div class="q">📋 最近 6 份</div>
<div class="a">
<table>
<thead><tr><th>ID</th><th>标题</th><th>状态</th><th>日期</th><th>链接</th></tr></thead>
<tbody>
<tr><td>ADR-0019</td><td>Hybrid storage mode（SQLite + jsonl 兜底）</td><td>Accepted</td><td>2026-05</td><td><a href="https://github.com/verdenmax/agentprof/blob/main/docs/internals/adr-0019-hybrid-storage-mode.md"><code>adr-0019</code></a></td></tr>
<tr><td>ADR-0021</td><td>OTLP receiver architecture</td><td>Accepted</td><td>2026-05</td><td><a href="https://github.com/verdenmax/agentprof/blob/main/docs/internals/adr-0021-otlp-receiver-architecture.md"><code>adr-0021</code></a></td></tr>
<tr><td>ADR-0022</td><td>OTLP capacity caps + LRU eviction</td><td>Accepted</td><td>2026-05</td><td><a href="https://github.com/verdenmax/agentprof/blob/main/docs/internals/adr-0022-otlp-capacity-caps-and-lru-eviction.md"><code>adr-0022</code></a></td></tr>
<tr><td>ADR-0023</td><td>Cache metrics（honest + naive hit rate）</td><td>Accepted</td><td>2026-06</td><td><a href="https://github.com/verdenmax/agentprof/blob/main/docs/internals/adr-0023-cache-metrics.md"><code>adr-0023</code></a></td></tr>
<tr><td>ADR-0024</td><td>Web dashboard architecture（<code>serve</code>）</td><td>Accepted</td><td>2026-06</td><td><a href="https://github.com/verdenmax/agentprof/blob/main/docs/internals/adr-0024-web-dashboard-architecture.md"><code>adr-0024</code></a></td></tr>
<tr><td>ADR-0025</td><td>Visual guide（本指南）</td><td>Pending（T21 落盘）</td><td>2026-06</td><td>—</td></tr>
</tbody>
</table>
<p style="font-size:.88rem;color:var(--muted);margin-top:.3em">完整 24 份索引见 <a href="https://github.com/verdenmax/agentprof/tree/main/docs/internals"><code>docs/internals/</code></a>。</p>
</div>
<div class="q">🤔 为什么必要</div>
<div class="a">决策不写下来，半年后没人记得「为什么 storage 选 hybrid 不选纯 SQLite」「为什么 cache hit rate 同时算 honest 和 naive」。ADR 给每个非显然的选择一份「上下文 + 候选方案 + 决策 + 后果」的固定模板。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a"><code>.github/skills/create-architectural-decision-record/</code>（vendored 自 <code>github/awesome-copilot</code>）是项目专属 skill；触发门槛写在 <code>copilot-instructions.md §5.5</code> — 「设计含 ≥2 个值得文档化方案 / 新 crate / 新公开 API / 否决既有 ADR」就 MUST 写。编号单调递增，旧 ADR 被推翻就加 <code>Status: Superseded by adr-MMMM</code>。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>commit message</strong> — 太散，半年后翻不回来；<strong>GitHub Wiki</strong> — 容易和代码漂移、PR review 不到；<strong>ADR markdown 入 git</strong> — 跟代码同源、PR review 可看、grep 可找，是当前业界共识（参考 Michael Nygard 原始提案）。</div>
</div>"#,
    ));

    s.push_str(r#"<h2>9 阶段 pipeline 一览</h2>
<p>对外开发流程也用同一套约束 — 见 <a href="https://github.com/verdenmax/agentprof/blob/main/.github/copilot-instructions.md"><code>.github/copilot-instructions.md §5</code></a>。九个 stage 每个挂一个主 skill：</p>
<table>
<thead><tr><th>Stage</th><th>阶段</th><th>主 skill</th></tr></thead>
<tbody>
<tr><td>0</td><td>Boot（会话开头）</td><td><code>using-superpowers</code></td></tr>
<tr><td>1</td><td>Discovery / Design</td><td><code>brainstorming</code></td></tr>
<tr><td>2</td><td>Decision Records</td><td><code>create-architectural-decision-record</code> ★</td></tr>
<tr><td>3</td><td>Planning</td><td><code>writing-plans</code></td></tr>
<tr><td>4</td><td>Implementation</td><td><code>test-driven-development</code></td></tr>
<tr><td>5</td><td>CI / Infra（横切）</td><td><code>create-github-action-workflow-specification</code> ★</td></tr>
<tr><td>6</td><td>Debugging loop（横切）</td><td><code>systematic-debugging</code></td></tr>
<tr><td>7</td><td>Completion verification</td><td><code>verification-before-completion</code></td></tr>
<tr><td>8</td><td>Release</td><td><code>github-release</code> ★</td></tr>
</tbody>
</table>
<p style="font-size:.9rem;color:var(--muted)">★ = project skill（<code>.github/skills/</code>，跟随 clone）；其余 = <code>obra/superpowers</code> 全局 plugin。Stage 5 / 6 是横切层，可在主线任意点切入；完成后回原 stage。</p>

<h2>下一步</h2>
<p>本课给出了 5 crate 是什么、依赖怎么排、文档怎么分层、决策怎么记。接下来的 Wiki 课会逐 crate 深入 — 下一课「<strong><code>agentprof-core</code> 深度解读</strong>」拆解 data model、analyzer、tokenizer 的内部结构。</p>
"#);

    s.push_str(&source_ref("agentprof-cli", "main.rs", "Cli"));
    s.push_str(&source_ref("agentprof-core", "lib.rs", "agentprof_core"));

    s
}
