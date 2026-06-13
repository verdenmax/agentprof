//! Usage lesson 2 — 「5 分钟上手」(install).
//!
//! Target audience: 任何想跑出第一张火焰图的新用户。覆盖 3 种安装路径
//! （cargo install / one-line installer / from source）+ 第一次跑
//! `analyze --agent copilot` + 常见报错对应方案。

use super::components::{accordion, comparison_table, decision_tree, source_ref};

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
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
装好 agentprof 比装 IDE 插件还快 —— <strong>3 种装法任选其一</strong>，最后一句 <code>agentprof analyze --agent copilot</code> 就能看到你第一张<strong>火焰图</strong>。整个流程不超过 5 分钟，也<strong>不需要修改你的 agent 配置</strong>。
</p>
"#);

    s.push_str(&decision_tree(
        "你想几分钟内跑起来还是想改源码 / 调试？",
        &[
            (
                "🚀 最快路径",
                "<code>curl -fsSL https://github.com/verdenmax/agentprof/releases/latest/download/agentprof-cli-installer.sh | sh</code> —— 30 秒拉预编译 binary，不需要 Rust toolchain",
            ),
            (
                "📦 想要 sha256 校验",
                "去 <code>github.com/verdenmax/agentprof/releases/latest</code> 手工下 <code>.tar.xz</code> + <code>.sha256</code>，校验后解压到 <code>$PATH</code>",
            ),
            (
                "🛠️ 想改源码 / 跑测试",
                "<code>git clone</code> + <code>cargo install --path crates/agentprof-cli --features full</code>（需要 Rust 1.78+）",
            ),
        ],
    ));

    s.push_str(r#"
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
                "One-line installer ⭐",
                "大多数用户（不需要 Rust）",
                "<pre class=\"code\">curl -fsSL \\\n  https://github.com/verdenmax/agentprof/releases/latest/download/agentprof-cli-installer.sh \\\n  | sh</pre>",
            ),
            (
                "From source",
                "开发者 / 想跑最新 main / 想跑测试",
                "<pre class=\"code\">git clone https://github.com/verdenmax/agentprof \\\n  &amp;&amp; cd agentprof \\\n  &amp;&amp; cargo install --path crates/agentprof-cli \\\n        --features full</pre>",
            ),
            (
                "<code>cargo install agentprof-cli</code>",
                "Rust 用户（暂不可用）",
                "<em>暂未发布到 crates.io</em>，计划随 v0.4 上 publish；当前用上面两种之一",
            ),
        ],
    ));

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 每一种安装方式的细节都在下面的折叠卡片里：<strong>① 命令 · ② 为什么 · ③ agentprof 怎么做 · ④ 其他选择</strong>。装好以后翻到第 3 张卡片跑第一次 <code>analyze</code>。</p>"#);

    s.push_str(&accordion(
        1,
        "「One-line installer」最快路径 ⭐ 推荐",
        r#"<div class="qa">
<div class="q">🧪 命令</div>
<div class="a"><pre class="code">curl -fsSL https://github.com/verdenmax/agentprof/releases/latest/download/agentprof-cli-installer.sh | sh
# 自动检测 OS/arch，拉对应预编译 binary 到 ~/.cargo/bin/agentprof
# Windows PowerShell：
#   irm https://github.com/verdenmax/agentprof/releases/latest/download/agentprof-cli-installer.ps1 | iex</pre></div>
<div class="q">🤔 为什么必要</div>
<div class="a">你<strong>不需要装 Rust toolchain</strong>。脚本走 <a href="https://opensource.axo.dev/cargo-dist/">cargo-dist</a> 发布的 GitHub Release，支持 Linux x86_64 / aarch64、macOS Intel / Apple Silicon、Windows MSVC 共 5 个 target triple。安装时间 &lt; 10 秒。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">CI 在 tag push 时用 <code>cargo dist build</code> 交叉编译并签名打包，每个 binary 自带 <strong>所有默认 feature</strong>（含 <code>full</code>）。SHA256 sums 一并发布到 Release assets 里方便手工校验。<code>latest/download/</code> 会自动跳到含 asset 的最新 release —— 如果当前 tag 还没跑完 release workflow，会回退到上一个有 asset 的版本（详见 <strong>ADR-0014</strong>）。</div>
<div class="q">🔀 其他选择</div>
<div class="a">离线环境 / 想手动校验 sha256 时，直接到 <code>github.com/verdenmax/agentprof/releases</code> 手工下对应 <code>agentprof-cli-&lt;triple&gt;.tar.xz</code> + <code>.sha256</code>，解压放到 <code>$PATH</code>。</div>
</div>"#,
    ));

    s.push_str(&accordion(
        2,
        "「From source」开发者路径",
        r#"<div class="qa">
<div class="q">🧪 命令</div>
<div class="a"><pre class="code">git clone https://github.com/verdenmax/agentprof
cd agentprof
cargo install --path crates/agentprof-cli --features full
# 安装位置：~/.cargo/bin/agentprof
# 确认 PATH： echo $PATH | grep -q "$HOME/.cargo/bin" &amp;&amp; echo OK</pre></div>
<div class="q">🤔 为什么必要 / MSRV</div>
<div class="a">agentprof 用 <strong>Rust 2021 edition + MSRV 1.78</strong> 写成。<code>cargo install --path</code> 比 <code>cargo install agentprof-cli</code> 更稳 —— 后者目前<strong>还没发布到 crates.io</strong>（计划随 v0.4 启动 publish；当前唯一可执行路径是这条）。首次编译 ~2 分钟（带 <code>tiktoken-rs</code> / <code>rusqlite</code> / <code>ratatui</code> 三个稍大依赖），增量编译几秒。</div>
<div class="q">✅ agentprof 怎么做</div>
<div class="a">默认开 <code>full</code> feature，自动带上 <strong>anthropic-api</strong>（直连官方反查 cache 价格）+ <strong>otlp</strong>（接 OpenTelemetry receiver）+ <strong>web</strong>（HTML 报表渲染）+ <strong>tui</strong>。装完即用，不需要再 <code>--features</code>。</div>
<div class="q">🔀 其他选择</div>
<div class="a">只想要核心 analyzer 不要 OTLP / 网页报表 / TUI，可以 <code>cargo install --path crates/agentprof-cli --no-default-features --features core</code> 减小 binary 体积约 30%。一般用户不需要做这个优化。<code>cargo install agentprof-cli</code>（裸 crates.io 路径）暂时<strong>不可用</strong>，等 v0.4 ship 后再启用。</div>
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

    s.push_str(&accordion(
        4,
        "升级 / 卸载 / 切换版本",
        r#"<div class="qa">
<div class="q">⬆️ 升级到最新版</div>
<div class="a"><pre class="code"><span class="cm"># 路径 1（installer 装的）：再跑一次 installer，覆盖旧 binary</span>
curl -fsSL https://github.com/verdenmax/agentprof/releases/latest/download/agentprof-cli-installer.sh | sh

<span class="cm"># 路径 2（from source 装的）：</span>
cd agentprof &amp;&amp; git pull
cargo install --path crates/agentprof-cli --features full --force</pre>
两种路径都安装到 <code>~/.cargo/bin/agentprof</code>，覆盖即升级。升级前可 <code>agentprof --version</code> 看现状，升级后再跑一次确认。<strong>升级不会动你的 SQLite store</strong>（schema migration 是 forward-compatible，新 binary 直读旧 db；详见 <strong>ADR-0019</strong>）。</div>

<div class="q">⏪ 降到旧版本（troubleshoot 用）</div>
<div class="a"><pre class="code"><span class="cm"># 装指定版本（替换 v0.3.2 为你想要的 tag）</span>
curl -fsSL https://github.com/verdenmax/agentprof/releases/download/v0.3.2/agentprof-cli-installer.sh | sh

<span class="cm"># 或从 source 装某个 tag</span>
git checkout v0.3.2
cargo install --path crates/agentprof-cli --features full --force</pre></div>

<div class="q">🗑️ 卸载</div>
<div class="a"><pre class="code"><span class="cm"># installer / cargo install 装的都在这里</span>
rm ~/.cargo/bin/agentprof

<span class="cm"># 可选：删 SQLite cache / store（如果你不想保留分析数据）</span>
rm -rf ~/.cache/agentprof/   <span class="cm"># cache 默认位置 (XDG_CACHE_HOME)</span>
rm -rf ~/.local/share/agentprof/   <span class="cm"># store 默认位置 (XDG_DATA_HOME)</span>

<span class="cm"># 可选：删配置文件</span>
rm -f ~/.config/agentprof/config.toml</pre>
agentprof <strong>不写任何系统级文件</strong>（无 launchd / systemd unit / 注册表条目），所以卸载就是删 4 个用户态路径。</div>

<div class="q">🔀 并行多版本（不推荐但可行）</div>
<div class="a">把不同版本 binary 放在不同路径，rename 区分：<pre class="code">cp ~/.cargo/bin/agentprof ~/.local/bin/agentprof-v032
git checkout v0.3.3 &amp;&amp; cargo install --path crates/agentprof-cli --features full --force
<span class="cm"># 现在 agentprof = v0.3.3, agentprof-v032 = v0.3.2</span></pre>
适合「<strong>对比新旧版本输出</strong>」debug 场景；正常用户应该只装一个。</div>
</div>"#,
    ));

    s.push_str("<h2>下一步</h2>\n<p>装好工具、看到第一张火焰图后，下一课会带你<strong>看懂火焰图</strong>：每个矩形块代表什么、宽度怎么算、为什么 system prompt 经常是最长那条。</p>\n");

    s.push_str(&source_ref("agentprof-cli", "main.rs", "Cli"));
    s.push_str(&source_ref("agentprof-cli", "cmd/analyze.rs", "AnalyzeCmd"));

    s
}
