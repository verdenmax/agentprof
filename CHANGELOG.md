# Changelog

All notable changes to **agentprof** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries are prefixed with the affected crate when relevant:
`core:` / `adapters:` / `storage:` / `tui:` / `cli:` / `xtask:`.

Breaking changes are marked `BREAKING:` (matching the Conventional Commits
prefix used in commit messages).

## [Unreleased]

### Added
- **Skill pipeline integration** — five curated skills from `github/awesome-copilot` vendored into a local plugin alongside `obra/superpowers`, plus two `.instructions.md` files committed into the repo:
  - Plugin: `~/.copilot/installed-plugins/_direct/agentprof-extras/` with `create-architectural-decision-record`, `cli-mastery`, `copilot-cli-quickstart`, `github-release`, `create-github-action-workflow-specification` (20 files total).
  - In-repo: `.github/instructions/rust.instructions.md` and `.github/instructions/update-docs-on-code-change.instructions.md` (Stage-0 always-on rules).
- **Unified 9-stage pipeline** — `.github/copilot-instructions.md` §5 rewritten as a Boot → Discovery → Decision → Planning → Implementation → CI/Infra → Debugging → Completion → Release flowchart; covers every obra + agentprof-extras skill with stage, trigger, output, and exit criterion.
- `.github/copilot-instructions.md` §6 extended: §6.1/§6.2 expanded with the five new skills and the `Pipeline 阶段` column; new §6.6 "Stage 0 常驻 instructions" and §6.7 "Plugin 来源说明".
- `docs/architecture.md` §14.7 rewritten to map all 19 skills to pipeline stages and document outputs; new §14.8 acknowledging the two always-on instruction files.
- Skills usage matrix (`obra--superpowers` series) integrated into both AI and architecture docs:
  - `.github/copilot-instructions.md` §6 — enforcement list with 🔴 MUST / 🟡 recommended / 🟢 optional tiers, plus anti-patterns.
  - `docs/architecture.md` §14.7 — mapping table from each skill's output to the L1/L2/L3 documentation layer.
- Workspace skeleton with five crates (`agentprof-core`, `agentprof-adapters`, `agentprof-storage`, `agentprof-tui`, `agentprof-cli`) and an `xtask` helper.
- Architecture authority document (`docs/architecture.md`, L1).
- AI-assistant guide (`.github/copilot-instructions.md`).
- Adapter contributor guide placeholder (`docs/adapters.md`, L2).
- L1/L2/L3 documentation system definition (see `docs/architecture.md` §14).
- Repository configuration: `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.editorconfig`, `.gitignore`, dual `LICENSE-*` files.

[Unreleased]: https://github.com/agentprof/agentprof/compare/HEAD...HEAD
