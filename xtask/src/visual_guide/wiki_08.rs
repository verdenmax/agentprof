//! Wiki lesson 8 — 「贡献指南」.
//!
//! Final lesson of the Wiki chapter. Walkthrough of the contribution
//! workflow: Conventional Commits (every type already exercised in
//! this repo), the 9-stage pipeline from
//! `.github/copilot-instructions.md` §5 (brainstorming → ADR →
//! plan → TDD → CI → review → release), and the practical
//! checklist for opening a PR, adding an ADR, passing CI,
//! and writing a CHANGELOG entry. Pipeline stage names and skill
//! names are cross-checked against the live copilot-instructions
//! at T18 (HEAD `1634ec8`).
//!
//! Recon-confirmed corrections vs. the original brief:
//!
//!   - The pipeline has **9 stages** (0..8), but Stage 5 (CI/Infra)
//!     and Stage 6 (Debugging) are **horizontal / cross-cutting** —
//!     they don't sit between Stage 4 and Stage 7 on the main line.
//!     Main line is 0 → 1 → 2 → 3 → 4 → 7 → 8.
//!   - Stage 2 has a **trigger threshold** (§5.5) — not every
//!     change needs an ADR; only ≥2 considered options, new
//!     public API, supersession of an existing ADR, or post-hoc
//!     hotfix ADR.
//!   - `source_ref()` is a `crates/agentprof-X/src/...` URL builder;
//!     for non-crate paths (`.github/...`, `CONTRIBUTING.md`) the
//!     correct pattern is hand-written `<a href="...">`.

use super::components::{accordion, comparison_table};

/// Render the HTML body for wiki lesson 8.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_08::render();
/// assert!(html.contains("Conventional Commits"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"
<h1>贡献指南</h1>

<p class="lead">
给 agentprof 提 PR 不是「随便写写然后 push」—— 整个流程被钉成一个 <strong>9 阶段 pipeline</strong>（详见 <a href="https://github.com/verdenmax/agentprof/blob/main/.github/copilot-instructions.md"><code>.github/copilot-instructions.md</code></a> §5）：从 brainstorming 起步、写 spec、必要时写 ADR、写 plan、TDD 实现、本地 gate 验证、PR 审、合并。每个 stage 有专门 skill 支撑（meta、TDD、ADR、release 等）。Commit message 走 <strong>Conventional Commits</strong>，CHANGELOG 走 <strong>Keep a Changelog</strong>，SemVer 严格执行。
</p>

<div class="card analogy">
  <div class="tag">🐧 类比 — 像 Linux kernel 的 patch flow</div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong>kernel</strong>：RFC patch → mailing list 讨论 → maintainer review → Tested-by / Acked-by → 进 -next → mainline merge。</li>
    <li><strong>agentprof</strong>：design spec → 用户 approve → 必要时 ADR → plan → 多 commit 实现（每 commit 自带 test + docs）→ <code>verification-before-completion</code> → PR → review → merge。</li>
    <li><strong>共同点</strong>：先<strong>写清楚</strong>再写代码；任何「为什么这么做」的决策都有文档化痕迹；commit 粒度小、message 严谨；CI 是「最后一道门」而不是「我才发现 bug 的地方」。</li>
  </ul>
  <p style="margin:.6em 0 0;font-size:.92rem;color:var(--muted)">agentprof 不是 kernel 那种「百人协作 + 数十 maintainer 同步」的规模，所以 pipeline 比 kernel 轻 —— 没有强制 mailing list、没有 Tested-by 标签 —— 但<strong>「先 design 再写、文档随代码合并」</strong>这两条核心同款。</p>
</div>

<h2>9 阶段 pipeline 主线（主 stage 表）</h2>
"#);

    s.push_str(&comparison_table(
        &["Stage", "主产物", "谁负责 / 触发"],
        &[
            (
                "<strong>0</strong> Boot",
                "加载常驻 instructions（<code>rust.instructions.md</code> + <code>update-docs-on-code-change.instructions.md</code>）",
                "AI assistant 每次会话开头，invoke <code>using-superpowers</code>",
            ),
            (
                "<strong>1</strong> Discovery / Design",
                "<code>docs/superpowers/specs/YYYY-MM-DD-&lt;topic&gt;-design.md</code>",
                "contributor + user 走 <code>brainstorming</code> skill；user approve design 才进 stage 2",
            ),
            (
                "<strong>2</strong> Decision Records（条件，见 §5.5）",
                "<code>docs/internals/adr-NNNN-&lt;topic&gt;.md</code>",
                "contributor 走 <code>create-architectural-decision-record</code>；门槛：≥2 候选方案 / 新公开 API / 否决既有 ADR / 事后补 hotfix ADR",
            ),
            (
                "<strong>3</strong> Planning",
                "<code>docs/superpowers/specs/YYYY-MM-DD-&lt;topic&gt;-plan.md</code>",
                "contributor 走 <code>writing-plans</code>；user approve plan 才进 stage 4",
            ),
            (
                "<strong>4</strong> Implementation",
                "代码 + L3 rustdoc + 测试（每 commit 自带）",
                "contributor + subagents 走 <code>test-driven-development</code>；多 commit 也行，每 commit 满足「code + docs + tests 同 commit」",
            ),
            (
                "<strong>5</strong> CI / Infra（横切）",
                "<code>.github/workflows/*.yml</code> + <code>docs/internals/ci-&lt;workflow&gt;.md</code>",
                "<strong>仅当本次 PR 改 workflow</strong>时触发；走 <code>create-github-action-workflow-specification</code>",
            ),
            (
                "<strong>6</strong> Debugging（横切，回原 stage）",
                "失败测试 + 修复 commit（<code>fix:</code> 前缀）",
                "任意 stage 撞 bug → <code>systematic-debugging</code> → 修完<strong>返回原 stage</strong>",
            ),
            (
                "<strong>7</strong> Completion verification",
                "本地 gate 输出证据 + PR description + CHANGELOG entry",
                "contributor 走 <code>verification-before-completion</code>；CI 全绿 + reviewer approve 才能 merge",
            ),
            (
                "<strong>8</strong> Release（仅 release 任务）",
                "<code>CHANGELOG.md</code> + SemVer tag + GitHub Release (cargo-dist 多平台 binary)",
                "走 <code>github-release</code>；commit 类型决定 SemVer bump（feat=minor / fix=patch / BREAKING=major）",
            ),
        ],
    ));

    s.push_str(r#"<p style="font-size:.88rem;color:var(--muted)">⚠️ Recon 校正：「9 阶段」字面是 Stage 0..8，但 <strong>Stage 5 和 6 是横切层</strong>—— 不在主线上。主线只走 0 → 1 → 2 → 3 → 4 → 7 → 8。仅两种快通道允许越级：trivial 改动（typo / 注释 / lint fix）跳过 Stage 1-3；hotfix 走 0 → 6 → 7 → 8 然后<strong>事后补</strong> Stage 2 ADR。详见 <a href="https://github.com/verdenmax/agentprof/blob/main/.github/copilot-instructions.md"><code>.github/copilot-instructions.md</code> §5.3 / §5.5 / §5.6</a>。</p>"#);

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片：① Conventional Commits 全部 type 列表（本仓库已全用过）· ② 9 阶段 pipeline brief（recon §5 真实命名）· ③ 实操 4 步：开 PR / 加 ADR / 通过 CI / 写 CHANGELOG。</p>"#);

    // ---------------- Accordion 1: Conventional Commits ----------------
    let card1 = r#"<div class="qa">
<div class="q">📝 全部 type（按本仓库 git log 频率排序）</div>
<div class="a">
<table style="width:100%;border-collapse:collapse;font-size:.92rem">
<thead><tr style="background:var(--surface)">
<th style="text-align:left;padding:.3em .5em">type</th>
<th style="text-align:left;padding:.3em .5em">什么时候用</th>
<th style="text-align:left;padding:.3em .5em">SemVer 影响</th>
</tr></thead>
<tbody>
<tr><td style="padding:.3em .5em"><code>feat:</code></td><td style="padding:.3em .5em">新功能、新 cli 子命令、新 adapter、新 feature flag</td><td style="padding:.3em .5em"><strong>minor</strong> bump</td></tr>
<tr><td style="padding:.3em .5em"><code>fix:</code></td><td style="padding:.3em .5em">bug 修复（必须配回归测试）</td><td style="padding:.3em .5em"><strong>patch</strong> bump</td></tr>
<tr><td style="padding:.3em .5em"><code>docs:</code></td><td style="padding:.3em .5em">改 README / docs/ / rustdoc，<strong>不动代码逻辑</strong></td><td style="padding:.3em .5em">无（不发版）</td></tr>
<tr><td style="padding:.3em .5em"><code>test:</code></td><td style="padding:.3em .5em">加 / 改测试，<strong>不动产品代码</strong></td><td style="padding:.3em .5em">无</td></tr>
<tr><td style="padding:.3em .5em"><code>refactor:</code></td><td style="padding:.3em .5em">重构（不变外部行为），改完 test 全绿</td><td style="padding:.3em .5em">无（除非破坏 pub API）</td></tr>
<tr><td style="padding:.3em .5em"><code>chore:</code></td><td style="padding:.3em .5em">杂项：bump 依赖、调 Cargo.toml metadata、release commit</td><td style="padding:.3em .5em">无（chore(release) 触发 tag）</td></tr>
<tr><td style="padding:.3em .5em"><code>build:</code></td><td style="padding:.3em .5em">改构建系统：xtask、cargo-dist、Cargo workspace 结构</td><td style="padding:.3em .5em">无</td></tr>
<tr><td style="padding:.3em .5em"><code>ci:</code></td><td style="padding:.3em .5em">改 <code>.github/workflows/*.yml</code>（同时触发 Stage 5）</td><td style="padding:.3em .5em">无</td></tr>
<tr><td style="padding:.3em .5em"><code>BREAKING:</code><br>or <code>feat!:</code> / <code>fix!:</code></td><td style="padding:.3em .5em">破坏 pub API / wire format / CLI 协议</td><td style="padding:.3em .5em"><strong>major</strong> bump（pre-1.0 也可以走 minor）</td></tr>
</tbody>
</table>
<p style="margin:.6em 0 0;font-size:.88rem;color:var(--muted)"><strong>scope 写不写</strong>：optional，加了能锁定子系统 —— <code>feat(adapters): add gemini adapter</code> 比 <code>feat: add gemini adapter</code> 更易扫读。常用 scope：<code>core</code> / <code>adapters</code> / <code>storage</code> / <code>tui</code> / <code>cli</code> / <code>xtask</code>。</p>
</div>

<div class="q">✍️ commit message body 怎么写</div>
<div class="a"><strong>第一行</strong>：<code>type(scope): 一句话主谓宾</code>，≤72 字符。<strong>空一行</strong>。<strong>body</strong>：解释「<strong>为什么</strong>」（不是「做了什么」—— diff 自己看就行）；列关键决策、trade-off、为什么选这个不选那个。<strong>footer</strong>：<code>Refs: #N</code> / <code>Closes: #N</code> / <code>BREAKING CHANGE: ...</code> / <code>Co-authored-by: ...</code>。本仓库强制：AI assistant 写的 commit 必带 <code>Co-authored-by: Copilot &lt;...&gt;</code>。</div>
</div>"#;
    s.push_str(&accordion(
        1,
        "Conventional Commits 全部 type（本仓库已用全集）",
        card1,
    ));

    // ---------------- Accordion 2: pipeline brief ----------------
    let card2 = r#"<div class="qa">
<div class="q">🔄 主线 7 stage 工作流（recon §5.1 真实命名）</div>
<div class="a">
<pre class="code"><code>Stage 0 [Boot]
  - 加载 .github/instructions/*.instructions.md（rust + update-docs）
  - skill: using-superpowers（meta）

Stage 1 [Discovery / Design]
  - 触发：新 feature / 新 adapter / 架构变更
  - skill: brainstorming
  - 产物：docs/superpowers/specs/YYYY-MM-DD-&lt;topic&gt;-design.md
  - 出口：user approve design

Stage 2 [Decision Records]（条件触发，§5.5 门槛）
  - skill: create-architectural-decision-record
  - 产物：docs/internals/adr-NNNN-&lt;topic&gt;.md
  - 编号 NNNN 单调递增（冲突取已合并 max+1）

Stage 3 [Planning]
  - skill: writing-plans
  - 产物：docs/superpowers/specs/YYYY-MM-DD-&lt;topic&gt;-plan.md
  - 出口：user approve plan

Stage 4 [Implementation]
  - skill: test-driven-development（必）
  - 辅: executing-plans / subagent-driven-development /
        dispatching-parallel-agents / cli-mastery / copilot-cli-quickstart
  - 产物：代码 + L3 rustdoc + 测试（每 commit 自带）
  - 任意 bug → Stage 6（横切，修完回 Stage 4）
  - 改 workflow → Stage 5（横切，不阻塞主线）

Stage 7 [Completion verification]
  - skill: verification-before-completion（必）
  - 跑全部本地 gate（cargo fmt + clippy + test + doc）
  - 附输出证据到 PR description；写 CHANGELOG entry

Stage 8 [Release]（仅 release 任务）
  - skill: github-release
  - SemVer 决策（按 commit type 推 bump）+ Keep-a-Changelog + tag + GH Release</code></pre>
</div>

<div class="q">🔀 横切层 5 / 6</div>
<div class="a"><strong>Stage 5 CI/Infra</strong>：仅当 PR 改 <code>.github/workflows/*.yml</code> 时触发。和主线<strong>并行</strong>，不阻塞 Stage 4 / 7。skill：<code>create-github-action-workflow-specification</code>，产物含 spec md。<strong>Stage 6 Debugging</strong>：任意 stage 撞 bug / test 失败 / CI 红就进 → <code>systematic-debugging</code> 写复现 + 根因 + 修复 → 修完<strong>返回触发它的 stage</strong>（不前进）。修复 commit 用 <code>fix:</code> 前缀，关联失败测试。</div>

<div class="q">📜 Stage 2 ADR 触发门槛（§5.5）</div>
<div class="a"><strong>必写 ADR</strong>：① design 含 ≥2 个被认真考虑的方案；② 引入新 crate / 新 trait / 新公开 API 的关键设计；③ 否决 / 修改既有 ADR（旧的加 <code>Status: Superseded by adr-MMMM</code>）；④ hotfix（即使越级 Stage 1，也<strong>事后补</strong> ADR）。<strong>SKIP</strong>：① 显然方案无替代；② typo / README / lint fix；③ doctest 示例改；④ workspace 内部重构不变 pub API。<strong>口诀</strong>：「半年后回头看这次改动会问『为什么这么做』，就写 ADR」。</div>
</div>"#;
    s.push_str(&accordion(
        2,
        "9 阶段 pipeline brief（主线 + 横切层 + ADR 门槛）",
        card2,
    ));

    // ---------------- Accordion 3: 4-step checklist ----------------
    let card3 = r#"<div class="qa">
<div class="q">📋 步骤 1：开 PR 前本地 gate（<strong>必跑</strong>）</div>
<div class="a">
<pre class="code"><code># 格式 + lint
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 测试
cargo test --workspace --all-features
cargo insta test --check                  # snapshot 校验

# 文档（必须无 warning）
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace --all-features

# 依赖审计（如改了 dep）
cargo deny check</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem"><strong>全绿才能开 PR</strong>。CI 会跑同样的 gate；本地先跑省 PR 红/绿来回。</p>
</div>

<div class="q">📋 步骤 2：加 ADR（如触发 §5.5）</div>
<div class="a">
<ol style="margin:.5em 0 0 1.2em;font-size:.92rem">
<li>查最大编号：<code>ls docs/internals/adr-*.md | sort | tail -1</code> → 取 max+1。</li>
<li>创建文件：<code>docs/internals/adr-NNNN-&lt;topic&gt;.md</code>。</li>
<li>必备段落：<strong>Status</strong>（Accepted / Proposed / Superseded）、<strong>Date</strong>、<strong>Deciders</strong>、<strong>Related</strong>（链到上游 ADR）、<strong>Context</strong>、<strong>Decision</strong>、<strong>Consequences</strong>、<strong>Considered alternatives</strong>（每个有 rationale 说明为啥被否）。</li>
<li>commit 单独成一条：<code>docs(adr): NNNN &lt;short title&gt;</code>。</li>
<li>同步更新 <code>docs/internals/index.md</code>（如有）+ 引用它的代码顶部 <code>//!</code>。</li>
</ol>
</div>

<div class="q">📋 步骤 3：通过 CI</div>
<div class="a">CI workflow（<code>.github/workflows/ci.yml</code>）跑：<code>cargo fmt --check</code> · <code>cargo clippy -D warnings</code> · <code>cargo test --all-features</code> · <code>cargo deny check</code> · <code>RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps</code> · <code>docs-sync</code>（验证 pub API 改动配 rustdoc / README 改动）。<strong>red CI 的常见原因</strong>：① 公开 API 没加 <code># Examples</code>（<code>missing_docs</code> = error）；② 改了某 crate 没改对应 <code>README.md</code>（<code>docs-sync</code> fail）；③ 用了 <code>unwrap()</code>（clippy <code>unwrap_used = deny</code>）；④ lib crate 用了 <code>anyhow</code>（只允许 bin 用）。修完<strong>force-push</strong> 没问题 —— 本仓库没强制 linear history。</div>

<div class="q">📋 步骤 4：写 CHANGELOG entry</div>
<div class="a"><code>CHANGELOG.md</code> 走 <a href="https://keepachangelog.com/en/1.1.0/">Keep a Changelog</a> 格式：</p>
<pre class="code"><code>## [Unreleased]

### Added
- xtask: visual-guide subcommand generates 14-lesson HTML site (T18).

### Changed
- ...

### Fixed
- ...

### Deprecated / Removed / Security
- ...</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem"><strong>规则</strong>：① 每个 PR 至少加一行（小 typo 例外）；② release 时 <code>github-release</code> skill 把 <code>[Unreleased]</code> 整段挪到 <code>[v0.X.Y] — YYYY-MM-DD</code>；③ 用<strong>用户语言</strong>写（「now supports OTLP HTTP receiver」&gt; 「added serve_http fn」）；④ 链 issue/PR：<code>(#42)</code>。</p>
</div>

<div class="q">🆘 还有问题</div>
<div class="a">读完整指南：<a href="https://github.com/verdenmax/agentprof/blob/main/CONTRIBUTING.md"><code>CONTRIBUTING.md</code></a>（顶级规则）+ <a href="https://github.com/verdenmax/agentprof/blob/main/.github/copilot-instructions.md"><code>.github/copilot-instructions.md</code></a>（AI assistant 详尽版，也适合人读）。仍有疑问，开 GitHub Discussion，maintainer 会引导你走对应 stage 的 skill。</div>
</div>"#;
    s.push_str(&accordion(
        3,
        "实操 4 步：开 PR / 加 ADR / 通过 CI / 写 CHANGELOG",
        card3,
    ));

    s.push_str(r#"<h2>本套指南到此完结</h2>
<p>这是 agentprof 可视化指南的<strong>最后一课</strong> —— 用法 6 课带你从安装到 dashboard、Wiki 8 课带你从架构到贡献。如果你跟着读到了这里：感谢你的耐心，欢迎来 GitHub <a href="https://github.com/verdenmax/agentprof">verdenmax/agentprof</a> 提 issue / PR / Discussion。</p>

<p class="src-ref">📂 相关源码：<a href="https://github.com/verdenmax/agentprof/blob/main/CONTRIBUTING.md"><code>CONTRIBUTING.md</code></a> &nbsp;<code class="mono">顶级贡献规则</code></p>
<p class="src-ref">📂 相关源码：<a href="https://github.com/verdenmax/agentprof/blob/main/.github/copilot-instructions.md"><code>.github/copilot-instructions.md</code></a> &nbsp;<code class="mono">AI / 人共用的详尽 pipeline 文档</code></p>
<p class="src-ref">📂 相关源码：<a href="https://github.com/verdenmax/agentprof/blob/main/CHANGELOG.md"><code>CHANGELOG.md</code></a> &nbsp;<code class="mono">Keep-a-Changelog 历史</code></p>
"#);

    s
}
