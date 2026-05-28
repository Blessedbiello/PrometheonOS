//! Backoff math for retries (pure).

/// Exponential backoff in milliseconds for a 1-indexed `attempt`, doubling from `base_ms` and
/// capped at `max_ms`. Overflow-safe (saturates to the cap).
pub fn backoff_ms(attempt: u32, base_ms: u64, max_ms: u64) -> u64 {
    if attempt <= 1 {
        return base_ms.min(max_ms);
    }
    let shift = attempt - 1;
    // base_ms * 2^shift, saturating.
    let scaled = base_ms.checked_shl(shift).unwrap_or(u64::MAX);
    scaled.min(max_ms)
}

/// Add up to `jitter_frac` of `delay_ms` of positive jitter, controlled by `rand01` in `[0,1]`.
/// `rand01 = 0` ⇒ no jitter; `rand01 = 1` ⇒ full `+jitter_frac`.
pub fn with_jitter(delay_ms: u64, jitter_frac: f64, rand01: f64) -> u64 {
    let r = rand01.clamp(0.0, 1.0);
    let extra = (delay_ms as f64 * jitter_frac * r).round() as u64;
    delay_ms + extra
}
