//! Shared `--since` value parser for time-windowed subcommands
//! (`list`, `aggregate`, `watch aggregate`).
//!
//! Promoted from per-command helpers (`cmd/list.rs`, `cmd/aggregate.rs`)
//! to address full-review CLI #1 (parse_since-overflow). The two
//! historical copies were near-identical except `list`'s used plain
//! `n * unit_secs` which can panic in debug builds for absurd inputs
//! like `99999999999d`. The shared impl uses [`u64::saturating_mul`]
//! consistently.
//!
//! Grammar: `<N>d` / `<N>h` / `<N>m` / `<N>s` / `"all"` (unlimited).

use std::num::ParseIntError;
use std::time::Duration;

/// Parse a user-supplied `--since` argument value into a [`Duration`].
///
/// Accepts the suffixes `d` (days) / `h` (hours) / `m` (minutes) /
/// `s` (seconds), or the literal string `"all"` which returns
/// [`Duration::MAX`] (an unlimited window).
///
/// On out-of-range values (e.g. `99999999999d`) the result saturates
/// to [`Duration::MAX`] rather than panicking; users who specify
/// absurd windows get the same effective behavior as `"all"`.
///
/// # Errors
///
/// Returns a human-readable `String` error message when:
/// - the unit suffix is missing or unrecognized (`12x` / `12`);
/// - the numeric prefix is not a valid `u64` (`abcd` / `12.5d`).
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use agentprof_cli::cmd::since::parse_since;
///
/// assert_eq!(parse_since("7d").unwrap(), Duration::from_secs(7 * 86400));
/// assert_eq!(parse_since("30m").unwrap(), Duration::from_secs(30 * 60));
/// assert_eq!(parse_since("all").unwrap(), Duration::MAX);
/// assert!(parse_since("12x").is_err());
/// // Out-of-range saturates to u64::MAX seconds (Duration cap) instead
/// // of panicking on debug builds.
/// assert_eq!(
///     parse_since("1000000000000000000d").unwrap(),
///     Duration::from_secs(u64::MAX),
/// );
/// ```
pub fn parse_since(s: &str) -> Result<Duration, String> {
    if s == "all" {
        return Ok(Duration::MAX);
    }
    let (n_str, unit_secs): (&str, u64) = match s.chars().last() {
        Some('s') => (&s[..s.len() - 1], 1),
        Some('m') => (&s[..s.len() - 1], 60),
        Some('h') => (&s[..s.len() - 1], 3600),
        Some('d') => (&s[..s.len() - 1], 86400),
        _ => {
            return Err(format!(
                "unrecognized --since: {s}; use <N>d/h/m/s or 'all'"
            ));
        }
    };
    let n: u64 = n_str
        .parse()
        .map_err(|e: ParseIntError| format!("not a number: {n_str} ({e})"))?;
    // Saturating to catch absurd inputs like `99999999999d` (would
    // overflow `u64` × 86400). Pre-CLI-review-#1 the `cmd::list` copy
    // used plain `*`, panicking in debug builds.
    Ok(Duration::from_secs(n.saturating_mul(unit_secs)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_dhms_and_all() {
        assert_eq!(parse_since("1d").unwrap(), Duration::from_secs(86400));
        assert_eq!(parse_since("2h").unwrap(), Duration::from_secs(7200));
        assert_eq!(parse_since("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(parse_since("45s").unwrap(), Duration::from_secs(45));
        assert_eq!(parse_since("all").unwrap(), Duration::MAX);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_since("12x").is_err());
        assert!(parse_since("abcd").is_err());
        assert!(parse_since("12.5d").is_err());
    }

    #[test]
    fn saturates_on_overflow_instead_of_panic() {
        // u64::MAX = 18_446_744_073_709_551_615.
        // u64::MAX / 86_400 ≈ 2.135e14, so a value like 1e18 days
        // overflows when multiplied by 86400. With plain `n * unit_secs`
        // this panicked in debug builds; with saturating_mul it caps at
        // u64::MAX → Duration::from_secs(u64::MAX) → Duration::MAX
        // (the seconds field tops out at u64::MAX, so we compare against
        // that exact value rather than the abstract Duration::MAX).
        let r = parse_since("1000000000000000000d").unwrap();
        assert_eq!(r, Duration::from_secs(u64::MAX));
    }
}
