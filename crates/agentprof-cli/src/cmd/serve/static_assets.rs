//! Compile-time-bundled dashboard static assets (M2.3 T6).
//!
//! CSS / JS / favicon are baked into the binary via `include_str!` /
//! `include_bytes!`. No filesystem reads at runtime; no external
//! CDN dependencies. Mirrors the same self-contained ethos as the
//! single-file HTML report (M1.6.4).

pub const CSS: &str = include_str!("../../../templates/dashboard/static/dashboard.css");
pub const JS: &str = include_str!("../../../templates/dashboard/static/dashboard.js");
pub const FAVICON_SVG: &[u8] = include_bytes!("../../../templates/dashboard/static/favicon.svg");

/// MIME-type lookup for a `/static/<name>` request.
///
/// Returns `None` for unknown asset names so the handler can return `404`.
///
/// # Examples
///
/// ```text
/// lookup("dashboard.css") => Some(("text/css; charset=utf-8", <css bytes>))
/// lookup("dashboard.js")  => Some(("application/javascript; charset=utf-8", <js bytes>))
/// lookup("favicon.svg")   => Some(("image/svg+xml", <svg bytes>))
/// lookup("unknown.png")   => None
/// ```
#[must_use]
pub fn lookup(name: &str) -> Option<(&'static str, &'static [u8])> {
    match name {
        "dashboard.css" => Some(("text/css; charset=utf-8", CSS.as_bytes())),
        "dashboard.js" => Some(("application/javascript; charset=utf-8", JS.as_bytes())),
        "favicon.svg" => Some(("image/svg+xml", FAVICON_SVG)),
        _ => None,
    }
}
