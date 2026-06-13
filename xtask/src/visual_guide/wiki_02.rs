//! Wiki lesson 2 — 「数据模型」.
//!
//! Target audience: contributors who need to read or add code in
//! `agentprof-core`. Names every type in the three-layer pipeline
//! (Event → Episode → `AnalysisReport`) using the real struct/field
//! names from the live crate; cross-checked against
//! `crates/agentprof-core/src/{adapter,episode,analyzer}` at T15.

use super::components::{accordion, comparison_table, flow_diagram, schema_table, source_ref};

/// Render the HTML body for wiki lesson 2.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_02::render();
/// assert!(html.contains("AnalysisReport"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
agentprof 的数据生命周期是 <strong>3 层</strong>：原始 <code>Event</code> 流 → 聚合成 <code>Episodes</code>（一个 turn 内 tool / hook / skill 调用集合）→ 计算 <code>AnalysisReport</code>（整 session rollup 统计）。<strong>每一层独立可序列化、可单元测试</strong>，分别解决「忠实记录」「聚合视图」「OLAP 报表」三类问题。本课把每个类型的真实字段、入口函数、容错策略一次讲清。
</p>
"#);

    s.push_str(&schema_table(&[
        ("meta", "SessionMeta", "✓", "id / agent / started_at"),
        (
            "turn_summary",
            "Vec&lt;TurnSummaryRow&gt;",
            "✓",
            "per-turn token + duration",
        ),
        (
            "tool_rank",
            "Vec&lt;ToolRankRow&gt;",
            "✓",
            "top tools by total_duration + p50/p95",
        ),
        ("hook_rank", "Vec&lt;HookRankRow&gt;", "✓", "hook 调用排序"),
        (
            "model_metrics",
            "BTreeMap&lt;String, ModelUsage&gt;",
            "✓",
            "per-model token counts",
        ),
        (
            "warnings",
            "Vec&lt;DeriveWarning&gt;",
            "—",
            "analyzer-time 警告",
        ),
        (
            "parse_warnings",
            "Vec&lt;ParseWarning&gt;",
            "—",
            "parser-time 警告",
        ),
        (
            "loaded_mcp_tools",
            "BTreeSet&lt;String&gt;",
            "—",
            "本 session 加载过的 MCP tool",
        ),
    ]));

    s.push_str(r#"
<div class="card analogy">
  <div class="tag">🛢️ 工程类比 — 像数据库的 normalize → denormalize</div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong><code>Event</code> = 行级日志</strong> — 单事件 append-only 流，类似数据库的 transaction log，原子可重放。</li>
    <li><strong><code>Episodes</code> = 分组聚合视图</strong> — 把同一 tool / hook / skill 的多次调用 group by name，类似 SQL 的 <code>GROUP BY</code> 中间结果。</li>
    <li><strong><code>AnalysisReport</code> = OLAP cube</strong> — 跨 turn / tool / hook / model 的多维 rollup，给 CLI / TUI / HTML / JSON 多 surface 共用一份事实表。</li>
  </ul>
  <p style="margin:.6em 0 0;font-size:.92rem;color:var(--muted)">三层各自独立解决一类问题，下层永远是上层的输入，不反过来依赖。</p>
</div>

<h2>三层的职责与实际类型</h2>
"#);

    s.push_str(&comparison_table(
        &["层次", "责任", "实际类型（crate 内真实名）"],
        &[
            (
                "<strong>Event</strong>（原始事件）",
                "单个原始事件：<code>TurnStart</code> / <code>ToolExecStart</code> / <code>ToolExecComplete</code> / <code>HookStart</code> / <code>HookEnd</code> / <code>SkillInvoked</code> …",
                "<code>trait Event</code> + <code>enum EventKind</code>（<code>#[non_exhaustive]</code>, 30 个变体），每 adapter 一个具体 enum（如 <code>CopilotEvent</code>）",
            ),
            (
                "<strong>Episodes</strong>（turn 聚合）",
                "一个 session 内所有 tool / hook / skill / mode / abort 的分组视图，按 name 聚合调用",
                "<code>struct Episodes { turns, tools: BTreeMap&lt;String, ToolEpisode&gt;, hooks, skills, mode_segments, aborts, warnings, model_metrics, loaded_mcp_tools }</code>",
            ),
            (
                "<strong>AnalysisReport</strong>（session rollup）",
                "整 session 的多维 rollup 表 — meta / per-turn 行 / tool rank / hook rank / per-model token / cache",
                "<code>struct AnalysisReport { meta, turn_summary, tool_rank, hook_rank, warnings, parse_warnings, model_metrics, loaded_mcp_tools } + fn cache_metrics()</code>",
            ),
        ],
    ));

    s.push_str(r"<h2>数据流图</h2>
<p>四个节点的单向 pipeline — 每个箭头都是<strong>纯函数</strong>（无副作用、可单元测试、可在 storage 层缓存中间结果）：</p>
");

    s.push_str(&flow_diagram(&[
        "events.jsonl",
        "Event 流",
        "Episodes (一 turn 聚合)",
        "AnalysisReport (rollup)",
    ]));

    s.push_str(r#"<p style="font-size:.88rem;color:var(--muted)">① <code>parse_events_jsonl()</code> 把磁盘的 <code>events.jsonl</code> 读成 <code>Vec&lt;CopilotEvent&gt;</code>（实现 <code>Event</code> trait）+ 一组 <code>ParseWarning</code>；② <code>derive_episodes(&amp;events, &amp;meta)</code> 单遍扫描产出 <code>Episodes</code>；③ <code>analyze(&amp;episodes, &amp;meta, &amp;parse_warnings)</code> 计算 <code>AnalysisReport</code>。三段全在 <code>agentprof-core</code>，<strong>不依赖任何其他 workspace crate</strong>。</p>"#);

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片分别拆解：① <code>Event</code> 层（trait + EventKind）· ② <code>Episodes</code> 层（<code>derive_episodes</code> + Span 容错）· ③ <code>AnalysisReport</code> 层（完整字段表）。每张都给「为什么 / agentprof 怎么做 / 其他选择」。</p>"#);

    s.push_str(&accordion(
        1,
        "Event 层 — 单事件流",
        r#"<div class="qa">
<div class="q">📐 类型定义</div>
<div class="a">
<ul style="margin:.3em 0 0 1.2em">
<li><strong><code>trait Event</code></strong>（<code>crates/agentprof-core/src/adapter.rs:174</code>）— 4 个必备方法：<code>id() -&gt; &amp;str</code> / <code>kind() -&gt; EventKind</code> / <code>timestamp() -&gt; DateTime&lt;Utc&gt;</code> / <code>parent_id() -&gt; Option&lt;&amp;str&gt;</code>；可选 <code>payload_name()</code> / <code>payload_success()</code> / <code>payload_error_message()</code>（默认 <code>None</code>）。</li>
<li><strong><code>enum EventKind</code></strong>（<code>adapter.rs:108</code>，<code>#[non_exhaustive]</code>）— 30 个变体覆盖 session lifecycle / message / tool / hook / skill / mode / permission / subagent / abort / unknown，外加 <code>Unknown</code> forward-compat 兜底。</li>
<li>每个 adapter 提供一个具体 enum（如 <code>CopilotEvent</code>）实现 <code>Event</code> trait，承载该 agent 特有的 payload。</li>
</ul>
</div>
<div class="q">🤔 为什么</div>
<div class="a">Copilot CLI 的 <code>events.jsonl</code> 是行流；Claude / Codex 后续接入的 OTel 也是 push 事件流 — 三家底层模型一致。把它抽象成 <strong>trait + 类型化 kind</strong>，让上层 analyzer 完全不关心是哪家 agent 在喂数据。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">trait + extension methods（<code>payload_name</code> / <code>payload_success</code> / <code>payload_error_message</code>）让所有 adapter 暴露统一接口；<code>EventKind</code> 的 <code>#[non_exhaustive]</code> 允许将来新增变体不破坏 SemVer；<code>Unknown</code> 变体保证未识别事件类型也能保留 timestamp / parent，不丢数据。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>直接用 <code>serde_json::Value</code></strong> — 实现最简单，但<strong>失去类型安全</strong>（写错字段名编译过不报错）+ 失去编译期 exhaustive match 校验，重构时容易漏分支；<strong>每 adapter 单独 trait</strong> — 同名 tool 还得各自实现 rollup 不能复用 analyzer；trait + EventKind 是「类型化 + 跨 adapter 复用」的最优解。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "Episode 层 — turn 内聚合",
        r#"<div class="qa">
<div class="q">📐 类型 & 入口</div>
<div class="a">
<ul style="margin:.3em 0 0 1.2em">
<li><strong>入口</strong>：<code>pub fn derive_episodes&lt;E: Event&gt;(events: &amp;[E], meta: &amp;SessionMeta) -&gt; Episodes</code>（<code>episode/derive.rs:101</code>），<strong>单遍扫描</strong>产出全部聚合结果。</li>
<li><code>struct Episodes</code>（<code>episode/episodes.rs:31</code>）字段：<code>turns: Vec&lt;Turn&gt;</code> / <code>tools: BTreeMap&lt;String, ToolEpisode&gt;</code>（按 tool name 聚合）/ <code>hooks: BTreeMap&lt;String, HookEpisode&gt;</code> / <code>skills: BTreeMap&lt;String, SkillEpisode&gt;</code> / <code>mode_segments</code> / <code>aborts</code> / <code>warnings: Vec&lt;DeriveWarning&gt;</code> / <code>model_metrics</code> / <code>loaded_mcp_tools</code>。</li>
<li><code>struct ToolEpisode { name, source: ToolSource, calls: Vec&lt;ToolCall&gt;, total_duration, fail_count }</code>；每个 <code>ToolCall { span: Span, turn_id, status: ToolCallStatus, user_requested, arguments }</code>；<code>struct Span { started_at, ended_at }</code>（<code>episode/turn.rs:138</code>）。</li>
</ul>
</div>
<div class="q">🤔 为什么</div>
<div class="a">rollup 要回答的核心问题是「<strong>一个 turn 内 tool <code>Read</code> 被调用了几次、总耗时多少、失败几次</strong>」— 原始 <code>events.jsonl</code> 没有这个聚合视图，每次查询都重算太慢。Episode 层把它物化（且按 name BTreeMap 排序好），让上面的 rank / report 直接 iterate。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a"><strong>lenient 单遍 derive</strong>（ADR-0004 决策）— 遇到异常事件不 crash，而是发 <code>DeriveWarning</code> 入 <code>Episodes::warnings</code>。比如 <code>ToolExecComplete</code> 时间戳早于 <code>ToolExecStart</code>，会产出 <code>DeriveWarning::NonMonotonicTimestamp</code>（<code>episode/warning.rs:39</code>）且把 duration 截到 <code>0</code>，让报表照常生成。其他 lenient 变体还有 <code>SynthesizedStart</code> / <code>OpenAtEndOfSession</code> / <code>AbortWithoutOpenElement</code> / <code>PayloadNameMissing</code>。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>两遍扫描</strong>（先建索引再聚合）— 实现略简单但内存 / CPU 翻倍，对大 session 拖慢交互；<strong>严格 fail-fast</strong>（缺事件就 panic）— 太脆弱：Copilot CLI 偶尔会因为 SIGINT 漏写 <code>turn.end</code>，整 session 全废；单遍 lenient + warning 是「保 99 % 准确 + 永远出报表」的平衡。详见 <a href="https://github.com/verdenmax/agentprof/blob/main/docs/internals/adr-0004-episode-derivation.md"><code>ADR-0004 episode derivation</code></a>。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "AnalysisReport 层 — session rollup",
        r#"<div class="qa">
<div class="q">📐 完整字段表</div>
<div class="a">
<table>
<thead><tr><th>字段</th><th>类型</th><th>含义</th></tr></thead>
<tbody>
<tr><td><code>meta</code></td><td><code>SessionMeta</code></td><td>session id / <code>AgentKind</code> / started_at / 是否 resume；克隆自输入</td></tr>
<tr><td><code>turn_summary</code></td><td><code>Vec&lt;TurnSummaryRow&gt;</code></td><td>每 turn 一行：status / duration / model / mode / tool / hook / skill 计数 / output_tokens</td></tr>
<tr><td><code>tool_rank</code></td><td><code>Vec&lt;ToolRankRow&gt;</code></td><td>按 <code>total_duration</code> 降序：calls (success/failure/orphan/user-requested) / p50 / p95 / max</td></tr>
<tr><td><code>hook_rank</code></td><td><code>Vec&lt;HookRankRow&gt;</code></td><td>同上，针对 hook</td></tr>
<tr><td><code>warnings</code></td><td><code>Vec&lt;DeriveWarning&gt;</code></td><td>analyzer-time 数据异常（如 <code>NonMonotonicTimestamp</code>）</td></tr>
<tr><td><code>parse_warnings</code></td><td><code>Vec&lt;ParseWarning&gt;</code></td><td>parser-time 格式错误（如 schema 不匹配的行）</td></tr>
<tr><td><code>model_metrics</code></td><td><code>Option&lt;BTreeMap&lt;String, ModelUsage&gt;&gt;</code></td><td>per-model token：input / output / cache_read / cache_write；<code>None</code> 表 session 无 shutdown 事件</td></tr>
<tr><td><code>loaded_mcp_tools</code></td><td><code>BTreeSet&lt;String&gt;</code></td><td>session 加载的 MCP tool 集合（不论是否被调用）— 用于算 waste / ROI</td></tr>
<tr><td><code>cache_metrics()</code></td><td><code>fn(&amp;self) -&gt; Option&lt;CacheMetrics&gt;</code></td><td>方法而非字段：根据 <code>model_metrics</code> 推算，<strong><code>None</code> 表没 cache 活动</strong>，避免误把零当真实"零命中"</td></tr>
</tbody>
</table>
</div>
<div class="q">🤔 为什么</div>
<div class="a">CLI 的 <code>--export md/json/html/speedscope</code>、TUI 的 5 视图、HTML 看板、未来的 storage 缓存 — <strong>所有 surface 共用一份 rollup</strong>，避免每个表现层各自 reaggregate（重复代码 + 容易漂移）。<code>#[non_exhaustive]</code> 允许后续无破坏性加字段。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a"><strong>一个入口</strong>：<code>pub fn analyze(episodes: &amp;Episodes, meta: &amp;SessionMeta, parse_warnings: &amp;[ParseWarning]) -&gt; AnalysisReport</code>（<code>analyzer/mod.rs:415</code>），内部分别调 <code>turn_summary()</code> / <code>tool_rank()</code> / <code>hook_rank()</code>。所有 <code>Vec</code> 都是确定性顺序（snapshot-stable）；所有 <code>Duration</code> 序列化为整数 ms（<code>duration_ms</code> helper），便于 JSON diff 测试。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>每 surface 自己 reaggregate</strong>（不要中间 rollup 类型）— 第一版最快但很快漂移：md 输出和 json 输出对不上同一个数；<strong>把 rollup 推到 storage 层</strong> — 把 lib 逻辑塞进 storage 违反 §3 crate 边界，<code>core</code> 不再能独立 analyze。<code>AnalysisReport</code> 作为 <code>agentprof-core</code> 的公开类型 + <code>analyze()</code> 唯一入口是当前的最优拆分。</div>
</div>"#,
    ));

    s.push_str(r"<h2>三层为什么必须独立可序列化</h2>
<p>每一层都 <code>derive(Serialize, Deserialize)</code>，目的是：</p>
<ul>
<li><strong>storage 缓存</strong> — <code>agentprof-storage</code>（M2.2）把 <code>AnalysisReport</code> 直接存 SQLite blob，下次 <code>list</code> / <code>aggregate</code> 不必重算；<code>#[serde(default)]</code> + <code>skip_serializing_if</code> 兜住向后兼容。</li>
<li><strong>snapshot 测试</strong> — <code>insta</code> 对每层产出 YAML/JSON 快照，重构时一眼看见行为变化。</li>
<li><strong>跨进程 IPC</strong> — 未来 <code>serve</code>（M2.4 OTLP receiver）可以把 receiver 拿到的 <code>Event</code> 流直接 IPC 给 analyzer 进程；<code>AnalysisReport</code> 也可以推给浏览器看板。</li>
</ul>

<h2>下一步</h2>
<p>本课命名了 <code>Event</code> / <code>Episodes</code> / <code>AnalysisReport</code> 三层的所有真实类型。下一课「<strong>Tokenizer 与 token 计数</strong>」拆解 <code>agentprof-core::tokenizer</code>：tiktoken-rs 怎么挑模型、为何 cache token 单独计数、为何 ROI 用 <code>total_tokens</code> 而不是 <code>input_tokens</code>。</p>
");

    s.push_str(&source_ref("agentprof-core", "adapter.rs", "Event"));
    s.push_str(&source_ref(
        "agentprof-core",
        "episode/derive.rs",
        "derive_episodes",
    ));
    s.push_str(&source_ref("agentprof-core", "analyzer/mod.rs", "analyze"));

    s
}
