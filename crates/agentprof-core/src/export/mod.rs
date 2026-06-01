//! Export pipelines for `Episodes` / `AnalysisReport` into external
//! formats consumed by humans or other tools.
//!
//! # Module map
//!
//! - [`speedscope`] — emit a Speedscope evented JSON profile for upload
//!   to <https://speedscope.app>; see
//!   <https://github.com/jlfwong/speedscope/blob/main/file-format.md>
//!   for the wire format.
//! - [`svg_flamegraph`] — emit a self-contained, build-time-rendered SVG
//!   flamegraph for embedding in static HTML reports (stub in T1, filled
//!   in by M1.6.4 Task 2).
//! - [`warning`] — common [`ExportWarning`] enum surfaced by the pipelines.
//!
//! Each pipeline is a pure function over `&Episodes` / `&SessionMeta` /
//! `&AnalysisReport`; cli layers (`agentprof-cli::cmd::format`) are
//! responsible for IO + console formatting.

pub mod speedscope;
pub mod svg_flamegraph;
pub mod warning;

pub use warning::ExportWarning;
