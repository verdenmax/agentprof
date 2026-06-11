//! Cache token analytics — formulas for cache hit-rate + saved-tokens
//! (ADR-0023). Pure data; no I/O.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops
)]
// (precision-loss is acceptable for percent formatting; we always render
// at 1 decimal place via `{:.1}`, so f64 → display is lossy on purpose.
// cast_sign_loss: `read * 0.9` is always ≥ 0 because `read: u64`.
// suboptimal_flops: mul_add would hurt formula readability; difference
// is negligible for token-scale arithmetic.)

/// Anthropic Claude Sonnet 4.x cache-read discount (2026-06 published
/// rate). Cache reads cost 0.1× input rate, so each cached token "saves"
/// 0.9× the input rate. See ADR-0023 D-2.
pub const CACHE_READ_DISCOUNT: f64 = 0.9;

/// Anthropic Claude Sonnet 4.x cache-write premium (2026-06 published
/// rate). Cache creates cost 1.25× input rate, so each created token
/// "costs" 0.25× extra vs. uncached. See ADR-0023 D-2.
pub const CACHE_WRITE_PREMIUM: f64 = 0.25;

/// Derived cache-utilization metrics for one session, one bucket, or
/// one aggregate (constructed via [`CacheMetrics::from_raw`]).
///
/// All percentages are in `[0, 100]`; both hit-rate variants return 0
/// when both numerator and denominator are 0 (guards zero-division).
///
/// # Examples
///
/// ```
/// use agentprof_core::analyzer::cache::CacheMetrics;
///
/// // No cache activity → None.
/// assert!(CacheMetrics::from_raw(0, 0, 1000).is_none());
///
/// // Healthy cache: 8k read, 2k created, 10k uncached input.
/// let m = CacheMetrics::from_raw(2_000, 8_000, 10_000).unwrap();
/// assert_eq!(m.creation, 2_000);
/// assert_eq!(m.read, 8_000);
/// assert!((m.hit_rate_honest_pct - 80.0).abs() < 0.01);
/// assert_eq!(m.saved_gross, 7_200);
/// assert_eq!(m.saved_net, 6_700);  // 8000*0.9 - 2000*0.25 = 7200 - 500
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CacheMetrics {
    /// Raw `cache_creation` tokens (Anthropic terminology; field on the
    /// write side).
    pub creation: u64,
    /// Raw `cache_read` tokens.
    pub read: u64,
    /// Raw `input_tokens` (non-cached prompt tokens) — used only for the
    /// naive hit-rate formula's denominator.
    pub input: u64,
    /// `100 × read / (read + input)` — "% of my prompt tokens that came
    /// from cache". Intuitive but doesn't penalize over-caching.
    pub hit_rate_naive_pct: f64,
    /// `100 × read / (read + creation)` — "% of my cache attempts that
    /// paid off". Exposes high-creation-low-read mis-strategies.
    pub hit_rate_honest_pct: f64,
    /// `round(read × 0.9)` — gross input-token equivalent saved by the
    /// 90% cache-read discount.
    pub saved_gross: u64,
    /// `round(read × 0.9 - creation × 0.25)` — net savings after
    /// accounting for the 25% cache-write premium. Can be NEGATIVE
    /// when creation dominates.
    pub saved_net: i64,
}

impl CacheMetrics {
    /// Build a `CacheMetrics` from raw token counts. Returns `None`
    /// when both `creation == 0 && read == 0` (no cache activity).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::analyzer::cache::CacheMetrics;
    /// assert!(CacheMetrics::from_raw(0, 0, 0).is_none());
    /// assert!(CacheMetrics::from_raw(1, 0, 0).is_some());
    /// ```
    #[must_use]
    pub fn from_raw(creation: u64, read: u64, input: u64) -> Option<Self> {
        if creation == 0 && read == 0 {
            return None;
        }
        let naive_denom = read.saturating_add(input);
        let honest_denom = read.saturating_add(creation);
        let hit_rate_naive_pct = if naive_denom == 0 {
            0.0
        } else {
            100.0 * (read as f64) / (naive_denom as f64)
        };
        let hit_rate_honest_pct = if honest_denom == 0 {
            0.0
        } else {
            100.0 * (read as f64) / (honest_denom as f64)
        };
        let saved_gross = (read as f64 * CACHE_READ_DISCOUNT).round() as u64;
        let saved_net = (read as f64 * CACHE_READ_DISCOUNT - creation as f64 * CACHE_WRITE_PREMIUM)
            .round() as i64;
        Some(Self {
            creation,
            read,
            input,
            hit_rate_naive_pct,
            hit_rate_honest_pct,
            saved_gross,
            saved_net,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_when_zero_activity() {
        assert!(CacheMetrics::from_raw(0, 0, 0).is_none());
        assert!(CacheMetrics::from_raw(0, 0, 9_999).is_none());
    }

    #[test]
    fn some_on_any_creation_or_read() {
        assert!(CacheMetrics::from_raw(1, 0, 0).is_some());
        assert!(CacheMetrics::from_raw(0, 1, 0).is_some());
    }

    #[test]
    fn naive_formula_basic() {
        let m = CacheMetrics::from_raw(0, 80, 20).unwrap();
        assert!((m.hit_rate_naive_pct - 80.0).abs() < 0.01);
    }

    #[test]
    fn honest_formula_basic() {
        let m = CacheMetrics::from_raw(20, 80, 0).unwrap();
        assert!((m.hit_rate_honest_pct - 80.0).abs() < 0.01);
    }

    #[test]
    fn saved_net_negative_when_creation_dominates() {
        // 100 read × 0.9 = 90; 1000 creation × 0.25 = 250 → net = 90 - 250 = -160.
        let m = CacheMetrics::from_raw(1_000, 100, 0).unwrap();
        assert_eq!(m.saved_gross, 90);
        assert_eq!(m.saved_net, -160);
    }

    #[test]
    fn overflow_saturating_at_u64_max() {
        // saturating_add prevents panic; downstream f64 conversion is
        // lossy but doesn't blow up.
        let m = CacheMetrics::from_raw(u64::MAX, u64::MAX, u64::MAX).unwrap();
        // All we assert is "didn't panic and produced finite floats".
        assert!(m.hit_rate_naive_pct.is_finite());
        assert!(m.hit_rate_honest_pct.is_finite());
    }

    #[test]
    fn zero_div_guard_when_input_zero() {
        // creation=10, read=0 → naive denom = 0+0=0 → naive=0; honest
        // denom = 0+10=10 → honest=0.
        let m = CacheMetrics::from_raw(10, 0, 0).unwrap();
        assert!(m.hit_rate_naive_pct.abs() < f64::EPSILON);
        assert!(m.hit_rate_honest_pct.abs() < f64::EPSILON);
    }

    #[test]
    fn round_trip_serde_json() {
        let m = CacheMetrics::from_raw(100, 900, 500).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let m2: CacheMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }
}
