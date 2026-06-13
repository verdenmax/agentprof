//! Usage lesson 5 — 「serve：浏览器实时看板」.
//!
//! Target audience: 跑过 `analyze --export html` 后想要「边跑 agent
//! 边看实时数据」的用户。覆盖 `agentprof serve` 拉起本地 HTTP 看板、
//! 5 个视图（/sessions / /session/:id / /aggregate / /mcp-waste /
//! 工具栏）+ `[serve]` config block + serve vs static HTML 决策。

use super::components::{accordion, comparison_table, source_ref};

/// Render the HTML body for usage lesson 5.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::usage_05::render();
/// assert!(html.contains("/sessions"));
/// assert!(html.contains("127.0.0.1:4329"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
静态 HTML 报告（上节学的 <code>analyze --export html</code>）是<strong>快照</strong>；<code>agentprof serve</code> 拉起一个本机端口（默认 <code>127.0.0.1:4329</code>），<strong>5 个视图自动每 5 秒轮询刷新</strong>，跑 agent 边看 token 趋势 —— 不用每次手动重新导。
</p>

<div class="card analogy">
  <div class="tag">🔌 生活类比</div>
  从 <strong><code>cron</code> + 邮件升级到 <code>Grafana</code></strong> —— 不用每次手动 <code>analyze --export html</code> 邮件转发给自己，agent 跑着就能看实时数据；浏览器开着一个 tab，token 涨没涨、cache 命中没命中、哪个 tool 一直在调，全在那儿动。
</div>

<h2>3 个核心视图 — 一眼对照</h2>

<p>启动 <code>agentprof serve</code> 后，浏览器自动打开（除非 <code>--no-open</code>）。三个最常用的入口：</p>
"#);

    s.push_str(&comparison_table(
        &["视图", "URL", "用途"],
        &[
            (
                "Sessions list",
                "<code>/sessions</code>",
                "最近 sessions 概览（默认 30 天窗口、最多 200 条）",
            ),
            (
                "单 session 详情",
                "<code>/session/:id</code>",
                "完整火焰图 + 表 + cache（T10 同款 body 复用）",
            ),
            (
                "跨 session aggregate",
                "<code>/aggregate?by=model</code>",
                "跨模型对比（类似 list/aggregate 课的 <code>--by tool</code> / <code>--by day</code>）",
            ),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 五张卡片展开 4 个视图 + 1 个工具栏：<strong>① 看到什么 · ② 为什么这么设计 · ③ 怎么用</strong>。</p>"#);

    s.push_str(&accordion(
        1,
        "/sessions 列表视图 — 最近 200 sessions",
        r#"<div class="qa">
<div class="q">🧪 看到什么</div>
<div class="a">最近 200 个 session 的紧凑表，<strong>30 天窗口</strong>默认（见 <code>cmd/serve/handlers</code> 里的 <code>DEFAULT_SESSIONS_WINDOW</code>）。5 列：<strong>Started</strong>（开始时间，倒序）/ <strong>Model</strong> / <strong>Turns</strong> / <strong>Out-tokens</strong> / <strong>Cache%</strong>。点 session id 直接跳详情页。
<figure>
<img class="shot" src="../assets/dashboard-overview.svg" alt="dashboard /sessions 视图 (T19 添)">
<figcaption>sessions 列表 — Started/Model/Turns/Out-tokens/Cache% 5 列</figcaption>
</figure>
</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">列表是<strong>入口页</strong>，要快 —— 上来就给一周高频信息，不能等几秒。30 天窗口和 200 条上限保证「正常工作量下永远是亚秒响应」；想看更早的 session 就走 <code>analyze --path</code> 或者改 CLI 参数。<strong>不做分页</strong>是有意的：一屏看不完说明你应该缩窗口，而不是翻第二页。</div>
<div class="q">✅ 怎么用</div>
<div class="a">浏览器开着这个 tab，agent 跑着，每 5 秒它会自己刷一次 —— 你能直接看到「<strong>刚才那次 agent 调用花了 18k token</strong>」「<strong>cache 这一次只命中了 30%</strong>」这种实时反馈，无需手动 reload。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "/session/:id 详情视图 — 复用 analyze HTML body",
        r#"<div class="qa">
<div class="q">🧪 看到什么</div>
<div class="a"><strong>跟 T10 学的 <code>analyze --export html</code> 长得一模一样</strong>：Turn Summary 表 + Tool Rank 火焰图 + Cache 段 + Wasted Tool 提示。技术上是同一段 HTML —— serve handler 调 <code>format::html::render_body_only</code> 把 body 段抽出来，外层换上 serve 自己的 chrome（带工具栏的）。
<figure>
<img class="shot" src="../assets/dashboard-session.svg" alt="dashboard /session/:id 详情视图 (T19 添)">
<figcaption>详情视图 — 与 analyze --export html 同款 body</figcaption>
</figure>
</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">DRY —— 一份模板渲两个产物（静态文件 + serve 端动态）能保证<strong>"看到的东西是同一套"</strong>，避免「静态报告漂亮，看板里残缺」这种维护噩梦。每 5 秒重新跑一次 analyze 对单个 session 来说几乎免费（百毫秒级），所以不需要缓存。</div>
<div class="q">✅ 怎么用</div>
<div class="a">从 <code>/sessions</code> 点进来，或者直接 <code>http://127.0.0.1:4329/session/&lt;id&gt;</code>。如果你正盯着某个特定 session（比如刚跑了个长任务），把这个 URL bookmark 起来比 <code>analyze</code> 命令快得多。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "/aggregate?by=model|tool|day — 跨 session 聚合视图",
        r#"<div class="qa">
<div class="q">🧪 看到什么</div>
<div class="a">和 CLI <code>aggregate --by ...</code>（上节课）<strong>同样的三张表</strong>，只是渲染到浏览器：<code>?by=model</code> 出 CacheCr/CacheRd/Hit%/NetSaved；<code>?by=tool</code> 出调用次数 + 总 token；<code>?by=day</code> 出时间桶 + low-utilization 标记。
<figure>
<img class="shot" src="../assets/dashboard-aggregate.svg" alt="dashboard /aggregate 视图 (T19 添)">
<figcaption>aggregate 视图 — 浏览器版的 --by model / tool / day</figcaption>
</figure>
</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a"><code>?by=mcp-server</code> 会返回 <strong>400 Bad Request</strong> —— 不是 bug，是有意的：mcp-server 维度的浪费分析需要 sidecar（tool 描述 token 量），逻辑比一般 aggregate 复杂，单独走 <code>/mcp-waste</code> 专用视图能给更准确的展示。强行塞进 <code>/aggregate</code> 会让 URL 看起来一致但语义实际是两套，反而坑用户。</div>
<div class="q">✅ 怎么用</div>
<div class="a">三个 URL 各 bookmark 一个：<code>/aggregate?by=model</code>（模型对比）、<code>/aggregate?by=tool</code>（tool 排名）、<code>/aggregate?by=day</code>（趋势 + 空转日）。轮询会自动带上参数，所以刷新后视图不会跳走。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        4,
        "/mcp-waste — MCP 浪费分析（list + detail 两层）",
        r#"<div class="qa">
<div class="q">🧪 看到什么</div>
<div class="a">两层视图：<strong>list</strong>（每个 mcp-server 一行，浪费分数排序）+ <strong>detail</strong>（点进去看哪些 tool 加载了但从没被调用 / 调用次数极低）。<strong>heuristic-only 模式</strong> —— serve 端不要求 sidecar 在线，给的是基于启发式的估算。
<figure>
<img class="shot" src="../assets/dashboard-mcp-waste.svg" alt="dashboard /mcp-waste 视图 (T19 添)">
<figcaption>mcp-waste — 浪费分数 + 详情两层</figcaption>
</figure>
</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a">浏览器场景下要的是<strong>"低延迟、零外部依赖"</strong> —— 启发式（基于调用次数 / sessions 覆盖度推断浪费）足够给你"这个 server 该砍"的方向感。<strong>精准数字</strong>（"这个 tool 的描述吃了多少 token"）需要拉到 sidecar 跑 tokenizer，那条路径走专门的 CLI <code>agentprof mcp-waste --tool-descriptions</code>，serve 这边不强求。</div>
<div class="q">✅ 怎么用</div>
<div class="a">先在浏览器 <code>/mcp-waste</code> 大致看「<strong>哪些 server 嫌疑大</strong>」；锁定嫌疑 server 后回到 CLI 跑 <code>agentprof mcp-waste --tool-descriptions --server &lt;name&gt;</code> 拿精准 token 数，决定砍不砍 / 哪些 tool 关掉。serve 是<strong>探测雷达</strong>，CLI 是<strong>精确狙击</strong>。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        5,
        "工具栏 — 暂停 / 间隔切换 / localStorage 记忆",
        r#"<div class="qa">
<div class="q">🧪 看到什么</div>
<div class="a">页面顶部固定栏：<strong>暂停 / 继续按钮</strong> + <strong>间隔下拉</strong>（1s / 2s / 5s / 10s / 30s，默认 5s）。会议中临时不想刷新点暂停；演示火焰图细节调到 30s；正在 debug 一个高频任务调到 1s 看实时。</div>
<div class="q">🤔 为什么这么设计</div>
<div class="a"><strong>每次开新 tab 重新配置很烦</strong> —— 所以选择持久化到 <code>localStorage</code>，下次打开浏览器记住上次设置。实现是<strong>原生 JS poller</strong>（不引入 React / Vue / htmx 等任何前端框架），整个工具栏 + 轮询逻辑就一段 vanilla JS（per ADR-0024 D-2 决策：D-1/D-3/D-4/D-5 都是为了「<strong>无构建、无 node_modules、纯 Rust 出 binary 就能跑</strong>」）。</div>
<div class="q">✅ 怎么用</div>
<div class="a">不用任何配置 —— 打开页面就有。如果工具栏不在你期望的位置，看 ADR-0024 里 D-2「vanilla JS poller」的具体落点；想换 polling 策略改一处 JS 就好，不用动 Rust 代码。</div>
</div>"#,
    ));

    s.push_str(r#"<h2><code>[serve]</code> config block — 固化默认值</h2>

<p>每次敲 <code>--bind 127.0.0.1:4329 --interval-default 5 --no-open</code> 很烦，把它写进 <code>~/.config/agentprof/config.toml</code>（路径见 <code>config show</code>）：</p>

<pre class="code"><span class="kw">[serve]</span>
bind <span class="op">=</span> <span class="st">"127.0.0.1:4329"</span>
interval_default <span class="op">=</span> <span class="nm">5</span>
auto_open <span class="op">=</span> <span class="kw">true</span></pre>

<p>CLI flags 总是<strong>覆盖</strong>配置文件：临时换端口直接 <code>--bind 127.0.0.1:9999</code>，不影响默认值。</p>

<h2>CLI flags 速查</h2>
"#);

    s.push_str(&comparison_table(
        &["Flag", "默认值", "用途"],
        &[
            (
                "<code>--bind</code>",
                "<code>127.0.0.1:4329</code>",
                "监听地址；要从局域网访问改成 <code>0.0.0.0:4329</code>（注意没有 auth）",
            ),
            (
                "<code>--storage-path</code>",
                "OS data dir 下的默认 SQLite",
                "指向已有的 storage DB（比如 watch 在用的那一份）",
            ),
            (
                "<code>--interval-default</code>",
                "<code>5</code>（秒）",
                "首次打开时的轮询间隔；用户在工具栏改过会被 localStorage 覆盖",
            ),
            (
                "<code>--no-open</code>",
                "（默认会 open）",
                "禁止自动打开浏览器；SSH/容器/CI 里必须加",
            ),
        ],
    ));

    s.push_str("<h2>「serve vs static HTML」决策表</h2>\n");

    s.push_str(&comparison_table(
        &["你想做什么", "用哪个", "为什么"],
        &[
            (
                "分享给同事一份快照",
                "<code>analyze --export html</code>",
                "单个 HTML 文件，可邮件 / 上传 wiki / 截图存档；离线打开也能看",
            ),
            (
                "边跑 agent 边看实时数据",
                "<code>serve</code>",
                "5 秒轮询 + 多视图 + 暂停按钮，配合 watch 写入是「<strong>近实时</strong>」",
            ),
            (
                "团队共看 / 长期归档",
                "<code>serve</code> 配反向代理 + auth",
                "bind 到内网地址，前面挂 nginx / Caddy 加 basic auth 或 OAuth proxy",
            ),
        ],
    ));

    s.push_str("<h2>下一步</h2>\n<p>会用 serve 之后，下一课会带你看 <strong>watch + config</strong>：watch 守护进程把新 session 实时灌进 storage（serve 这边自动就能看到），config 把所有命令的默认值固化到一处避免每次敲长串。</p>\n");

    s.push_str(&source_ref(
        "agentprof-cli",
        "cmd/serve/router.rs",
        "build_router",
    ));
    s.push_str(&source_ref(
        "agentprof-cli",
        "cmd/serve/handlers.rs",
        "handlers",
    ));

    s
}
