//! Color palette + style modifiers for the TUI views.
//!
//! Maps `agentprof_core::model::ToolSource` variants to distinct foreground
//! colors used by `views::flamegraph` and `views::roi`. Failure / orphan /
//! aborted modifiers are exposed as `Style` helpers.

use agentprof_core::episode::ToolCallStatus;
use agentprof_core::model::ToolSource;
use ratatui::style::{Color, Modifier, Style};

/// Foreground color used for a tool call segment in `FlamegraphView`.
///
/// `ToolSource` is `#[non_exhaustive]`; the `_` arm renders future variants
/// as gray so a wire-format addition cannot cause a panic.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::ToolSource;
/// use agentprof_tui::theme::tool_source_color;
/// use ratatui::style::Color;
/// assert_eq!(tool_source_color(&ToolSource::Builtin), Color::Cyan);
/// ```
#[must_use]
pub const fn tool_source_color(source: &ToolSource) -> Color {
    match source {
        ToolSource::Builtin => Color::Cyan,
        ToolSource::Mcp { .. } => Color::Magenta,
        ToolSource::Skill { .. } => Color::Yellow,
        _ => Color::Gray,
    }
}

/// Style modifier overlay for a tool call given its terminal status.
///
/// `Failure` paints the cell background red (overrides the source color so
/// failures are unmissable). `OrphanSynthesizedStart` and `OpenAtEndOfSession`
/// use `Modifier::DIM` to signal "data gap, not real timing". `Success`
/// returns `Style::default()`.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::ToolCallStatus;
/// use agentprof_tui::theme::status_style;
/// use ratatui::style::{Color, Style};
/// assert_eq!(status_style(&ToolCallStatus::Success), Style::default());
/// assert_eq!(
///     status_style(&ToolCallStatus::Failure { message: None }).bg,
///     Some(Color::Red),
/// );
/// ```
#[must_use]
pub fn status_style(status: &ToolCallStatus) -> Style {
    match status {
        ToolCallStatus::Failure { .. } => Style::default().bg(Color::Red),
        ToolCallStatus::OrphanSynthesizedStart | ToolCallStatus::OpenAtEndOfSession => {
            Style::default().add_modifier(Modifier::DIM)
        }
        // ToolCallStatus::Success and future non_exhaustive variants:
        // unknown statuses render as default to stay panic-free.
        _ => Style::default(),
    }
}

/// Modifier applied to a tool call segment that was manually approved
/// (`ToolCall.user_requested == true`).
///
/// Renders the segment in italics so it visually separates from agent-driven
/// tool calls. Combined with `tool_source_color` in `FlamegraphView`.
///
/// # Examples
///
/// ```
/// use agentprof_tui::theme::user_requested_modifier;
/// use ratatui::style::Modifier;
/// assert_eq!(user_requested_modifier(), Modifier::ITALIC);
/// ```
#[must_use]
pub const fn user_requested_modifier() -> Modifier {
    Modifier::ITALIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_source_colors_distinct_across_known_variants() {
        let b = tool_source_color(&ToolSource::Builtin);
        let m = tool_source_color(&ToolSource::Mcp { server: "x".into() });
        let s = tool_source_color(&ToolSource::Skill { name: "y".into() });
        assert_ne!(b, m);
        assert_ne!(b, s);
        assert_ne!(m, s);
    }

    #[test]
    fn status_style_failure_paints_red_background() {
        let st = status_style(&ToolCallStatus::Failure { message: None });
        assert_eq!(st.bg, Some(Color::Red));
    }

    #[test]
    fn status_style_orphan_dims() {
        let st = status_style(&ToolCallStatus::OrphanSynthesizedStart);
        assert!(st.add_modifier.contains(Modifier::DIM));
    }
}
