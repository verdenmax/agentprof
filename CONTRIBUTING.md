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
cargo test --workspace --all-features --no-fail-fast
RUSTDOCFLAGS="-Dwarnings" cargo doc --workspace --no-deps --all-features
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

## Release process (maintainers only)

The release is two-stage. Stage 1 happens entirely locally; Stage 2 is
triggered by pushing the tag. See [ADR-0014](docs/internals/adr-0014-v0.1.0-release-strategy.md)
for the strategic decisions behind this flow.

### Stage 1 — Local prep

1. Update `CHANGELOG.md`: move `## [Unreleased]` body content into a new
   `## [X.Y.Z] - YYYY-MM-DD` section; leave an empty `## [Unreleased]`
   above. Append link references at the bottom:
   ```
   [Unreleased]: https://github.com/verdenmax/agentprof/compare/vX.Y.Z...HEAD
   [X.Y.Z]: https://github.com/verdenmax/agentprof/releases/tag/vX.Y.Z
   ```
2. Bump `Cargo.toml` `[workspace.package].version` to `X.Y.Z` AND update
   the four `[workspace.dependencies]` version pins for
   `agentprof-{core,adapters,storage,tui}` to the same `X.Y.Z` (they
   are released in lockstep per ADR-0014 D-3).
3. Run the full local gate:
   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace --all-features --no-fail-fast
   RUSTDOCFLAGS="-Dwarnings" cargo doc --workspace --no-deps --all-features
   cargo deny check     # cargo install cargo-deny  (one-time setup; same as PR gate)
   cargo dist plan
   ```
4. Commit: `git commit -am "chore(release): vX.Y.Z"`.

### Stage 2 — Tag push

5. `git tag vX.Y.Z`
6. `git push origin main --tags`
7. Watch the Actions tab: `release.yml` should run plan → build × 4
   platforms → upload to GitHub Release.
8. Verify the GitHub Release page has 4 platform tarballs + installer.sh
   + per-file `.sha256` checksums (cargo-dist default).
9. Test the installer from a clean shell:
   ```sh
   curl -fsSL https://github.com/verdenmax/agentprof/releases/latest/download/agentprof-cli-installer.sh | sh
   agentprof --version        # → agentprof X.Y.Z
   ```

### If any Stage-2 step fails

```sh
git push --delete origin vX.Y.Z
git tag -d vX.Y.Z
gh release delete vX.Y.Z --yes
```

Fix the underlying issue, return to Stage 1 step 4 (re-commit with the
fix folded in) and re-tag. Do NOT patch-forward from a broken release.

### Updating cargo-dist

When a new `cargo-dist` minor version lands:
```sh
cargo install cargo-dist --version "^0.NEW" --locked
cargo dist init                          # re-confirm the same answers
```

**After `cargo dist init`, verify `dist-workspace.toml` still contains
the M1.7-era hand-edited lines** (init may strip them since they're
not part of its default emission):

- `pr-run-mode = "plan"` (M1.7 T4 — skip 4-platform builds on PR)
- `allow-dirty = ["ci"]` (M1.7 T5 — tolerates SHA-pinned `release.yml`
  diverging from cargo-dist's tag-form template; without this,
  `cargo dist plan` aborts on every CI run)
- The doc header comment (cargo-dist conventions for editing)

If any are missing, restore from `git diff dist-workspace.toml` or the
previous `v0.1.0` commit's version, then proceed:

```sh
cargo dist generate --mode ci            # regenerates release.yml
```

Manually re-SHA-pin all `uses:` references in the new `release.yml`
(ADR-0014 D-11 + D-6). SHA-lookup snippet (curl + jq):

```sh
for repo in actions/checkout actions/upload-artifact actions/download-artifact softprops/action-gh-release; do
  tag=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | jq -r '.tag_name')
  sha=$(curl -fsSL "https://api.github.com/repos/$repo/git/refs/tags/$tag" | jq -r '.object.sha')
  echo "$repo@$sha  # $tag"
done
```

For annotated tags (rare), follow the tag object to the commit:

```sh
curl -fsSL "https://api.github.com/repos/<repo>/git/tags/<sha>" | jq -r '.object.sha'
```

Action list is illustrative — adjust based on what `cargo dist generate --mode ci` actually emits.
