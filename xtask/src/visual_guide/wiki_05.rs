//! Wiki lesson 5 — 「存储层 hybrid mode」.
//!
//! Walkthrough of `agentprof-storage`: the `Db` handle that owns a
//! `rusqlite::Connection` with all embedded migrations applied, the
//! `StorageMode` enum that picks between cache (`$XDG_CACHE_HOME`)
//! and store (`$XDG_DATA_HOME`) roles, and the dual-path read pattern
//! introduced by ADR-0018 / ADR-0019 / ADR-0020. All table names,
//! column names, default values, and file names are cross-checked
//! against live code at T18 (HEAD `1634ec8`).
//!
//! Recon-confirmed corrections vs. the original brief:
//!
//!   - Schema has **3 tables**, not 3 alternate views: `sessions`,
//!     `tools_loaded`, `turn_buckets`. Migration `002_episodes_column`
//!     adds an `episodes_json TEXT NOT NULL DEFAULT '{}'` column to
//!     `sessions` (not a 4th table).
//!   - The default `SQLite` filename is `cache.sqlite` / `store.sqlite`
//!     (not `cache.db` / `store.db` as the brief suggested).
//!   - "Dual-path" in agentprof historically refers to the **read**
//!     fan-out introduced by ADR-0018 + ADR-0020 (`SessionDataSource`
//!     reads from both `SQLite` and live adapter scans, picking the
//!     fresher of the two by `raw_mtime`), not an OTLP "write to both
//!     cache + store" mode.

use super::components::{accordion, comparison_table, schema_table, source_ref};

/// Render the HTML body for wiki lesson 5.
///
/// Returns a string fragment (no `<html>` / `<body>` wrapper) consumed
/// by `shell::render_page` to produce the final standalone page.
///
/// # Examples
///
/// ```text
/// let html = xtask::visual_guide::wiki_05::render();
/// assert!(html.contains("StorageMode"));
/// ```
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render() -> String {
    let mut s = String::new();

    s.push_str(r#"

<p class="lead">
agentprof 的持久化层只有<strong>一个文件 + 一个 enum</strong>：<code>agentprof_storage::Db</code> 是 <code>rusqlite::Connection</code> 的薄封装，开库时自动跑全部嵌入式 migration；<code>StorageMode</code> 枚举（<code>Cache</code> / <code>Store</code>）决定 SQLite 文件落在 <code>$XDG_CACHE_HOME/agentprof/cache.sqlite</code> 还是 <code>$XDG_DATA_HOME/agentprof/store.sqlite</code>。「hybrid」不是「双 DB 同步」，而是「<strong>同一套 schema、两种生命周期策略</strong>」—— 用户按场景选 mode，agentprof 行为完全一致，只是数据落点不同。
</p>
"#);

    s.push_str(&schema_table(&[
        ("id", "TEXT PRIMARY KEY", "✓", "Session UUID"),
        (
            "agent",
            "TEXT NOT NULL",
            "✓",
            "agent 名（copilot/claude/codex）",
        ),
        ("started_at_ms", "INTEGER", "—", "session 起始 ms epoch"),
        (
            "raw_path",
            "TEXT NOT NULL",
            "✓",
            "原 events.jsonl 路径 or \"otlp://&lt;id&gt;\"",
        ),
        ("raw_mtime_ms", "INTEGER NOT NULL", "✓", "raw_path 的 mtime"),
        (
            "ingested_at_secs",
            "INTEGER NOT NULL",
            "✓",
            "进入 SQLite 时间戳",
        ),
        (
            "analysis_report_json",
            "TEXT NOT NULL",
            "✓",
            "AnalysisReport 序列化",
        ),
        (
            "episodes_json",
            "TEXT NOT NULL DEFAULT '{}'",
            "—",
            "Episodes 序列化（M2.1.1 加列）",
        ),
    ]));

    s.push_str(r#"
<div class="card analogy">
  <div class="tag">🍎 类比 — 像 macOS Time Machine 的 local snapshot 和外接 backup volume</div>
  <ul style="margin:.5em 0 0 1.2em">
    <li><strong>cache mode（默认）</strong> = local snapshot：随系统清理可删；不出现在备份计划里；为了「快、零配置、随时可丢」。</li>
    <li><strong>store mode（显式）</strong> = 外接 backup volume：用户主动挂载；需要保护；长期累积成趋势。</li>
    <li><strong>dual-path 读</strong>（ADR-0018 / ADR-0020）= Time Machine 的「按时间挑最新版本」：<code>SessionDataSource</code> 同时看 SQLite 和 live adapter scan，按 <code>raw_mtime</code> 挑新的。</li>
  </ul>
  <p style="margin:.6em 0 0;font-size:.92rem;color:var(--muted)">「Hybrid」的关键洞察：<strong>schema 一样、code path 一样、只有策略不同</strong>。这样 cli 子命令不需要写两套读写逻辑，只在配置层做选择。</p>
</div>

<h2>三种使用模式对比（recon 真实路径）</h2>
"#);

    s.push_str(&comparison_table(
        &["模式", "何时用", "落点路径 / 数据策略"],
        &[
            (
                "<code>cache</code>（默认）<br><span style=\"font-size:.85rem;color:var(--muted)\">StorageMode::Cache</span>",
                "本地 dev、单次分析、CI 跑完即弃；想要「随时删都没事」",
                "<code>$XDG_CACHE_HOME/agentprof/cache.sqlite</code><br>fallback <code>~/.cache/agentprof/cache.sqlite</code><br><code>auto_prune_days = 30</code>（30 天自动清）",
            ),
            (
                "<code>store</code>（显式）<br><span style=\"font-size:.85rem;color:var(--muted)\">StorageMode::Store</span>",
                "CI 跨 run 累计 / 团队共享 / 长期 7-30 天趋势分析 / OTLP push gateway 接收端",
                "<code>$XDG_DATA_HOME/agentprof/store.sqlite</code><br>fallback <code>~/.local/share/agentprof/store.sqlite</code><br>用户负责备份；<code>--storage-path</code> 可显式覆写",
            ),
            (
                "<code>dual-path</code> 读<br><span style=\"font-size:.85rem;color:var(--muted)\">SessionDataSource (ADR-0018/0020)</span>",
                "<code>list</code> / <code>analyze</code> / <code>mcp-waste</code> 默认走这条 — 既看 SQLite 也看 adapter scan",
                "<strong>不是</strong>「同时写两个 DB」；是「同时<strong>读</strong>两个 source」，按 <code>raw_mtime</code> 选最新；<code>--no-cache</code> 降级为纯 adapter scan",
            ),
        ],
    ));

    s.push_str(r#"<p style="font-size:.88rem;color:var(--muted)">⚠️ Recon 校正：「dual-path」在 agentprof 里指 <strong>read-path 融合</strong>（ADR-0018 + ADR-0020），不是「写到两个 DB」。OTLP receiver 写的是<strong>单一</strong>当前 storage（由 <code>--storage-mode</code> 决定 cache 或 store）。<code>aggregate</code> 子命令暂未接入 dual-path —— 它需要 <code>Episodes</code> 数据，目前由 <code>002_episodes_column</code> 这一列承担。</p>"#);

    s.push_str(r#"<p class="acc-intro" style="color:var(--muted);font-size:.92rem">👇 三张卡片：① SQLite schema 真实 DDL（3 表 + episodes 列）· ② cache vs store 决策维度对比 · ③ ADR-0019 摘要 + dual-path 读路径解释。</p>"#);

    // ---------------- Accordion 1: schema ----------------
    let card1 = r#"<div class="qa">
<div class="q">📋 真实 schema（<code>crates/agentprof-storage/migrations/001_initial.sql</code> + <code>002_episodes_column.sql</code>）</div>
<div class="a">
<pre class="code"><code>-- 001_initial.sql (schema_version = 1)
CREATE TABLE sessions (
    id                    TEXT    PRIMARY KEY,
    agent                 TEXT    NOT NULL,
    dominant_model        TEXT,
    started_at            INTEGER,
    duration_ms           INTEGER,
    raw_path              TEXT NOT NULL,
    raw_mtime             INTEGER NOT NULL,        -- 驱动 dual-path freshness 比较
    total_input_tokens    INTEGER,
    total_output_tokens   INTEGER,
    total_cache_read      INTEGER,
    total_cache_creation  INTEGER,
    schema_version        INTEGER NOT NULL DEFAULT 1,
    ingested_at           INTEGER NOT NULL,
    analysis_report_json  TEXT NOT NULL            -- 完整 AnalysisReport 序列化
);
CREATE INDEX idx_sessions_started       ON sessions(started_at DESC);
CREATE INDEX idx_sessions_agent_started ON sessions(agent, started_at DESC);

CREATE TABLE tools_loaded (
    session_id        TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tool_name         TEXT NOT NULL,
    source            TEXT NOT NULL,       -- Builtin / Mcp / Skill / User / Unknown
    call_count        INTEGER NOT NULL,
    total_duration_ms INTEGER NOT NULL,
    tokens            INTEGER,
    token_source      TEXT,                -- heuristic / tokenizer / config / sidecar
    PRIMARY KEY (session_id, tool_name)
);

CREATE TABLE turn_buckets (
    session_id     TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_index     INTEGER NOT NULL,
    input_tokens   INTEGER,
    output_tokens  INTEGER,
    cache_read     INTEGER,
    cache_creation INTEGER,
    model          TEXT,
    PRIMARY KEY (session_id, turn_index)
);

-- 002_episodes_column.sql (additive migration, M2.1.1)
ALTER TABLE sessions ADD COLUMN episodes_json TEXT NOT NULL DEFAULT '{}';</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem"><strong>开库时自动应用</strong>：<code>Db::open_and_migrate(path)</code> 先设 PRAGMA <code>journal_mode=WAL</code> + <code>synchronous=NORMAL</code> + <code>foreign_keys=ON</code>，再按 <code>MIGRATIONS</code> 数组顺序跑全部 SQL。幂等 —— 重开旧库是 no-op。</p>
</div>

<div class="q">🧮 为什么是 3 表 + 1 大 JSON blob，不是「关系正交化」？</div>
<div class="a"><strong>关系拆细</strong>会膨胀写入路径（一份 session ≈ 50-500 turn + 30-200 tool），SQLite 单事务里大几千 row 的 insert 比一条 <code>analysis_report_json</code> blob 慢 5-10x；<strong>blob 缺点</strong>是不能 SQL 直接 group / filter。折中方案：<strong>关键聚合列</strong>（total tokens、cache read/creation、dominant_model、started_at）单独成列让 <code>list</code> / <code>aggregate</code> 子命令能直接 SQL 查；<strong>per-turn / per-tool 明细</strong>有自己的表给 <code>mcp-waste</code> 用；<strong>完整 AnalysisReport</strong>则 blob 化让 <code>analyze --from-cache</code> 秒级还原渲染。<code>episodes_json</code> 列（M2.1.1 加）是 <code>aggregate</code> 的 escape hatch — 它需要 per-call/per-turn 原始数据。</div>

<div class="q">🔗 索引设计</div>
<div class="a">两个索引覆盖了 99% 的 query pattern：<code>idx_sessions_started</code>（按时间倒序列出最近 N 个 session — <code>list --since 7d</code>）+ <code>idx_sessions_agent_started</code>（同上但限 agent — <code>list --agent claude</code>）。<code>tools_loaded</code> 和 <code>turn_buckets</code> 的复合 PK 已经覆盖按 <code>session_id</code> 的 join，不需要额外索引。</div>
</div>"#;
    s.push_str(&accordion(
        1,
        "SQLite schema 真实 DDL（migration 001 + 002）",
        card1,
    ));

    // ---------------- Accordion 2: cache vs store ----------------
    let card2 = r#"<table style="width:100%;border-collapse:collapse;font-size:.92rem">
<thead><tr style="background:var(--surface)">
<th style="text-align:left;padding:.3em .5em">维度</th>
<th style="text-align:left;padding:.3em .5em">cache (默认)</th>
<th style="text-align:left;padding:.3em .5em">store (显式)</th>
</tr></thead>
<tbody>
<tr><td style="padding:.3em .5em"><strong>XDG var</strong></td><td style="padding:.3em .5em"><code>XDG_CACHE_HOME</code></td><td style="padding:.3em .5em"><code>XDG_DATA_HOME</code></td></tr>
<tr><td style="padding:.3em .5em"><strong>fallback</strong></td><td style="padding:.3em .5em"><code>~/.cache/agentprof/</code></td><td style="padding:.3em .5em"><code>~/.local/share/agentprof/</code></td></tr>
<tr><td style="padding:.3em .5em"><strong>文件名</strong></td><td style="padding:.3em .5em"><code>cache.sqlite</code></td><td style="padding:.3em .5em"><code>store.sqlite</code></td></tr>
<tr><td style="padding:.3em .5em"><strong>auto-prune</strong></td><td style="padding:.3em .5em">默认 30 天（T2.7+）</td><td style="padding:.3em .5em">建议 0（永不自动清，用户管）</td></tr>
<tr><td style="padding:.3em .5em"><strong>能删？</strong></td><td style="padding:.3em .5em">✅ 随便删，下一次 ingest 重建</td><td style="padding:.3em .5em">⚠️ 删了等于丢历史 trend，要先备份</td></tr>
<tr><td style="padding:.3em .5em"><strong>需要备份？</strong></td><td style="padding:.3em .5em">❌ 不要 — cache 应该是 derivative</td><td style="padding:.3em .5em">✅ 用户责任，但 SQLite 文件单一好备</td></tr>
<tr><td style="padding:.3em .5em"><strong>团队共享？</strong></td><td style="padding:.3em .5em">不适合 — 每人本地一份</td><td style="padding:.3em .5em">可放共享 volume，但读为主写要谨慎</td></tr>
<tr><td style="padding:.3em .5em"><strong>OTLP receiver 目标</strong></td><td style="padding:.3em .5em">能用，但更适合 store</td><td style="padding:.3em .5em">✅ 长期 ingest，<code>serve</code> 子命令<strong>要求</strong> store（ADR-0024 D-5）</td></tr>
</tbody>
</table>
<p style="margin:.6em 0 0;font-size:.88rem;color:var(--muted)">决策口诀：「<strong>单人单机 dev → cache；CI 或团队或 OTLP push 或 serve → store</strong>」。从 cache 升级到 store 不需要数据迁移 —— 重新 <code>analyze --storage-mode store</code> 即可（SQLite 文件可以并存）。</p>"#;
    s.push_str(&accordion(2, "cache vs store 决策维度对比", card2));

    // ---------------- Accordion 3: ADR-0019 + dual-path ----------------
    let card3 = r#"<div class="qa">
<div class="q">📜 ADR-0019 摘要 — 为什么不只有一种 mode？</div>
<div class="a">两种用户画像<strong>冲突</strong>：<strong>个人 dogfooder</strong>要「装一次、随时跑、清盘没顾虑」（cache 语义）；<strong>团队 / 多月审计</strong>要「保住每一份 session、自定路径、不被 cron 清掉」（store 语义）。一种 mode 满足不了两边 —— 默认 cache 太激进会让团队丢数据，默认 store 太保守会让个人用户的 disk 越来越大。ADR-0019 的决策：<strong>同一套 schema、同一套 code path，配置层选 mode</strong>。OS 已有 XDG 约定区分 cache vs data，agentprof 借用即可，零教育成本。</div>

<div class="q">🔀 dual-path 读路径（ADR-0018 + ADR-0020）</div>
<div class="a">
<pre class="code"><code>// SessionDataSource 抽象（agentprof-storage::SessionDataSource）
// 关键接口：list_sessions(filter) -&gt; Vec&lt;SessionRef&gt;
//          load_episodes(session_id) -&gt; Episodes
//
// 默认实现 fan out 到两个 source：
//   1. SQLite（如果存在）— 通过 raw_mtime 判断新鲜度
//   2. Live adapter scan — fallback 或 freshness 后备
// 取 union，遇到同 id 时按 raw_mtime 取 newer。</code></pre>
<p style="margin:.4em 0 0;font-size:.92rem">这个设计让用户在<strong>没显式 ingest 的情况下</strong>也能跑 <code>list</code> / <code>analyze</code> —— SQLite 是优化 path，不是必经 path。<code>--no-cache</code> 显式跳过 SQLite I/O，dual-path 就降级成纯 adapter scan（M2.1 给「不想留痕」的 use case 的 escape）。</p>
</div>

<div class="q">📊 哪些 cli 走 dual-path</div>
<div class="a"><strong>走</strong>：<code>list</code>、<code>analyze</code>（隐式）、<code>mcp-waste</code>。<strong>不走（仅 SQLite）</strong>：<code>serve</code>（要求 store mode）、<code>db</code> 子命令（直接 SQL）。<strong>暂未走</strong>：<code>aggregate</code> — 它需要 <code>Episodes</code> 这层数据，当前由 <code>002_episodes_column</code> 列承担，对未 re-ingest 的旧 row 退化为 <code>Episodes::default()</code>（零贡献到 percentile pool）。下一次 ingest 自动补齐。</div>

<div class="q">🔄 Migration 策略</div>
<div class="a"><code>MIGRATIONS</code> 静态数组（<code>db.rs:24</code>）— 按数字前缀顺序、用 <code>rusqlite_migration</code> 跑。新增 migration 永远 <strong>append-only</strong>，加 <code>NOT NULL DEFAULT '...'</code> 这种 additive 改动；<strong>禁止</strong> drop column / rename column。schema_version 列在 <code>sessions</code> 表里保留 — 未来如果 <code>analysis_report_json</code> 反序列化失败，能按 version 走 fallback decoder。</div>

<div class="q">🤔 为什么不用 SeaORM / sqlx 这类 ORM？</div>
<div class="a">agentprof 的 schema 只有 3 表 + 一个 blob，<strong>没有 join 链 &gt; 2 张表的 query</strong>；ORM 引入<strong>编译时 cost</strong>（sqlx 的 compile-time 校验需要 live DB 或 prepared cache）和<strong>运行时 cost</strong>（动态 schema reflection），换来的 type safety 已被 <code>rusqlite</code> + 手写 <code>FromRow</code> + 集成测试覆盖。<code>rusqlite</code> 的 <code>bundled</code> feature 还省了「用户先装 libsqlite」的安装摩擦。</div>
</div>"#;
    s.push_str(&accordion(
        3,
        "ADR-0019 hybrid mode + dual-path 读路径",
        card3,
    ));

    s.push_str(r"<h2>下一步</h2>
<p>本课讲清了 <code>StorageMode</code> 二选一的决策、3 表 schema 的取舍、以及 dual-path 读路径的真实 fan-out 逻辑。下一课「<strong>OTLP receiver</strong>」深入 <code>agentprof serve --ingest-otlp</code> 背后的 gRPC/HTTP 双栈、4 层防御（ADR-0022），以及 receiver → router → buffer → flush sink → upsert 的真实 pipeline。</p>
");

    s.push_str(&source_ref("agentprof-storage", "db.rs", "Db"));
    s.push_str(&source_ref("agentprof-storage", "config.rs", "StorageMode"));

    s
}
