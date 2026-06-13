//! Wiki lesson 7 — 「Web dashboard 架构」.
//!
//! Walkthrough of `agentprof-cli::cmd::serve`: the M2.3 localhost
//! dashboard wired up by `build_router` (axum) + askama 0.16
//! templates + a ~80 LOC vanilla JS poller that swaps HTML chunks
//! via `innerHTML` every 5 s. Five views: sessions list, session
//! detail, aggregate, MCP waste list, MCP waste detail. All
//! decisions enumerated in ADR-0024 (D-1..D-7). Hander names and
//! route paths cross-checked at T18.
//!
//! Recon-confirmed corrections vs. the original brief:
//!
//!   - The chunk endpoints are `/api/sessions.html`,
//!     `/api/session/:id.html`, `/api/aggregate.html`,
//!     `/api/mcp-waste.html`, `/api/mcp-waste/:tool.html` —
//!     `.html` suffix is significant (D-3: HTML, not JSON).
//!   - Handler names are `*_chunk` (chunk) and `*_page` (full page
//!     shell). E.g. `sessions_page` + `sessions_chunk`.
//!   - Per-session detail reuses `format::html::render_body_only`
//!     to surface the same HTML the `analyze --export html` cli
//!     subcommand prints (Cache section / Tool rank / etc.) —
//!     guaranteeing semantic parity terminal-vs-browser.

use super::components::{accordion, comparison_table, flow_diagram, source_ref};

/// Render the HTML body for wiki lesson 7.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_07::render();
/// assert!(html.contains("dashboard"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
<code>agentprof serve</code>（M2.3）拉起一个 <strong>localhost-only</strong> 的 HTTP 看板 —— 5 个视图（sessions / session detail / aggregate / mcp-waste list / mcp-waste detail），<strong>5 秒轮询</strong>自动刷新，<strong>零 JS 框架</strong>。整套方案 reuse M2.2 已经在用的 axum + askama 栈，<strong>workspace top-level 零新增依赖</strong>。ADR-0024 把它当作 7 个独立决策（D-1..D-7）逐条钉死，本课带着真实代码把这 7 个决策摸一遍。
</p>
"#);

    s.push_str(&flow_diagram(&[
        "浏览器 5s 轮询",
        "GET /api/&lt;view&gt;.html",
        "axum handler",
        "innerHTML swap",
    ]));

    s.push_str(r#"
<div class="card analogy">
  <div class="tag">📦 类比 — 像 <code>cargo doc --open</code></div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong>cargo doc</strong>：把当前 crate 的文档渲染成静态 HTML、起一个 localhost server、浏览器一拉就看。</li>
    <li><strong>agentprof serve</strong>：把当前 store DB 的 session 数据渲染成 HTML、起一个 localhost server、浏览器一拉就看。</li>
    <li><strong>共同点</strong>：单进程、不需要 docker / nginx / 数据库 / 任何 ops；杀掉进程数据就没人能访问；不留 cookie / 不挂登录。</li>
  </ul>
  <p style="margin:.6em 0 0;font-size:.92rem;color:var(--muted)">这种「ephemeral local dashboard」模式比「企业级 BI 看板」轻 100x —— 个人用户和小团队的 95% 需求都被覆盖，剩下 5% 的场景留给 grafana + OTLP collector + prometheus。agentprof 不想做 grafana 的活，做<strong>「本地 ROI 速查」</strong>就够了。</p>
</div>

<h2>ADR-0024 全 7 决策对比（D-1..D-7）</h2>
"#);

    s.push_str(&comparison_table(
        &["决策编号", "选什么", "为什么 / 关键 trade-off"],
        &[
            (
                "<strong>D-1</strong> 后端栈",
                "axum 0.7 + tokio + askama 0.16",
                "复用 M2.2 OTLP HTTP receiver 同款；workspace 已 depend，<strong>top-level 零新增依赖</strong>",
            ),
            (
                "<strong>D-2</strong> 前端框架",
                "无（vanilla JS poller, ~80 LOC）",
                "不引 React/Vue/Svelte = 不引 npm/webpack/vite 整个 build chain；本地看板规模 ≤ 千行 HTML，原生 DOM 完全够",
            ),
            (
                "<strong>D-3</strong> 数据传输",
                "HTML 切片（chunk-endpoint pattern）",
                "JS 拉 <code>/api/&lt;view&gt;.html</code> → <code>innerHTML</code> 替换；不需要 JSON → client-side 模板 → 渲染。少一层 = 少一次 bug",
            ),
            (
                "<strong>D-4</strong> 缓存策略",
                "无（每请求一次 SQLite read）",
                "5 秒轮询 × 单用户 ≈ 0.2 QPS，SQLite 完全顶得住；缓存层带 invalidation bug 多于性能收益",
            ),
            (
                "<strong>D-5</strong> 数据源",
                "store mode（不 fallback adapter scan）",
                "serve 要长期跑、要全量 trend；adapter scan 每次 walk filesystem 太重，必须有 SQLite store 撑",
            ),
            (
                "<strong>D-6</strong> 网络绑定",
                "默认 <code>127.0.0.1:4329</code>，loopback only，无 auth",
                "本地工具，认证靠 OS user；显式绑非 loopback 时 warn 用户（与 ADR-0022 同款 capacity-cap 风格防御）",
            ),
            (
                "<strong>D-7</strong> 发版策略",
                "v0.3.3（patch bump，不是 0.4.0）",
                "feature gated 在 <code>serve</code> feature 下；不破坏 v0.3.x 用户的 cli 兼容性；零 breaking change",
            ),
        ],
    ));

    s.push_str(r#"<p style="font-size:.88rem;color:var(--muted)">⚠️ Recon 校正：chunk endpoint 真实路径形如 <code>/api/sessions.html</code> / <code>/api/session/:id.html</code>（<code>.html</code> 后缀是 D-3 的 signal —— 这是<strong>HTML</strong>不是 JSON）；handler 名是 <code>*_chunk</code>（5 个 chunk）+ <code>*_page</code>（5 个 page shell）共 10 个 + 1 个 <code>healthz</code>。Detail 页 reuse <code>format::html::render_body_only</code> —— 浏览器看到的 Cache 段、Tool rank 表和 <code>analyze --export html</code> cli 完全一致。</p>"#);

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片：① 7 决策 (D-1..D-7) 逐条摘要 · ② chunk-endpoint pattern 真实 router 表 · ③ 5 视图源码 walk-through。</p>"#);

    // ---------------- Accordion 1: D-1..D-7 ----------------
    let card1 = r#"<div class="qa">
<div class="q">📋 D-1 Reuse axum + askama —— 「不新建栈」原则</div>
<div class="a">M2.2 已经把 axum 0.7 / tokio / askama 0.16 都 add 到 workspace.dependencies；M2.3 不允许新增 top-level dep（要走 cargo deny allowlist + ADR）。决策结果：复用同一个 axum router builder pattern、同一个 askama 0.16 模板规约 —— 一致性 &gt; 「找个更新潮的 web framework」。serve feature 跟 otlp feature 共享传输层 mod，binary size 增长几十 KB 内。</div>

<div class="q">📋 D-2 Vanilla JS —— 「不引 build chain」原则</div>
<div class="a">引 React 等于引 node + npm + webpack/vite + 一堆 tsconfig；CI 容器要先装 node 才能 build agentprof —— 这违背 agentprof「一个 cargo install 完事」的 USP。Vanilla JS poller 80 LOC，写在 <code>static_assets.rs</code> 里、build 时编进 binary（include_str!）—— 用户连 npm 都不知道在哪。<strong>取舍</strong>：失去 SPA 路由 / 状态管理 / vd-DOM 加速 —— 但本地 5 视图根本用不上。</div>

<div class="q">📋 D-3 HTML chunk endpoint —— 「不引 client template」原则</div>
<div class="a"><strong>主流做法</strong>：API 吐 JSON，前端用模板引擎渲染 DOM。<strong>chunk-endpoint pattern</strong>：服务端用 askama 渲染好 HTML 片段（<strong>不含</strong> <code>&lt;html&gt;</code> / <code>&lt;head&gt;</code>），前端拉到后 <code>el.innerHTML = chunk</code> 直接换。<strong>少一层</strong> = 少写一份模板 + 少一份「服务端 JSON schema 和前端期望对不上」的故障源。<strong>用户感知</strong>：每 5 秒页面区域 flash 一下 —— 加 CSS transition 就柔和了。</div>

<div class="q">📋 D-4 / D-5 / D-6 安全 + 性能</div>
<div class="a"><strong>D-4 无缓存层</strong>：5s × 单用户 ≈ 0.2 QPS，加缓存 = 加 invalidation bug；SQLite 跑 <code>SELECT * FROM sessions ORDER BY started_at LIMIT 100</code> &lt; 10ms 内即返。<strong>D-5 必须 store mode</strong>：serve 跑数小时 / 天，adapter scan 每次 walk filesystem ≈ 几秒，根本不行；启动时检查 storage_mode==Store，否则 cli 报错退码 1。<strong>D-6 loopback-only + 无 auth</strong>：本地工具，谁能 connect 127.0.0.1 谁就是本机用户 —— OS 已经做了认证。绑 0.0.0.0 / 公网 IP 时显式 warn，提醒用户加反代或 ssh tunnel。</div>

<div class="q">📋 D-7 v0.3.3 patch release</div>
<div class="a">serve 是 <strong>additive</strong>：feature gated（<code>--features serve</code>），不动现有 cli/cmd/* 任何子命令，不改 schema，不改 wire format。SemVer 规则下，additive feature = patch bump。v0.4.0 留给真正的 breaking change（比如 ADR-0019 决策反转之类）。这条决策也借鉴 ADR-0022 的 v0.3.2 hardening 经验 —— 硬化和 additive feature 都走 patch。</div>
</div>"#;
    s.push_str(&accordion(1, "ADR-0024 7 决策 (D-1..D-7) 摘要", card1));

    // ---------------- Accordion 2: chunk pattern ----------------
    let card2 = r#"<div class="qa">
<div class="q">🛣️ 真实 router 表（<code>crates/agentprof-cli/src/cmd/serve/router.rs:29</code>）</div>
<div class="a">
<pre class="code"><code>pub fn build_router(state: AppState) -&gt; Router {
    Router::new()
        // page shells（完整 HTML，浏览器首次进入）
        .route("/",                        get(|| async { Redirect::permanent("/sessions") }))
        .route("/sessions",                get(handlers::sessions_page))
        .route("/session/:id",             get(handlers::session_page))
        .route("/aggregate",               get(handlers::aggregate_page))
        .route("/mcp-waste",               get(handlers::mcp_waste_list_page))
        .route("/mcp-waste/:tool",         get(handlers::mcp_waste_detail_page))
        // chunk endpoints（HTML 片段，JS 5 秒轮询）
        .route("/api/sessions.html",       get(handlers::sessions_chunk))
        .route("/api/session/:id.html",    get(handlers::session_chunk))
        .route("/api/aggregate.html",      get(handlers::aggregate_chunk))
        .route("/api/mcp-waste.html",      get(handlers::mcp_waste_list_chunk))
        .route("/api/mcp-waste/:tool.html",get(handlers::mcp_waste_detail_chunk))
        // 健康检查 + 静态资产
        .route("/healthz",                 get(handlers::healthz))
        .route("/static/:name",            get(handlers::static_asset))
        .with_state(state)
}</code></pre>
</div>

<div class="q">🔄 chunk-endpoint pattern 时序</div>
<div class="a">
<ol style="margin:.5em 0 0 1.2em;font-size:.92rem">
<li>浏览器 GET <code>/sessions</code> → <code>sessions_page</code> 返完整 HTML（含 askama base 模板 + nav + 一个空 <code>&lt;div id="chunk"&gt;</code>）。</li>
<li>页面 JS（include 在 base template 里，~80 LOC）启动 <code>setInterval(refresh, 5000)</code>。</li>
<li>每 5 秒 <code>fetch('/api/sessions.html')</code> → <code>sessions_chunk</code> 返 HTML 片段（不含 <code>&lt;html&gt;</code>）。</li>
<li>JS <code>document.getElementById('chunk').innerHTML = htmlText</code>。</li>
<li>浏览器原生 parse + render；表格、链接、CSS hover 全部按浏览器规则工作。</li>
</ol>
<p style="margin:.6em 0 0;font-size:.92rem"><strong>等价心智模型</strong>：把每个视图当成「会自动 reload 的静态页」—— 用户连刷新都不用按。如果未来要换 framework（HTMX / Turbo / Hotwire），切换的也只是「fetch + innerHTML」这一小段 JS，handler 一行都不动。</p>
</div>

<div class="q">🤝 为什么 page shell + chunk 是<strong>两个</strong> handler？</div>
<div class="a"><strong>page</strong>需要把 nav / footer / CSS / JS poller bootstrap script 全装进去（一次性）；<strong>chunk</strong>只需要 data 区域（per refresh）。如果只用 page handler，每 5 秒浏览器要重新 parse CSS / 重启 JS —— 卡顿。两个 handler 各做一件事，page 的 askama 模板 <code>{% include %}</code> chunk 模板，<strong>同一份 HTML 渲染逻辑被两个端点 reuse</strong> —— 改一个地方两处生效。</div>
</div>"#;
    s.push_str(&accordion(
        2,
        "chunk-endpoint pattern 真实 router 表",
        card2,
    ));

    // ---------------- Accordion 3: 5 views ----------------
    let card3 = r#"<div class="qa">
<div class="q">👁️ 视图 1：sessions list (<code>sessions_chunk</code>)</div>
<div class="a"><strong>SQL</strong>：<code>SELECT id, agent, dominant_model, started_at, duration_ms, total_input_tokens, total_output_tokens FROM sessions ORDER BY started_at DESC LIMIT 100</code>。<strong>askama 模板</strong>：渲染表格，每行 <code>&lt;a href="/session/{id}"&gt;</code> 跳详情。<strong>用户体验</strong>：dev 一边跑 agent 一边开浏览器，每 5 秒新 session 自动浮上来 —— 不用手动 <code>list --since 1h</code>。</div>

<div class="q">👁️ 视图 2：session detail (<code>session_chunk</code>) —— 关键 reuse 点</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-cli/src/cmd/format/html.rs:175
pub fn render_body_only(
    report: &amp;AnalysisReport,
    cache_metrics: Option&lt;&amp;CacheMetrics&gt;,
    /* ... */
) -&gt; askama::Result&lt;String&gt;;</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem"><code>analyze --export html</code> 和 web detail<strong>共用</strong>这个 fn —— 浏览器里看到的 Turn Summary / Tool Rank / Cache 段，和 <code>agentprof analyze --export html &gt; out.html</code> 输出完全一致。这是 ADR-0024 显式提到的「与 ADR-0023 cache metrics 一致」的<strong>实现机制</strong>。<strong>意义</strong>：CLI 和 web 用户看到的「真相」是同一份；不会出现「CLI 显示 80% hit rate，web 显示 78%」这种诡异 bug。</p>
</div>

<div class="q">👁️ 视图 3：aggregate (<code>aggregate_chunk</code>)</div>
<div class="a">Reuse <code>agentprof-core::aggregate</code>（M2.1.1 加 episodes_json 后开放）。SQL 拉 <code>sessions + episodes_json</code>，分组维度（model / tool / day）由 URL query 决定（<code>?by=model</code>）。<strong>展示</strong>：表 + 简单 SVG bar chart（也是手写 SVG，不引 chart.js）。</div>

<div class="q">👁️ 视图 4 + 5：mcp-waste list / detail</div>
<div class="a">List 视图聚合<strong>跨 session</strong>的 MCP waste —— 调 <code>aggregate_waste(per_session)</code>（见 wiki 4），按 tool 名展示「累计 wasted tokens」+「loaded session count」+「actually-called session count」。Detail 视图点进单 tool，列出哪些 session 加载了它但从没调用 —— 用户能直接定位「哪个 server 该从 mcp.json 删了」。这是 agentprof 的<strong>核心 ROI 价值</strong>在 web 里的直接呈现。</div>

<div class="q">🚀 为什么不加 WebSocket / SSE？</div>
<div class="a">两者都比 5 秒 polling「更优雅」，但代价：① 后端要维护连接状态；② 防火墙 / 反代里有时被掐；③ 测试 / 调试比 HTTP 复杂。轮询的<strong>简单性</strong>压倒了「更新延迟从 0-5s 降到 0-1s」的边际收益 —— 本地看板的用户不在乎 5 秒 lag。如果未来真有需求，加 SSE 不破坏 chunk-endpoint pattern（chunk endpoint 升级为 stream）—— 后向兼容。</div>
</div>"#;
    s.push_str(&accordion(3, "5 视图源码 walk-through", card3));

    s.push_str(r"<h2>下一步</h2>
<p>本课讲清了 <code>agentprof serve</code> 的 7 决策、chunk-endpoint pattern 真实路由、5 视图各自的数据源。下一课「<strong>贡献指南</strong>」是 Wiki 章节的收官 —— 怎么给 agentprof 提 PR、9 阶段 pipeline 怎么走、Conventional Commits 怎么写、加 ADR / 通过 CI 的实操清单。</p>
");

    s.push_str(&source_ref(
        "agentprof-cli",
        "cmd/serve/router.rs",
        "build_router",
    ));
    s.push_str(&source_ref(
        "agentprof-cli",
        "cmd/serve/handlers.rs",
        "sessions_chunk",
    ));
    s.push_str(&source_ref(
        "agentprof-cli",
        "cmd/format/html.rs",
        "render_body_only",
    ));

    s
}
