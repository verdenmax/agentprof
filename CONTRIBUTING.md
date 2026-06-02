# Contributing to agentprof

Thanks for considering a contribution! This file lists the **non-negotiable
rules** that apply to every change. For deeper context read, in order:

1. [`docs/plan.md`](docs/plan.md) — what we are building and why
2. [`docs/architecture.md`](docs/architecture.md) — how the system fits together (L1)
3. The `README.md` of the crate you intend to touch (L2)
4. [`.github/copilot-instructions.md`](.github/copilot-instructions.md) —
   condensed rules for AI assistants; humans will find it useful too

---

## The four rules

### 1. Write the docs in the same commit as the code

Documentation lives at three levels (see `docs/architecture.md` §14):

| Level | Where | Trigger |
|---|---|---|
| **L1** — architecture | `docs/architecture.md`, `docs/plan.md` | Layout / crate / protocol change |
| **L2** — per crate / feature | `crates/<name>/README.md`, `docs/features/<feature>.md`, `docs/adapters.md` | New crate, module, or cross-crate feature |
| **L3** — implementation | rustdoc (`///` + `# Examples` + `# Errors` + `# Panics`), `docs/internals/<topic>.md` | New / changed function, type, algorithm |

The CI job `docs-sync` enforces this. A PR that touches `pub` API without
updating rustdoc, or that adds a crate without a `README.md`, will fail.

### 2. TDD by default

For both features and bug fixes: write a failing test first, then make it
pass. Bug fix PRs that don't include a regression test will be asked for one
in review.

### 3. Conventional Commits

Commit messages use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(adapters): add codex adapter
fix(core): correct waste estimate when call_count is zero
docs: clarify L2 README template
refactor(cli): split aggregate command into its own module
test(storage): cover migration idempotency
chore: bump tiktoken-rs to 0.6
BREAKING: rename Adapter::discover to Adapter::discover_sessions
```

Scope is the crate name without the `agentprof-` prefix (`adapters`, `core`,
`storage`, `tui`, `cli`, `xtask`) or omitted for cross-cutting changes.

### 4. Run the gate locally before pushing

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace
cargo deny check     # cargo install cargo-deny  (one-time setup)
```

If any of these fail locally they will fail on CI — fix before opening the PR.

---

## Workflow for a new feature

1. **Spec it first**: open a draft `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`. If you prefer a thread, file an issue and link the spec from it.
2. **Get the design approved** before writing implementation code. For
   non-trivial designs, attach a brainstorming output (see the
   [Superpowers](https://github.com/obra/superpowers) skill if you use Claude
   Code).
3. **Write failing tests** that capture the spec's acceptance criteria.
4. **Implement + write rustdoc in the same commits.** Update L1/L2 docs as
   layout changes.
5. **Open a PR** with:
   - Conventional-Commit title
   - Description listing which docs you touched (use the checklist in
     `.github/PULL_REQUEST_TEMPLATE.md` once it lands)
   - Linked issue or spec

## Workflow for a bug fix

1. Reproduce the bug with a **failing test** in the appropriate crate.
2. Fix the code. The test goes green.
3. Update rustdoc if the fix changes documented behaviour.
4. Add a `fix(<crate>): …` entry to `CHANGELOG.md` under `[Unreleased]`.

---

## Don't

- Don't add a workspace dependency without updating
  `[workspace.dependencies]` in the root `Cargo.toml`.
- Don't introduce a new license to the dependency tree without updating
  `deny.toml` and explaining why.
- Don't `unwrap()` outside of `main.rs` and `#[cfg(test)]`.
- Don't put CLI subcommand logic inside a `lib` crate.
- Don't break the dependency graph (lib crates must form a DAG; no lib
  crate may depend on `agentprof-cli`).
- Don't merge `TODO: write docs` — write them now.
- Don't add `eprintln!` for diagnostics anywhere outside of `main.rs`
  bootstrap / `#[cfg(test)]`. Use `tracing::warn!` / `info!` / `debug!`
  with structured fields. Global flags `--log-level <LEVEL>` and
  `--log-file <PATH>` (also `AGENTPROF_LOG_LEVEL` / `AGENTPROF_LOG_FILE`
  envs) control the subscriber; TUI subcommands auto-redirect to
  `$XDG_STATE_HOME/agentprof/agentprof.log` to avoid corrupting the
  alt-screen. See
  [ADR-0010](docs/internals/adr-0010-tracing-infrastructure.md) and
  `docs/architecture.md` §15.5.
- Don't attach a raw filesystem session path to a `tracing` span field;
  wrap it with
  `agentprof_core::observability::pii::hash_path` so it renders as the
  default 8-hex-char `sha256` short hash (opt-out via
  `AGENTPROF_LOG_FULL_PATHS=1`). See
  [`docs/features/privacy.md`](docs/features/privacy.md) §7.

---

## License

By contributing, you agree that your contributions are licensed under the
project's dual MIT OR Apache-2.0 license — same as the rest of the codebase.
