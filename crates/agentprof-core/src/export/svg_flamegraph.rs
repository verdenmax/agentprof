//! SVG flamegraph emitter for embedding in static HTML reports.
//!
//! Renders a responsive (viewBox-scaled) SVG of the session timeline:
//! a grey root rect for the session, one grey rect per turn, and
//! [`crate::model::ToolSource`]-colored rects for each tool / hook /
//! skill call. No JS; styling is inline `fill` attributes for maximum
//! portability across email clients and static viewers.
//!
//! See `docs/superpowers/specs/2026-05-29-m1.6.4-speedscope-and-html-export-design.md`
//! D-4 (responsive viewBox) and D-5 (color palette).

use std::fmt::Write as _;

use chrono::{DateTime, Utc};

use crate::episode::Episodes;
use crate::model::{SessionMeta, ToolSource};

const ROW_HEIGHT: i64 = 50;
const MIN_VIEWBOX_W: i64 = 1000;
const MAX_VIEWBOX_W: i64 = 5000;
const PER_TURN_VIEWBOX: i64 = 30;
const MAX_DEPTH: usize = 4;
const TEXT_MIN_RECT_WIDTH: i64 = 30;

// Colors per spec D-5.
const COLOR_SESSION: &str = "#6b7280"; // grey
const COLOR_TURN: &str = "#9ca3af"; // lighter grey
const COLOR_BUILTIN: &str = "#3b82f6"; // blue
const COLOR_MCP: &str = "#a855f7"; // purple
const COLOR_HOOK: &str = "#f97316"; // orange
const COLOR_SKILL: &str = "#10b981"; // green

/// In-memory representation of an SVG flamegraph, ready to serialize.
///
/// Build via [`Self::from_episodes`] then call [`Self::into_svg_string`]
/// to obtain a self-contained SVG string suitable for embedding in HTML
/// (write as raw markup; do **not** HTML-escape the `<`/`>`).
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::export::svg_flamegraph::SvgFlamegraph;
/// use agentprof_core::model::SessionMeta;
/// use chrono::Utc;
///
/// let meta = SessionMeta::new("s1".into(), AgentKind::Copilot, Utc::now(), false);
/// let svg = SvgFlamegraph::from_episodes(&Episodes::new(), &meta).into_svg_string();
/// assert!(svg.starts_with("<svg"));
/// assert!(svg.ends_with("</svg>"));
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SvgFlamegraph {
    rects: Vec<Rect>,
    viewbox_w: i64,
    viewbox_h: i64,
}

#[derive(Debug, Clone)]
struct Rect {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    fill: &'static str,
    label: String,
    title: String,
}

impl SvgFlamegraph {
    /// Build a flamegraph from `episodes` + `meta`.
    ///
    /// The session start anchors the X origin; the X scale is chosen so
    /// the X axis spans the full session wall-clock duration. The viewBox
    /// width grows with turn count (`30 units/turn`, clamped to
    /// `[1000, 5000]`) so wide sessions remain readable without changing
    /// per-row height. Skills are emitted as zero-duration rects at the
    /// invocation instant (still visible thanks to the `>= 1` width
    /// width floor in `position`).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::export::svg_flamegraph::SvgFlamegraph;
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s1".into(), AgentKind::Copilot, Utc::now(), false);
    /// let fg = SvgFlamegraph::from_episodes(&Episodes::new(), &meta);
    /// // Even an empty Episodes still yields the session root rect.
    /// assert!(fg.into_svg_string().contains("session"));
    /// ```
    #[must_use]
    pub fn from_episodes(episodes: &Episodes, meta: &SessionMeta) -> Self {
        let session_start = meta.started_at;
        let session_end = compute_session_end(episodes, session_start);
        let total_ms = (session_end - session_start).num_milliseconds().max(1);

        let turn_count = i64::try_from(episodes.turns.len())
            .unwrap_or(i64::MAX)
            .max(1);
        let viewbox_w =
            (turn_count.saturating_mul(PER_TURN_VIEWBOX)).clamp(MIN_VIEWBOX_W, MAX_VIEWBOX_W);
        let viewbox_h = ROW_HEIGHT * (i64::try_from(MAX_DEPTH).unwrap_or(4) + 1);

        let mut rects: Vec<Rect> = Vec::new();

        // Session root spans the full width.
        rects.push(Rect {
            x: 0,
            y: 0,
            w: viewbox_w,
            h: ROW_HEIGHT,
            fill: COLOR_SESSION,
            label: "session".to_string(),
            title: format!("session ({total_ms} ms)"),
        });

        for turn in &episodes.turns {
            push_turn_rects(
                &mut rects,
                turn,
                episodes,
                session_start,
                session_end,
                total_ms,
                viewbox_w,
            );
        }

        Self {
            rects,
            viewbox_w,
            viewbox_h,
        }
    }

    /// Serialize to a self-contained SVG string suitable for embedding
    /// into HTML (write as raw markup; do not escape `<` / `>`).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::adapter::AgentKind;
    /// use agentprof_core::episode::Episodes;
    /// use agentprof_core::export::svg_flamegraph::SvgFlamegraph;
    /// use agentprof_core::model::SessionMeta;
    /// use chrono::Utc;
    ///
    /// let meta = SessionMeta::new("s1".into(), AgentKind::Copilot, Utc::now(), false);
    /// let svg = SvgFlamegraph::from_episodes(&Episodes::new(), &meta).into_svg_string();
    /// assert!(svg.contains("viewBox=\""));
    /// ```
    #[must_use]
    pub fn into_svg_string(self) -> String {
        let mut out = String::with_capacity(2048);
        // `write!` into String is infallible — using `let _ =` keeps it
        // panic-free without `unwrap`.
        let _ = write!(
            &mut out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="100%" preserveAspectRatio="xMidYMid meet" font-family="sans-serif" font-size="12">"#,
            w = self.viewbox_w,
            h = self.viewbox_h,
        );
        for r in &self.rects {
            let _ = write!(
                &mut out,
                r#"<g><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="white" stroke-width="0.5"><title>{title}</title></rect>"#,
                x = r.x,
                y = r.y,
                w = r.w,
                h = r.h,
                fill = r.fill,
                title = escape_xml(&r.title),
            );
            if r.w >= TEXT_MIN_RECT_WIDTH {
                let max_chars = usize::try_from(((r.w - 6) / 7).max(0)).unwrap_or(0);
                let _ = write!(
                    &mut out,
                    r#"<text x="{tx}" y="{ty}" fill="white" pointer-events="none">{label}</text>"#,
                    tx = r.x + 4,
                    ty = r.y + r.h - 18,
                    label = escape_xml(truncate(&r.label, max_chars)),
                );
            }
            out.push_str("</g>");
        }
        out.push_str("</svg>");
        out
    }
}

fn push_turn_rects(
    rects: &mut Vec<Rect>,
    turn: &crate::episode::Turn,
    episodes: &Episodes,
    session_start: DateTime<Utc>,
    session_end: DateTime<Utc>,
    total_ms: i64,
    viewbox_w: i64,
) {
    let turn_end_ts = turn.ended_at.unwrap_or(session_end);
    let (tx, tw) = position(
        turn.started_at,
        turn_end_ts,
        session_start,
        total_ms,
        viewbox_w,
    );
    let turn_dur = (turn_end_ts - turn.started_at).num_milliseconds().max(0);
    let label = format!("turn-{}", turn.id);
    rects.push(Rect {
        x: tx,
        y: ROW_HEIGHT,
        w: tw,
        h: ROW_HEIGHT,
        fill: COLOR_TURN,
        label: label.clone(),
        title: format!("{label} ({turn_dur} ms)"),
    });

    for tool_ref in &turn.tool_calls {
        if let Some(tool) = episodes.tools.get(&tool_ref.name) {
            if let Some(call) = tool.calls.get(tool_ref.index) {
                let (cx, cw) = position(
                    call.span.started_at,
                    call.span.ended_at,
                    session_start,
                    total_ms,
                    viewbox_w,
                );
                let fill = color_for_source(&tool.source);
                let label = display_name(&tool.name, &tool.source);
                rects.push(Rect {
                    x: cx,
                    y: ROW_HEIGHT * 2,
                    w: cw,
                    h: ROW_HEIGHT,
                    fill,
                    label: label.clone(),
                    title: label,
                });
            }
        }
    }

    for hook_ref in &turn.hook_calls {
        if let Some(hook) = episodes.hooks.get(&hook_ref.name) {
            if let Some(call) = hook.calls.get(hook_ref.index) {
                let (cx, cw) = position(
                    call.span.started_at,
                    call.span.ended_at,
                    session_start,
                    total_ms,
                    viewbox_w,
                );
                let label = format!("hook:{}", hook_ref.name);
                rects.push(Rect {
                    x: cx,
                    y: ROW_HEIGHT * 2,
                    w: cw,
                    h: ROW_HEIGHT,
                    fill: COLOR_HOOK,
                    label: label.clone(),
                    title: label,
                });
            }
        }
    }

    // Skills are instants per T1's `SkillInvocation { at }` shape;
    // emit a zero-duration rect (still visible due to the >=1 width
    // floor) so the green skill marker is preserved on the timeline.
    for skill_ref in &turn.skill_calls {
        if let Some(skill) = episodes.skills.get(&skill_ref.name) {
            if let Some(inv) = skill.invocations.get(skill_ref.index) {
                let (cx, cw) = position(inv.at, inv.at, session_start, total_ms, viewbox_w);
                let label = format!("skill:{}", skill_ref.name);
                rects.push(Rect {
                    x: cx,
                    y: ROW_HEIGHT * 2,
                    w: cw,
                    h: ROW_HEIGHT,
                    fill: COLOR_SKILL,
                    label: label.clone(),
                    title: label,
                });
            }
        }
    }
}

fn position(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    session_start: DateTime<Utc>,
    total_ms: i64,
    viewbox_w: i64,
) -> (i64, i64) {
    let s = (start - session_start).num_milliseconds().max(0);
    let e = (end - session_start).num_milliseconds().max(s);
    let x = s.saturating_mul(viewbox_w) / total_ms;
    let w = ((e - s).saturating_mul(viewbox_w) / total_ms).max(1);
    (x, w)
}

const fn color_for_source(source: &ToolSource) -> &'static str {
    match source {
        ToolSource::Builtin => COLOR_BUILTIN,
        ToolSource::Mcp { .. } => COLOR_MCP,
        ToolSource::Skill { .. } => COLOR_SKILL,
    }
}

fn display_name(raw: &str, source: &ToolSource) -> String {
    match source {
        ToolSource::Builtin => raw.to_string(),
        ToolSource::Mcp { server } => {
            let leaf = raw.strip_prefix(&format!("mcp__{server}__")).unwrap_or(raw);
            format!("mcp:{server}::{leaf}")
        }
        ToolSource::Skill { name } => {
            let leaf = raw.strip_prefix(&format!("skill__{name}__")).unwrap_or(raw);
            format!("skill:{name}:{leaf}")
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    s.char_indices()
        .nth(max_chars)
        .map_or(s, |(idx, _)| &s[..idx])
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn compute_session_end(episodes: &Episodes, session_start: DateTime<Utc>) -> DateTime<Utc> {
    let mut latest = session_start;
    for turn in &episodes.turns {
        if let Some(end) = turn.ended_at {
            if end > latest {
                latest = end;
            }
        }
    }
    for tool in episodes.tools.values() {
        for call in &tool.calls {
            if call.span.ended_at > latest {
                latest = call.span.ended_at;
            }
        }
    }
    for hook in episodes.hooks.values() {
        for call in &hook.calls {
            if call.span.ended_at > latest {
                latest = call.span.ended_at;
            }
        }
    }
    for skill in episodes.skills.values() {
        for inv in &skill.invocations {
            if inv.at > latest {
                latest = inv.at;
            }
        }
    }
    latest
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::adapter::AgentKind;
    use chrono::TimeZone;

    fn meta_at(s: u32) -> SessionMeta {
        SessionMeta::new(
            "test-session".into(),
            AgentKind::Copilot,
            Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, s).unwrap(),
            false,
        )
    }

    #[test]
    fn empty_episodes_still_emits_session_root() {
        let svg = SvgFlamegraph::from_episodes(&Episodes::new(), &meta_at(0)).into_svg_string();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("fill=\"#6b7280\""));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn no_script_tags_emitted() {
        let svg = SvgFlamegraph::from_episodes(&Episodes::new(), &meta_at(0)).into_svg_string();
        assert!(!svg.contains("<script"));
    }

    #[test]
    fn viewbox_clamped_within_bounds() {
        let svg = SvgFlamegraph::from_episodes(&Episodes::new(), &meta_at(0)).into_svg_string();
        // Empty: turn_count clamped to 1 → 30 → clamped to MIN_VIEWBOX_W=1000.
        assert!(svg.contains("viewBox=\"0 0 1000 250\""), "got: {svg}");
    }

    #[test]
    fn escape_xml_handles_specials() {
        assert_eq!(
            escape_xml("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn truncate_handles_multibyte_safely() {
        assert_eq!(truncate("héllo", 2), "hé");
        assert_eq!(truncate("abc", 100), "abc");
        assert_eq!(truncate("abc", 0), "");
    }

    #[test]
    fn display_name_strips_known_prefixes() {
        assert_eq!(display_name("bash", &ToolSource::Builtin), "bash");
        assert_eq!(
            display_name(
                "mcp__github__search_issues",
                &ToolSource::Mcp {
                    server: "github".into()
                }
            ),
            "mcp:github::search_issues"
        );
        assert_eq!(
            display_name(
                "skill__brainstorming__present",
                &ToolSource::Skill {
                    name: "brainstorming".into()
                }
            ),
            "skill:brainstorming:present"
        );
    }
}
