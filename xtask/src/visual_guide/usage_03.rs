//! Usage lesson 3 — 「analyze：看懂一次 session」.
//!
//! Target audience: 已经装好 agentprof 跑出了第一张报表的用户。覆盖
//! 5 种 export 格式（md / tui / html / json / speedscope）如何挑，
//! Turn Summary / Tool Rank / Cache 段怎么读，以及 `--section` 控制
//! 输出范围。

use super::components::{accordion, comparison_table, flow_diagram, source_ref};

/// Render the HTML body for usage lesson 3.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::usage_03::render();
/// assert!(html.contains("--export"));
/// ```
#[must_use]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
<code>agentprof analyze</code> 是核心子命令 —— 把<strong>同一份 session 数据</strong>渲染成 <strong>5 种 export</strong>（<code>md</code> / <code>tui</code> / <code>html</code> / <code>json</code> / <code>speedscope</code>），按你下一步想做什么挑：CI 流水线看 <code>md</code>，浏览器分享 <code>html</code>，本地开发调优 <code>tui</code>。底层都是一份 <code>Analysis</code> 结构体，渲染层只是换了个壳。
</p>
"#);

    s.push_str(&flow_diagram(&[
        "events.jsonl",
        "Adapter parse",
        "compute_analysis",
        "5 种 export",
    ]));

    s.push_str(r#"
<div class="card analogy">
  <div class="tag">🔌 生活类比</div>
  像 <strong><code>perf top</code> 看 CPU 热点</strong> —— <code>agentprof analyze</code> 看 <strong>token 热点</strong>。
  同一份 <code>perf.data</code> 可以 <code>perf report</code> 看 table、<code>perf script | flamegraph.pl</code> 出火焰图、<code>perf data convert</code> 喂给别的工具；
  <code>agentprof analyze</code> 的 <code>--export</code> 也是同样的思路：<strong>采集只做一次，渲染按需切换</strong>，看你下一步要喂给谁（人 / CI / 浏览器 / 第三方 profiler）。
</div>

<h2>5 种导出格式 — 怎么挑？</h2>

<p>下面 3 种是最常用的 <strong>主路径</strong>；<code>json</code> 与 <code>speedscope</code> 用于工具链集成，在下面卡片里展开：</p>
"#);

    s.push_str(&comparison_table(
        &["导出格式", "适用场景", "命令"],
        &[
            (
                "<code>--export md</code>（默认）",
                "CI 日志 / 控制台 grep 友好 / PR diff",
                "<pre class=\"code\">agentprof analyze --agent copilot</pre>",
            ),
            (
                "<code>--export html</code>",
                "浏览器分享 / 单文件自包含 / 邮件 IM 直发",
                "<pre class=\"code\">agentprof analyze --export html \\\n  --output report.html</pre>",
            ),
            (
                "<code>--export tui</code>",
                "终端交互 / 火焰图 + ROI 表 + Models view 切视图",
                "<pre class=\"code\">agentprof analyze --export tui</pre>",
            ),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三种主路径展开 + <code>json</code> / <code>speedscope</code> 在 html / tui 卡片末尾提及：<strong>① 输出长什么样 · ② 为什么这么设计 · ③ agentprof 怎么做</strong>。</p>"#);

    s.push_str(&accordion(
        1,
        "md 导出 — CI 与 grep 友好",
        r#"<div class="qa">
<div class="q">🧪 输出长什么样</div>
<div class="a">标准 Markdown，按段顺序输出：<strong>Session header</strong>（id / agent / 时间窗口 / 总 token）→ <strong>Turn Summary 表</strong>（每轮 user/assistant/tool token + cache hit）→ <strong>Tool Rank 表</strong>（按调用次数 + 平均 token 排序）→ <strong>Hook Rank 表</strong>（hook 触发次数）→ <strong>Cache 段</strong>（<em>仅当本 session 有 cache 活动时才出现</em>，命中率 + 节省 token 数）→ <strong>Warnings tail</strong>（"loaded but never called" 的 tool 列表）。可以用 <code>--section turn-summary,tool-rank</code> 只输出指定段。</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">纯文本最易 <strong>diff</strong>：CI 把上一次 main 的 md 报表存成 artifact，PR 跑完直接 <code>diff old.md new.md</code> 就能看到 token 涨跌；不需要专用 viewer。表格列固定 + 排序固定也方便 <code>grep "ToolName"</code> 在大量 session 里定位某次调用。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">用 <strong>hand-written markdown renderer</strong>（<em>不</em>引 <code>pulldown-cmark</code> 等通用库）—— 输出的是已知形状的固定段落，专用代码反而比通用 AST 短而稳。Cache 段 <strong>条件出现</strong>是设计取舍：没有 cache 活动时塞一段「Cache hit: 0%」会污染 grep；干脆不写。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "html 导出 — 浏览器分享 + 自包含",
        r#"<div class="qa">
<div class="q">🧪 输出长什么样</div>
<div class="a">单个 HTML 文件，含 <strong>内联 CSS</strong> + <strong>内嵌 SVG 火焰图</strong>，零外部依赖（无 CDN、无字体请求）。直接邮件 / IM 发给同事，对方双击就能看；离线打开也没问题。
<figure style="margin:.75rem 0">
  <img class="shot" src="../assets/report-html-sample.svg" alt="HTML 报告示例" style="max-width:100%;border:1px solid var(--border);border-radius:6px">
  <figcaption style="color:var(--muted);font-size:.85rem;margin-top:.3rem">HTML 报表示例 — 火焰图 + Tool Rank 表（真实截图在 T19 落）</figcaption>
</figure></div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">分享场景里「<strong>对方不装 agentprof</strong>」是常态。如果用 markdown 还得让对方装 viewer + 火焰图渲染不了；如果用 PNG 截图就丢了可点击 / 可复制。单 HTML 文件兼顾「<strong>视觉</strong>」与「<strong>可交互</strong>」（hover tooltip / 折叠表）+「<strong>可归档</strong>」。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">用 <strong>askama 模板</strong>编译期渲染 HTML 骨架；火焰图走 <code>agentprof-core::flamegraph</code> 生成 SVG 字符串后内嵌；CSS 直接 <code>include_str!</code> 进 binary。整张报表生成后 <strong>一次写盘</strong>（<code>--output report.html</code>）。想要纯数据喂给别的工具链（grafana / lakera / 自家 dashboard）改 <code>--export json</code> 输出结构化 <code>Analysis</code>；想喂给 <a href="https://www.speedscope.app/">speedscope.app</a> 看交互火焰图改 <code>--export speedscope</code> —— 输出的 SVG 长这样：
<figure style="margin:.5rem 0">
  <img class="diagram" src="../assets/flamegraph-sample.svg" alt="flamegraph SVG 示例" style="max-width:100%">
  <figcaption style="color:var(--muted);font-size:.85rem;margin-top:.3rem">speedscope 火焰图示例 — 真实样本在 T19 落</figcaption>
</figure></div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "tui 导出 — 终端 5 视图（F1..F5）",
        r#"<div class="qa">
<div class="q">🧪 输出长什么样</div>
<div class="a">在当前终端拉起 <strong>ratatui 全屏 UI</strong>，<code>F1..F5</code> 切 5 个视图：<strong>F1 Sessions</strong>（session 列表 + 时间窗口过滤）→ <strong>F2 TurnDetail</strong>（单轮拆解 + token 流向）→ <strong>F3 ToolRank</strong>（按 ROI 排序的 tool 表）→ <strong>F4 HookRank</strong>（hook 触发表）→ <strong>F5 Models</strong>（按模型聚合 token / 成本，M1.6.x + F1.7）。<code>q</code> / <code>Esc</code> 退出。</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">开发调优场景里你需要的是<strong>同一窗口快速切视角</strong>：先看 ToolRank 找浪费最多的 tool → 切 TurnDetail 看是哪一轮调的 → 切 Models 看不同模型的占比对比。每切一次 view 就开新 markdown 报表不现实，TUI 是这个场景的天然形态。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">5 视图实现在 <code>agentprof-tui</code> crate，每个 view 是独立 <code>Widget</code>。打 <strong>panic-safe lifecycle</strong>（ADR-0006）—— <code>main()</code> 装 <code>std::panic::set_hook</code> 先还原 raw mode 再 abort，避免 unwrap panic 让你的 shell 卡死成无回显黑屏。<strong>非-TTY 启动 TUI 退出码 3</strong>（I/O 错误），用户重定向到管道时立即报错，不会卡在初始化阶段。</div>
</div>"#,
    ));

    s.push_str("<h2>下一步</h2>\n<p>会读单次 session 之后，下一课会带你看 <strong>aggregate</strong>：把 7 天 / 30 天的 session 聚合在一起，按 mcp-server / tool / 模型分组看趋势 —— 单次 session 看局部，aggregate 看全局。</p>\n");

    s.push_str(&source_ref("agentprof-cli", "cmd/analyze.rs", "AnalyzeCmd"));
    s.push_str(&source_ref("agentprof-cli", "cmd/format/html.rs", "render"));

    s
}
