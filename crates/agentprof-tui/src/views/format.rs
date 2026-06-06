//! Shared display helpers used by all views.
//!
//! Exposes [`human_short`] for compact duration formatting
//! (`ms` / `s` / `m` / `h` buckets), plus
//! [`format_tokens_short`] / [`format_tokens_detailed`] for the
//! `Turn.output_tokens` column shown in the `FlamegraphView` prefix
//! and `TurnDetailView` header.

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
    } else if ms < 86_400_000 {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    } else {
        // Wave D2 (`m1.6.3-t1-followup-subday-window-label`): add a day
        // branch so multi-day windows (e.g. `aggregate --since 30d`)
        // render `30.0d` instead of the unwieldy `720.0h`.
        format!("{:.1}d", ms as f64 / 86_400_000.0)
    }
}

/// Format an output-token count for the `FlamegraphView` prefix column.
///
/// Caps at 5 chars (uses k/M abbreviations for large counts) so the
/// flamegraph prefix retains a fixed-width grid regardless of token
/// magnitude. `None` (no `assistant.message` events in the turn —
/// typically an end-of-session orphan) renders as a centered `-` to
/// be visually distinguishable from `0` ("zero tokens reported").
///
/// Buckets:
///
/// | input | output |
/// |---|---|
/// | `None` | `"  -  "` |
/// | `n < 1_000` | `"{n:>5}"` (e.g. `"   42"`) |
/// | `1_000 ≤ n < 100_000` | `"{:>5.1}k"` (e.g. `" 1.2k"`, `"99.9k"`) |
/// | `100_000 ≤ n < 1_000_000` | `"{:>4}k"` (e.g. `" 100k"`, `" 999k"`) |
/// | `n ≥ 1_000_000` | `"{:>4.1}M"` (e.g. `" 1.0M"`, `"12.3M"`) |
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::format::format_tokens_short;
/// assert_eq!(format_tokens_short(Some(42)), "   42");
/// assert_eq!(format_tokens_short(Some(1234)), " 1.2k");
/// assert_eq!(format_tokens_short(Some(99_999)), "99.9k");
/// assert_eq!(format_tokens_short(Some(123_456)), " 123k");
/// assert_eq!(format_tokens_short(Some(1_500_000)), " 1.5M");
/// assert_eq!(format_tokens_short(None), "  -  ");
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_tokens_short(tokens: Option<u32>) -> String {
    match tokens {
        None => "  -  ".to_string(),
        Some(n) if n < 1_000 => format!("{n:>5}"),
        // Truncate (not round) to avoid e.g. 99_999 → "100.0k" (6 chars,
        // breaks the 5-char cap). 99_999 / 100 = 999, / 10.0 = 99.9 → "99.9k".
        Some(n) if n < 100_000 => format!("{:>4.1}k", f64::from(n / 100) / 10.0),
        Some(n) if n < 1_000_000 => format!("{:>4}k", n / 1_000),
        // Same truncation trick for the sub-10M bucket: 9_999_999 / 100_000 = 99,
        // / 10.0 = 9.9 → " 9.9M".
        Some(n) if n < 10_000_000 => format!("{:>4.1}M", f64::from(n / 100_000) / 10.0),
        // Beyond 10M, drop the decimal so u32::MAX (≈4.29G tokens → "4294M")
        // still fits in 5 chars.
        Some(n) => format!("{:>4}M", n / 1_000_000),
    }
}

/// Format an output-token count for the `TurnDetailView` header.
///
/// Unlike [`format_tokens_short`], this variant has no width cap and no
/// padding — the header has more room than the flamegraph prefix grid.
/// Returns `None` to signal "omit this segment entirely" (i.e. when the
/// turn had no `assistant.message` events, which would normally make
/// `output_tokens` `None`).
///
/// Buckets:
///
/// | input | output |
/// |---|---|
/// | `None` | `None` (caller should omit the segment) |
/// | `n < 1_000` | `Some("{n}")` (e.g. `"42"`) |
/// | `1_000 ≤ n < 999_500` | `Some("{:.1}k")` (e.g. `"1.2k"`, `"999.5k"` excluded) |
/// | `n ≥ 999_500` | `Some("{:.2}M")` (e.g. `"1.00M"`) |
///
/// The k→M boundary is at `999_500` (not `1_000_000`) so the display
/// transition is round-aware: `999_499` formats as `"999.5k"`, while
/// `999_500..=999_999` rolls over to `"1.00M"`. The naive
/// `1_000_000` cutoff would otherwise emit the ugly, arithmetically
/// misleading `"1000.0k"` for that 500-wide window.
///
/// # Examples
///
/// ```
/// use agentprof_tui::views::format::format_tokens_detailed;
/// assert_eq!(format_tokens_detailed(Some(42)).as_deref(), Some("42"));
/// assert_eq!(format_tokens_detailed(Some(1234)).as_deref(), Some("1.2k"));
/// assert_eq!(format_tokens_detailed(Some(999_499)).as_deref(), Some("999.5k"));
/// assert_eq!(format_tokens_detailed(Some(999_500)).as_deref(), Some("1.00M"));
/// assert_eq!(format_tokens_detailed(Some(1_234_000)).as_deref(), Some("1.23M"));
/// assert_eq!(format_tokens_detailed(None), None);
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_tokens_detailed(tokens: Option<u32>) -> Option<String> {
    let n = tokens?;
    Some(if n < 1_000 {
        n.to_string()
    } else if n < 999_500 {
        // Switch to M at 999_500 instead of 1_000_000 so the boundary is
        // round-aware: 999_499 → "999.5k"; 999_500 → "1.00M". Avoids the
        // ugly "1000.0k" display that the naive 1_000_000 boundary
        // produces for 999_500..=999_999.
        format!("{:.1}k", f64::from(n) / 1_000.0)
    } else {
        format!("{:.2}M", f64::from(n) / 1_000_000.0)
    })
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

    #[test]
    fn format_tokens_short_none_yields_dash() {
        let s = format_tokens_short(None);
        assert_eq!(s, "  -  ");
        assert_eq!(s.chars().count(), 5);
    }

    #[test]
    fn format_tokens_short_small_count_no_abbrev() {
        assert_eq!(format_tokens_short(Some(0)), "    0");
        assert_eq!(format_tokens_short(Some(42)), "   42");
        assert_eq!(format_tokens_short(Some(999)), "  999");
    }

    #[test]
    fn format_tokens_short_thousands_uses_k() {
        assert_eq!(format_tokens_short(Some(1000)), " 1.0k");
        assert_eq!(format_tokens_short(Some(1234)), " 1.2k");
        assert_eq!(format_tokens_short(Some(99_999)), "99.9k");
        assert_eq!(format_tokens_short(Some(100_000)), " 100k");
        assert_eq!(format_tokens_short(Some(999_000)), " 999k");
    }

    #[test]
    fn format_tokens_short_millions_uses_m() {
        assert_eq!(format_tokens_short(Some(1_000_000)), " 1.0M");
        assert_eq!(format_tokens_short(Some(1_500_000)), " 1.5M");
        assert_eq!(format_tokens_short(Some(9_999_999)), " 9.9M");
        // ≥10M drops the decimal to preserve the 5-char cap.
        assert_eq!(format_tokens_short(Some(12_300_000)), "  12M");
        assert_eq!(format_tokens_short(Some(u32::MAX)), "4294M");
    }

    #[test]
    fn format_tokens_short_caps_at_five_chars() {
        // Defensive sweep across the bucket boundaries + extreme values.
        for n in [
            0,
            1,
            999,
            1_000,
            9_999,
            10_000,
            99_999,
            100_000,
            999_000,
            999_999,
            1_000_000,
            9_999_999,
            50_000_000,
            u32::MAX,
        ] {
            let s = format_tokens_short(Some(n));
            assert!(
                s.chars().count() <= 5,
                "format_tokens_short({n}) = {s:?} exceeded 5 chars ({})",
                s.chars().count()
            );
        }
        assert_eq!(format_tokens_short(None).chars().count(), 5);
    }

    #[test]
    fn format_tokens_detailed_none_returns_none() {
        assert_eq!(format_tokens_detailed(None), None);
    }

    #[test]
    fn format_tokens_detailed_uses_appropriate_unit() {
        assert_eq!(format_tokens_detailed(Some(0)).as_deref(), Some("0"));
        assert_eq!(format_tokens_detailed(Some(42)).as_deref(), Some("42"));
        assert_eq!(format_tokens_detailed(Some(1234)).as_deref(), Some("1.2k"));
        assert_eq!(
            format_tokens_detailed(Some(1_234_000)).as_deref(),
            Some("1.23M")
        );
    }

    #[test]
    fn format_tokens_detailed_boundary_rolls_to_m_at_999_500() {
        // Round-aware boundary: 999_499 stays in k (display would round to
        // 999.5k); 999_500 rolls over to M (display 1.00M). Avoids the
        // ugly "1000.0k" display the naive 1_000_000 boundary would emit
        // for 999_500..=999_999.
        assert_eq!(
            format_tokens_detailed(Some(999_499)).as_deref(),
            Some("999.5k")
        );
        assert_eq!(
            format_tokens_detailed(Some(999_500)).as_deref(),
            Some("1.00M")
        );
        assert_eq!(
            format_tokens_detailed(Some(999_999)).as_deref(),
            Some("1.00M")
        );
        assert_eq!(
            format_tokens_detailed(Some(1_000_000)).as_deref(),
            Some("1.00M")
        );
    }
}
