//! Usage lesson 2 — 「5 分钟上手」(install).
//!
//! Target audience: 任何想跑出第一张火焰图的新用户。覆盖 3 种安装路径
//! （cargo install / one-line installer / from source）+ 第一次跑
//! `analyze --agent copilot` + 常见报错对应方案。

use super::components::{accordion, comparison_table, source_ref};

/// Render the HTML body for usage lesson 2.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::usage_02::render();
/// assert!(html.contains("cargo install"));
/// ```
#[must_use]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
装好 agentprof 比装 IDE 插件还快 —— <strong>3 种装法任选其一</strong>，最后一句 <code>agentprof analyze --agent copilot</code> 就能看到你第一张<strong>火焰图</strong>。整个流程不超过 5 分钟，也<strong>不需要修改你的 agent 配置</strong>。
</p>

<div class="card analogy">
  <div class="tag">🔌 生活类比</div>
  把 agentprof 想成 <strong>事后回放的 Wireshark</strong>：
  你不需要在 agent 里埋点、不需要改 prompt、不需要重启服务 ——
  只要 agent CLI 在本地留下了 <code>events.jsonl</code> / <code>session.jsonl</code>，
  agentprof 就能事后把每一次对话「抓包回放」成火焰图 + ROI 表。
  零侵入、零运行时开销、零外发数据。
</div>

<h2>3 种安装方式怎么选？</h2>

<p>按你<strong>有没有 Rust toolchain</strong> + <strong>是不是开发者</strong>分三档，先看一眼对照表再决定：</p>
"#);

    s.push_str(&comparison_table(
        &["安装方式", "适用人群", "命令"],
        &[
            (
                "cargo install",
                "已有 Rust toolchain（cargo ≥ 1.78）",
                "<pre class=\"code\">cargo install agentprof-cli</pre>",
            ),
            (
                "One-line installer",
                "大多数用户（不写 Rust）",
                "<pre class=\"code\">curl -fsSL https://agentprof.dev/install.sh | sh</pre>",
            ),
            (
                "From source",
                "开发者 / 想改代码 / 想跑最新 main",
                "<pre class=\"code\">git clone https://github.com/verdenmax/agentprof \\\n  &amp;&amp; cd agentprof \\\n  &amp;&amp; cargo build --release \\\n        -p agentprof-cli --features full</pre>",
            ),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 每一种安装方式的细节都在下面的折叠卡片里：<strong>① 命令 · ② 为什么 · ③ agentprof 怎么做 · ④ 其他选择</strong>。装好以后翻到第 3 张卡片跑第一次 <code>analyze</code>。</p>"#);

    s.push_str(&accordion(
        1,
        "「cargo install」最快路径",
        r#"<div class="qa">
<div class="q">🧪 命令</div>
<div class="a"><pre class="code">cargo install agentprof-cli
# 安装位置：~/.cargo/bin/agentprof
# 确认 PATH： echo $PATH | grep -q "$HOME/.cargo/bin" &amp;&amp; echo OK</pre></div>
<div class="q">🤔 为什么必要 / MSRV</div>
<div class="a">agentprof 用 <strong>Rust 2021 edition + MSRV 1.78</strong> 写成，<code>cargo install</code> 会直接从 crates.io 拉最新 release 在本机编译。第一次编译 ~2 分钟（带 <code>tiktoken-rs</code> / <code>rusqlite</code> / <code>ratatui</code> 三个稍大的依赖），之后增量编译几秒。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">默认开 <code>full</code> feature，自动带上 <strong>anthropic-api</strong>（直连官方反查 cache 价格）+ <strong>otlp</strong>（接 OpenTelemetry receiver）+ <strong>web</strong>（HTML 报表渲染）。装完即用，不需要再 <code>--features</code>。</div>
<div class="q">🔀 其他选择</div>
<div class="a">如果你只想要核心 analyzer，不想要 OTLP / 网页报表，可以 <code>cargo install agentprof-cli --no-default-features --features core</code> 减小 binary 体积约 30%。但一般用户不需要做这个优化。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "「one-line installer」预编译 binary",
        r#"<div class="qa">
<div class="q">🧪 命令</div>
<div class="a"><pre class="code">curl -fsSL https://agentprof.dev/install.sh | sh
# 自动检测 OS/arch，拉对应预编译 binary 到 /usr/local/bin/agentprof
# 也支持： powershell -c "iwr https://agentprof.dev/install.ps1 | iex"</pre></div>
<div class="q">🤔 为什么必要</div>
<div class="a">你<strong>不需要装 Rust toolchain</strong>。脚本走 <a href="https://opensource.axo.dev/cargo-dist/">cargo-dist</a> 发布的 GitHub Release，支持 Linux x86_64 / aarch64、macOS Intel / Apple Silicon、Windows MSVC 共 5 个 target triple。安装时间 &lt; 10 秒。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">CI 在 release 时用 <code>cargo dist build</code> 交叉编译并签名打包，每个 binary 自带 <strong>所有默认 feature</strong>（含 full）。SHA256 sums 一并发布到 Release assets 里方便手工校验。</div>
<div class="q">🔀 其他选择</div>
<div class="a">如果离线环境不能 curl，可以直接到 <code>github.com/verdenmax/agentprof/releases</code> 手工下对应 <code>agentprof-x86_64-unknown-linux-gnu.tar.xz</code> 解压放到 <code>$PATH</code>。需要某个 cargo-dist 没打包进去的 feature 时回退到 <code>cargo install</code> 路径。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        3,
        "第一次跑 — analyze --agent copilot",
        r#"<div class="qa">
<div class="q">🧪 命令</div>
<div class="a"><pre class="code">agentprof analyze --agent copilot
# 默认行为：
#   1. 扫描 ~/.copilot/session-state/  找最新一条 session
#   2. 解析 events.jsonl  生成 Episodes
#   3. TUI 打开火焰图 + Turn Summary + Tool Rank 三个 tab</pre></div>
<div class="q">🤔 为什么必要</div>
<div class="a"><strong>不需要传任何参数</strong>是设计目标：第一次跑就能看到东西，建立"这工具有用"的直觉。指定 session / 时间窗口 / 导出格式都是<strong>第二步</strong>的事。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">CLI 入口在 <code>agentprof-cli::Cli</code>（clap derive）。<code>--agent copilot</code> 会调用 <code>agentprof-adapters::copilot</code> 解析；想跑 Claude Code 用 <code>--agent claude</code>（默认扫 <code>~/.claude/projects/</code>），Codex 用 <code>--agent codex</code>（扫 <code>~/.codex/</code>）。想导出 Markdown 报表加 <code>--export md</code>，HTML 加 <code>--export html</code>。</div>
<div class="q">⚠️ 常见报错对应方案</div>
<div class="a"><pre class="code">Error: no session found under ~/.copilot/session-state/
# → 还没用过 Copilot CLI 跑过对话。先跑一句 `copilot "hello"`，
#   产生第一条 session，再回来 analyze。

Error: failed to parse session abc-123 at /home/me/.copilot/.../events.jsonl
# → events.jsonl 损坏（通常是断电中断写入）。跳过这条用
#   `agentprof list --agent copilot --since 7d` 看其他可用 session。

Error: agent 'copilot' not registered
# → adapters 没编进 binary。从 source 装时漏了 default feature，
#   重新 `cargo install agentprof-cli` 即可（默认会带 copilot adapter）。</pre></div>
</div>"#,
    ));

    s.push_str("<h2>下一步</h2>\n<p>装好工具、看到第一张火焰图后，下一课会带你<strong>看懂火焰图</strong>：每个矩形块代表什么、宽度怎么算、为什么 system prompt 经常是最长那条。</p>\n");

    s.push_str(&source_ref("agentprof-cli", "main.rs", "Cli"));
    s.push_str(&source_ref("agentprof-cli", "cmd/analyze.rs", "AnalyzeCmd"));

    s
}
