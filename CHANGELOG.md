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
- Workspace skeleton with five crates (`agentprof-core`, `agentprof-adapters`, `agentprof-storage`, `agentprof-tui`, `agentprof-cli`) and an `xtask` helper.
- Architecture authority document (`docs/architecture.md`, L1).
- AI-assistant guide (`.github/copilot-instructions.md`).
- Adapter contributor guide placeholder (`docs/adapters.md`, L2).
- L1/L2/L3 documentation system definition (see `docs/architecture.md` §14).
- Repository configuration: `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.editorconfig`, `.gitignore`, dual `LICENSE-*` files.

[Unreleased]: https://github.com/agentprof/agentprof/compare/HEAD...HEAD
