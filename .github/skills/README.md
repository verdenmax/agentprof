# .github/skills — agentprof project skills

> **Project skills**（按 [Adding agent skills for GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-skills) 文档定义的"per-repository"位置）。

这些 skill 跟随仓库版本化、跟随 `git clone` 自动获取，**不需要任何全局安装步骤**。Copilot CLI 启动时会自动从这里 + `~/.copilot/skills/` + 已装 plugin 中合并所有 skill 列表。

## 在本项目中的 5 个 skill

全部 vendored 自 [`github/awesome-copilot`](https://github.com/github/awesome-copilot)，未本地修改（便于将来 sync）。每个 skill 在 agentprof 工作流 pipeline 中的位置见 [`.github/copilot-instructions.md`](../copilot-instructions.md) §5（pipeline）和 §6（清单）。

| Skill | 阶段 | 作用 |
|---|---|---|
| [`cli-mastery`](./cli-mastery/) | Stage 4 | Copilot CLI 工作流交互式培训（slash commands / shortcuts / modes / agents / skills / MCP / config） |
| [`copilot-cli-quickstart`](./copilot-cli-quickstart/) | Stage 4 | Copilot CLI 入门教程（Developer + Non-Developer track） |
| [`github-release`](./github-release/) | Stage 8 | 端到端 GitHub 发布：SemVer 决策 + Keep-a-Changelog 自动化 |
| [`create-github-action-workflow-specification`](./create-github-action-workflow-specification/) | Stage 5 | 给已有 `.github/workflows/*.yml` 生成 AI 友好的规约文档 |
| [`create-architectural-decision-record`](./create-architectural-decision-record/) | Stage 2 | 生成 `docs/internals/adr-NNNN-*.md` 的 AI-optimized ADR |

## 它和 obra/superpowers 的区别

| 维度 | obra/superpowers | `.github/skills/`（本目录） |
|---|---|---|
| 来源 | GitHub plugin（marketplace install） | vendored from `github/awesome-copilot` |
| 位置 | `~/.copilot/installed-plugins/_direct/obra--superpowers/` | `<repo>/.github/skills/`（入 git） |
| 跨项目 | 全部项目可见 | 仅本项目；跟随 clone 自动获取 |
| 更新方式 | `/plugin update`（或重装 plugin） | git commit 上游内容 |

## 验证 / 列出

在本项目里启动 `copilot` 后，运行：

```text
/skills list                            # 列出所有可用 skill（含本目录 + 全局）
/skills info create-architectural-decision-record
/skills reload                          # 当前 session 内重载（修改后无需重启）
```

## 同步 upstream

若想拉取上游最新版本：

```sh
# 来源（不要改）
UPSTREAM="https://raw.githubusercontent.com/github/awesome-copilot/main"

# 例：更新 create-architectural-decision-record
curl -sSL "$UPSTREAM/skills/create-architectural-decision-record/SKILL.md" \
     -o .github/skills/create-architectural-decision-record/SKILL.md
```

## License

每个 skill 内容来自 `github/awesome-copilot`，遵守上游 MIT License。本目录的 `README.md` 由 agentprof contributors 撰写，同样 MIT。

完整上游 license：<https://github.com/github/awesome-copilot/blob/main/LICENSE>
