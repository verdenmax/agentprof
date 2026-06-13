//! CSS design tokens for the visual guide. Adapted from
//! `langchain-visual-guide/src/shell.py` palette but rebranded to
//! agentprof's dashboard accent (#1a1a2e) so both surfaces feel
//! like one product.
//!
//! Single concatenated `ALL_CSS` const inlined into every page by
//! `shell.rs::PageTemplate`. Keeping it as a single string means
//! self-contained HTML — no external stylesheet, works `file://`.

#![allow(dead_code)] // consumed by shell.rs via super::css::ALL_CSS — verified at T7+

/// Concatenated CSS for the entire visual-guide site.
pub const ALL_CSS: &str = include_str!("guide.css");
