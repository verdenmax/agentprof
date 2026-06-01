//! Speedscope evented-JSON renderer.
//!
//! Thin wrapper: builds the
//! [`agentprof_core::export::speedscope::SpeedscopeProfile`] and serializes
//! it via `serde_json::to_string_pretty`. Returns any
//! [`ExportWarning`]s alongside the JSON so the cli layer can stream
//! them to stderr without altering exit code.

use agentprof_core::episode::Episodes;
use agentprof_core::export::speedscope::to_speedscope;
use agentprof_core::export::ExportWarning;
use agentprof_core::model::SessionMeta;

/// Render `episodes` + `meta` to a pretty-printed Speedscope JSON string.
///
/// Returns `(json, warnings)` so callers can stream warnings to stderr
/// without altering the success exit code.
///
/// # Examples
///
/// ```ignore
/// // agentprof-cli is bin-only; this doctest is illustrative only.
/// let (json, warnings) = speedscope::render(&episodes, &meta, "0.1.0");
/// assert!(json.starts_with("{"));
/// ```
#[must_use]
pub fn render(
    episodes: &Episodes,
    meta: &SessionMeta,
    agentprof_version: &str,
) -> (String, Vec<ExportWarning>) {
    let (profile, warnings) = to_speedscope(episodes, meta, agentprof_version);
    let json = serde_json::to_string_pretty(&profile)
        .unwrap_or_else(|e| format!("{{\"error\":\"speedscope serialize failed: {e}\"}}"));
    (json, warnings)
}
