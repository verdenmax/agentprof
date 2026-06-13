//! Usage lesson 6 — 「db + ingest-otlp：存数据库 + 接入 OTLP」.
//!
//! Target audience: 已经会用 analyze / list / aggregate / serve 的用户，
//! 想要让 agentprof「<strong>不再每次重 parse JSONL</strong>」 + 「<strong>接 Claude
//! Code / Codex 的 `OTel` SDK 实时 push</strong>」。覆盖 hybrid storage
//! (cache / store) + `agentprof db` 6 个子命令 + `agentprof ingest-otlp`
//! 接入流程。本节是「用法」章节最后一节（6/6）。

use super::components::{accordion, comparison_table, flow_diagram, source_ref};

/// Render the HTML body for usage lesson 6.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::usage_06::render();
/// assert!(html.contains("ingest-otlp"));
/// assert!(html.contains("SQLite"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"
<h1>db + ingest-otlp：存数据库 + 接入 OTLP</h1>

<p class="lead">
agentprof 默认每次 <code>analyze</code> 都要重新 parse JSONL —— 单 session 还好，<strong>跨 30 天几百个 session</strong> 就开始肉眼可见地慢。<code>hybrid storage</code> 让 cache 自动接管（dev 默认开），<code>db</code> 子命令族把 sessions 显式持久化到 store，<code>ingest-otlp</code> 让 <strong>Claude Code / Codex 的 OTel SDK 直接 push</strong> session 进来 —— file-based 之外的第二条数据进入路径。
</p>

<div class="card analogy">
  <div class="tag">🔌 生活类比</div>
  从 <strong><code>grep</code> 单文件</strong>升级到 <strong>SQLite + Prometheus push gateway</strong> —— 不再每次扫一遍 JSONL，不再要求 agent 必须先写 <code>events.jsonl</code> 才能被分析；Claude Code 跑着就把 spans 推进来，agentprof 在另一边实时聚合。
</div>

<h2>3 种数据流模式 — 一眼对照</h2>

<p>同一份 session 数据，可以从三条路径进入 agentprof。<strong>不是互斥的</strong> —— 大多数用户 <em>cache 默认开</em> + <em>偶尔升级到 store</em> + <em>团队场景上 OTLP</em>。</p>
"#);

    s.push_str(&comparison_table(
        &["模式", "数据流", "命令"],
        &[
            (
                "Cache（默认）",
                "Adapter → SQLite cache（<code>XDG_CACHE_HOME</code>）→ <code>analyze</code> 走 cache",
                "<code>agentprof analyze</code>（隐式 cache 启用）",
            ),
            (
                "Store（显式持久化）",
                "Adapter → SQLite store（<code>XDG_DATA_HOME</code>）",
                "<code>agentprof db init --storage-path ~/.local/share/agentprof/store.db</code>",
            ),
            (
                "OTLP push",
                "Claude Code / Codex OTel SDK → gRPC :4317 or HTTP :4318 → agentprof-storage",
                "<code>agentprof ingest-otlp --bind 127.0.0.1:4317</code>",
            ),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片展开 db 子命令家族 / hybrid 概念（含流程图）/ OTLP 接入：<strong>① 为什么 · ② agentprof 怎么做 · ③ 其他选择</strong>。</p>"#);

    s.push_str(&accordion(
        1,
        "agentprof db {init,ingest,stats,prune,vacuum,export} — SQLite 存储管理",
        r#"<div class="qa">
<div class="q">🤔 为什么</div>
<div class="a">repeated <code>analyze</code> 慢 —— 30 天窗口里 200 个 session、每个 JSONL 几 MB，重 parse 就是「<strong>每次跑都要等几秒</strong>」。需要一个持久化层把 parse 结果固化，下次跨 session 查询直接走 SQLite index，<strong>毫秒级响应</strong>。</div>
<div class="q">🛠 agentprof 怎么做</div>
<div class="a"><code>agentprof db</code> 是 6 个子命令的家族：
<ul>
<li><code>init</code> —— 在指定路径建空 store（schema migration 自动跑）</li>
<li><code>ingest</code> —— 批量把 adapter 输出的 sessions 写进 store</li>
<li><code>stats</code> —— 看 store 当前大小 / session 数 / 最早最晚时间</li>
<li><code>prune --since 30d</code> —— 按时间窗口删除老 session（释放空间）</li>
<li><code>vacuum</code> —— SQLite <code>VACUUM</code> 整理碎片</li>
<li><code>export</code> —— 把 store 内容导出成 JSONL（迁移 / 备份）</li>
</ul>
配合 <strong>hybrid mode</strong>（见下一张卡片），dev 端基本不用动 store；CI / 团队场景才显式 <code>init</code>。</div>
<div class="q">🪜 其他选择</div>
<div class="a">直接走 Adapter 不开 cache（<code>--no-cache</code>）—— 每次重 parse，适合「<strong>跑完即弃</strong>」的一次性诊断；或者「<strong>怀疑 cache 损坏</strong>」想强制重读 source-of-truth 的场景。生产 / 长期 trend 场景几乎总是要 store。</div>
</div>"#,
    ));

    let mut hybrid_card = String::from(
        r#"<div class="qa">
<div class="q">🤔 为什么</div>
<div class="a">dev 单人本地用，cache 自动够用（开箱即用，<code>analyze</code> 第二次跑直接走 SQLite，sub-second 响应）；CI / 团队 / 长期 trend 分析需要<strong>稳定路径 + 显式备份 / 同步</strong>，那就升级到 store。<strong>同一套 schema、两个物理位置</strong>，关键差异在「<strong>谁拥有这个 db 文件 + 何时清理</strong>」。</div>
<div class="q">🛠 agentprof 怎么做</div>
<div class="a">数据流：
"#,
    );
    hybrid_card.push_str(&flow_diagram(&[
        "events.jsonl",
        "Adapter",
        "compute_analysis",
        "SQLite",
    ]));
    hybrid_card.push_str(
        r#"<p><code>StorageConfig</code> + <code>StorageMode</code> enum 在 <code>agentprof-storage::config</code> 里定义两个 variant：<code>Cache</code>（<code>XDG_CACHE_HOME/agentprof/cache.db</code>，OS 可以随时清，agentprof 容忍丢）/ <code>Store</code>（<code>XDG_DATA_HOME/agentprof/store.db</code>，用户拥有，agentprof 不主动动）。<code>analyze</code> 看 mode 决定写哪边；OTLP receiver 总是写 store。</p></div>
<div class="q">🪜 其他选择</div>
<div class="a">考虑过 <strong>dual-path 模式</strong>（cache + store 同步双写）—— 否决，复杂度高 + 一致性问题不值得，详见 <strong>[ADR-0018]</strong>。当前模式：cache 是「<strong>性能优化</strong>」，store 是「<strong>业务数据</strong>」，两者不混。</div>
</div>"#,
    );

    s.push_str(&accordion(
        2,
        "hybrid cache vs store — XDG path + ownership + 何时升级",
        &hybrid_card,
    ));

    s.push_str(&accordion(
        3,
        "agentprof ingest-otlp — Claude Code / Codex OTel SDK 接入",
        r#"<div class="qa">
<div class="q">🤔 为什么</div>
<div class="a">file-based 模式（read JSONL）只覆盖 <strong>Copilot CLI / 老 Claude</strong> 这类「<strong>写日志到磁盘</strong>」的 agent。<strong>Claude Code（新版）/ Codex</strong> 用 OTel SDK 是 native —— 它们已经会发 spans，agentprof 只需要在另一头听就行，不用让 agent 团队额外写「<strong>导出 events.jsonl</strong>」逻辑。</div>
<div class="q">🛠 agentprof 怎么做</div>
<div class="a">启动 <code>agentprof ingest-otlp --bind 127.0.0.1:4317</code>（gRPC 默认端口）或 <code>:4318</code>（HTTP）。需要编译时开 <code>otlp</code> feature（拉 workspace dep <code>tonic</code> + <code>prost</code> + <code>axum</code>）。Claude Code 的 OTel SDK 配 endpoint 指向 agentprof 即可。
<ul>
<li>spans 进来 → <code>StorageFlushSink</code> → <code>upsert_report</code> → SQLite（<strong>无新 schema migration</strong>，复用 store 的 sessions 表）</li>
<li><strong>4 层防御</strong>（详见 <strong>[ADR-0022]</strong>）：Bearer token <em>常时间比较</em>（防时序攻击）/ 每信号大小上限（防 OOM）/ LRU eviction（防内存撑爆）/ <code>session.id</code> 256-byte 上限（防 path injection）</li>
</ul></div>
<div class="q">🪜 其他选择</div>
<div class="a">自建 <strong>OTel Collector</strong> + ETL 到 agentprof —— 技术上行，但多一层进程 + 多一套配置，单机 / 小团队不值得。agentprof 内置 OTLP receiver 已够覆盖「<strong>5–50 个 dev 同时推</strong>」量级；超过这个量级再上正经 collector。</div>
</div>"#,
    ));

    s.push_str(r#"<h2>典型工作流：从 cache 升级到 store + OTLP</h2>

<p>三步走，对应上面三张卡片：</p>

<pre class="code"><span class="cm"># 1. dev 阶段 — 啥都不做，cache 自动开</span>
agentprof analyze
agentprof list --since 7d

<span class="cm"># 2. 想长期保留 / 跨机器同步 — 显式 store</span>
agentprof db init --storage-path ~/.local/share/agentprof/store.db
agentprof db ingest --since 30d
agentprof db stats

<span class="cm"># 3. 接 Claude Code / Codex 实时数据 — OTLP receiver</span>
agentprof ingest-otlp --bind 127.0.0.1:4317 \
  --storage-path ~/.local/share/agentprof/store.db
<span class="cm"># （另一边 Claude Code 配 OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317）</span></pre>

<h2>结语：用法章节完结</h2>

<p>这是「用法」章节的<strong>最后一节（6/6）</strong>。学完之后你已经能用 agentprof 覆盖：单 session 分析（<code>analyze</code>）、跨 session 聚合（<code>list</code> / <code>aggregate</code>）、浏览器看板（<code>serve</code>）、持久化 + OTLP 接入（本节）。</p>

<p>接下来是 <strong>Wiki 章节（8 节）</strong>，面向想<strong>深入原理 + 给项目贡献代码</strong>的中阶 / 开发者读者：火焰图算法、ROI 公式、tokenizer 选型、adapter 协议、SQLite schema、OTLP 防御、TUI 渲染、xtask 工具链 —— 每节对应一篇 ADR + 一组 source 链接。</p>
"#);

    s.push_str(&source_ref("agentprof-storage", "db.rs", "Db"));
    s.push_str(&source_ref(
        "agentprof-storage",
        "query.rs",
        "query_sessions_since",
    ));
    s.push_str(&source_ref("agentprof-storage", "otlp/mod.rs", "otlp"));

    s
}
