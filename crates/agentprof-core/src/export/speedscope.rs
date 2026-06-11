//! Speedscope evented JSON profile emitter.
//!
//! See `docs/internals/adr-0007-speedscope-export.md` (added in T3) for the
//! decisions behind frame naming, dedup, orphan handling, and span overlap
//! adjustment.
//!
//! The wire format is documented at
//! <https://github.com/jlfwong/speedscope/blob/main/file-format.md>.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::episode::Episodes;
use crate::model::{SessionMeta, ToolSource};

use super::ExportWarning;

/// Root document of a Speedscope file.
///
/// Conforms to the schema at <https://github.com/jlfwong/speedscope/blob/main/file-format.md>.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::speedscope::SpeedscopeProfile;
/// assert!(SpeedscopeProfile::SCHEMA_URL.starts_with("https://"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SpeedscopeProfile {
    /// JSON Schema URL Speedscope uses to validate uploaded files.
    #[serde(rename = "$schema")]
    pub schema: String,
    /// Human-readable profile name shown in the Speedscope UI.
    pub name: String,
    /// Free-form identifier of the tool that produced this file.
    pub exporter: String,
    /// Shared frame table referenced by `profiles[*].events[*].frame`.
    pub shared: Shared,
    /// One or more profiles inside this document; agentprof emits exactly one.
    pub profiles: Vec<Profile>,
}

impl SpeedscopeProfile {
    /// JSON Schema URL embedded as the `$schema` field.
    pub const SCHEMA_URL: &'static str = "https://www.speedscope.app/file-format-schema.json";

    /// Construct a `SpeedscopeProfile` from parts.
    ///
    /// Provided so external callers (and tests in other crates) can build
    /// a profile despite `#[non_exhaustive]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::export::speedscope::{Shared, SpeedscopeProfile};
    /// let p = SpeedscopeProfile::new(
    ///     SpeedscopeProfile::SCHEMA_URL.into(),
    ///     "n".into(),
    ///     "agentprof v0.0.0".into(),
    ///     Shared::new(vec![]),
    ///     vec![],
    /// );
    /// assert!(p.profiles.is_empty());
    /// ```
    #[must_use]
    pub const fn new(
        schema: String,
        name: String,
        exporter: String,
        shared: Shared,
        profiles: Vec<Profile>,
    ) -> Self {
        Self {
            schema,
            name,
            exporter,
            shared,
            profiles,
        }
    }
}

/// Shared per-document data referenced by all profiles in the file.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::speedscope::{Frame, Shared};
/// let s = Shared::new(vec![Frame::new("session".into())]);
/// assert_eq!(s.frames.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Shared {
    /// Global frame table. Indices into this vector appear in `Event::frame`.
    pub frames: Vec<Frame>,
}

impl Shared {
    /// Construct from a frame table.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::export::speedscope::Shared;
    /// assert!(Shared::new(vec![]).frames.is_empty());
    /// ```
    #[must_use]
    pub const fn new(frames: Vec<Frame>) -> Self {
        Self { frames }
    }
}

/// One entry in the shared frame table.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::speedscope::Frame;
/// let f = Frame::new("bash".into());
/// assert_eq!(f.name, "bash");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Frame {
    /// Display name shown in the Speedscope flamegraph.
    pub name: String,
}

impl Frame {
    /// Construct a `Frame` with the given display name.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::export::speedscope::Frame;
    /// assert_eq!(Frame::new("bash".into()).name, "bash");
    /// ```
    #[must_use]
    pub const fn new(name: String) -> Self {
        Self { name }
    }
}

/// A single profile inside a Speedscope document.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::speedscope::{Profile, ProfileType};
/// let p = Profile::new(
///     ProfileType::Evented,
///     "wall-clock".into(),
///     "milliseconds".into(),
///     0,
///     0,
///     vec![],
/// );
/// assert_eq!(p.unit, "milliseconds");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Profile {
    /// Profile kind; agentprof always emits [`ProfileType::Evented`].
    #[serde(rename = "type")]
    pub ty: ProfileType,
    /// Profile name shown in the Speedscope sidebar.
    pub name: String,
    /// Unit of `at` / `start_value` / `end_value`; agentprof emits
    /// `"milliseconds"` (D-9).
    pub unit: String,
    /// Inclusive start of the profile's time domain; agentprof emits `0`
    /// after anchoring to `session.started_at` (D-10).
    pub start_value: i64,
    /// Inclusive end of the profile's time domain, in `unit`.
    pub end_value: i64,
    /// Ordered open/close events; must be strictly nested.
    pub events: Vec<Event>,
}

impl Profile {
    /// Construct a `Profile` from parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::export::speedscope::{Profile, ProfileType};
    /// let p = Profile::new(ProfileType::Evented, "wall-clock".into(), "milliseconds".into(), 0, 0, vec![]);
    /// assert_eq!(p.unit, "milliseconds");
    /// ```
    #[must_use]
    pub const fn new(
        ty: ProfileType,
        name: String,
        unit: String,
        start_value: i64,
        end_value: i64,
        events: Vec<Event>,
    ) -> Self {
        Self {
            ty,
            name,
            unit,
            start_value,
            end_value,
            events,
        }
    }
}

/// Kind of a Speedscope profile.
///
/// agentprof only emits the `evented` variant; `sampled` is reserved for
/// future use.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::speedscope::ProfileType;
/// let json = serde_json::to_string(&ProfileType::Evented).unwrap();
/// assert_eq!(json, "\"evented\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ProfileType {
    /// Evented (open/close pair) profile.
    Evented,
}

/// One open or close event in an evented profile.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::speedscope::{Event, EventType};
/// let e = Event::new(EventType::Open, 0, 0);
/// assert_eq!(e.at, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Event {
    /// Whether the event opens or closes a stack frame.
    #[serde(rename = "type")]
    pub ty: EventType,
    /// Time of the event in the profile's `unit`.
    pub at: i64,
    /// Index into `Shared::frames` naming the frame being opened/closed.
    pub frame: usize,
}

impl Event {
    /// Construct an `Event` from parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::export::speedscope::{Event, EventType};
    /// assert_eq!(Event::new(EventType::Open, 0, 0).at, 0);
    /// ```
    #[must_use]
    pub const fn new(ty: EventType, at: i64, frame: usize) -> Self {
        Self { ty, at, frame }
    }
}

/// Whether a Speedscope event opens or closes a frame.
///
/// # Examples
///
/// ```
/// use agentprof_core::export::speedscope::EventType;
/// assert_eq!(serde_json::to_string(&EventType::Open).unwrap(), "\"O\"");
/// assert_eq!(serde_json::to_string(&EventType::Close).unwrap(), "\"C\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventType {
    /// `"O"` — open / enter frame.
    #[serde(rename = "O")]
    Open,
    /// `"C"` — close / leave frame.
    #[serde(rename = "C")]
    Close,
}

/// Convert episodes + metadata into a Speedscope profile.
///
/// The resulting [`SpeedscopeProfile`] is timestamp-normalized so the
/// session's `started_at` sits at `at = 0` (D-10); this makes the output
/// reproducible for snapshot tests. The unit is `"milliseconds"` (D-9).
///
/// Frame naming follows D-11: builtin `<name>`, MCP `mcp:<server>::<leaf>`,
/// hook `hook:<name>`, skill `skill:<skill>` (one frame per skill,
/// aggregated across all invocations — mirrors the dedup behavior used
/// for tools), synthetic `session`, `turn-<N>`, `turn-<N> (open)`,
/// `turn-orphan`. Frames are deduplicated globally (D-12) and orphan
/// tool calls are grouped under a trailing `turn-orphan` frame (D-14).
/// Open turns get an `(open)` suffix and a synthetic close at the last
/// observed event timestamp (D-13).
///
/// Skill invocations are zero-duration instants (`Open` and `Close`
/// emitted at the same `inv.at` timestamp); per-invocation timing is
/// preserved while all invocations of the same skill share a single
/// frame in `shared.frames`.
///
/// Returns the profile plus any [`ExportWarning`]s emitted while
/// adjusting overlapping sibling spans for Speedscope's strict-nesting
/// requirement (D-15).
///
/// # Examples
///
/// ```
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::episode::Episodes;
/// use agentprof_core::export::speedscope::to_speedscope;
/// use agentprof_core::model::SessionMeta;
/// use chrono::{TimeZone, Utc};
///
/// let meta = SessionMeta::new(
///     "abc-123".into(),
///     AgentKind::Copilot,
///     Utc.with_ymd_and_hms(2026, 5, 31, 13, 0, 0).unwrap(),
///     false,
/// );
/// let episodes = Episodes::default();
/// let (profile, warnings) = to_speedscope(&episodes, &meta, "0.0.0");
/// assert_eq!(profile.profiles[0].unit, "milliseconds");
/// assert!(warnings.is_empty());
/// ```
#[must_use]
pub fn to_speedscope(
    episodes: &Episodes,
    meta: &SessionMeta,
    agentprof_version: &str,
) -> (SpeedscopeProfile, Vec<ExportWarning>) {
    let session_start = meta.started_at;
    let session_end = compute_session_end(episodes, session_start);
    let total_ms = duration_ms(session_end - session_start);

    let has_orphan_tool_calls = episodes
        .tools
        .values()
        .any(|t| t.calls.iter().any(|c| c.turn_id.is_none()));

    let frame_names = build_frame_table(episodes, has_orphan_tool_calls);
    let frame_index: BTreeMap<String, usize> = frame_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();

    let mut events: Vec<Event> = Vec::new();
    let mut warnings: Vec<ExportWarning> = Vec::new();

    let session_frame = lookup(&frame_index, "session");
    events.push(Event::new(EventType::Open, 0, session_frame));

    let final_total_ms;
    {
        let mut ctx = EmitCtx {
            episodes,
            session_start,
            session_end,
            total_ms,
            frame_index: &frame_index,
            events: &mut events,
            warnings: &mut warnings,
            last_emitted_at: 0,
        };
        for (ordinal, turn) in episodes.turns.iter().enumerate() {
            emit_turn(&mut ctx, ordinal, turn);
        }

        if has_orphan_tool_calls {
            emit_orphans(&mut ctx);
        }
        // The session-close must come after the last emitted event;
        // otherwise overlap adjustments inside the orphan section (or a
        // clamp-induced shift) can push a child past `total_ms`,
        // violating per-stack at-monotonicity at the session boundary.
        final_total_ms = total_ms.max(ctx.last_emitted_at);
    }

    events.push(Event::new(EventType::Close, final_total_ms, session_frame));

    let profile = SpeedscopeProfile::new(
        SpeedscopeProfile::SCHEMA_URL.to_string(),
        format!("agentprof {} {}", short_id(&meta.id), meta.agent),
        format!("agentprof v{agentprof_version}"),
        Shared::new(frame_names.into_iter().map(Frame::new).collect()),
        vec![Profile::new(
            ProfileType::Evented,
            "wall-clock".to_string(),
            "milliseconds".to_string(),
            0,
            final_total_ms,
            events,
        )],
    );

    (profile, warnings)
}

fn build_frame_table(episodes: &Episodes, has_orphan_tool_calls: bool) -> Vec<String> {
    let mut frame_names: Vec<String> = Vec::new();
    frame_names.push("session".to_string());

    for (ordinal, turn) in episodes.turns.iter().enumerate() {
        frame_names.push(turn_frame_name(ordinal, turn.ended_at.is_none()));
    }
    if has_orphan_tool_calls {
        frame_names.push("turn-orphan".to_string());
    }

    let mut leaf_names: BTreeSet<String> = BTreeSet::new();
    for tool in episodes.tools.values() {
        leaf_names.insert(format_tool_frame_name(&tool.name, &tool.source));
    }
    for hook in episodes.hooks.values() {
        leaf_names.insert(format!("hook:{}", hook.name));
    }
    for skill in episodes.skills.values() {
        leaf_names.insert(format!("skill:{}", skill.name));
    }
    frame_names.extend(leaf_names);
    frame_names
}

fn collect_turn_children(turn: &crate::episode::Turn, episodes: &Episodes) -> Vec<Child> {
    let mut children: Vec<Child> = Vec::new();
    for tool_ref in &turn.tool_calls {
        if let Some(tool) = episodes.tools.get(&tool_ref.name) {
            if let Some(call) = tool.calls.get(tool_ref.index) {
                children.push(Child {
                    started_at: call.span.started_at,
                    ended_at: call.span.ended_at,
                    frame_name: format_tool_frame_name(&tool_ref.name, &tool.source),
                    warning_label: tool_ref.name.clone(),
                });
            }
        }
    }
    for hook_ref in &turn.hook_calls {
        if let Some(hook) = episodes.hooks.get(&hook_ref.name) {
            if let Some(call) = hook.calls.get(hook_ref.index) {
                children.push(Child {
                    started_at: call.span.started_at,
                    ended_at: call.span.ended_at,
                    frame_name: format!("hook:{}", hook_ref.name),
                    warning_label: format!("hook:{}", hook_ref.name),
                });
            }
        }
    }
    for skill_ref in &turn.skill_calls {
        if let Some(skill) = episodes.skills.get(&skill_ref.name) {
            if let Some(inv) = skill.invocations.get(skill_ref.index) {
                // Skill invocations are instants — emit a zero-duration span.
                // All invocations of the same skill share one frame
                // (`skill:<name>`), matching the dedup behavior used for
                // tools so the viewer can show cumulative skill cost.
                children.push(Child {
                    started_at: inv.at,
                    ended_at: inv.at,
                    frame_name: format!("skill:{}", skill_ref.name),
                    warning_label: format!("skill:{}", skill_ref.name),
                });
            }
        }
    }
    children.sort_by_key(|c| c.started_at);
    children
}

/// Shared context threaded through every `emit_*` helper.
///
/// Bundles the session-scoped inputs (episodes, time bounds, frame
/// dedup table) and the two shared output buffers (`events`, `warnings`)
/// so per-call signatures only carry per-call data (e.g. ordinal +
/// `&Turn`). Lifetime `'a` ties all borrows to the surrounding
/// `to_speedscope` invocation.
///
/// `last_emitted_at` tracks the largest `at` value pushed so far (in
/// ms from session start). It backs the B-4.2 clamp that prevents the
/// orphan section from beginning with a descending `at` across the
/// last-in-turn → first-orphan boundary.
struct EmitCtx<'a> {
    episodes: &'a Episodes,
    session_start: DateTime<Utc>,
    session_end: DateTime<Utc>,
    total_ms: i64,
    frame_index: &'a BTreeMap<String, usize>,
    events: &'a mut Vec<Event>,
    warnings: &'a mut Vec<ExportWarning>,
    last_emitted_at: i64,
}

impl EmitCtx<'_> {
    /// Push an event and advance `last_emitted_at` if `at` is later than
    /// any previously emitted timestamp. Centralizing the push keeps the
    /// monotonicity tracker honest.
    fn push_event(&mut self, ty: EventType, at: i64, frame: usize) {
        self.events.push(Event::new(ty, at, frame));
        if at > self.last_emitted_at {
            self.last_emitted_at = at;
        }
    }
}

/// Compute the duration from `started_at` to `ended_at` in ms, clamped
/// to `0` for output safety, and emit a [`ExportWarning::NegativeDurationClamped`]
/// when the input would have been negative.
///
/// This is the `duration_ms_warn` helper from the B-4.3 follow-up: the
/// underlying `duration_ms` already silently `.max(0)`s the value, but
/// surfacing a warning lets the user know that a real timestamp inversion
/// exists in their session data (clock skew, parser bug, out-of-order
/// upstream events).
fn duration_ms_warn(
    ctx: &mut EmitCtx<'_>,
    name: &str,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
) -> i64 {
    if ended_at < started_at {
        ctx.warnings.push(ExportWarning::NegativeDurationClamped {
            name: name.to_string(),
            started_at,
            ended_at,
        });
    }
    duration_ms(ended_at - started_at)
}

fn emit_turn(ctx: &mut EmitCtx<'_>, ordinal: usize, turn: &crate::episode::Turn) {
    let turn_name = turn_frame_name(ordinal, turn.ended_at.is_none());
    let turn_frame = lookup(ctx.frame_index, &turn_name);
    let turn_start_ms = duration_ms_warn(
        ctx,
        &format!("turn-{} start", ordinal + 1),
        ctx.session_start,
        turn.started_at,
    );
    let turn_end_ms = if let Some(e) = turn.ended_at {
        duration_ms_warn(
            ctx,
            &format!("turn-{} end", ordinal + 1),
            ctx.session_start,
            e,
        )
    } else {
        // B-4.1: synthetic close for an open turn defaults to
        // `total_ms` (session end). If the open turn is followed by
        // another turn that began before session end, the synthetic
        // Close would overshoot that next-turn Open, violating
        // Speedscope's per-stack at-monotonicity invariant. Clamp
        // to `min(total_ms, turns[N+1].started_at_ms)` and emit a
        // warning when the clamp actually fires.
        let next_turn_start_ms = ctx
            .episodes
            .turns
            .get(ordinal + 1)
            .map(|next| duration_ms(next.started_at - ctx.session_start));
        let clamped = next_turn_start_ms.map_or(ctx.total_ms, |n| ctx.total_ms.min(n));
        if clamped < ctx.total_ms {
            ctx.warnings.push(ExportWarning::OpenTurnTruncated {
                turn_id: turn.id.clone(),
                original_at: ctx.total_ms,
                clamped_at: clamped,
            });
        }
        clamped
    };

    ctx.push_event(EventType::Open, turn_start_ms, turn_frame);

    let children = collect_turn_children(turn, ctx.episodes);
    let turn_close_bound = turn.ended_at.unwrap_or(ctx.session_end);
    let mut last_end = turn.started_at;
    for child in children {
        let (effective_start, effective_end) = adjust_for_overlap(
            child.started_at,
            child.ended_at,
            last_end,
            &child.warning_label,
            ctx.warnings,
        );
        let clamped_end = effective_end.min(turn_close_bound);
        let final_end = clamped_end.max(effective_start);
        let frame_idx = lookup(ctx.frame_index, &child.frame_name);
        ctx.push_event(
            EventType::Open,
            duration_ms(effective_start - ctx.session_start),
            frame_idx,
        );
        ctx.push_event(
            EventType::Close,
            duration_ms(final_end - ctx.session_start),
            frame_idx,
        );
        last_end = final_end;
    }

    ctx.push_event(EventType::Close, turn_end_ms, turn_frame);
}

fn emit_orphans(ctx: &mut EmitCtx<'_>) {
    let orphan_frame = lookup(ctx.frame_index, "turn-orphan");
    let mut orphans: Vec<(DateTime<Utc>, DateTime<Utc>, String)> = Vec::new();
    for tool in ctx.episodes.tools.values() {
        for call in &tool.calls {
            if call.turn_id.is_none() {
                let display = format_tool_frame_name(&tool.name, &tool.source);
                orphans.push((call.span.started_at, call.span.ended_at, display));
            }
        }
    }
    orphans.sort_by_key(|o| o.0);
    let (Some(first), Some(last)) = (orphans.first(), orphans.last()) else {
        return;
    };
    let orphan_start_ms = duration_ms(first.0 - ctx.session_start);

    // B-4.2: the first orphan may have begun before the last in-turn
    // event ended (e.g. an orphan tool call whose timestamp predates an
    // open turn's synthetic close). Clamp the orphan section's open
    // forward to `last_emitted_at` so Speedscope's monotonicity holds
    // across the boundary; emit a warning when the shift fires.
    let clamped_start_ms = orphan_start_ms.max(ctx.last_emitted_at);
    if clamped_start_ms > orphan_start_ms {
        ctx.warnings.push(ExportWarning::OrphanTimeShifted {
            orphan_kind: first.2.clone(),
            original_at: orphan_start_ms,
            shifted_to: clamped_start_ms,
        });
    }

    let orphan_end_ms = duration_ms(last.1 - ctx.session_start);
    ctx.push_event(EventType::Open, clamped_start_ms, orphan_frame);

    // Initialize `last_end` to the clamped boundary in `DateTime` form so
    // subsequent `adjust_for_overlap` calls preserve the shift for any
    // orphans whose `started_at` is also < `clamped_start_ms`.
    let clamped_start_dt = ctx.session_start + Duration::milliseconds(clamped_start_ms);
    let mut last_end = first.0.max(clamped_start_dt);
    for (s, e, display) in orphans {
        let (effective_start, effective_end) =
            adjust_for_overlap(s, e, last_end, &display, ctx.warnings);
        let final_end = effective_end.max(effective_start);
        let frame_idx = lookup(ctx.frame_index, &display);
        ctx.push_event(
            EventType::Open,
            duration_ms(effective_start - ctx.session_start),
            frame_idx,
        );
        ctx.push_event(
            EventType::Close,
            duration_ms(final_end - ctx.session_start),
            frame_idx,
        );
        last_end = final_end;
    }
    ctx.push_event(
        EventType::Close,
        orphan_end_ms
            .max(duration_ms(last_end - ctx.session_start))
            .max(clamped_start_ms),
        orphan_frame,
    );
}

// ---- Helpers ----

struct Child {
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    frame_name: String,
    warning_label: String,
}

fn adjust_for_overlap(
    original_start: DateTime<Utc>,
    original_end: DateTime<Utc>,
    last_end: DateTime<Utc>,
    warning_label: &str,
    warnings: &mut Vec<ExportWarning>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    if original_start < last_end {
        let adjusted = last_end + Duration::milliseconds(1);
        warnings.push(ExportWarning::SpanAdjustedForSpeedscope {
            tool_name: warning_label.to_string(),
            original_start,
            adjusted_start: adjusted,
        });
        let adj_end = if original_end <= adjusted {
            adjusted + Duration::milliseconds(1)
        } else {
            original_end
        };
        (adjusted, adj_end)
    } else {
        (original_start, original_end)
    }
}

fn turn_frame_name(ordinal: usize, open: bool) -> String {
    if open {
        format!("turn-{} (open)", ordinal + 1)
    } else {
        format!("turn-{}", ordinal + 1)
    }
}

fn format_tool_frame_name(name: &str, source: &ToolSource) -> String {
    match source {
        ToolSource::Builtin => name.to_string(),
        ToolSource::Mcp { server } => {
            let leaf = name
                .strip_prefix(&format!("mcp__{server}__"))
                .unwrap_or(name);
            format!("mcp:{server}::{leaf}")
        }
        ToolSource::Skill { name: skill } => {
            let leaf = name
                .strip_prefix(&format!("skill__{skill}__"))
                .unwrap_or(name);
            format!("skill:{skill}:{leaf}")
        }
    }
}

fn compute_session_end(episodes: &Episodes, session_start: DateTime<Utc>) -> DateTime<Utc> {
    let mut latest = session_start;
    for turn in &episodes.turns {
        if let Some(end) = turn.ended_at {
            if end > latest {
                latest = end;
            }
        } else if turn.started_at > latest {
            latest = turn.started_at;
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

fn duration_ms(d: Duration) -> i64 {
    d.num_milliseconds().max(0)
}

fn short_id(session_id: &str) -> String {
    session_id.chars().take(8).collect()
}

/// Look up a frame's index in the dedup table. Returns 0 (the `session`
/// frame) if the name is unknown, which should be impossible because
/// `build_frame_table` is the single source of truth.
///
/// In debug builds we additionally assert the name is present so any
/// regression in `build_frame_table` (e.g. an `emit_*` site emitting a
/// frame it forgot to register) trips a test failure instead of silently
/// collapsing onto the session frame. Release builds keep the silent
/// `0` fallback so a profile is still produced for end users.
fn lookup(idx: &BTreeMap<String, usize>, name: &str) -> usize {
    debug_assert!(
        idx.contains_key(name),
        "speedscope frame lookup miss for {name:?}: \
         build_frame_table must register every frame emitted by emit_*"
    );
    idx.get(name).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! B-4 timestamp-robustness unit tests (M1.6.4 follow-ups i1/i2/i3).
    //!
    //! Each test constructs a synthetic `Episodes` that triggers one of
    //! the three monotonicity-preserving clamps and asserts that:
    //!
    //! 1. the emitted events satisfy Speedscope's per-stack `at`
    //!    monotonicity invariant; and
    //! 2. the corresponding [`ExportWarning`] variant is surfaced.

    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::adapter::AgentKind;
    use crate::episode::tool::{ToolCall, ToolEpisode};
    use crate::episode::turn::{Span, Turn};
    use crate::model::{SessionMeta, ToolSource};
    use chrono::TimeZone;

    fn meta_at(start: DateTime<Utc>) -> SessionMeta {
        SessionMeta::new("sess-b4".into(), AgentKind::Copilot, start, false)
    }

    // ----- B-4.1: open turn followed by closed turn ------------------

    #[test]
    fn open_turn_close_is_clamped_to_next_turn_start() {
        let session_start = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let t1_start = session_start + Duration::seconds(1);
        let t2_start = session_start + Duration::seconds(2);
        let t2_end = session_start + Duration::seconds(5);

        let mut t1 = Turn::new("turn-1".into(), t1_start);
        t1.ended_at = None;
        let mut t2 = Turn::new("turn-2".into(), t2_start);
        t2.ended_at = Some(t2_end);

        let episodes = Episodes {
            turns: vec![t1, t2],
            ..Episodes::default()
        };

        let meta = meta_at(session_start);
        let (profile, warnings) = to_speedscope(&episodes, &meta, "0.0.0");

        let truncated: Vec<_> = warnings
            .iter()
            .filter_map(|w| match w {
                ExportWarning::OpenTurnTruncated {
                    turn_id,
                    original_at,
                    clamped_at,
                } => Some((turn_id.clone(), *original_at, *clamped_at)),
                _ => None,
            })
            .collect();
        assert_eq!(
            truncated.len(),
            1,
            "expected exactly one OpenTurnTruncated; warnings={warnings:?}"
        );
        let (turn_id, original_at, clamped_at) = &truncated[0];
        assert_eq!(turn_id, "turn-1");
        // total_ms = session_end - session_start = 5000 ms.
        assert_eq!(*original_at, 5000);
        // Clamped to next turn's start = 2000 ms.
        assert_eq!(*clamped_at, 2000);

        let events = &profile.profiles[0].events;
        for win in events.windows(2) {
            assert!(
                win[0].at <= win[1].at,
                "events must be at-monotonic; got {win:?}"
            );
        }
    }

    // ----- B-4.2: orphan tool call before last in-turn event ---------

    #[test]
    fn orphan_section_open_is_clamped_to_last_emitted_at() {
        let session_start = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let t1_start = session_start + Duration::seconds(1);
        let t1_end = session_start + Duration::seconds(10);

        let mut t1 = Turn::new("turn-1".into(), t1_start);
        t1.ended_at = Some(t1_end);

        // Orphan whose started_at is BEFORE turn-1 ended → its absolute
        // ms-offset is 2000 but the last emitted in-turn event (turn-1
        // close) is at 10000.
        let orphan_start = session_start + Duration::seconds(2);
        let orphan_end = session_start + Duration::seconds(3);
        let mut tool = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        let mut call = ToolCall::new(Span::new(orphan_start, orphan_end));
        call.turn_id = None;
        tool.calls.push(call);

        let mut tools = std::collections::BTreeMap::new();
        tools.insert("bash".to_string(), tool);
        let episodes = Episodes {
            turns: vec![t1],
            tools,
            ..Episodes::default()
        };

        let meta = meta_at(session_start);
        let (profile, warnings) = to_speedscope(&episodes, &meta, "0.0.0");

        let shifted: Vec<_> = warnings
            .iter()
            .filter_map(|w| match w {
                ExportWarning::OrphanTimeShifted {
                    orphan_kind,
                    original_at,
                    shifted_to,
                } => Some((orphan_kind.clone(), *original_at, *shifted_to)),
                _ => None,
            })
            .collect();
        assert_eq!(
            shifted.len(),
            1,
            "expected exactly one OrphanTimeShifted; warnings={warnings:?}"
        );
        let (kind, original_at, shifted_to) = &shifted[0];
        assert_eq!(kind, "bash");
        assert_eq!(*original_at, 2000);
        assert_eq!(*shifted_to, 10000);

        let events = &profile.profiles[0].events;
        for win in events.windows(2) {
            assert!(
                win[0].at <= win[1].at,
                "events must be at-monotonic across the orphan boundary; got {win:?}"
            );
        }
    }

    // ----- B-4.3: negative-duration helper ---------------------------

    #[test]
    fn duration_ms_warn_clamps_inverted_input_and_warns() {
        // A turn whose `started_at` predates the session's `started_at`
        // exercises the `duration_ms_warn` call on `turn_start_ms`: the
        // helper computes `(session_start → turn.started_at)` and finds
        // ended_at < started_at.
        let session_start = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let t1_start = session_start - Duration::seconds(3); // before session
        let t1_end = session_start + Duration::seconds(5);

        let mut t1 = Turn::new("turn-1".into(), t1_start);
        t1.ended_at = Some(t1_end);

        let episodes = Episodes {
            turns: vec![t1],
            ..Episodes::default()
        };

        let meta = meta_at(session_start);
        let (profile, warnings) = to_speedscope(&episodes, &meta, "0.0.0");

        let neg: Vec<_> = warnings
            .iter()
            .filter_map(|w| match w {
                ExportWarning::NegativeDurationClamped {
                    name,
                    started_at,
                    ended_at,
                } => Some((name.clone(), *started_at, *ended_at)),
                _ => None,
            })
            .collect();
        assert!(
            neg.iter().any(|(n, _, _)| n == "turn-1 start"),
            "expected NegativeDurationClamped for 'turn-1 start'; got {neg:?}"
        );
        let (_, started_at, ended_at) = neg
            .iter()
            .find(|(n, _, _)| n == "turn-1 start")
            .expect("turn-1 start warning present");
        assert!(*ended_at < *started_at);

        // Emitted ms is clamped to 0 — search for the Open event whose
        // frame matches turn-1 and confirm its `at == 0`.
        let events = &profile.profiles[0].events;
        let turn_open_at = events
            .iter()
            .find(|e| {
                e.ty == EventType::Open && profile.shared.frames[e.frame].name.starts_with("turn-1")
            })
            .map(|e| e.at)
            .expect("turn-1 open event present");
        assert_eq!(turn_open_at, 0);
    }
}
