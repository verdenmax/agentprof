# CI Workflow Specification

**Workflow:** `.github/workflows/ci.yml`  
**Status:** Accepted  
**Last updated:** 2026-06-29

## Purpose

Validate every push to `main` and every pull request against the Rust workspace
quality gates, dependency policy, privacy guardrails, documentation shape, and
non-default feature combinations.

## Jobs

| Job | Purpose | Required outcome |
|---|---|---|
| `lint` | Format and clippy gate | `cargo fmt --check` and clippy `-D warnings` pass |
| `test` | Cross-platform Rust tests | workspace all-features tests pass on Linux/macOS/Windows stable and Linux beta |
| `docs` | Rustdoc warnings gate | workspace docs build with `RUSTDOCFLAGS=-D warnings` |
| `snapshots` | Snapshot drift gate | `cargo insta test --check` passes |
| `feature-matrix` | Feature-gate build coverage | no-default and independent `otlp` / `web` / `progress` checks pass |
| `deny` | Supply-chain policy | cargo-deny reports advisories/bans/licenses/sources ok |
| `pii-guard` | Fixture/source PII guard | `xtask audit-pii crates` finds no real home paths |
| `docs-sync` | L2 crate docs shape | every crate/xtask has README; lib crates have top-level `//!` |

## Security constraints

- Immutable third-party actions are SHA-pinned with a trailing version comment.
- Rolling Rust toolchain refs remain tag-based by design and carry comments.
- The workflow uses read-only repository checkout except where a job's action
  requires standard cache/artifact permissions.

## Change management

When adding a workflow job, update `docs/architecture.md` §15.3 and this file in
the same PR.
