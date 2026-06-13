//! Wiki lesson 4 — 「分析层 rollups」.
//!
//! Walkthrough of `agentprof_core::analyzer`: the `analyze()` entry
//! pipeline (`turn_summary` → `tool_rank` → `hook_rank` → clone of
//! `model_metrics` + `loaded_mcp_tools`), the `CacheMetrics` dual
//! hit-rate formulas (ADR-0023), and the `compute_waste` /
//! `aggregate_waste` MCP-waste accounting with its 4 builder layers
//! (heuristic / tokenizer / config / sidecar). All `fn` names, field
//! names, and numeric constants are cross-checked against live code
//! at T17 (HEAD `34c4e87`).
//!
//! Recon-confirmed corrections vs. the original brief:
//!
//!   - `analyze()` does NOT compute `cache_metrics` — that's a separate
//!     on-demand `AnalysisReport::cache_metrics()` derivation from
//!     `model_metrics`. The pipeline is 3 rollups + 2 clones.
//!   - `TurnSummaryRow` exposes `output_tokens` only (no `input_tokens`
//!     field; per-turn input is not separately tracked).
//!   - `ToolRankRow::source` is `ToolSource` with variants
//!     `Builtin` / `Mcp` / `Skill` / `User` / `Unknown` — NOT including
//!     "Hook" (hooks are a parallel `hook_rank` rollup).
//!   - `WasteComputeContext` has 3 additive builder methods
//!     (`with_tokenizer` / `with_config` / `with_sidecar`); heuristic
//!     is the default baseline, not a 4th explicit mode.

use super::components::{accordion, comparison_table, flow_diagram, source_ref};

/// Render the HTML body for wiki lesson 4.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_04::render();
/// assert!(html.contains("analyze"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"
<h1>分析层 rollups</h1>

<p class="lead">
agentprof 的 <strong>「花得值不值」</strong>信号全来自 <code>agentprof_core::analyzer</code> 模块：它吃 <code>Episodes + SessionMeta + &amp;[ParseWarning]</code> 三组输入，吐 <code>AnalysisReport</code>。每个 rollup（<code>turn_summary</code> / <code>tool_rank</code> / <code>hook_rank</code>）都是<strong>独立可测的纯函数</strong>，cache 段和 MCP waste 则是 <code>AnalysisReport</code> 上的<strong>派生方法</strong>（按需算，不入 pipeline），保证 analyzer 本身没有 I/O、没有时间副作用、可以 snapshot test。
</p>

<div class="card analogy">
  <div class="tag">🏭 工程类比 — 像 ETL pipeline 的 transform 层</div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong>Event = raw input</strong>（adapter 从 jsonl 解析出的原始记录）。</li>
    <li><strong>Episode = staged data</strong>（一层规整化：tool call 配对、turn 划分、hook 聚合）。</li>
    <li><strong>AnalysisReport = business view</strong>（per-turn / per-tool / per-hook 的 rollup，渲染器直接消费）。</li>
  </ul>
  <p style="margin:.6em 0 0;font-size:.92rem;color:var(--muted)"><code>analyzer</code> 模块就是那一层 SQL <code>SELECT ... GROUP BY ...</code> — 输入是 <code>Episodes</code> 这张「staged 表」，输出是若干张 rollup 表（<code>turn_summary</code> / <code>tool_rank</code> / <code>hook_rank</code>）。<code>CacheMetrics</code> 和 <code>WasteReport</code> 是 view-of-view（在 report 上再聚合），不进 transform 层。</p>
</div>

<h2>3 类 rollup 对比（recon 真实公式）</h2>
"#);

    s.push_str(&comparison_table(
        &["Rollup", "关键公式 / 算法", "输出字段（节选）"],
        &[
            (
                "<code>turn_summary(episodes)</code>",
                "按 turn 分组：iterate <code>episodes.turns</code>，每 turn 收集 tool/hook/skill 调用计数 + 累计 <code>duration</code> + assistant 最后一条 message 的 <code>output_tokens</code>",
                "<code>Vec&lt;TurnSummaryRow&gt;</code>：<code>turn_id</code>, <code>started_at</code>, <code>duration: Option&lt;Duration&gt;</code>, <code>status: TurnStatus</code>, <code>model</code>, <code>mode</code>, <code>output_tokens: Option&lt;u32&gt;</code>, <code>tool_call_count</code>, <code>hook_call_count</code>, <code>skill_call_count</code>",
            ),
            (
                "<code>tool_rank(episodes)</code>",
                "按 tool name 分组：iterate <code>episodes.tools</code>，统计 success/failure/orphan 计数 + 累计 <code>total_duration</code> + 收集 sorted durations 算 <code>percentile_nearest_rank(50.0)</code> / <code>(95.0)</code>；最后按 <code>total_duration</code> 降序",
                "<code>Vec&lt;ToolRankRow&gt;</code>：<code>name</code>, <code>source: ToolSource</code>（<code>Builtin/Mcp/Skill/User/Unknown</code>）, <code>call_count</code>, <code>success_count</code>, <code>failure_count</code>, <code>orphan_count</code>, <code>total_duration</code>, <code>p50_duration</code>, <code>p95_duration</code>, <code>max_duration</code>, <code>is_user_blocking</code>",
            ),
            (
                "<code>CacheMetrics::from_raw(creation, read, input)</code><br><span style=\"font-size:.85rem;color:var(--muted)\">（M2.5 派生，非 pipeline）</span>",
                "<strong>honest</strong> = <code>100 × read / (read + creation)</code><br><strong>naive</strong> = <code>100 × read / (read + input)</code><br><strong>saved_net</strong> = <code>round(read × 0.9 − creation × 0.25)</code>",
                "<code>Option&lt;CacheMetrics&gt;</code>（<code>None</code> 当 <code>creation == 0 &amp;&amp; read == 0</code>）：<code>creation</code>, <code>read</code>, <code>input</code>, <code>hit_rate_naive_pct</code>, <code>hit_rate_honest_pct</code>, <code>saved_gross: u64</code>, <code>saved_net: i64</code>（可负）",
            ),
        ],
    ));

    s.push_str(r#"<p style="font-size:.88rem;color:var(--muted)">⚠️ Recon 校正：<code>analyze()</code> 本体<strong>只算</strong> <code>turn_summary</code>/<code>tool_rank</code>/<code>hook_rank</code> 这 3 个 rollup，<code>model_metrics</code> 和 <code>loaded_mcp_tools</code> 是从 <code>Episodes</code> 直接 <code>clone()</code> 进 report；<code>cache_metrics</code> 和 <code>WasteReport</code> 是 report 上的<strong>派生方法</strong>，渲染时按需算。</p>"#);

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片：① <code>analyze()</code> 流水线真实顺序（recon 后修正）· ② Cache 段双 hit rate（ADR-0023 的「honest vs naive」）· ③ MCP waste 的 <code>compute_waste</code> + <code>aggregate_waste</code> 4 层精度。</p>"#);

    // ---------------- Accordion 1: analyze() pipeline ----------------
    let mut card1 = String::new();
    card1.push_str(r#"<div class="qa">
<div class="q">📐 真实签名（<code>crates/agentprof-core/src/analyzer/mod.rs:415</code>）</div>
<div class="a">
<pre class="code"><code>pub fn analyze(
    episodes: &amp;Episodes,
    meta: &amp;SessionMeta,
    parse_warnings: &amp;[ParseWarning],
) -&gt; AnalysisReport {
    let report = AnalysisReport {
        meta: meta.clone(),
        turn_summary: turn_summary(episodes),
        tool_rank:    tool_rank(episodes),
        hook_rank:    hook_rank(episodes),
        warnings:        episodes.warnings.clone(),
        parse_warnings:  parse_warnings.to_vec(),
        model_metrics:   episodes.model_metrics.clone(),
        loaded_mcp_tools: episodes.loaded_mcp_tools.clone(),
    };
    // tracing::debug! 记录 tool/hook count 后 return
    report
}</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem;color:var(--muted)">字段构造顺序就是流水线顺序：3 个独立 rollup 函数 + 4 个字段透传（meta / warnings / parse_warnings / model_metrics / loaded_mcp_tools）。<strong>没有 cache_metrics 或 waste 字段</strong> — 这俩是 <code>AnalysisReport</code> 上的方法。</p>
</div>

<div class="q">🌊 SVG 流程图</div>
<div class="a">
"#);
    card1.push_str(&flow_diagram(&[
        "Episodes + meta + parse_warnings",
        "turn_summary()",
        "tool_rank()",
        "hook_rank()",
        "AnalysisReport（+ clone model_metrics / loaded_mcp_tools）",
    ]));
    card1.push_str(r#"<p style="margin:.4em 0 0;font-size:.88rem;color:var(--muted)">3 个 rollup 之间互不依赖（pure functions of <code>Episodes</code>），理论上可并行；当前实现单线程顺序执行 —— 因为单 session 通常 &lt; 10 MB，并行开销大于收益。</p>
</div>

<div class="q">🤔 为什么单一 entry point</div>
<div class="a">把 <code>turn_summary</code> / <code>tool_rank</code> / <code>hook_rank</code> 这 3 个 <code>pub fn</code> 都 export 出去当然可以（且<strong>确实</strong> export 了 — 便于单元测试），但<strong>大多数 caller</strong>（cli / serve / storage）需要的是一份完整 <code>AnalysisReport</code> 而非单个 rollup。提供 <code>analyze()</code> 这个 single entry 让调用方少写 5 行 boilerplate，也方便后续加 <code>tracing::instrument</code> 统一观测（<code>name = "analyzer.analyze"</code>）。</div>

<div class="q">✅ agentprof 怎么做</div>
<div class="a"><code>analyze()</code> 是<strong>纯函数 + 无 I/O</strong>：输入是 borrowed refs，输出 owned <code>AnalysisReport</code>，不读文件、不写 SQLite、不发 OTLP — 全部 side effect 在 caller 那侧。这让它在 doctest / unit test / snapshot test 三个层级都 trivial，也是 ADR-0004 「lenient parsing + pure analyzer」分层决策的体现。</div>

<div class="q">🔀 其他选择</div>
<div class="a"><strong>streaming 增量算法</strong>（adapter 每读 1 个 event 立刻喂给 analyzer，rollup 在线更新）— 内存占用 O(unique_tools) 而非 O(events)，对超大 session 友好；但当前 session 通常 &lt; 10 MB（≤ 数万 events），<strong>批量算 + 内存常驻</strong>简单且足够快。streaming 版本作为 M4+ 「OTLP live mode」的潜在升级路径在 ADR-0017 留了待办标记，目前不需要。</div>
</div>"#);
    s.push_str(&accordion(
        1,
        "<code>analyze()</code> 流水线（recon 真实顺序）",
        &card1,
    ));

    // ---------------- Accordion 2: cache dual hit rate ----------------
    s.push_str(&accordion(
        2,
        "Cache 段双 hit rate（ADR-0023）",
        r#"<div class="qa">
<div class="q">📐 两个 hit rate 公式</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-core/src/analyzer/cache.rs:86
pub fn from_raw(creation: u64, read: u64, input: u64) -&gt; Option&lt;Self&gt; {
    if creation == 0 &amp;&amp; read == 0 { return None; }
    let naive_denom  = read.saturating_add(input);
    let honest_denom = read.saturating_add(creation);
    let hit_rate_naive_pct  = 100.0 * (read as f64) / (naive_denom  as f64);
    let hit_rate_honest_pct = 100.0 * (read as f64) / (honest_denom as f64);
    let saved_gross = (read as f64 * CACHE_READ_DISCOUNT).round() as u64;
    let saved_net   = (read as f64 * CACHE_READ_DISCOUNT
                     - creation as f64 * CACHE_WRITE_PREMIUM).round() as i64;
    Some(Self { creation, read, input,
                hit_rate_naive_pct, hit_rate_honest_pct,
                saved_gross, saved_net })
}</code></pre>
<ul style="margin:.5em 0 0 1.2em">
<li><strong>honest_pct = read / (read + creation)</strong> — 「我尝试 cache 的 token 里，有几成被 reuse 了」。<strong>暴露 over-caching</strong>：高 creation + 低 read = cache 策略在烧钱。</li>
<li><strong>naive_pct = read / (read + input)</strong> — 「我的 prompt 里有几成走了 cache」。直观但<strong>不惩罚</strong> over-caching — creation 多了 naive 也好看（因为分母不变）。</li>
<li>两者都报：让用户自己判断 cache 策略是否健康。</li>
</ul>
</div>

<div class="q">💰 净节省公式与常数</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-core/src/analyzer/cache.rs:19,24
pub const CACHE_READ_DISCOUNT:  f64 = 0.9;   // cache_read = 10% of input price → 节省 90%
pub const CACHE_WRITE_PREMIUM:  f64 = 0.25;  // cache_creation = 125% of input → 多花 25%

saved_gross = round(read × 0.9)                               // 毛节省（input-token equivalent）
saved_net   = round(read × 0.9 − creation × 0.25)             // 净节省，可负</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem;color:var(--muted)">常数对应 <strong>Claude Sonnet 4.x（2026-06 价格表）</strong>：cache read 价格是 input 的 10%（discount 0.9），cache write 价格是 input 的 125%（premium 0.25）。<code>saved_net</code> 类型是 <code>i64</code> 而非 <code>u64</code> — <strong>可以为负</strong>，表示 cache 策略反而花了更多钱（creation 远大于 read 时）。</p>
</div>

<div class="q">🚫 关键约束：aggregate 视图<strong>不</strong>显示 cache 列</div>
<div class="a"><code>agentprof aggregate --by tool</code> 和 <code>--by mcp-server</code> 视图<strong>故意省略 cache 列</strong>。原因（<strong>ADR-0023 D-3 条决策</strong>）：<code>cache_creation</code> / <code>cache_read</code> 是 <strong>prompt-level</strong>（API request 维度）token 计数，而 tool / MCP server 是 <strong>turn-level</strong> 维度 —— per-tool cache attribution 在<strong>语义上 undefined</strong>（一个 prompt 可能触发 5 个 tool call，cache token 怎么分？）。强行均摊会误导用户，所以这两个 aggregate 视图刻意留空 cache 段；<code>--by model</code> 和 <code>--by day</code> 视图保留 cache 列，因为这两个维度的归并是 well-defined 的。</div>

<div class="q">🤔 为什么不只报 honest_pct</div>
<div class="a">honest 是更<strong>诚实</strong>的指标但<strong>不直观</strong>：用户问「我有多少 prompt 命中了 cache？」时 honest 不能直接回答。naive 直观但容易自欺欺人。两个都报 + 文档解释差异，让用户自己判断 — 这是 ADR-0023 的核心权衡（informative over prescriptive）。</div>

<div class="q">🔀 其他选择</div>
<div class="a"><strong>只报 saved_net（dollar / token 节省）</strong>不报 hit rate — 简洁，但用户无法判断 cache 策略本身的健康度（同样 net 节省可能来自高效 cache 也可能来自天量 prompt）；<strong>报 cache_creation / cache_read 原始值不算 rate</strong> — 信息无损但要用户脑补算除法。当前「两个 rate + 一个净值 + 一个毛值」组合是「教育成本」与「决策支持」的平衡点。</div>
</div>"#,
    ));

    // ---------------- Accordion 3: MCP waste ----------------
    s.push_str(&accordion(
        3,
        "MCP waste — <code>compute_waste</code> + <code>aggregate_waste</code> 与 4 层精度",
        r#"<div class="qa">
<div class="q">📐 两层 API</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-core/src/analyzer/waste.rs:402
pub fn compute_waste(report: &amp;AnalysisReport, ctx: &amp;WasteComputeContext) -&gt; WasteReport;

// crates/agentprof-core/src/analyzer/waste.rs:648
pub fn aggregate_waste(per_session: &amp;[(SessionRef, WasteReport)]) -&gt; AggregateWasteReport;</code></pre>
<ul style="margin:.5em 0 0 1.2em">
<li><code>compute_waste(report, ctx)</code> — <strong>单 session</strong>：对 <code>report.loaded_mcp_tools</code> 里每个 tool name，看 <code>report.tool_rank</code> 里是否真的被调用过，没调用的就是 wasted context；token 估算精度由 <code>ctx</code> 决定。</li>
<li><code>aggregate_waste(per_session)</code> — <strong>跨 session</strong>：把多份 <code>WasteReport</code> 按 tool / mcp-server 维度合并，输出 <code>AggregateWasteReport</code>，给 <code>aggregate --by mcp-server</code> 渲染用。</li>
</ul>
</div>

<div class="q">🎚️ <code>WasteComputeContext</code> 的 4 层精度（recon: 3 个 builder + 1 默认）</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-core/src/analyzer/waste.rs:124
pub struct WasteComputeContext&lt;'a&gt; { /* fields ... */ }

impl&lt;'a&gt; WasteComputeContext&lt;'a&gt; {
    pub fn new(wire: &amp;'a BTreeSet&lt;String&gt;) -&gt; Self;          // 层 1：默认（启发式估算）
    pub const fn with_tokenizer(mut self, kind: TokenizerKind) -&gt; Self;   // 层 2
    pub const fn with_config(mut self, cfg: &amp;'a BTreeMap&lt;String, Vec&lt;String&gt;&gt;) -&gt; Self;  // 层 3
    pub fn with_sidecar(mut self, sidecar: &amp;'a dyn SidecarLookup) -&gt; Self;             // 层 4（最准）
}</code></pre>
<table style="margin-top:.6em;width:100%;border-collapse:collapse;font-size:.92rem">
<thead><tr style="background:var(--surface)">
<th style="text-align:left;padding:.3em .5em">层</th>
<th style="text-align:left;padding:.3em .5em">配置</th>
<th style="text-align:left;padding:.3em .5em">token 估算来源</th>
<th style="text-align:left;padding:.3em .5em">精度</th>
</tr></thead>
<tbody>
<tr><td style="padding:.3em .5em"><strong>1</strong></td><td style="padding:.3em .5em">仅 <code>new(wire)</code></td><td style="padding:.3em .5em">heuristic：每 tool ≈ 100 tokens 默认估值</td><td style="padding:.3em .5em">⭐ 粗糙</td></tr>
<tr><td style="padding:.3em .5em"><strong>2</strong></td><td style="padding:.3em .5em">+ <code>.with_tokenizer(TokenizerKind::O200kBase)</code></td><td style="padding:.3em .5em">用 tiktoken-rs 真算 schema 字符数</td><td style="padding:.3em .5em">⭐⭐ 中等</td></tr>
<tr><td style="padding:.3em .5em"><strong>3</strong></td><td style="padding:.3em .5em">+ <code>.with_config(mcp.json)</code></td><td style="padding:.3em .5em">读 MCP server 配置，按 declared tool list 算</td><td style="padding:.3em .5em">⭐⭐⭐ 较准</td></tr>
<tr><td style="padding:.3em .5em"><strong>4</strong></td><td style="padding:.3em .5em">+ <code>.with_sidecar(--tool-descriptions)</code></td><td style="padding:.3em .5em">读用户提供的真实 schema JSON sidecar，每 schema 单独 tokenize</td><td style="padding:.3em .5em">⭐⭐⭐⭐ 最准</td></tr>
</tbody>
</table>
<p style="margin:.4em 0 0;font-size:.88rem;color:var(--muted)">builder 是<strong>可叠加</strong>的：<code>WasteComputeContext::new(&amp;wire).with_tokenizer(...).with_sidecar(...)</code> 同时启用层 2 + 层 4。当多源信息可用时取最准的；缺什么 fall back 上一层。</p>
</div>

<div class="q">🔧 Tokenizer 选择：<code>infer_tokenizer</code> / <code>build_bpe</code></div>
<div class="a">
<pre class="code"><code>// crates/agentprof-core/src/analyzer/waste.rs:297
pub fn infer_tokenizer(model: Option&lt;&amp;str&gt;) -&gt; TokenizerKind;   // 按 dominant model 推断
// crates/agentprof-core/src/analyzer/waste.rs:267
pub fn build_bpe(kind: TokenizerKind) -&gt; Option&lt;CoreBPE&gt;;        // 构造 tiktoken-rs BPE</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem">cli / serve 在调 <code>compute_waste</code> 前用 <code>report.dominant_model()</code> 推断 tokenizer kind（gpt-5 → O200kBase / claude → cl100k_base 近似），自动选最合适的 BPE — 用户不需要手动指定。</p>
</div>

<div class="q">🤔 为什么 MCP waste 单独成模块</div>
<div class="a">MCP tool 的<strong>规模特性</strong>独特：用户配置了 30+ MCP servers、每个 server 暴露 5-50 个 tool，<strong>大多数 tool 在一次 session 里从未被调用</strong> — 但 schema 全部塞进 prompt 占 context window。这是 LLM agent 时代<strong>最大的浪费来源之一</strong>，但传统 token profiler（只算「用了多少」）<strong>看不到</strong>。<code>compute_waste</code> 算的是「<strong>加载了但没被调用</strong>」的 schema token —— agentprof 的 ROI 卖点核心。</div>

<div class="q">🔀 其他选择</div>
<div class="a"><strong>用 char count × 4 估算 token</strong>（业界 rule of thumb）— 实现一行，精度 ±30%，对 ROI 排序够用；但 agentprof 已经 depend on tiktoken-rs（cache token 也要算），<strong>精算</strong>边际成本接近零。<strong>不算 waste 只显示 loaded 列表</strong> — 用户得自己脑补，决策支持弱。当前「heuristic 默认 + 4 层精度」是「易用性」与「准确度」的平衡 — 用户随时可以加 <code>--tool-descriptions sidecar.json</code> 升精度，零配置也能用。</div>
</div>"#,
    ));

    s.push_str(r"<h2>下一步</h2>
<p>本课讲清了 <code>analyze()</code> pipeline 的真实顺序、<code>CacheMetrics</code> 的双 hit-rate 决策（ADR-0023），以及 MCP waste 的 4 层精度模式。下一课「<strong>Tokenizer 与 token 计数</strong>」深入 <code>agentprof-core::tokenizer</code> 与 <code>tiktoken-rs</code> 集成的细节 — 为什么 cache 段需要 3 列 token、Claude 用什么 encoding、跨 model 比较为什么要慎重。</p>
");

    s.push_str(&source_ref("agentprof-core", "analyzer/mod.rs", "analyze"));
    s.push_str(&source_ref(
        "agentprof-core",
        "analyzer/cache.rs",
        "CacheMetrics",
    ));
    s.push_str(&source_ref(
        "agentprof-core",
        "analyzer/waste.rs",
        "compute_waste",
    ));

    s
}
