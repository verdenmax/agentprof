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

    for (ordinal, turn) in episodes.turns.iter().enumerate() {
        emit_turn(
            ordinal,
            turn,
            episodes,
            session_start,
            session_end,
            total_ms,
            &frame_index,
            &mut events,
            &mut warnings,
        );
    }

    if has_orphan_tool_calls {
        emit_orphans(
            episodes,
            session_start,
            &frame_index,
            &mut events,
            &mut warnings,
        );
    }

    events.push(Event::new(EventType::Close, total_ms, session_frame));

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
            total_ms,
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

#[allow(clippy::too_many_arguments)]
fn emit_turn(
    ordinal: usize,
    turn: &crate::episode::Turn,
    episodes: &Episodes,
    session_start: DateTime<Utc>,
    session_end: DateTime<Utc>,
    total_ms: i64,
    frame_index: &BTreeMap<String, usize>,
    events: &mut Vec<Event>,
    warnings: &mut Vec<ExportWarning>,
) {
    let turn_name = turn_frame_name(ordinal, turn.ended_at.is_none());
    let turn_frame = lookup(frame_index, &turn_name);
    let turn_start_ms = duration_ms(turn.started_at - session_start);
    let turn_end_ms = turn
        .ended_at
        .map_or(total_ms, |e| duration_ms(e - session_start));

    events.push(Event::new(EventType::Open, turn_start_ms, turn_frame));

    let children = collect_turn_children(turn, episodes);
    let turn_close_bound = turn.ended_at.unwrap_or(session_end);
    let mut last_end = turn.started_at;
    for child in children {
        let (effective_start, effective_end) = adjust_for_overlap(
            child.started_at,
            child.ended_at,
            last_end,
            &child.warning_label,
            warnings,
        );
        let clamped_end = effective_end.min(turn_close_bound);
        let final_end = clamped_end.max(effective_start);
        let frame_idx = lookup(frame_index, &child.frame_name);
        events.push(Event::new(
            EventType::Open,
            duration_ms(effective_start - session_start),
            frame_idx,
        ));
        events.push(Event::new(
            EventType::Close,
            duration_ms(final_end - session_start),
            frame_idx,
        ));
        last_end = final_end;
    }

    events.push(Event::new(EventType::Close, turn_end_ms, turn_frame));
}

fn emit_orphans(
    episodes: &Episodes,
    session_start: DateTime<Utc>,
    frame_index: &BTreeMap<String, usize>,
    events: &mut Vec<Event>,
    warnings: &mut Vec<ExportWarning>,
) {
    let orphan_frame = lookup(frame_index, "turn-orphan");
    let mut orphans: Vec<(DateTime<Utc>, DateTime<Utc>, String)> = Vec::new();
    for tool in episodes.tools.values() {
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
    let orphan_start_ms = duration_ms(first.0 - session_start);
    let orphan_end_ms = duration_ms(last.1 - session_start);
    events.push(Event::new(EventType::Open, orphan_start_ms, orphan_frame));
    let mut last_end = first.0;
    for (s, e, display) in orphans {
        let (effective_start, effective_end) =
            adjust_for_overlap(s, e, last_end, &display, warnings);
        let final_end = effective_end.max(effective_start);
        let frame_idx = lookup(frame_index, &display);
        events.push(Event::new(
            EventType::Open,
            duration_ms(effective_start - session_start),
            frame_idx,
        ));
        events.push(Event::new(
            EventType::Close,
            duration_ms(final_end - session_start),
            frame_idx,
        ));
        last_end = final_end;
    }
    events.push(Event::new(
        EventType::Close,
        orphan_end_ms.max(duration_ms(last_end - session_start)),
        orphan_frame,
    ));
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
fn lookup(idx: &BTreeMap<String, usize>, name: &str) -> usize {
    idx.get(name).copied().unwrap_or(0)
}
