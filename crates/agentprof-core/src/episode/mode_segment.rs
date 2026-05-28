//! Mode-segment aggregation: contiguous time ranges in a given Copilot mode.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Copilot CLI execution mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Mode {
    /// `ask` mode — every tool call requires user approval.
    Ask,
    /// `auto` mode — most tools auto-approve within policy.
    Auto,
    /// `expert` / autopilot mode — broadest auto-approval set.
    Expert,
    /// Wire value not in the known set — preserved verbatim.
    Unknown(String),
}

impl Mode {
    /// Map from the wire-format string.
    #[must_use]
    pub fn from_wire(s: &str) -> Self {
        match s {
            "ask" => Self::Ask,
            "auto" => Self::Auto,
            "expert" => Self::Expert,
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
        assert_eq!(Mode::from_wire("ask"), Mode::Ask);
        assert_eq!(Mode::from_wire("auto"), Mode::Auto);
        assert_eq!(Mode::from_wire("expert"), Mode::Expert);
    }

    #[test]
    fn mode_from_wire_unknown_preserved() {
        assert_eq!(Mode::from_wire("plan"), Mode::Unknown("plan".into()));
    }

    #[test]
    fn segment_new_is_open() {
        let t = Utc.with_ymd_and_hms(2026, 5, 27, 0, 0, 0).unwrap();
        let s = ModeSegment::new(Mode::Auto, t);
        assert_eq!(s.ended_at, None);
    }
}
