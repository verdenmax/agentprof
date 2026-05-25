## Summary

<!-- One paragraph: what does this PR do and why. -->

## Linked issue / spec

<!-- Closes #N, or link to docs/superpowers/specs/YYYY-MM-DD-*.md -->

## Documentation checklist (L1 / L2 / L3)

Per `CONTRIBUTING.md` and `docs/architecture.md` §14, every PR must update
the documentation level(s) that match the kind of change. Tick all that apply
or strike through (`~~...~~`) the ones that do not.

- [ ] **L1** — `docs/architecture.md` updated (layering, crate inventory, protocol, schema, config)
- [ ] **L1** — `docs/plan.md` updated (roadmap, phase, open question resolved)
- [ ] **L2** — affected crate's `README.md` updated
- [ ] **L2** — `docs/features/<feature>.md` updated or created
- [ ] **L2** — `docs/adapters.md` updated (only for new / changed adapters)
- [ ] **L3** — rustdoc updated; every new `pub` item has `# Examples`
- [ ] **L3** — `docs/internals/<topic>.md` updated (algorithm / ADR)
- [ ] **CHANGELOG.md** — entry added under `[Unreleased]`

## Local gate

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps --workspace`
- [ ] `cargo deny check`

## Notes for reviewer

<!-- Anything that needs context, trade-offs, follow-up work, etc. -->
