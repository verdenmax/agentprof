//! Wiki lesson 6 — 「OTLP receiver」.
//!
//! Walkthrough of the `agentprof-storage::otlp` module: the dual
//! gRPC (`serve_grpc`) + HTTP/protobuf (`serve_http`) servers
//! that ingest OpenTelemetry logs/metrics/traces from Claude Code,
//! Codex, Copilot CLI, and any OTel-compatible emitter. Architecture
//! follows ADR-0021 (receiver → router → buffer → flush sink →
//! upsert → `SQLite`); hardening per ADR-0022's four defenses
//! (constant-time Bearer compare via `subtle::ConstantTimeEq`,
//! per-signal `max_decoding_message_size` / `DefaultBodyLimit`,
//! `SessionRouter` LRU eviction at `max_open_sessions = 1024`,
//! and the 256-byte session.id cap in `mapper.rs:521`). All public
//! `fn` names, constants, and module paths cross-checked at T18.
//!
//! Recon-confirmed corrections vs. the original brief:
//!
//!   - The HTTP transport speaks **HTTP/protobuf only** in the
//!     current impl (no `OTLP/HTTP+JSON` path); routes are
//!     `/v1/logs`, `/v1/metrics`, `/v1/traces` per `OTel` spec.
//!   - The "1024 LRU cap" is `SessionBufferCaps::max_open_sessions`
//!     in `router.rs:171`, not a global per-process limit.
//!   - The "256-byte cap" lives in `mapper.rs:521` and applies to
//!     `session.id` BEFORE allocation (defends against attacker-set
//!     giant ids exhausting the `SessionRouter` map). The 16 MiB /
//!     100 000 events / 5 min idle caps are **per-session** OOM caps
//!     pre-dating ADR-0022, also in `router.rs:168`.

use super::components::{accordion, comparison_table, flow_diagram, metric_grid, source_ref};

/// Render the HTML body for wiki lesson 6.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_06::render();
/// assert!(html.contains("OTLP"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
启用 <code>otlp</code> feature 后，<code>agentprof-storage</code> 内置一个<strong>双栈 OpenTelemetry receiver</strong> —— gRPC 在 <strong>:4317</strong>、HTTP/protobuf 在 <strong>:4318</strong>，原生收 Claude Code / Codex / Copilot CLI / 任何 OTel SDK 的 <code>logs / metrics / traces</code> 三信号，落到 SQLite store。<strong>不需要外置 collector</strong> —— agent 直接 push 到 agentprof，本地循环到「写文件 → 跑 cli」的路径在生产场景里消失。架构由 ADR-0021 钉死，安全硬化由 ADR-0022 钉死。
</p>
"#);

    s.push_str(&metric_grid(&[
        ("Bearer 比较", "subtle::ConstantTimeEq", "防 timing attack"),
        ("单包大小", "8 / 2 / 8 MiB", "logs / metrics / traces 上限"),
        ("Session 上限", "1024", "LRU eviction (M2.4)"),
        ("session.id 长度", "≤ 256 字节", "防 GB-OOM (mapper.rs:521)"),
    ]));

    s.push_str(r#"
<div class="card analogy">
  <div class="tag">📡 类比 — 像 Prometheus 的 <code>remote_write</code> endpoint</div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong>producer</strong>：agent (Claude Code / Codex / Copilot CLI) 的 OTel SDK，按 OTLP 规范 push 三信号。</li>
    <li><strong>wire format</strong>：OTLP — gRPC (binary protobuf) 或 HTTP/protobuf；agentprof 两个都接。</li>
    <li><strong>endpoint</strong>：agentprof <code>serve --ingest-otlp</code>（M2.2 子命令），监听 4317/4318。</li>
    <li><strong>sink</strong>：SQLite — 通过 <code>IngestPipeline</code> → <code>SessionRouter</code> → 同 schema 的 store DB。</li>
  </ul>
  <p style="margin:.6em 0 0;font-size:.92rem;color:var(--muted)">和 Prometheus remote_write 不同的关键点：OTLP 是<strong>长连接 + 流式</strong>（一个 agent session 可能用一条 gRPC stream 跑数十分钟），所以 agentprof 必须维护 per-session buffer 状态 —— 这就是 <code>SessionRouter</code> 的存在意义，也是 ADR-0022 LRU cap 要保护的对象。</p>
</div>

<h2>双栈协议对比（recon 真实端口 / 路径）</h2>
"#);

    s.push_str(&comparison_table(
        &["协议 / 入口", "默认端口 / 路径", "适用场景"],
        &[
            (
                "<strong>gRPC</strong><br><span style=\"font-size:.85rem;color:var(--muted)\">serve_grpc(cfg, pipeline)</span>",
                ":4317（OTel 官方默认）<br>3 个 service：<code>LogsServiceServer</code> / <code>MetricsServiceServer</code> / <code>TraceServiceServer</code>",
                "高吞吐、长连接、流式 — agent SDK 默认首选；防火墙允许 :4317 时用",
            ),
            (
                "<strong>HTTP/protobuf</strong><br><span style=\"font-size:.85rem;color:var(--muted)\">serve_http(cfg, pipeline)</span>",
                ":4318（OTel 官方默认）<br>3 条路由：<code>POST /v1/logs</code> / <code>POST /v1/metrics</code> / <code>POST /v1/traces</code>",
                "穿透只允许 HTTP/HTTPS 的网络环境；和 reverse proxy（nginx / Caddy）友好；OTel SDK 通常 fallback 选这个",
            ),
            (
                "<strong>TLS + Bearer</strong><br><span style=\"font-size:.85rem;color:var(--muted)\">tls.rs + auth.rs</span>",
                "两栈都支持：<code>tls_cert</code> + <code>tls_key</code>（必须成对）+ 可选 <code>tls_client_ca</code>（mTLS）+ <code>listen_token</code>（Bearer）",
                "跨主机 / 生产暴露；本地回环可省。Bearer 单独可用（HTTP 上等于裸传 token，不推荐生产）",
            ),
        ],
    ));

    s.push_str(r#"<p style="font-size:.88rem;color:var(--muted)">⚠️ Recon 校正：HTTP 栈当前<strong>只支持 protobuf body</strong>（没 OTLP/HTTP+JSON 实现路径）。两栈共享同一个 <code>OtlpServerConfig</code> 配置 + 同一个 <code>IngestPipeline</code> 后端，所以混合部署（同时开 gRPC + HTTP）共享 store 状态没问题。</p>"#);

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片：① gRPC vs HTTP 怎么选 · ② ADR-0021 整体架构（含 flow diagram）· ③ ADR-0022 4 层防御逐条拆解。</p>"#);

    // ---------------- Accordion 1: gRPC vs HTTP ----------------
    let card1 = r#"<div class="qa">
<div class="q">🚦 优先选哪个 — gRPC 还是 HTTP？</div>
<div class="a"><strong>默认选 gRPC</strong>（:4317）：序列化效率高、长连接复用、双向流支持，是 OTel 生态的「first-class」transport。<strong>选 HTTP</strong>（:4318）当：① 网络只允许 HTTP/HTTPS（企业代理、严格防火墙）；② 后端要走 reverse proxy 做 TLS 卸载 / load balance；③ OTel SDK 版本太旧没 gRPC 支持。两栈在 agentprof 这边完全等价 —— 同一份 mapper 把 OTLP typed event 转 <code>TypedEvent</code> 进 pipeline。</div>

<div class="q">🔌 真实接口签名</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-storage/src/otlp/server_grpc.rs:84
pub async fn serve_grpc(
    cfg: OtlpServerConfig,
    pipeline: Arc&lt;IngestPipeline&gt;,
) -&gt; Result&lt;GrpcServerHandle, OtlpServerError&gt;;

// crates/agentprof-storage/src/otlp/server_http.rs:96
pub async fn serve_http(
    cfg: OtlpServerConfig,
    pipeline: Arc&lt;IngestPipeline&gt;,
) -&gt; Result&lt;HttpServerHandle, OtlpServerError&gt;;</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem">两个 fn 都返回 <code>JoinHandle</code> + <code>oneshot::Sender&lt;()&gt;</code> 的 graceful-shutdown 句柄；<code>serve --ingest-otlp</code> 同时 spawn 两者，await 任一异常即整体退出。</p>
</div>

<div class="q">🛠️ 为什么不外置 collector？</div>
<div class="a">业界标准做法是「<strong>agent → OTel Collector → backend</strong>」三段式。agentprof 把后两段合并是为了<strong>降低个人 / 小团队的安装摩擦</strong> —— 不需要先装 collector 配 receiver 配 exporter 配 pipeline yaml 才能看 agent 数据。trade-off 是 agentprof 自己要处理一部分 collector 该做的事（per-signal size cap、bearer auth、LRU eviction）—— ADR-0022 就是补这部分的硬化。如果用户已经有 collector，<code>serve --ingest-otlp</code> 也可以当 backend exporter target —— 兼容并存。</div>
</div>"#;
    s.push_str(&accordion(1, "gRPC vs HTTP/protobuf 双栈选择", card1));

    // ---------------- Accordion 2: ADR-0021 architecture ----------------
    let mut card2 = String::new();
    card2.push_str(
        r#"<div class="qa">
<div class="q">🏗️ ADR-0021 整体架构 — 6 层流水线</div>
<div class="a">
"#,
    );
    card2.push_str(&flow_diagram(&[
        "OTel SDK",
        "receiver",
        "router",
        "buffer",
        "flush sink",
        "SQLite",
    ]));
    card2.push_str(r#"
<ol style="margin:.5em 0 0 1.2em;font-size:.92rem">
<li><strong>receiver</strong>（<code>server_grpc</code> / <code>server_http</code>）— 收 OTLP bytes，过 bearer auth / TLS / per-signal size cap，decode 成 protobuf 结构。</li>
<li><strong>mapper</strong>（<code>otlp/mapper.rs</code>）— OTLP <code>ResourceLogs / ResourceMetrics / ResourceSpans</code> → agentprof 的 <code>TypedEvent</code>；从 resource attrs 抽 <code>session.id</code>（截断到 256 byte，<code>mapper.rs:521</code>）。</li>
<li><strong>router</strong>（<code>otlp/router.rs::SessionRouter</code>）— <code>DashMap&lt;SessionId, SessionBuffer&gt;</code>；新 session 创 buffer、已存在的拿现成的；LRU evict 超过 <code>max_open_sessions = 1024</code>。</li>
<li><strong>buffer</strong>（<code>otlp/router.rs::SessionBuffer</code>）— per-session 攒事件；每 buffer 自己有 OOM cap（默认 16 MiB / 100 000 events / 5 min idle）；达到任一阈值触发 flush。</li>
<li><strong>flush sink</strong>（<code>otlp/sink_storage.rs</code>）— buffer 满 / session 结束 / idle 超时 → 序列化整 session 的 events → 喂 agentprof-core 的 analyzer 算 <code>AnalysisReport</code>。</li>
<li><strong>upsert</strong>（<code>Db</code> 写）— <code>INSERT OR REPLACE INTO sessions ... WHERE id = ?</code>；同 schema 同 path 同 dual-path 兼容性。</li>
</ol>
</div>

<div class="q">🧵 为什么用 <code>DashMap</code> 而不是 <code>Mutex&lt;HashMap&gt;</code>？</div>
<div class="a">OTLP receiver 是 high-concurrency 场景 —— 多个 gRPC stream 并发 push，每个 stream 进 router 找自己 session 的 buffer。<code>Mutex&lt;HashMap&gt;</code> 会让全部 stream 串行化，吞吐塌方。<code>DashMap</code> 是 sharded map（默认 64 段），每个 shard 独立 lock，不同 session 完全并行。Tradeoff：iter 不是一致 snapshot（LRU sweep 时要小心），但 agentprof 的 LRU 是 epoch + heap 维护、不依赖 map iter 顺序。</div>

<div class="q">🔁 graceful shutdown 协议</div>
<div class="a">两个 server fn 都返回 <code>(JoinHandle, oneshot::Sender&lt;()&gt;)</code>。cli 收到 SIGINT/SIGTERM → 通过 <code>oneshot::send(())</code> 触发；server 内部 select shutdown signal vs tonic / axum 的 graceful 关闭，等 in-flight request 处理完才退；同时 <code>SessionRouter</code> flush 所有 buffer 落 SQLite —— 不丢已收的 event。这是 cli 退出码 <strong>0</strong>（正常）vs <strong>130</strong>（SIGINT）的关键。</div>
</div>"#);
    s.push_str(&accordion(2, "ADR-0021 整体架构 + 6 层 pipeline", &card2));

    // ---------------- Accordion 3: ADR-0022 4 defenses ----------------
    let card3 = r#"<div class="qa">
<div class="q">🛡️ 防御 1：常时间 Bearer 比较（<code>subtle::ConstantTimeEq</code>）</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-storage/src/otlp/auth.rs
use subtle::ConstantTimeEq;
const BEARER_PREFIX: &amp;str = "Bearer ";

// 字符比较走 ConstantTimeEq -- 不让 attacker 用 timing 探测 token 前缀</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem"><strong>为什么</strong>：naive 的 <code>==</code> short-circuit —— attacker 喂「a」「b」「c」...，agentprof 在错的字节就早退；通过测「拒绝多花了几纳秒」能逐字符暴力。<code>subtle::ConstantTimeEq</code> 对每个字节都走完比较再返结果，时间分布不泄露信息。<strong>不防什么</strong>：长度差异本身可被旁路检测（agentprof 假设攻击者已知 token 长度 —— 通常 32 字节 hex）；这不是问题，密码学社区认可的取舍。</p>
</div>

<div class="q">🛡️ 防御 2：per-signal 消息大小上限（<code>max_decoding_message_size</code> / <code>DefaultBodyLimit</code>）</div>
<div class="a"><strong>问题</strong>：tonic 和 axum 的<strong>缺省</strong>解码限制都很宽（4 MB 量级），但 OTLP 是 protobuf —— 一份 4 MB 的恶意 protobuf 解码后可能膨胀几百 MB（嵌套 list、recursive message）。<strong>方案</strong>：gRPC 侧每个 service 单独 <code>.max_decoding_message_size(cfg.max_*_request_bytes)</code>（在 <code>InterceptedService</code> 之前）；HTTP 侧每条路由单独挂 <code>DefaultBodyLimit::max(...)</code> layer。<strong>关键细节</strong>（<code>server_http.rs</code> 注释里专门标了）：router-level middleware 不能 drain body，否则 413 保护会先 OOM 再触发 —— 加 logger / decompressor 时要把它移到 per-route layer 里。</div>

<div class="q">🛡️ 防御 3：<code>SessionRouter</code> LRU eviction (cap = 1024)</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-storage/src/otlp/router.rs:171
SessionBufferCaps {
    max_bytes:         16 * 1024 * 1024,   // 16 MiB per session
    max_events:        100_000,            // per session
    max_idle:          Duration::from_secs(5 * 60),  // 5 min
    max_open_sessions: 1024,               // 全局 router 容量
}</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem"><strong>问题</strong>：attacker 用大量不同 <code>session.id</code> 发 1 event 然后停 —— 每个 session 都创 buffer，<code>DashMap</code> 无限增长。<strong>方案</strong>：第 1025 个 session 来时，evict 最久未触达的那个 —— 触发 <code>CloseReason::CapacityEvict</code>，flush 它的 buffer 到 SQLite，腾出位。<strong>取舍</strong>：被 evict 的 session 如果之后又有 event 来，会被视作新 session 重新建 buffer —— 极端情况会被切成多段 <code>SessionRef</code>，但数据不丢。1024 这个数字是「单机一天健康活跃 session ≤ 几十、留 20x 余地」拍的，可调。</p>
</div>

<div class="q">🛡️ 防御 4：<code>session.id</code> 256-byte 上限</div>
<div class="a">
<pre class="code"><code>// crates/agentprof-storage/src/otlp/mapper.rs:521
// ADR-0022 D-5: cap session_id at 256 bytes BEFORE allocating
//               the SessionId String -- prevents an attacker
//               from forcing GiB-sized id allocations.</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem"><strong>问题</strong>：OTLP resource attrs 是 string —— attacker 设 <code>session.id</code> 为 1 GB string，agentprof 在 <code>String::from</code> 时就 OOM。<strong>方案</strong>：从 wire bytes 取 id 前 <strong>先看长度</strong>，超 256 直接截断 / reject。256 byte 远超合理 session id（UUID 36 字节、SHA256 64 hex 字符）—— 给 prefix path 也留足空间。这个 cap 在 <code>mapper.rs</code> 里、在 <code>SessionRouter::lookup_or_create</code> 之前，确保<strong>恶意 id 永远进不了 router map</strong>。</p>
</div>

<div class="q">🤔 还有没漏的攻击面？</div>
<div class="a">ADR-0022 还有 D-1 / D-4 等条目处理别的边缘（如 tonic 的 TLS 配置必须成对、TLS provider 安装时机）。<strong>暂未硬化</strong>的有：① per-IP rate limit（依赖前置 reverse proxy）；② OTLP attribute key 长度（理论上 attacker 可塞 1 GB 的 key name 进 resource attrs）—— audit list 里记着，但低优先级，没真实场景。</div>
</div>"#;
    s.push_str(&accordion(3, "ADR-0022 4 层防御逐条拆解", card3));

    s.push_str(r"<h2>下一步</h2>
<p>本课讲清了 OTLP receiver 的双栈、ADR-0021 6 层 pipeline、以及 ADR-0022 4 层防御的真实代码位置。下一课「<strong>Web dashboard 架构</strong>」翻 <code>agentprof serve</code> 的另一面 —— 5 视图 localhost 看板怎么 reuse M2.2 axum 栈、chunk-endpoint pattern 怎么省掉 React/Vue 整个构建链（ADR-0024 D-1..D-7）。</p>
");

    s.push_str(&source_ref(
        "agentprof-storage",
        "otlp/server_grpc.rs",
        "serve_grpc",
    ));
    s.push_str(&source_ref(
        "agentprof-storage",
        "otlp/server_http.rs",
        "serve_http",
    ));
    s.push_str(&source_ref(
        "agentprof-storage",
        "otlp/router.rs",
        "SessionBufferCaps",
    ));

    s
}
