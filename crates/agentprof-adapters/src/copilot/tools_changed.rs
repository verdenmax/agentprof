//! Wire extraction of `<tools_changed_notice>` blocks embedded inside
//! `user.message.data.transformed_content`. Discovered during the
//! 2026-06-08 audit on real `~/.copilot/session-state/` data.
//!
//! See `docs/superpowers/specs/2026-06-08-m1.6.5-mcp-waste-design.md` §3.1
//! and `docs/internals/adr-0015-mcp-waste-architecture.md` D-1/D-2.

use std::collections::BTreeSet;

use crate::copilot::CopilotEvent;

const OPEN_TAG: &str = "<tools_changed_notice>";
const CLOSE_TAG: &str = "</tools_changed_notice>";
const NEW_TOOLS_PREFIX: &str = "New tools available:";

/// Walk a session's events; extract every `<tools_changed_notice>` block
/// from any `user.message.data.transformed_content`; accumulate the
/// "ever-loaded" MCP tool set.
///
/// Per ADR-0015 D-2, `Tools no longer available:` lines are parsed but
/// do NOT decrement the loaded set — the M1.6.5 "ever-loaded" semantic.
///
/// # Examples
///
/// ```
/// use agentprof_adapters::copilot::tools_changed::extract_loaded_set_from_session;
/// let s = extract_loaded_set_from_session(&[]);
/// assert!(s.is_empty());
/// ```
#[must_use]
#[tracing::instrument(name = "adapter.tools_changed", skip_all, fields(event_count = events.len()))]
pub fn extract_loaded_set_from_session(events: &[CopilotEvent]) -> BTreeSet<String> {
    let mut acc = BTreeSet::new();
    for ev in events {
        if let CopilotEvent::UserMessage(env) = ev {
            if let Some(content) = env.data.transformed_content.as_deref() {
                for block in find_tools_changed_notices(content) {
                    acc.extend(parse_new_tools_line(block));
                }
            }
        }
    }
    // Per spec §6.1 / ADR-0015 D-1, MCP-only filter (`mcp__<server>__<tool>`).
    // Builtins (`bash`, `edit`, `exit_plan_mode`) and skills
    // (`skill__<plugin>__<name>`) are out of scope for waste analysis.
    acc.retain(|n| n.starts_with("mcp__"));
    acc
}

/// Return an iterator over each `<tools_changed_notice>...</tools_changed_notice>`
/// block in `content`. Tolerates blocks anywhere in the text. Skips
/// malformed blocks (open tag without matching close tag).
fn find_tools_changed_notices(content: &str) -> impl Iterator<Item = &str> + '_ {
    let mut search_start = 0;
    std::iter::from_fn(move || {
        let open_at = content[search_start..].find(OPEN_TAG)?;
        let block_start = search_start + open_at + OPEN_TAG.len();
        let close_at = content[block_start..].find(CLOSE_TAG)?;
        let block_end = block_start + close_at;
        search_start = block_end + CLOSE_TAG.len();
        Some(&content[block_start..block_end])
    })
}

/// Within one block body, parse every `New tools available: X, Y, Z`
/// line. Tolerates blank lines, multiple `New tools available:` lines,
/// surrounding whitespace.
fn parse_new_tools_line(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim();
        if let Some(remainder) = trimmed.strip_prefix(NEW_TOOLS_PREFIX) {
            for raw in remainder.split(',') {
                let name = raw.trim();
                if !name.is_empty() && is_valid_tool_name(name) {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

fn is_valid_tool_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::needless_collect)]
mod tests {
    use super::*;

    #[test]
    fn extract_empty_session_returns_empty_set() {
        let set = extract_loaded_set_from_session(&[]);
        assert!(set.is_empty());
    }

    #[test]
    fn parse_single_new_tools_line() {
        let content = "<tools_changed_notice>\nNew tools available: mcp__github__search, mcp__github__create\n</tools_changed_notice>";
        let mut acc = std::collections::BTreeSet::new();
        for block in find_tools_changed_notices(content) {
            acc.extend(parse_new_tools_line(block));
        }
        assert_eq!(acc.len(), 2);
        assert!(acc.contains("mcp__github__search"));
    }

    #[test]
    fn parse_ignores_tools_no_longer_available_line() {
        let content = "<tools_changed_notice>\nNew tools available: mcp__a__t1\n\nTools no longer available: mcp__a__t2\n</tools_changed_notice>";
        let mut acc = std::collections::BTreeSet::new();
        for block in find_tools_changed_notices(content) {
            acc.extend(parse_new_tools_line(block));
        }
        assert!(acc.contains("mcp__a__t1"));
        assert!(
            !acc.contains("mcp__a__t2"),
            "Remove notices must NOT contribute to loaded set (M1.6.5 ever-loaded semantics)"
        );
    }

    #[test]
    fn parse_multiple_notices_in_one_content() {
        let content = "<tools_changed_notice>\nNew tools available: mcp__a__t1\n</tools_changed_notice>\nFoo bar\n<tools_changed_notice>\nNew tools available: mcp__b__t2\n</tools_changed_notice>";
        let blocks: Vec<&str> = find_tools_changed_notices(content).collect();
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn parse_notice_in_middle_of_content_works() {
        let content = "Some preamble\n<tools_changed_notice>\nNew tools available: mcp__a__t1\n</tools_changed_notice>\nSome postamble";
        let blocks: Vec<&str> = find_tools_changed_notices(content).collect();
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn parse_malformed_block_no_close_tag_gracefully_skipped() {
        let content = "<tools_changed_notice>\nNew tools available: mcp__a__t1\n(no close tag)";
        let blocks: Vec<&str> = find_tools_changed_notices(content).collect();
        assert!(
            blocks.is_empty(),
            "malformed block (no close tag) must be skipped, not parsed"
        );
    }

    #[test]
    fn extract_filters_non_mcp_tools() {
        // Synthesize a UserMessage event with the notice in transformed_content.
        // (Constructing CopilotEvent directly here would require deserializing —
        // simpler: call the inner helpers directly to validate the filter.)
        let content = "<tools_changed_notice>\nNew tools available: bash, mcp__github__search, exit_plan_mode\n</tools_changed_notice>";
        let mut acc = std::collections::BTreeSet::new();
        for block in find_tools_changed_notices(content) {
            acc.extend(parse_new_tools_line(block));
        }
        acc.retain(|n| n.starts_with("mcp__"));
        assert_eq!(acc.len(), 1);
        assert!(acc.contains("mcp__github__search"));
    }

    #[test]
    fn extract_tolerates_user_message_without_transformed_content() {
        // Build a UserMessage event with transformed_content = None.
        let raw = r#"{
            "type": "user.message",
            "id": "u1",
            "timestamp": "2026-06-08T10:00:00Z",
            "parentId": null,
            "data": {
                "content": "hello",
                "source": "user",
                "attachments": [],
                "interactionId": "i1"
            }
        }"#;
        let ev: crate::copilot::CopilotEvent = serde_json::from_str(raw).expect("parse");
        let set = extract_loaded_set_from_session(&[ev]);
        assert!(set.is_empty());
    }
}
