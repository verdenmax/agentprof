# agentprof-core

> Core domain model, tokenizer, analyzer, and exporters. The **leaf** of the workspace dependency graph — no workspace crate is allowed to be a dependency.

## Position in the agentprof architecture

Sits at the bottom of the dependency graph. See [`docs/architecture.md`](../../docs/architecture.md) §3 (system layering) and §4 (crate inventory). All other workspace crates depend on `agentprof-core`; this crate must not depend back.

## Public interface

The crate exposes (planned — see source rustdoc as work lands):

- `model::*` — domain types: `RawSession`, `Turn`, `ToolDef`, `ToolCall`, `TokenBucket`, `RoiRow`, `AnalysisReport`, `Adapter` trait
- `tokenizer::*` — `count_tokens(model, text)` + Anthropic API (feature-gated)
- `analyzer::*` — `compute_roi`, `schema_utilization`, `waste_estimate`
- `export::*` — Speedscope JSON, Markdown, CSV serializers
- `error::CoreError` — strongly typed errors (`thiserror`)

Typical usage:

```rust
// (will become a doctest once analyzer ships)
// let session = adapter.load_session(&session_ref)?;
// let report  = agentprof_core::analyzer::compute_roi(&session)?;
```

## Modules (planned)

| Module | Purpose |
|---|---|
| `model` | Domain types and `Adapter` trait |
| `tokenizer` | Local (`tiktoken-rs`) + optional Anthropic API tokenization |
| `analyzer` | ROI scoring, schema utilization, waste estimation |
| `export` | Speedscope / Markdown / CSV / shared HTML helpers |
| `error` | `CoreError` enum |

## Features

| Feature | Default | Effect |
|---|---|---|
| `anthropic-api` | off | Enables HTTP-based Anthropic `count_tokens` API for precise Anthropic tokenization. Pulls in `reqwest` + `tokio`. |

## Dependencies

- Workspace: `serde`, `serde_json`, `thiserror`, `chrono`, `tracing`, `tiktoken-rs`
- Optional (feature `anthropic-api`): `reqwest`, `tokio`

## Local commands

```sh
cargo test  -p agentprof-core --all-features
cargo doc   -p agentprof-core --no-deps --open
cargo clippy -p agentprof-core --all-features -- -D warnings
```

## Change history

See the root [`CHANGELOG.md`](../../CHANGELOG.md) — entries for this crate are prefixed `core:`.
