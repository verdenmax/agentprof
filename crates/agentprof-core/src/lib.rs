//! # agentprof-core
//!
//! Core domain model and analysis for **agentprof**. Defines the `Adapter`
//! trait that adapters implement, the token bucket / ROI / analysis report
//! data model, and the tokenizer / analyzer / exporter modules.
//!
//! This crate is the **leaf** of the dependency graph: it does **not**
//! depend on any other workspace crate.
//!
//! See [`docs/architecture.md`](https://github.com/agentprof/agentprof/blob/main/docs/architecture.md)
//! for the full system design (L1 documentation).
//!
//! ## Modules (planned, populated as features land)
//!
//! - `model`     — domain types (`RawSession`, `ToolDef`, `TokenBucket`, `RoiRow`, ...)
//! - `tokenizer` — local tokenization + optional Anthropic `count_tokens` API
//! - `analyzer`  — ROI scoring, schema utilization, waste estimation
//! - `export`    — speedscope JSON / Markdown / CSV / HTML serializers
//! - `error`     — `CoreError` enum
