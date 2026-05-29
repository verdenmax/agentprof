//! Mode-segment aggregation: contiguous time ranges in a given Copilot mode.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Copilot CLI execution mode.
///
/// The variant names match Copilot CLI's actual wire-format vocabulary,
/// not generic agent-mode concepts. Verified against 73 real
/// `session.mode_changed` events from local Copilot CLI 1.0.54 sessions:
/// the only observed values are `"interactive"`, `"plan"`, and
/// `"autopilot"`.
///
/// Future agents (Claude Code, Codex CLI) will likely use different
/// vocabularies; if so, this enum becomes Copilot-specific and a more
/// neutral abstraction (e.g. a per-agent `ModeKind` trait) may be
/// introduced. For M1.4 we accept the Copilot-bound naming because it's
/// the only adapter wired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Mode {
    /// `interactive` — default conversational mode. User approval is
    /// typically requested for tool calls; specifics depend on per-tool
    /// policy. Most common observed value (52/146 in local data).
    Interactive,
    /// `plan` — Copilot writes / refines a plan without executing tools.
    /// Most common observed value (60/146 in local data).
    Plan,
    /// `autopilot` — broadest auto-approval set; Copilot executes tools
    /// without per-call user confirmation within policy.
    Autopilot,
    /// Wire value not in the known set — preserved verbatim.
    Unknown(String),
}

impl Mode {
    /// Map from the wire-format string.
    ///
    /// Known values: `"interactive"`, `"plan"`, `"autopilot"` (verified
    /// against real Copilot CLI 1.0.54 `session.mode_changed` events).
    /// Anything else is preserved in `Mode::Unknown` for forward-compat.
    #[must_use]
    pub fn from_wire(s: &str) -> Self {
        match s {
            "interactive" => Self::Interactive,
            "plan" => Self::Plan,
            "autopilot" => Self::Autopilot,
            other => Self::Unknown(other.to_string()),
        }
    }
}

/// A contiguous time range in a given mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModeSegment {
    /// Mode active during this segment.
    pub mode: Mode,
    /// Timestamp at which this segment opened.
    pub started_at: DateTime<Utc>,
    /// Timestamp at which this segment closed; `None` if still open.
    pub ended_at: Option<DateTime<Utc>>,
}

impl ModeSegment {
    /// Construct an open `ModeSegment`.
    #[must_use]
    pub const fn new(mode: Mode, started_at: DateTime<Utc>) -> Self {
        Self {
            mode,
            started_at,
            ended_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn mode_from_wire_known_values() {
        assert_eq!(Mode::from_wire("interactive"), Mode::Interactive);
        assert_eq!(Mode::from_wire("plan"), Mode::Plan);
        assert_eq!(Mode::from_wire("autopilot"), Mode::Autopilot);
    }

    #[test]
    fn mode_from_wire_unknown_preserved() {
        // 'ask' / 'auto' / 'expert' (the old assumed vocabulary) are NOT
        // known wire values — they round-trip through Unknown.
        assert_eq!(Mode::from_wire("ask"), Mode::Unknown("ask".into()));
        assert_eq!(Mode::from_wire("auto"), Mode::Unknown("auto".into()));
        assert_eq!(Mode::from_wire("default"), Mode::Unknown("default".into()));
    }

    #[test]
    fn segment_new_is_open() {
        let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
        let s = ModeSegment::new(Mode::Autopilot, t);
        assert_eq!(s.ended_at, None);
    }
}
