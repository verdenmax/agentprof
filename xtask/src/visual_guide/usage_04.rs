//! Usage lesson 4 — 「list / aggregate：跨 session 视角」.
//!
//! Target audience: 会读单次 session 之后想看趋势的用户。覆盖
//! `list --since` 列最近 sessions，`aggregate --by model / tool /
//! mcp-server / day` 跨 session 聚合，以及「何时用哪个 `--by`」决策。

use super::components::{accordion, comparison_table, decision_tree, source_ref};

/// Render the HTML body for usage lesson 4.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::usage_04::render();
/// assert!(html.contains("--by"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
一次 session 只是数据点，<strong>连续观测才是趋势</strong> —— <code>agentprof list</code> 列最近 sessions 给你「本周用得多吗」的即时感，<code>agentprof aggregate --by model/tool/day</code> 给跨 session 报表回答「<strong>哪个模型 cache 命中率高</strong>」「<strong>哪个 tool 是 token 大户</strong>」「<strong>这一周 agent 是不是在空转</strong>」。
</p>
"#);

    s.push_str(&decision_tree(
        "你想看的是什么维度？",
        &[
            (
                "📊 模型对比",
                "<code>aggregate --by model</code> — 含 cache 命中列 (ADR-0023)",
            ),
            (
                "🔧 找浪费 tool",
                "<code>aggregate --by tool</code> — 按 total_duration 排",
            ),
            (
                "📅 时间趋势",
                "<code>aggregate --by day</code> + <code>--low-utilization-threshold</code>",
            ),
            (
                "📋 单 session 列表",
                "<code>list --since 7d --limit 20</code>",
            ),
        ],
    ));

    s.push_str(r#"
<div class="card analogy">
  <div class="tag">🔌 生活类比</div>
  从 <strong><code>top</code> 升级到 <code>Grafana</code></strong> —— <code>analyze</code> 是<strong>单点快照</strong>（这一刻 CPU 在干嘛），<code>list</code> + <code>aggregate</code> 是<strong>仪表盘</strong>（这一周 CPU 用在了哪里、哪个进程一直涨）。同样的数据源，换一个时间维度看就是另一个故事。
</div>

<h2>3 条命令 — 跨 session 怎么挑？</h2>

<p>下面三条覆盖最常见的「<strong>本周用量</strong> / <strong>模型对比</strong> / <strong>tool 排名</strong>」三种问法：</p>
"#);

    s.push_str(&comparison_table(
        &["命令", "输出", "典型场景"],
        &[
            (
                "<pre class=\"code\">agentprof list --since 7d</pre>",
                "最近 sessions 列表（Cache% / Turns / Out-tokens）",
                "「本周用得多吗？哪几次最贵？」",
            ),
            (
                "<pre class=\"code\">agentprof aggregate --by model</pre>",
                "跨模型对比（CacheCr / CacheRd / Hit% / NetSaved）",
                "「Sonnet vs Opus 哪个 cache 命中率高？」",
            ),
            (
                "<pre class=\"code\">agentprof aggregate --by tool \\\n  --since 30d</pre>",
                "Tool 维度排名（调用次数 + 总 token）",
                "「过去一个月哪个 tool 是 token 大户？」",
            ),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 四张卡片展开 <strong>list</strong> 与三种 <code>--by</code> 模式：<strong>① 输出长什么样 · ② 为什么这么设计 · ③ agentprof 怎么做</strong>。</p>"#);

    s.push_str(&accordion(
        1,
        "list --since 7d --limit 20 — 最近 sessions",
        r#"<div class="qa">
<div class="q">🧪 输出长什么样</div>
<div class="a">一张紧凑表：每行 <strong>session id</strong> + <strong>start 时间</strong> + <strong>turns</strong> + <strong>out-tokens</strong> + <strong>Cache%</strong>（M2.5 加的列），按时间倒序。<code>--since</code> 支持 <code>7d</code> / <code>12h</code> / <code>30m</code> / <code>all</code>；<code>--limit 20</code> 控制行数。一眼能看出「最近哪几次特别费 token」「哪几次 cache 完全没命中」。</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">单看一次 session 容易误判 —— 也许这次就是任务特别复杂。<strong>列最近 N 次</strong>才看得出基线：如果你每天平均 30k token，今天突然 300k，那就值得深挖。<code>--since</code> 用 <strong>文件 mtime</strong> 过滤而不是解析每个 session 头里的时间戳，便宜很多 —— 列表场景不需要精确到秒。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">走 <code>Adapter::discover()</code> trait —— 每家适配器（<code>ClaudeAdapter</code> / <code>CopilotAdapter</code> / <code>CodexAdapter</code>）自己实现「在我的默认路径下扫 session 文件」。<code>--since</code> 解析在 <code>cmd/since.rs</code> 抽出复用，被 <code>list</code> / <code>aggregate</code> / <code>watch</code> 三处共享。Cache% 列只读 session header 不做完整解析，所以 20 行的列表能在百毫秒级返回。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "aggregate --by model — 跨模型 cache 对比",
        r#"<div class="qa">
<div class="q">🧪 输出长什么样</div>
<div class="a">按模型分组的表：每行一个模型，列出 <strong>CacheCr</strong>（cache_creation_input_tokens 总和）/ <strong>CacheRd</strong>（cache_read_input_tokens 总和）/ <strong>Hit%</strong>（命中率）/ <strong>NetSaved</strong>（净节省 token 数 = read - creation × write_multiplier）。直接能看出「<strong>Sonnet 在这批 session 里 cache 命中 85%</strong>」vs「<strong>Opus 只有 40%</strong>」这种对比。</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">不同模型的 <strong>cache 行为差异巨大</strong> —— 有的 prompt 结构对 prefix cache 友好，有的因为 system message 频繁变动一直 miss。聚合到模型维度才看得出「<strong>应不应该把这条 workload 迁去某个模型</strong>」。NetSaved 不是单纯算 CacheRd，要扣掉 creation 的额外成本（per ADR-0023），才是真实省下的钱。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">核心算法在 <code>agentprof-core::analyzer::aggregate::aggregate_by_model</code>，CLI 入口在 <code>agentprof-cli::cmd::aggregate::compute_aggregate</code> 调度。Cache 列的口径完全跟随 <strong>ADR-0023</strong>（"cache attribution"）—— per-session cache 算出 <code>CacheMetrics</code> 后按模型 sum，避免重复计入。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "aggregate --by tool / --by mcp-server — 找 token 大户",
        r#"<div class="qa">
<div class="q">🧪 输出长什么样</div>
<div class="a">按 tool（或 mcp-server）分组：每行 <strong>tool 名 / 调用次数 / 总 out-tokens / 平均每次 token / sessions 覆盖数</strong>。一眼能看出「<strong>filesystem.read_file 调了 800 次平均 12k token</strong>」这种红色信号 —— 是真的需要读这么多，还是 agent 在重复读同一个文件？<strong>不出 Cache 列</strong>（per ADR-0023 D-3：per-tool cache attribution undefined，硬编码列也会误导）。</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">"<strong>哪个 tool 是 token 大户</strong>"是优化第一性问题 —— 砍掉一个高频高 token 的 tool 比调任何 prompt 都管用。聚合到 tool / mcp-server 两个维度对应两种砍法：tool 维度找<strong>单一调用</strong>过贵，mcp-server 维度找<strong>整个 server</strong>该不该挂。<strong>不出 cache</strong>是诚实 —— cache 是 prompt prefix 维度的概念，把它强行摊到 tool 上要么算不准要么误导用户。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">两个独立函数 <code>aggregate_by_tool</code> / <code>aggregate_by_mcp_server</code> 在 <code>agentprof-core::analyzer::aggregate</code>，输出 schema <strong>不含</strong> cache 字段（编译期就拒绝误用）。<code>compute_aggregate</code> 根据 <code>--by</code> 参数派发到对应函数，渲染层（md / tui）也对应两套表头。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        4,
        "aggregate --by day + --low-utilization-threshold — 时间序列",
        r#"<div class="qa">
<div class="q">🧪 输出长什么样</div>
<div class="a">按天分桶的时间序列：每行一个日期 + <strong>sessions 数</strong> + <strong>turns 数</strong> + <strong>总 token</strong> + <strong>是否低利用率</strong>。配合 <code>--low-utilization-threshold 5000</code>（默认 token / day 阈值）能直接标红那些「<strong>开了 agent 但实际没怎么用</strong>」的日子 —— 比如 IDE 后台跑着 watch 但你那天去开会了。</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">订阅制 agent（Copilot / Claude）按月付费，<strong>低利用率 = 钱白花</strong>。时间维度能暴露两种浪费：① 长期空转的 watcher / hook，② 周末 / 节假日的无效保持。把判定<strong>内建</strong>到 aggregate 而不是让用户自己看曲线，因为大部分人不会主动审。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a"><code>aggregate_by_day</code> 返回一组 <code>DayBucket</code>，每个桶上挂 <code>is_low_utilization</code> 方法 —— 输入是用户给的阈值（默认值见 <code>agentprof-core::analyzer::aggregate</code> 常量）。CLI 层只负责渲染时把 <code>true</code> 的行标红 / 加 ⚠ 前缀，不在渲染层做判定逻辑（per L1 分层规约：业务判定归 core）。</div>
</div>"#,
    ));

    s.push_str("<h2>「何时用哪个 <code>--by</code>」决策表</h2>\n");

    s.push_str(&comparison_table(
        &["想回答的问题", "用这个 --by", "为什么"],
        &[
            (
                "哪个模型 cache 命中率最高 / NetSaved 最多？",
                "<code>--by model</code>",
                "唯一含 cache 列的维度（per ADR-0023）",
            ),
            (
                "哪个 tool / mcp-server 是 token 大户？",
                "<code>--by tool</code> 或 <code>--by mcp-server</code>",
                "调用次数 × 平均 token 直接排序",
            ),
            (
                "这一周 / 一月趋势如何？有没有空转日？",
                "<code>--by day</code>",
                "时间桶 + low-utilization 自动告警",
            ),
        ],
    ));

    s.push_str("<h2>下一步</h2>\n<p>会看跨 session 之后，下一课会带你看 <strong>watch + serve</strong>：把这些聚合做成<strong>常驻面板</strong>，agent 在跑的时候实时刷新；以及 <strong>config</strong> 怎么把这些参数固化到本地配置避免每次敲一长串。</p>\n");

    s.push_str(&source_ref("agentprof-cli", "cmd/list.rs", "run"));
    s.push_str(&source_ref(
        "agentprof-cli",
        "cmd/aggregate.rs",
        "compute_aggregate",
    ));

    s
}
