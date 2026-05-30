//! Shared display helpers used by all views.
//!
//! Currently exposes [`human_short`] for compact duration formatting
//! (`ms` / `s` / `m` / `h` buckets). Used by `flamegraph`, `roi`, and
//! `aggregate` views.

use chrono::Duration;

/// Format a `chrono::Duration` as a compact human-readable string.
///
/// Buckets: `<N>ms` for sub-second, `<N.N>s` for sub-minute,
/// `<N.N>m` for sub-hour, `<N.N>h` otherwise. Negative durations render
/// as their absolute value (a defensive choice — durations on
/// `AnalysisReport` rollups should already be non-negative, but
/// renderers should not panic on edge cases).
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::format::human_short;
/// use chrono::Duration;
/// assert_eq!(human_short(Duration::milliseconds(500)), "500ms");
/// assert_eq!(human_short(Duration::seconds(3)), "3.0s");
/// assert_eq!(human_short(Duration::seconds(90)), "1.5m");
/// assert_eq!(human_short(Duration::seconds(5400)), "1.5h");
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn human_short(d: Duration) -> String {
    let ms = d.num_milliseconds().abs();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_short_handles_all_buckets() {
        assert_eq!(human_short(Duration::milliseconds(500)), "500ms");
        assert_eq!(human_short(Duration::seconds(3)), "3.0s");
        assert_eq!(human_short(Duration::seconds(90)), "1.5m");
        assert_eq!(human_short(Duration::seconds(5400)), "1.5h");
    }

    #[test]
    fn human_short_treats_negative_as_absolute_value() {
        assert_eq!(human_short(Duration::milliseconds(-500)), "500ms");
    }
}
