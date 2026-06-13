//! Wiki lesson 3 — 「Adapter trait + 怎么写新 adapter」.
//!
//! Contributor-facing walkthrough of `agentprof_core::adapter::Adapter`,
//! the `AgentKind` enum, the shipped `CopilotAdapter`, and the 6-step
//! checklist for adding a Claude / Codex adapter in M3.1 / M3.2.
//! All type names, method signatures, and file paths cross-checked
//! against the live crates at T16 (HEAD `b59150f`).

use super::components::{accordion, comparison_table, source_ref};

/// Render the HTML body for wiki lesson 3.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_03::render();
/// assert!(html.contains("Adapter"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"
<h1>Adapter trait + 怎么写新 adapter</h1>

<p class="lead">
agentprof 支持多 agent CLI 的关键抽象是 <code>agentprof_core::adapter::Adapter</code> trait — 每个 agent（Copilot CLI / Claude Code / OpenAI Codex）把它的 session 日志格式实现成一个 adapter。<strong>Copilot 已 ship（M1.2）</strong>，Claude / Codex 在 <code>AgentKind</code> 中保留位置但 adapter 尚未接入（M3.1 / M3.2 路线图）。<code>agentprof-core</code> 完全不知道任何 agent 的具体文件格式 — 它只接受 <code>Event</code> trait 流，分层关注分离。
</p>

<div class="card analogy">
  <div class="tag">🔌 工程类比 — 像数据库 ODBC driver / 浏览器 codec</div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong>Adapter = driver</strong>：一套统一接口，每个供应商写自己的实现 — Postgres / MySQL / SQLite 都给 ODBC 提供 driver。</li>
    <li><strong>core = query 层</strong>：上游 analyzer / TUI / CLI 跨 driver 完全通用，换 agent CLI = 换 driver，不动 query 代码。</li>
    <li>关键约束：driver 永远<strong>向上吐数据</strong>，不调用 query 层 — 否则依赖图反向，破坏分层。</li>
  </ul>
  <p style="margin:.6em 0 0;font-size:.92rem;color:var(--muted)">同理，<code>Adapter</code> 永远不返回 <code>AnalysisReport</code>（那是 analyzer 的活）— 它只产 <code>RawSession&lt;Self::Event&gt;</code>。</p>
</div>

<h2>3 个 agent 的当前状态</h2>
"#);

    s.push_str(&comparison_table(
        &["Agent", "当前状态", "数据源 / 接入路径"],
        &[
            (
                "<strong>GitHub Copilot CLI</strong>",
                "✅ ship (M1.2) — <code>CopilotAdapter</code>",
                "<code>~/.copilot/session-state/&lt;uuid&gt;/events.jsonl</code>（流式 jsonl）",
            ),
            (
                "<strong>Anthropic Claude Code</strong>",
                "🚧 planned (M3.1) — <code>AgentKind::Claude</code> 已占位",
                "<code>~/.claude/projects/&lt;hash&gt;/&lt;uuid&gt;.jsonl</code> 或 OTel push（已有 <code>ingest-otlp</code> M2.4）",
            ),
            (
                "<strong>OpenAI Codex CLI</strong>",
                "🚧 planned (M3.2) — <code>AgentKind::Codex</code> 已占位",
                "<code>~/.codex/sessions/...</code> 或 OTel push",
            ),
        ],
    ));

    s.push_str(r#"<p style="font-size:.88rem;color:var(--muted)">M3.1/M3.2 之前 <code>registry::adapter_for(AgentKind::Claude)</code> 返回 <code>None</code>，CLI 会输出 <em>"agent not supported yet"</em> 提示。<code>AgentKind</code> 是 <code>#[non_exhaustive]</code> enum，新增 variant 不破坏 SemVer。</p>"#);

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片：① <code>Adapter</code> trait 的真实签名（4 方法 + 1 associated type）· ② <code>CopilotAdapter</code> 案例（文件结构 + 流式解析）· ③ 写新 adapter 的 6 步清单（M3.1 ClaudeAdapter 入门）。</p>"#);

    s.push_str(&accordion(
        1,
        "Adapter trait 接口（真实签名）",
        r#"<div class="qa">
<div class="q">📐 类型定义（<code>crates/agentprof-core/src/adapter.rs:608</code>）</div>
<div class="a">
<pre class="code"><code>pub trait Adapter: Send + Sync {
    /// Adapter-specific event enum (must implement core::adapter::Event).
    type Event: Event + serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug;

    fn agent_kind(&amp;self) -&gt; AgentKind;

    fn default_session_root(&amp;self) -&gt; Option&lt;PathBuf&gt;;

    fn discover_sessions(&amp;self, root: &amp;Path) -&gt; Result&lt;Vec&lt;SessionRef&gt;, AdapterError&gt;;

    fn load_session(&amp;self, sref: &amp;SessionRef) -&gt; Result&lt;RawSession&lt;Self::Event&gt;, AdapterError&gt;;
}</code></pre>
<ul style="margin:.5em 0 0 1.2em">
<li><strong><code>type Event</code></strong> — 关联类型，每 adapter 定义自己具体的 event enum（如 <code>CopilotEvent</code>），它必须实现 <code>Event</code> trait 并能 serde 双向。</li>
<li><strong><code>agent_kind()</code></strong> — 返回 <code>AgentKind</code>（Copilot / Claude / Codex / future），让上层判别。</li>
<li><strong><code>default_session_root()</code></strong> — 该 agent 默认日志目录（<code>None</code> 表示该平台没默认）。CLI 用它 fall back 当用户没传 <code>--path</code>。</li>
<li><strong><code>discover_sessions(root)</code></strong> — 遍历目录，返回 <code>Vec&lt;SessionRef&gt;</code>（轻量索引，不解析 payload）。错误：<code>AdapterError::RootNotFound</code> / <code>AdapterError::Io</code>。</li>
<li><strong><code>load_session(sref)</code></strong> — 真正读 + 解析单个 session 为 <code>RawSession&lt;Self::Event&gt;</code>。错误：<code>AdapterError::MissingSessionStart</code> / <code>AdapterError::UnsupportedVersion</code>。</li>
</ul>
</div>
<div class="q">🚫 关键：trait <strong>不</strong>返回 <code>AnalysisReport</code></div>
<div class="a"><code>Adapter</code> 的责任停在 <code>RawSession&lt;Self::Event&gt;</code>（事件 + meta），后续 <code>derive_episodes()</code> 和 <code>analyze()</code> 是 <code>agentprof-core</code> 自己的活。这条规约由 L1 §3 dependency rule 强制 — <code>core</code> 不依赖 <code>adapters</code>。</div>
<div class="q">🤔 为什么</div>
<div class="a">每个 agent 的 session 文件格式天差地别（Copilot 是 <code>events.jsonl</code>，Claude 是 <code>session.jsonl</code> 嵌套 message，Codex 又另一套），但它们抽象到 <strong>「lifecycle / message / tool / hook」</strong>四类事件后高度同构。trait + 关联类型 <code>Event</code> 让 adapter 自由定义具体 enum，而 analyzer 只 iterate <code>EventKind</code> 通用 view。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">Adapter trait 在 <code>agentprof-core</code> 定义；具体实现在 <code>agentprof-adapters</code>（per-agent 子模块）；CLI 通过 <code>registry::adapter_for(kind)</code> dispatch。事件 schema 不同的部分（如 Copilot 的 <code>code.changes</code>）走具体 enum 的字段，统一关切（kind / timestamp / parent）走 <code>Event</code> trait 方法。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>让 adapter 直接吐 <code>AnalysisReport</code></strong> — 实现门槛最低，但 <code>core</code> 反而要依赖 <code>adapters</code>（因为 report 是 core 的类型），破坏依赖图，且每个 adapter 都得重写 rollup 逻辑；<strong>把所有 agent 的 event 塞同一个 enum</strong> — 字段乘积爆炸，无关 agent 的字段污染 schema。trait + 关联类型是「类型安全 + 跨 agent 复用 analyzer」的平衡。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "CopilotAdapter 案例（M1.2 已 ship）",
        r#"<div class="qa">
<div class="q">📁 文件结构</div>
<div class="a">
<pre class="code"><code>crates/agentprof-adapters/src/copilot/
├── mod.rs            // re-exports (CopilotAdapter, CopilotEvent, ...)
├── adapter.rs        // impl Adapter for CopilotAdapter
├── event.rs          // CopilotEvent enum + payload structs
├── parser.rs         // events.jsonl 流式解析
├── paths.rs          // ~/.copilot/session-state 路径约定
├── mcp_config.rs     // 读 MCP 配置（loaded_mcp_tools）
├── tool_sidecar.rs   // tool execution sidecar 解析
└── tools_changed.rs  // tool 集合变化事件</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem;color:var(--muted)">单元结构体 <code>pub struct CopilotAdapter;</code> 定义在 <code>adapter.rs:27</code>，从 <code>copilot/mod.rs</code> 通过 <code>pub use adapter::CopilotAdapter;</code> 暴露。</p>
</div>
<div class="q">🔧 关键实现要点</div>
<div class="a">
<ul style="margin:.3em 0 0 1.2em">
<li><strong><code>agent_kind()</code></strong> → <code>AgentKind::Copilot</code>（常量）。</li>
<li><strong><code>default_session_root()</code></strong> → <code>~/.copilot/session-state</code>（用 <code>dirs::home_dir()</code> 拼接，Windows / macOS 兼容）。</li>
<li><strong><code>discover_sessions()</code></strong> → 遍历 <code>root/&lt;uuid&gt;/</code>，每个子目录里有 <code>events.jsonl</code> 即视为一个 session；<code>SessionRef</code> 含路径 + 文件 mtime（用于 list 排序）。</li>
<li><strong><code>load_session()</code></strong> → 调 <code>parser::parse_events_jsonl()</code> 流式读 jsonl，每行 <code>serde_json::from_str::&lt;CopilotEvent&gt;()</code>；解析失败行不 crash，产 <code>ParseWarning</code> 并继续（lenient — 与 episode derive 一致）。</li>
<li><strong><code>type Event = CopilotEvent</code></strong>，其 <code>EventKind</code> 映射在 <code>event.rs</code>：<code>"session.info"</code> → <code>EventKind::SessionInfo</code>、<code>"tool.exec.start"</code> → <code>EventKind::ToolExecStart</code>、未识别 type 落入 <code>EventKind::Unknown</code> 兜底。</li>
</ul>
</div>
<div class="q">📋 registry 注册（<code>crates/agentprof-adapters/src/registry.rs</code>）</div>
<div class="a">
<pre class="code"><code>pub const fn adapter_for(kind: AgentKind) -&gt; Option&lt;CopilotAdapter&gt; {
    match kind {
        AgentKind::Copilot =&gt; Some(CopilotAdapter),
        _ =&gt; None,  // Claude / Codex 暂未接入
    }
}

pub const fn supported_agents() -&gt; &amp;'static [AgentKind] {
    &amp;[AgentKind::Copilot]
}</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem;color:var(--muted)">⚠️ 当前签名是 <code>Option&lt;CopilotAdapter&gt;</code>（concrete 类型），<strong>一旦第二个 adapter（M3.1 Claude）ship，签名必须改</strong> — 候选方案：<code>Option&lt;AnyAdapter&gt;</code> enum 或 trait-object 擦除。registry.rs 顶部 doc 已标记 TODO 等下一份 adapter-layer ADR。</p>
</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "怎么写新 adapter — 6 步清单（M3.1 ClaudeAdapter 入门）",
        r#"<div class="qa">
<div class="q">📝 6 步</div>
<div class="a">
<ol style="margin:.3em 0 0 1.5em">
<li><strong>实现 trait</strong>：新建 <code>crates/agentprof-adapters/src/claude/</code>（建议 mod 拆 <code>adapter.rs</code> / <code>event.rs</code> / <code>parser.rs</code> / <code>paths.rs</code>，照搬 copilot 结构）；<code>pub struct ClaudeAdapter;</code> + <code>impl Adapter for ClaudeAdapter</code> 实现 4 方法 + 关联类型 <code>type Event = ClaudeEvent</code>。</li>
<li><strong>修 <code>registry.rs</code></strong>：把 <code>adapter_for</code> 的返回类型从 <code>Option&lt;CopilotAdapter&gt;</code> 改为类型擦除形式（<strong>这是 M3.1 真正的设计决策</strong>，要走 brainstorming → ADR → plan 三阶段；当前 registry 头部 doc 已标 TODO）；同时 <code>supported_agents()</code> 加入 <code>AgentKind::Claude</code>。</li>
<li><strong>fixture</strong>：<code>crates/agentprof-adapters/tests/fixtures/claude/</code> 放至少 1 个匿名化过的 <code>session.jsonl</code>；用户 prompt / file path / API key 等敏感字段必须替换为 <code>&lt;redacted&gt;</code> 占位（参考 <code>xtask anonymize</code> 子命令的未来计划，或手动 sed）。</li>
<li><strong>集成测试</strong>：<code>crates/agentprof-adapters/tests/claude.rs</code> 至少 1 个 <code>assert_cmd</code> case：<code>agentprof analyze --agent claude --path tests/fixtures/claude --export json</code> 应返回 exit 0 且 JSON 含至少 1 个 <code>tool_rank</code> 行。</li>
<li><strong>文档</strong>：更新 <code>docs/adapters.md</code>（L2 adapter 指南）+ <code>crates/agentprof-adapters/README.md</code>（"支持的 agent"段）；新 adapter mod 顶部写 <code>//!</code> 模块文档；公开类型加 <code># Examples</code>（否则 CI <code>missing_docs</code> 报错）。</li>
<li><strong>CHANGELOG</strong>：<code>CHANGELOG.md</code> <code>[Unreleased]</code> 段下 <code>### Added</code> 加 entry：<code>feat(adapters): add ClaudeAdapter for ~/.claude/projects/**/*.jsonl</code>；Conventional Commits 风格。</li>
</ol>
</div>
<div class="q">⚠️ 注意点</div>
<div class="a">
<ul style="margin:.3em 0 0 1.2em">
<li><strong><code>EventKind</code> 是 <code>#[non_exhaustive]</code></strong>（<code>adapter.rs:107</code>）— 如果 Claude 有 Copilot 没有的事件类型（如 Claude 特有的 <code>"thinking"</code> 块），可直接在 <code>EventKind</code> 加 variant，不破坏 SemVer。</li>
<li><strong><code>AgentKind</code> 也是 <code>#[non_exhaustive]</code></strong>，但加 variant 时要同步更新 <code>crates/agentprof-storage/src/query.rs:167</code> 的 <code>fn parse_agent(s: &amp;str) -&gt; AgentKind</code>（从 DB 字符串还原 enum），漏改会导致 <code>list</code> / <code>aggregate</code> 跨 session 查询时该 agent 显示为 <code>Unknown</code>。</li>
<li><strong>lenient 解析</strong>：所有解析失败必须收集成 <code>ParseWarning</code> 入 <code>RawSession::parse_warnings</code>，<strong>不</strong>能 <code>panic!</code>，也<strong>不</strong>建议 fail-fast 整 session（参考 ADR-0004 决策）。</li>
<li><strong>错误信息要带 session id + 文件路径 + 修复建议</strong>（L1 §7 编码规约 7）：<code>"failed to parse claude session abc-123 at /home/me/.claude/.../session.jsonl: ...; try `agentprof config show` to verify path"</code>。</li>
<li><strong>禁止 <code>unwrap()</code></strong>（clippy <code>unwrap_used = "deny"</code>）；<code>expect()</code> 只允许在 <code>main.rs</code> / <code>#[cfg(test)]</code>。</li>
</ul>
</div>
<div class="q">🤔 为什么 6 步缺一不可</div>
<div class="a">缺 fixture → CI 没法 regression test；缺集成测试 → 只测得了 parser、测不到 CLI 链路；缺文档 → 用户不知道 <code>--agent claude</code> 怎么用；缺 CHANGELOG → release notes 漂移。<strong>每一步都对应 L1 §9.1 「加新东西的菜谱」</strong>。</div>
<div class="q">🔀 其他选择</div>
<div class="a"><strong>把 Claude / Codex 都塞 <code>CopilotAdapter</code> 用 if-else 分流</strong> — 短期最快但 <code>CopilotAdapter</code> 名字立刻撒谎，且 trait 关联类型只能绑一个 event enum；<strong>用插件机制 dylib 动态加载</strong> — 跨平台 ABI 噩梦，且 Rust 没稳定的 trait-object plugin ABI。<strong>「per-agent struct + registry dispatch」</strong>是 Rust 生态对这类「同接口多实现」最 idiomatic 的答案。</div>
</div>"#,
    ));

    s.push_str(r"<h2>下一步</h2>
<p>本课讲清了 <code>Adapter</code> trait 接口、<code>CopilotAdapter</code> 案例和写新 adapter 的 6 步规约。下一课「<strong>Tokenizer 与 token 计数</strong>」拆解 <code>agentprof-core::tokenizer</code> — tiktoken-rs 怎么挑模型、为什么 cache token 单独计数、ROI 公式为何用 <code>total_tokens</code> 而非 <code>input_tokens</code>。</p>
");

    s.push_str(&source_ref("agentprof-core", "adapter.rs", "Adapter"));
    s.push_str(&source_ref(
        "agentprof-adapters",
        "registry.rs",
        "adapter_for",
    ));
    s.push_str(&source_ref(
        "agentprof-adapters",
        "copilot/adapter.rs",
        "CopilotAdapter",
    ));

    s
}
