//! Usage lesson 1 — 「agentprof 是什么」.
//!
//! Target audience: complete newcomer who has never run agentprof and
//! is not sure what "token profiling" means.

use super::components::{accordion, comparison_table, source_ref};

/// Render the HTML body for usage lesson 1.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::usage_01::render();
/// assert!(html.contains("agentprof"));
/// ```
#[must_use]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"
<h1>agentprof 是什么</h1>

<p class="lead">
agentprof 是用 Rust 写的<strong>开源命令行工具</strong>，专门用来分析 AI agent CLI（Claude Code / GitHub Copilot CLI / OpenAI Codex）<strong>每一次对话烧掉的 token 都花在哪</strong>。
不止统计花了多少，更回答<strong>「花得值不值」</strong>。
</p>

<div class="card analogy">
  <div class="tag">🔌 生活类比</div>
  把 AI agent 想成一台<strong>很费油的车</strong>。市面上的 token 计费工具只告诉你「这次开了 500 公里、烧了 30 升油」。
  agentprof 是装在车上的<strong>仪表盘 + 行车记录仪</strong>：
  把每次踩油门 / 刹车 / 怠速都记下来，做成一张「这趟旅程的油耗火焰图」+「每个绕路决策的 ROI 表」，
  让你能下一次<strong>少绕远路、关掉没用的发动机配件、避开堵车路段</strong>。
</div>

<h2>它到底解决什么问题？</h2>

<p>直接看 OpenAI / Anthropic Dashboard 当然能看到 token 总数，但真实排查里你会很快遇到三类痛点，
这正是 agentprof 要替你抹平的：</p>
"#);

    s.push_str(&comparison_table(
        &["痛点", "没工具时", "agentprof 的做法"],
        &[
            ("看不见 token 去向", "总数 = 几万，不知道哪个 turn / tool / hook 拿走的", "<strong>火焰图 + Turn Summary + Tool Rank</strong> 把 token 切到 turn / tool / hook 级"),
            ("没有 ROI 信号", "MCP 装了 20 个 tool，不知道哪个真正被 agent 用过", "<strong>MCP Waste 报表</strong>：标出加载了但从没被调用的 tool + 估算浪费 token"),
            ("Prompt cache 黑盒", "Claude 缓存命中率多少？省了多少？不知道", "<strong>Cache 段</strong>：诚实 / 朴素两种 hit rate + 净节省 + 总节省"),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 想深入理解每一类痛点？点开下面的折叠卡片，每张都给你：<strong>① 示例 · ② 为什么必要 · ③ agentprof 的做法 · ④ 还有什么其他方案</strong>。</p>"#);

    s.push_str(&accordion(
        1,
        "看不见 token 去向",
        r#"<div class="qa">
<div class="q">🧪 示例</div>
<div class="a">直接看官方 dashboard：</div>
<pre class="code">2026-06-11  conversation-7f3a  <span class="nm">42,317</span> input  <span class="nm">8,124</span> output</pre>
<div class="q">🤔 为什么必要</div>
<div class="a">这只告诉你「整次对话用了 50k」，但你不知道：哪一个 turn 是 token 大户？哪个 tool call 拿走最多 context？是不是有 MCP 工具加载了 schema 但从来没用？</div>
<div class="q">✅ agentprof 的做法</div>
<div class="a">把 events.jsonl 流式解析成 <code>Episodes</code>（一个 turn 的所有 tool/hook/skill 调用集合），再用 <code>compute_analysis</code> 生成<strong>火焰图 SVG + Turn Summary 表 + Tool Rank 表</strong>。每一行 token 都有「归属」。</div>
<div class="q">🔀 其他方案</div>
<div class="a">手写脚本 grep + jq 也能从 events.jsonl 提数据，但火焰图渲染 / 跨 session 聚合 / TUI 交互都得自己写一遍。agentprof 把这些做成开箱即用。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "没有 ROI 信号",
        r#"<div class="qa">
<div class="q">🧪 示例</div>
<div class="a">你给 Copilot CLI 装了 GitHub MCP server（17 个工具）+ Filesystem MCP（8 个工具）+ 自家 Jira MCP（12 个工具）。每次对话 system prompt 多 5k tokens 描述这 37 个工具。问题：agent 到底用过哪些？</div>
<div class="q">🤔 为什么必要</div>
<div class="a">MCP 工具的 schema 是常驻 context window 的；加载了但不用 = 直接浪费钱 + 挤掉真正需要的上下文。</div>
<div class="q">✅ agentprof 的做法</div>
<div class="a"><code>agentprof mcp-waste --tool-descriptions sidecar.json</code> 给出每个 MCP server 的「加载次数 / 零调用次数 / 估算浪费 token」三栏报表，并 list 出"从没被调用过的工具"。这是 agentprof 区别于其他 token 工具的<strong>核心卖点</strong>。</div>
<div class="q">🔀 其他方案</div>
<div class="a">官方 Anthropic console 不显示工具维度；某些自建可观测平台（如 Helicone）可以打 tag，但需要先在 prompt 中手工注入 metadata。agentprof 直接从 session 日志反推，零侵入。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "Prompt cache 黑盒",
        r#"<div class="qa">
<div class="q">🧪 示例</div>
<div class="a">Claude Sonnet 启用 prompt caching 后理论上能省到 10% 价格。但你的 cache hit rate 到底是 30% 还是 90%？省了多少钱？</div>
<div class="q">🤔 为什么必要</div>
<div class="a">cache 是按 5 分钟 TTL 自动失效的；如果 agent 调用之间间隔过长，cache 实际命中率会比想象的低很多。</div>
<div class="q">✅ agentprof 的做法</div>
<div class="a"><code>agentprof analyze --export md</code> 输出的 Cache 段会给出 <strong>honest hit rate</strong>（cache_read / (cache_read + cache_creation)）+ <strong>naive hit rate</strong>（cache_read / (cache_read + input_tokens)）+ <strong>net saved / gross saved tokens</strong>（按 Claude Sonnet 4.x 2026-06 价格估算）。<code>aggregate --by model</code> 给出跨 session 的 CacheCr / CacheRd / Hit% / NetSaved 列。</div>
<div class="q">🔀 其他方案</div>
<div class="a">官方 console 显示 cache_read / cache_creation 原始数字但不算 hit rate 也不算节省金额；agentprof 一次性给出 6 个数字 + 价格对照表。</div>
</div>"#,
    ));

    s.push_str("<h2>下一步</h2>\n<p>读完本课你已经知道 agentprof 解决什么问题。下一课用 <strong>5 分钟</strong>装好工具，跑出你的第一张火焰图。</p>\n");

    s.push_str(&source_ref(
        "agentprof-core",
        "analyzer/mod.rs",
        "compute_analysis",
    ));
    s.push_str(&source_ref("agentprof-cli", "cmd/analyze.rs", "run"));

    s
}
