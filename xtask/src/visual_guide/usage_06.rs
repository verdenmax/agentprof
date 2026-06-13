//! Usage lesson 6 — 「db + ingest-otlp：存数据库 + 接入 OTLP」.
//!
//! Target audience: 已经会用 analyze / list / aggregate / serve 的用户，
//! 想要让 agentprof「<strong>不再每次重 parse JSONL</strong>」 + 「<strong>接 Claude
//! Code / Codex 的 `OTel` SDK 实时 push</strong>」。覆盖 hybrid storage
//! (cache / store) + `agentprof db` 6 个子命令 + `agentprof ingest-otlp`
//! 接入流程。本节是「用法」章节最后一节（6/6）。

use super::components::{accordion, comparison_table, flow_diagram, source_ref, visual_compare};

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

<p class="lead">
agentprof 默认每次 <code>analyze</code> 都要重新 parse JSONL —— 单 session 还好，<strong>跨 30 天几百个 session</strong> 就开始肉眼可见地慢。<code>hybrid storage</code> 让 cache 自动接管（dev 默认开），<code>db</code> 子命令族把 sessions 显式持久化到 store，<code>ingest-otlp</code> 让 <strong>Claude Code / Codex 的 OTel SDK 直接 push</strong> session 进来 —— file-based 之外的第二条数据进入路径。
</p>
"#);

    s.push_str(&visual_compare(&[
        (
            "💾",
            "Cache (默认)",
            "Adapter → SQLite cache @ XDG_CACHE_HOME",
        ),
        ("🗄️", "Store (显式)", "agentprof db init --storage-path ..."),
        (
            "📡",
            "OTLP push",
            "OTel SDK → :4317 (gRPC) or :4318 (HTTP) → SQLite",
        ),
    ]));

    s.push_str(r#"
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
                "<code>agentprof db init --storage-path ~/.local/share/agentprof/store.sqlite</code>",
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
<div class="q">🤔 用户视角：cache 还是 store？</div>
<div class="a">一句话决策：<strong>dev 本地 → cache</strong>（不用管，自动开）；<strong>CI / 团队 / 长期 trend / OTLP push → store</strong>（显式 <code>--storage-path</code>）。两者 schema 完全相同，<strong>从 cache 升级到 store 不需要数据迁移</strong>：重新 <code>analyze --storage-mode store</code> 即可，旧 cache 文件可保留也可删。</div>
<div class="q">🛠 怎么开</div>
<div class="a">默认就是 cache，啥都不用做。想用 store：
<pre class="code">agentprof db init --storage-path ~/.local/share/agentprof/store.sqlite
agentprof analyze --storage-mode store   <span class="cm"># 后续命令显式带 --storage-mode store</span></pre>
也可以写进 <code>~/.config/agentprof/config.toml</code> 一劳永逸：
<pre class="code">[storage]
mode = "store"
path = "~/.local/share/agentprof/store.sqlite"</pre></div>
<div class="q">📂 文件落在哪</div>
<div class="a"><code>Cache</code>：<code>$XDG_CACHE_HOME/agentprof/cache.sqlite</code>（默认 <code>~/.cache/agentprof/</code>，OS 可以随时清，agentprof 容忍丢）<br><code>Store</code>：<code>$XDG_DATA_HOME/agentprof/store.sqlite</code>（默认 <code>~/.local/share/agentprof/</code>，用户拥有，agentprof 不主动动）。<br>OTLP receiver 写当前 mode 对应的<strong>单一</strong> storage，由 <code>--storage-mode</code> / <code>--storage-path</code> 决定，默认 cache。</div>
<div class="q">🔁 数据流（cache / store 通用）</div>
<div class="a">"#,
    );
    hybrid_card.push_str(&flow_diagram(&[
        "events.jsonl",
        "Adapter",
        "compute_analysis",
        "SQLite",
    ]));
    hybrid_card.push_str(
        r#"</div>
<div class="q">🪜 为什么这么设计</div>
<div class="a">完整设计决策（XDG 命名背后的取舍 / dual-path read fan-out / dual-write 为何被否决）在 <strong>Wiki §5 「存储层 hybrid mode」</strong>详述，本课只覆盖「用户怎么选」。简短回答：cache 是「<strong>性能优化</strong>」，store 是「<strong>业务数据</strong>」，两者职责清晰不混 —— 见 <strong>ADR-0019</strong>。</div>
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

<div class="q">🛠 启动 receiver（agentprof 端）</div>
<div class="a"><pre class="code"><span class="cm"># gRPC（默认，性能最好）</span>
agentprof ingest-otlp --bind 127.0.0.1:4317 \
  --storage-path ~/.local/share/agentprof/store.sqlite

<span class="cm"># HTTP/protobuf（防火墙穿透更友好）</span>
agentprof ingest-otlp --bind 127.0.0.1:4318 --protocol http \
  --storage-path ~/.local/share/agentprof/store.sqlite

<span class="cm"># 想加 bearer auth（生产建议）</span>
agentprof ingest-otlp --bind 0.0.0.0:4317 \
  --auth-token-file ~/.config/agentprof/otlp-bearer.txt \
  --tls-cert ./server.pem --tls-key ./server.key</pre>
需要编译时开 <code>otlp</code> feature（拉 workspace dep <code>tonic</code> + <code>prost</code> + <code>axum</code>）。<code>full</code> 默认含。</div>

<div class="q">⚙️ Agent 端（OTel SDK / Claude Code）配置</div>
<div class="a">Claude Code / Codex SDK 都遵循 OTel 标准环境变量协议：
<pre class="code"><span class="cm"># Bash / zsh（gRPC 默认）</span>
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317
export OTEL_EXPORTER_OTLP_PROTOCOL=grpc
export OTEL_SERVICE_NAME=claude-code
<span class="cm"># 可选：bearer auth</span>
export OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer $(cat ~/.config/agentprof/otlp-bearer.txt)"

<span class="cm"># 之后跑 agent，spans 自动 push 到 agentprof</span>
claude code "your task"</pre>

<p style="margin:.4em 0 0;font-size:.92rem;color:var(--muted)">⚠️ Claude Code 当前 v1.x 通过 <code>OTEL_*</code> 环境变量配 OTLP。如果你跑的是自家 agent / 用了 OTel collector 链路（agent → collector → agentprof），collector 端把 agentprof 作为 exporter：</p>

<pre class="code"><span class="cm"># otel-collector-config.yaml</span>
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

exporters:
  otlp/agentprof:
    endpoint: 127.0.0.1:4317
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [otlp/agentprof]</pre></div>

<div class="q">🛡 4 层防御 + 排错</div>
<div class="a">详见 <strong>ADR-0022</strong>：Bearer 常时间比较（防时序攻击）/ 每信号大小上限 8/2/8 MiB（防 OOM）/ LRU eviction 1024 sessions（防内存撑爆）/ <code>session.id</code> 256-byte 上限（防 path injection）。
<pre class="code"><span class="cm"># 验证 receiver 在线</span>
curl -v http://127.0.0.1:4318/v1/traces -X POST   <span class="cm"># 期望 415 (no protobuf body)</span>

<span class="cm"># 看进了多少 session</span>
agentprof db stats --storage-path ~/.local/share/agentprof/store.sqlite

<span class="cm"># 看实时 receiver 日志</span>
RUST_LOG=agentprof_storage::otlp=debug agentprof ingest-otlp ...</pre></div>

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
agentprof db init --storage-path ~/.local/share/agentprof/store.sqlite
agentprof db ingest --since 30d
agentprof db stats

<span class="cm"># 3. 接 Claude Code / Codex 实时数据 — OTLP receiver</span>
agentprof ingest-otlp --bind 127.0.0.1:4317 \
  --storage-path ~/.local/share/agentprof/store.sqlite
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
