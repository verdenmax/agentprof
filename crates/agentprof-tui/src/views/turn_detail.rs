//! Full-screen detail view for a single turn — opens when the user
//! presses `Enter` on a selected turn in [`crate::views::flamegraph`].
//!
//! Renders one block per tool call in the turn (sorted by duration desc):
//! tool name (colored by [`agentprof_core::model::ToolSource`]), duration,
//! ✓/✗ status, source badge, and a one-line `args` preview (truncated to
//! 80 chars + `…`). Selecting a tool call (`↑`/`↓`/`j`/`k`/`G`/`gg`) and
//! pressing `Enter` toggles the call's `args` between truncated and
//! fully-expanded JSON. `Esc` returns to flamegraph; `1`/`2`/`3` pop
//! detail and switch views.
//!
//! See `docs/superpowers/specs/2026-06-03-turn-detail-view-design.md`
//! and ADR-0011 for the design rationale.

use std::collections::HashSet;

use agentprof_core::episode::ToolCallStatus;

/// Per-detail-view persistent state. Lives on
/// `crate::app::state::AppState::detail_view` and (in watch mode)
/// on `WatchViewState::detail_view`.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::turn_detail::TurnDetailState;
/// let mut s = TurnDetailState::new("T3");
/// assert_eq!(s.turn_id, "T3");
/// assert_eq!(s.selected_tool_idx, 0);
/// s.move_down(5);
/// assert_eq!(s.selected_tool_idx, 1);
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TurnDetailState {
    /// Identifier of the turn being shown
    /// ([`agentprof_core::episode::Turn::id`]).
    pub turn_id: String,
    /// Currently selected tool call index in the per-turn duration-sorted
    /// list. `0` when the turn has no tool calls.
    pub selected_tool_idx: usize,
    /// Tool-call indices whose args row is currently expanded
    /// (toggled by `Enter`).
    pub expanded_tools: HashSet<usize>,
    /// Vertical viewport offset for scrolling past the visible rect.
    /// Owned here so the render fn (lands in F1 Task 6) can persist
    /// scroll position across redraws without mutating its own
    /// arguments. Currently always `0` — Task 5 has no `render_*`
    /// helper that would update it.
    pub viewport_top: u16,
    /// Vim-style `gg` two-key sequence in-progress flag, mirroring
    /// `crate::app::state::AppState::pending_gg` but scoped to the
    /// detail view.
    pub pending_gg: bool,
}

impl TurnDetailState {
    /// Construct a detail-view state pointing at the given turn id with
    /// the first tool call selected.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::views::turn_detail::TurnDetailState;
    /// let s = TurnDetailState::new("turn-abc");
    /// assert!(!s.pending_gg);
    /// assert!(s.expanded_tools.is_empty());
    /// ```
    #[must_use]
    pub fn new(turn_id: impl Into<String>) -> Self {
        Self {
            turn_id: turn_id.into(),
            selected_tool_idx: 0,
            expanded_tools: HashSet::new(),
            viewport_top: 0,
            pending_gg: false,
        }
    }

    /// Move selection up by one (saturating at 0). Clears `pending_gg`.
    pub fn move_up(&mut self) {
        self.selected_tool_idx = self.selected_tool_idx.saturating_sub(1);
        self.pending_gg = false;
    }

    /// Move selection down by one (clamped to `max.saturating_sub(1)`).
    /// `max` is the number of tool calls in the turn. Clears `pending_gg`.
    pub fn move_down(&mut self, max: usize) {
        if max > 0 && self.selected_tool_idx + 1 < max {
            self.selected_tool_idx += 1;
        }
        self.pending_gg = false;
    }

    /// Jump to first tool call (`gg`). Clears `pending_gg`.
    pub fn jump_first(&mut self) {
        self.selected_tool_idx = 0;
        self.pending_gg = false;
    }

    /// Jump to last tool call (`G`). `max` is the number of tool calls.
    /// Clears `pending_gg`.
    pub fn jump_last(&mut self, max: usize) {
        self.selected_tool_idx = max.saturating_sub(1);
        self.pending_gg = false;
    }

    /// Toggle args expansion for the selected tool call. Clears `pending_gg`.
    pub fn toggle_expand(&mut self) {
        if !self.expanded_tools.insert(self.selected_tool_idx) {
            self.expanded_tools.remove(&self.selected_tool_idx);
        }
        self.pending_gg = false;
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn new_initializes_first_selected() {
        let s = TurnDetailState::new("t1");
        assert_eq!(s.turn_id, "t1");
        assert_eq!(s.selected_tool_idx, 0);
        assert!(s.expanded_tools.is_empty());
        assert_eq!(s.viewport_top, 0);
        assert!(!s.pending_gg);
    }

    #[test]
    fn move_up_saturates_at_zero() {
        let mut s = TurnDetailState::new("t");
        s.move_up();
        assert_eq!(s.selected_tool_idx, 0);
    }

    #[test]
    fn move_down_clamps_at_max() {
        let mut s = TurnDetailState::new("t");
        s.selected_tool_idx = 2;
        s.move_down(3);
        assert_eq!(s.selected_tool_idx, 2, "already at last");
        s.move_down(4);
        assert_eq!(s.selected_tool_idx, 3, "advances when room");
    }

    #[test]
    fn move_down_zero_max_no_panic() {
        let mut s = TurnDetailState::new("t");
        s.move_down(0);
        assert_eq!(s.selected_tool_idx, 0);
    }

    #[test]
    fn jump_last_handles_empty() {
        let mut s = TurnDetailState::new("t");
        s.jump_last(0);
        assert_eq!(s.selected_tool_idx, 0);
        s.jump_last(7);
        assert_eq!(s.selected_tool_idx, 6);
    }

    #[test]
    fn toggle_expand_flips() {
        let mut s = TurnDetailState::new("t");
        s.toggle_expand();
        assert!(s.expanded_tools.contains(&0));
        s.toggle_expand();
        assert!(!s.expanded_tools.contains(&0));
    }

    #[test]
    fn movement_clears_pending_gg() {
        let mut s = TurnDetailState::new("t");
        s.pending_gg = true;
        s.move_down(2);
        assert!(!s.pending_gg);
        s.pending_gg = true;
        s.jump_first();
        assert!(!s.pending_gg);
    }
}

// === Pure formatters (rendering helpers) ==================================

/// Format a `Some(args)` JSON value as a single-line preview, truncated to
/// `max_chars` characters with a trailing `…` when over budget. `None`
/// yields the dim-rendered `(not captured)` placeholder text.
///
/// Uses `chars().count()` not byte length; CJK/wide-glyph widths are
/// approximate (off by ±1 cell). Documented limitation per ADR-0011 D-11.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::turn_detail::format_args_preview;
/// use serde_json::json;
///
/// let v = json!({"command": "ls -la"});
/// let s = format_args_preview(Some(&v), 80);
/// assert!(s.starts_with('{'));
///
/// assert_eq!(format_args_preview(None, 80), "(not captured)");
///
/// let big = json!({"x": "a".repeat(200)});
/// let s = format_args_preview(Some(&big), 30);
/// assert!(s.ends_with('…'));
/// assert!(s.chars().count() <= 30);
///
/// // max_chars = 0 yields the empty string (degenerate).
/// assert_eq!(format_args_preview(Some(&json!({"x": 1})), 0), "");
/// ```
#[must_use]
pub fn format_args_preview(args: Option<&serde_json::Value>, max_chars: usize) -> String {
    let Some(v) = args else {
        return "(not captured)".to_string();
    };
    if max_chars == 0 {
        return String::new();
    }
    let s = serde_json::to_string(v).unwrap_or_else(|_| "<unserializable>".to_string());
    if s.chars().count() <= max_chars {
        s
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

/// Format `Some(args)` as multi-line pretty JSON, wrapped to `width`
/// columns. `None` yields a single `(not captured)` line.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::turn_detail::wrap_args_full;
/// use serde_json::json;
///
/// let v = json!({"a": 1, "b": [2, 3]});
/// let lines = wrap_args_full(Some(&v), 40);
/// assert!(!lines.is_empty());
/// assert!(lines.iter().all(|l| l.chars().count() <= 40));
///
/// let lines = wrap_args_full(None, 40);
/// assert_eq!(lines, vec!["(not captured)".to_string()]);
/// ```
#[must_use]
pub fn wrap_args_full(args: Option<&serde_json::Value>, width: usize) -> Vec<String> {
    let Some(v) = args else {
        return vec!["(not captured)".to_string()];
    };
    let pretty = serde_json::to_string_pretty(v).unwrap_or_else(|_| "<unserializable>".to_string());
    let mut out = Vec::new();
    let width = width.max(1);
    for raw_line in pretty.lines() {
        if raw_line.chars().count() <= width {
            out.push(raw_line.to_string());
        } else {
            let indent: String = raw_line.chars().take_while(|c| c.is_whitespace()).collect();
            let body = &raw_line[indent.len()..];
            let effective_width = width.saturating_sub(indent.chars().count()).max(1);
            // Word-wrap on whitespace; fallback to char-chunk for long
            // tokens (e.g. long strings without spaces).
            let mut cur = String::new();
            for word in body.split_whitespace() {
                let candidate_len =
                    cur.chars().count() + usize::from(!cur.is_empty()) + word.chars().count();
                if candidate_len <= effective_width {
                    if !cur.is_empty() {
                        cur.push(' ');
                    }
                    cur.push_str(word);
                } else {
                    if !cur.is_empty() {
                        out.push(format!("{indent}{cur}"));
                        cur.clear();
                    }
                    // Word itself longer than effective_width → char-chunk it.
                    if word.chars().count() > effective_width {
                        let mut chunk = String::with_capacity(effective_width);
                        let mut chunk_chars = 0usize;
                        for c in word.chars() {
                            if chunk_chars == effective_width {
                                out.push(format!("{indent}{chunk}"));
                                chunk.clear();
                                chunk_chars = 0;
                            }
                            chunk.push(c);
                            chunk_chars += 1;
                        }
                        cur = chunk;
                    } else {
                        cur = word.to_string();
                    }
                }
            }
            if !cur.is_empty() {
                out.push(format!("{indent}{cur}"));
            }
        }
    }
    out
}

/// Status sigil for the per-call status badge.
///
/// Each [`ToolCallStatus`] variant gets its own glyph so the badge can
/// distinguish "we faked the start" from "never finished" at a glance:
///
/// - [`ToolCallStatus::Success`] → `"✓"` (U+2713 CHECK MARK)
/// - [`ToolCallStatus::Failure`] → `"✗"` (U+2717 BALLOT X)
/// - [`ToolCallStatus::OrphanSynthesizedStart`] → `"↯"` (U+21AF DOWNWARDS ZIGZAG ARROW)
/// - [`ToolCallStatus::OpenAtEndOfSession`] → `"⊘"` (U+2298 CIRCLED DIVISION SLASH)
///
/// Every known variant has an explicit arm. A trailing `_ => "?"` is
/// required only because [`ToolCallStatus`] is `#[non_exhaustive]` in
/// `agentprof-core`; if a new variant is added upstream the compiler
/// will still happily fall through, so reviewers MUST check this
/// function whenever `ToolCallStatus` grows a variant. The `"?"`
/// fallback is intentionally distinct so it surfaces visibly in the UI
/// if it ever fires.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::ToolCallStatus;
/// use agentprof_tui::views::turn_detail::status_sigil;
/// assert_eq!(status_sigil(&ToolCallStatus::Success), "✓");
/// assert_eq!(status_sigil(&ToolCallStatus::Failure { message: None }), "✗");
/// assert_eq!(status_sigil(&ToolCallStatus::OrphanSynthesizedStart), "↯");
/// assert_eq!(status_sigil(&ToolCallStatus::OpenAtEndOfSession), "⊘");
/// ```
#[must_use]
pub const fn status_sigil(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Success => "✓",
        ToolCallStatus::Failure { .. } => "✗",
        ToolCallStatus::OrphanSynthesizedStart => "↯",
        ToolCallStatus::OpenAtEndOfSession => "⊘",
        // Required because `ToolCallStatus` is `#[non_exhaustive]`.
        // See rustdoc above — reviewer must extend the match if a new
        // variant lands upstream rather than relying on this arm.
        _ => "?",
    }
}

#[cfg(test)]
mod formatter_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_args_preview_long_truncates() {
        let s_long = "a".repeat(200);
        let v = json!({"k": s_long});
        let s = format_args_preview(Some(&v), 80);
        assert!(s.chars().count() <= 80);
        assert!(s.ends_with('…'));
        assert!(
            s.chars().count() >= 2,
            "truncation should preserve a meaningful prefix, not just ellipsis"
        );
    }

    #[test]
    fn format_args_preview_short_no_truncation() {
        let v = json!({"x": 1});
        let s = format_args_preview(Some(&v), 80);
        assert_eq!(s, r#"{"x":1}"#);
    }

    #[test]
    fn format_args_preview_none_yields_not_captured() {
        assert_eq!(format_args_preview(None, 80), "(not captured)");
    }

    #[test]
    fn format_args_preview_zero_max_yields_empty_string() {
        let v = json!({"x": 1});
        assert_eq!(format_args_preview(Some(&v), 0), "");
    }

    #[test]
    fn wrap_args_full_pretty_short_fits() {
        let v = json!({"a": 1});
        let lines = wrap_args_full(Some(&v), 80);
        assert!(lines.iter().all(|l| l.chars().count() <= 80));
        // pretty JSON of {"a":1} is "{\n  \"a\": 1\n}" → 3 lines.
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn wrap_args_full_none_yields_not_captured() {
        assert_eq!(wrap_args_full(None, 80), vec!["(not captured)".to_string()]);
    }

    #[test]
    fn wrap_args_full_zero_width_no_panic() {
        let v = json!({"a": 1});
        let _ = wrap_args_full(Some(&v), 0);
    }

    #[test]
    fn wrap_args_full_long_no_whitespace_token_perf_safe() {
        // 1024-char single token with no whitespace, wrap width 80.
        // Implementation must not re-walk the chunk on every push.
        let long_token = "a".repeat(1024);
        let v = json!({"k": long_token});
        let lines = wrap_args_full(Some(&v), 80);
        assert!(lines.iter().all(|l| l.chars().count() <= 80));
        assert!(
            lines.len() >= 12,
            "expected ≥12 wrapped lines for 1024-char token"
        );
    }

    #[test]
    fn wrap_args_full_preserves_indentation() {
        // Pretty JSON of {"k": "a long string that needs wrapping..."}
        // should produce wrapped lines that preserve the leading 2-space
        // indentation on the body lines.
        let v = json!({"k": "a long string that exceeds the wrap width when included"});
        let lines = wrap_args_full(Some(&v), 30);
        // First and last lines are "{" and "}" (no indent).
        assert_eq!(lines[0], "{");
        assert_eq!(&lines[lines.len() - 1], "}");
        // All middle lines (the wrapped "k": "..." value) should start with "  " (2-space indent).
        for line in &lines[1..lines.len() - 1] {
            assert!(line.starts_with("  "), "middle line lost indent: {line:?}");
        }
    }

    #[test]
    fn status_sigil_all_variants() {
        assert_eq!(status_sigil(&ToolCallStatus::Success), "✓");
        assert_eq!(
            status_sigil(&ToolCallStatus::Failure { message: None }),
            "✗"
        );
        assert_eq!(status_sigil(&ToolCallStatus::OrphanSynthesizedStart), "↯");
        assert_eq!(status_sigil(&ToolCallStatus::OpenAtEndOfSession), "⊘");
    }
}

// render_turn_detail() lands in Task 6
