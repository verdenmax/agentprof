# Copilot Instructions —— agentprof

> 给 GitHub Copilot / Copilot Agent / 其他 AI assistants 的项目指南。
> 人类开发者也欢迎读。**任何与本文档冲突的回答都是错的**——先读这里再动手。

---

## 0. 必读上下文

在写任何代码或回答任何技术问题之前，**必须读完**这三份文档：

1. [`docs/plan.md`](../docs/plan.md) —— 项目要解决的问题、市场现状、路线图（**为什么做**）
2. [`docs/architecture.md`](../docs/architecture.md) —— **L1 架构**：分层、crate 边界、数据流、协议、规约（**怎么做**）
3. 当前要改的 crate 的 `crates/<name>/README.md` —— **L2 功能文档**（**这块代码做什么**）

如果对架构有疑问，**永远以 `docs/architecture.md` 为准**。本文档是它的"AI agent 友好的精简版 + 强制规则"。

---

## 1. 项目一句话

**agentprof** = 给 AI agent 用的 perf flamegraph + ROI 报告器。读 Claude / Codex / Copilot CLI 留下的 session 日志，算清楚 context window 里每类 token 占比，标出"加载了但从没被调用"的 tool。市面同类工具只算"花了多少 token"，本项目算"**花得值不值**"。

---

## 2. 技术栈（不要换）

- **语言**：Rust 2021，MSRV **1.78**
- **工程组织**：Cargo workspace，5 lib/bin crate + 1 个 `xtask`
- **关键 crate**：`ratatui` + `crossterm`（TUI）/ `tiktoken-rs`（tokenizer）/ `rusqlite` bundled（存储）/ `clap` derive（CLI）/ `serde` + `serde_json`（解析）/ `askama`（HTML 模板）/ `thiserror`（lib 错误）/ `anyhow`（bin 错误）/ `tracing`（日志）/ `tokio`（仅 storage/OTLP/Anthropic API 使用）
- **测试**：`assert_cmd` + `predicates`（集成）/ `insta`（snapshot）/ 可选 `proptest`
- **CI**：GitHub Actions，`cargo fmt` + `cargo clippy -D warnings` + `cargo test --all-features` + `cargo deny` + `cargo doc -Dwarnings` + `docs-sync`
- **Release**：`cargo-dist` 多平台 binary + 预留 `cargo publish`
- **License**：MIT OR Apache-2.0（双协议）

---

## 3. Crate 边界（背下来）

```
agentprof-cli  ──▶  agentprof-tui
       │                │
       ├──────────────▶ agentprof-adapters ──▶ agentprof-core
       │                                          ▲
       └──▶ agentprof-storage ───────────────────┘
```

| Crate | 类型 | 职责 |
|---|---|---|
| `agentprof-core` | lib | model + tokenizer + analyzer + export。**不依赖任何 workspace crate** |
| `agentprof-adapters` | lib | Claude/Codex/Copilot 三家日志解析，实现 `Adapter` trait |
| `agentprof-storage` | lib | SQLite 持久化 + OTLP receiver（feature gated） |
| `agentprof-tui` | lib | ratatui 视图（火焰图 / ROI 表 / 聚合视图） |
| `agentprof-cli` | bin (`agentprof`) | 唯一的组装层，CLI 子命令、配置、HTML 模板。**不允许 lib crate 依赖它** |
| `xtask` | bin | 构建辅助（anonymize 日志、release 校验） |

**规则**：
- `agentprof-core` 是依赖图的**叶子**——它绝不能 `use agentprof_adapters::...` 或任何 workspace crate
- CLI 子命令的逻辑**只能**放在 `agentprof-cli`，不准下沉到 lib
- 跨 crate 通信走 `agentprof-core` 里定义的 trait（如 `Adapter`），不允许 lib 之间直接耦合

---

## 4. 三级文档体系（L1 / L2 / L3）—— 边写代码边写文档

> 这是本项目最重要的工程纪律之一。AI 最常违反的就是"只写代码不更文档"。

### 4.1 三级范围

| 级别 | 范围 | 存放位置 | 写作时机 |
|---|---|---|---|
| **L1** | 全局架构、分层、依赖图、CLI 协议、数据流、关键规约 | `docs/architecture.md`、`docs/plan.md` | 架构变动时**同 commit**更新 |
| **L2** | crate / 模块 / 跨 crate feature 级 | `crates/<name>/README.md`（每个 crate 必有）、`docs/features/<feature>.md`、`docs/adapters.md` | 新建/重命名 crate / 新增 feature 时**同 PR**写 |
| **L3** | 函数 / 类型 / 算法 / 决策的具体细节 | **rustdoc**（`///` + `# Examples` + `# Errors` + `# Panics`） + `docs/internals/<topic>.md`（ADR / 算法笔记） | 与代码**同 commit** |

### 4.2 触发表（什么改动 → 必须更新哪些文档）

| 你做的改动 | 必须同 PR 更新的文档 |
|---|---|
| 新建/删除/重命名 crate | L1（架构图、依赖图、crate 一览） + L2 `crates/<name>/README.md` |
| 新建/删除模块（`pub mod`） | 对应 crate 的 L2 README + 模块顶部 `//!` |
| 新增 `pub fn` / `pub struct` / `pub trait` | L3 rustdoc（**包含 `# Examples`**）；必要时 L2 的"对外接口"段 |
| 修改公开 API 签名或语义 | L3 rustdoc + `CHANGELOG.md`（破坏性变更用 `BREAKING:` 前缀） |
| 新增/修改算法（analyzer / tokenizer / waste） | L3 rustdoc 解释 *what* + `docs/internals/<topic>.md` 解释 *why* |
| 新增/删除 CLI 子命令或参数 | L1（§8）+ L2（`agentprof-cli/README.md`）+ L3 rustdoc + 根 `README.md` |
| 新增/修改 SQLite migration | L1（§9）+ L2（`agentprof-storage/README.md`） |
| 新增/修改配置字段 | L1（§10）+ L2（`agentprof-cli/README.md`） |
| 新增 adapter | L1（§6）+ `crates/agentprof-adapters/src/<name>.rs` 顶部 `//!` + `docs/adapters.md` + ≥1 fixture |

### 4.3 L3 rustdoc 最低形态（不达标 → CI fail）

```rust
/// 一行简介，动词开头。
///
/// 多行展开：语义、副作用、与相邻 API 的关系。
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::compute_roi;
/// // 必须能 `cargo test --doc` 通过
/// ```
///
/// # Errors
///
/// 列出哪些错误变体会被返回，及触发条件。
///
/// # Panics
///
/// 如果会 panic，必须明确说明。默认不应 panic。
pub fn compute_roi(/* ... */) -> Result<Vec<RoiRow>, CoreError> { /* ... */ }
```

### 4.4 L2 `crates/<name>/README.md` 必备段落

```
# <crate-name>
> 一句话定位（做什么、不做什么）

## 在 agentprof 架构中的位置（链接到 architecture.md 相应小节）
## 对外接口（链接到 rustdoc）+ 典型用法（最短 Rust 代码片段）
## 模块（mod）一览（表格）
## Features（表格）
## 依赖（workspace 内 / 外部关键）
## 测试与本地命令
## 变更历史（指向 CHANGELOG）
```

### 4.5 反模式（**禁止**）

- ❌ 改了 `pub fn` 签名但没改 rustdoc / CHANGELOG
- ❌ 新建 crate 但没建 `README.md`
- ❌ 用 `docs/internals/*.md` 写"接口文档"——内部细节归 internals，接口归 rustdoc 或 L2 README
- ❌ 在 L1 architecture.md 写具体函数的实现细节——这是 L3 rustdoc 的事
- ❌ 留 `TODO: 待补文档`、`// TODO: write docs` ——文档必须在合并前补齐

---

## 5. 工作流（每个 feature / bug-fix 都走这个）

1. **写 spec**（如果是新 feature）：`docs/superpowers/specs/YYYY-MM-DD-<topic>.md`，列出决策点 / API 草案 / 测试列表
2. **写 failing test**（TDD）：先有红，再写绿
3. **写实现 + L3 rustdoc**（**同一 commit**）
4. 如引入新模块/feature/crate → **同一 PR** 更新 L2 README + L1 `architecture.md`
5. PR 描述里列 "动了哪些文档"，reviewer 用清单核对
6. 合并前所有 CI job 必须绿（含 `docs-sync`）

---

## 6. 关键编码规约（违反 = CI fail）

1. **错误模型分层**：lib crate 用 `thiserror` 定义强类型错误（`CoreError` / `AdapterError` 等）；bin（cli）用 `anyhow::Result<()>`。**lib 里出现 `anyhow` → fail**。
2. **禁止 `unwrap()`**（clippy `unwrap_used = "deny"`）；`expect()` 仅限 `main.rs` 与 `#[cfg(test)]`。
3. **公开 API 必带 doc + `# Examples`**（`missing_docs` 已升 error）。
4. **每个 crate 必须有 `README.md`**（L2 文档）；与 `lib.rs` 顶部 `//!` 内容一致。
5. **公共结构体优先 `#[non_exhaustive]`**，便于无破坏性扩展。
6. **TUI 内绝不 panic**：所有可能 panic 的调用走 `Result`；`main()` 装 `std::panic::set_hook` 还原 raw mode 再 abort。
7. **错误消息面向用户**：必须包含 session id、文件路径、可执行的修复建议。例如 `"failed to parse session abc-123 at /home/me/.claude/projects/.../session.jsonl; try `agentprof config show` to verify path"`。
8. **解析单个 session 失败不能拖垮整个命令**：`aggregate`/`list` 用 `Vec<Result<…>>` 收集，末尾 stderr 汇总失败计数。
9. **Conventional Commits**：`feat:` / `fix:` / `refactor:` / `docs:` / `test:` / `chore:` / `BREAKING:`。
10. **依赖图无环**：lib crate 之间不允许 cycle；CI 通过 grep + `cargo metadata` 校验。
11. **退出码**：`0` 成功 / `1` 用户错误 / `2` 数据错误 / `3` 外部服务错误 / `130` SIGINT。
12. **新 adapter 必须**：`crates/agentprof-adapters/src/<name>.rs` 实现 `Adapter` trait + `registry.rs` 注册 + ≥1 anonymized fixture（`crates/agentprof-adapters/tests/fixtures/<name>/`）+ ≥1 `assert_cmd` 集成测试 + 更新 `docs/adapters.md`。

---

## 7. 关键命令（提交前必跑）

```bash
# 格式 & lint
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 测试
cargo test --workspace --all-features
cargo insta test --check                  # snapshot 校验（CI 等价）

# 文档（必须无 warning）
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace

# 依赖审计
cargo deny check                          # 需 `cargo install cargo-deny`

# MSRV 校验（可选，每周 CI 会跑）
cargo +1.78 check --workspace
```

跑 binary 本身：

```bash
cargo run -p agentprof-cli -- analyze --agent claude --export md
cargo run -p agentprof-cli -- list --agent claude --since 7d
cargo run -p agentprof-cli -- aggregate --by mcp-server --since 30d --export md
```

---

## 8. 加新东西的菜谱

### 8.1 加一个新 adapter（如 `gemini`）

1. `crates/agentprof-adapters/src/gemini.rs` —— 实现 `Adapter` trait，文件顶部写 `//!` 模块文档
2. `crates/agentprof-adapters/src/registry.rs` —— 在 `register_default_adapters()` 里登记
3. `crates/agentprof-adapters/tests/fixtures/gemini/` —— 至少 1 个匿名化 jsonl
4. `crates/agentprof-adapters/tests/gemini.rs` —— 解析正确性测试
5. `crates/agentprof-cli/tests/cli.rs` —— `assert_cmd` 跑 `analyze --agent gemini --path <fixture>`
6. **文档**：`docs/architecture.md` §6（添加默认路径）+ `docs/adapters.md`（详细指南）+ `crates/agentprof-adapters/README.md`（"支持的 agent"段）+ rustdoc
7. `CHANGELOG.md` 加 `feat: add gemini adapter`

### 8.2 加一个新 CLI 子命令

1. `crates/agentprof-cli/src/cmd/<name>.rs` —— 实现 + clap derive 结构
2. `crates/agentprof-cli/src/main.rs` —— 在主枚举里挂上
3. **文档**：L1 §8 CLI 协议表 + L2 `agentprof-cli/README.md` + 根 `README.md` + rustdoc
4. `crates/agentprof-cli/tests/cli.rs` —— 成功 + 错误退出码两条 case
5. `CHANGELOG.md` 加条目

### 8.3 加一个新 feature flag

1. 对应 crate 的 `Cargo.toml` `[features]` 段
2. 用 `#[cfg(feature = "<name>")]` 标记代码
3. **文档**：L1 §15.4 Feature flags + L2 README 的 Features 段 + rustdoc 顶部 `//!` 段
4. `CHANGELOG.md` 加条目
5. CI 已经跑 `--all-features`，但**也要**确保关闭该 feature 时能编译：`cargo check -p <crate> --no-default-features`

### 8.4 加一个新 crate

1. `crates/agentprof-<name>/` —— `Cargo.toml`（继承 workspace.lints / workspace.package）+ `src/lib.rs`（含 `//!`）+ `README.md`（L2）
2. 根 `Cargo.toml` 的 `members` 里登记
3. **文档**：L1 §3 / §4 / §15.1 全部更新 + 新建 L2 README
4. 至少 1 个单元测试
5. `CHANGELOG.md` 加条目

---

## 9. AI 助手特别注意事项

1. **不要从 plan.md 的"待回答问题"里随机挑一个就开干**——那些是 spec 化前的开放问题。先在 `docs/superpowers/specs/` 写 spec 走 brainstorming 流程，决策完再写代码。
2. **不要为了"看起来更简单"而把 lib 逻辑塞进 `agentprof-cli`**——保持依赖图无环、lib/bin 分层。
3. **不要新增 dependency 不更新 `cargo deny` allowlist**——CI 会 fail。优先用 workspace `[workspace.dependencies]` 中已有的依赖。
4. **不要写"未来再做"的占位代码**：要么 TDD 写完整，要么不要碰。
5. **不要在 L1 文档里写函数级细节**——那是 rustdoc 的事；不要在 rustdoc 里写跨 crate 决策——那是 L1/internals 的事。
6. **不要绕过 panic hook**：TUI 里 `unwrap()` 会让终端卡在 raw mode，用户看到的是死掉的 shell。
7. **不要省 `# Examples`**：rustdoc 没 examples = CI fail。哪怕一行 ` # use ...;` 也行。
8. **不要修改本指令文件而不与用户确认**——本文件改动必须显式提出并取得用户同意。
9. **遇到与 `docs/architecture.md` 冲突时**，**先停下来**告诉用户冲突点，由用户裁决；**不要**自己擅自"修正"架构。
10. **Commit message 用英文**（Conventional Commits 习惯），但 PR 描述、issue、文档可以中文。

---

## 10. 仓库地图速查

```
agentprof/
├── Cargo.toml                 workspace + 共享 lints
├── README.md                  用户向，安装 + quickstart
├── CHANGELOG.md               Keep-a-Changelog + SemVer
├── CONTRIBUTING.md            含"边写代码边写文档"规则
├── LICENSE-MIT / LICENSE-APACHE
├── docs/
│   ├── plan.md                L1：产品/路线图
│   ├── architecture.md        L1：代码架构（权威）
│   ├── adapters.md            L2：怎么加新 agent
│   ├── features/<name>.md     L2：跨 crate feature
│   ├── internals/<topic>.md   L3：算法 / ADR
│   └── superpowers/specs/     每个 feature 的 spec
├── .github/
│   ├── copilot-instructions.md   本文件
│   └── workflows/             ci / release / nightly-msrv
├── crates/
│   ├── agentprof-core/        ├── Cargo.toml + README.md (L2) + src/
│   ├── agentprof-adapters/    │  src/lib.rs 顶部 //! 与 README 一致
│   ├── agentprof-storage/     │
│   ├── agentprof-tui/         │
│   └── agentprof-cli/         └─
└── xtask/                     辅助构建
```

---

## 11. 最后的话

> 你（AI 助手）每次准备结束一次回答时，自问：
>
> 1. 我改了代码吗？
> 2. 改的代码涉及到的 L1 / L2 / L3 文档**都**更新了吗？
> 3. 新增的公开 API **都**有 `# Examples` 吗？
> 4. CHANGELOG 写了吗？
> 5. PR / commit message 是 Conventional Commits 风格吗？
>
> 任何一条 "No"——回去补，**不要**提交。
