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
    let json = serde_json::to_string_pretty(&profile).unwrap_or_else(|e| {
        // Build the fallback via `serde_json::json!` so that error
        // messages containing `"` or `\` produce valid JSON. The
        // outer `unwrap_or_else` is a final defensive escape — it is
        // unreachable in practice because the value is a flat string
        // object that `serde_json` always serializes successfully.
        let payload = serde_json::json!({
            "error": format!("speedscope serialize failed: {e}"),
        });
        serde_json::to_string(&payload)
            .unwrap_or_else(|_| r#"{"error":"unrepresentable error"}"#.to_string())
    });
    (json, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::episode::Episodes;
    use agentprof_core::model::SessionMeta;
    use chrono::Utc;

    fn empty_meta() -> SessionMeta {
        SessionMeta::new("test".into(), AgentKind::Copilot, Utc::now(), false)
    }

    #[test]
    fn fallback_uses_valid_json_even_with_quote_in_error_message() {
        // We exercise the fallback indirectly: the production code path
        // uses `serde_json::json!` so any error string would be properly
        // escaped. Verify the same machinery here on the message that
        // historically broke `format!`-template embedding.
        let nasty = "boom \"quoted\" and \\back\\slash\nand newline";
        let payload = serde_json::json!({
            "error": format!("speedscope serialize failed: {nasty}"),
        });
        let s = serde_json::to_string(&payload).expect("flat string object always serializes");
        let parsed: serde_json::Value =
            serde_json::from_str(&s).expect("fallback output must be valid JSON");
        assert!(parsed["error"].as_str().unwrap().contains(nasty));
    }

    #[test]
    fn happy_path_returns_pretty_json() {
        // Sanity: the success branch is JSON-parseable.
        let (json, _warnings) = render(&Episodes::default(), &empty_meta(), "0.0.0-test");
        let _parsed: serde_json::Value =
            serde_json::from_str(&json).expect("speedscope output must be valid JSON");
    }
}
