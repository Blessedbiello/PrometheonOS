//! Pure network-health and execution-quality metric functions.
//!
//! Each metric has an explicit, deterministic definition so the AI agent's inputs are reproducible
//! and its reasoning is explainable. All scores are bounded `[0.0, 1.0]` unless noted.

/// Weights for the [`congestion_score`] blend. Must conceptually sum to 1.0 (not enforced).
#[derive(Debug, Clone, Copy)]
pub struct CongestionWeights {
    pub skip: f64,
    pub latency: f64,
    pub tip: f64,
    /// Confirm-latency (ms) treated as "fully congested".
    pub latency_ceiling_ms: f64,
    /// Tip-floor (lamports) treated as "fully congested".
    pub tip_ceiling_lamports: u64,
}

impl Default for CongestionWeights {
    fn default() -> Self {
        Self {
            skip: 0.40,
            latency: 0.35,
            tip: 0.25,
            latency_ceiling_ms: 3_000.0,
            tip_ceiling_lamports: 1_000_000,
        }
    }
}

/// Fraction of scheduled slots that produced a block, `[0,1]`. No evidence (0 total) ⇒ `1.0`.
pub fn slot_stability_score(produced: u64, skipped: u64) -> f64 {
    let total = produced + skipped;
    if total == 0 {
        1.0
    } else {
        produced as f64 / total as f64
    }
}

/// Blended congestion score `[0,1]` from the skip rate, confirm latency, and tip-floor level.
///
/// Each component is normalized to `[0,1]` (skip rate is already a fraction; latency and tip are
/// scaled against their ceilings and clamped), then combined by the configured weights.
pub fn congestion_score(
    skip_rate: f64,
    confirm_latency_ms: f64,
    tip_floor_lamports: u64,
    w: &CongestionWeights,
) -> f64 {
    let skip = skip_rate.clamp(0.0, 1.0);
    let latency = (confirm_latency_ms / w.latency_ceiling_ms).clamp(0.0, 1.0);
    let tip = (tip_floor_lamports as f64 / w.tip_ceiling_lamports as f64).clamp(0.0, 1.0);
    (w.skip * skip + w.latency * latency + w.tip * tip).clamp(0.0, 1.0)
}

/// Risk `[0,1]` that a transaction expires before landing: expected confirm time divided by the
/// wall-clock time remaining in the blockhash window. No blocks left ⇒ `1.0`.
pub fn expiry_risk_score(blocks_remaining: u64, confirm_latency_ms: f64, slot_ms: u64) -> f64 {
    let time_remaining_ms = blocks_remaining as f64 * slot_ms as f64;
    if time_remaining_ms <= 0.0 {
        return 1.0;
    }
    (confirm_latency_ms / time_remaining_ms).clamp(0.0, 1.0)
}

/// Landed / submitted, `[0,1]`. No submissions ⇒ `0.0` (no evidence of success).
pub fn bundle_landing_probability(landed: u64, submitted: u64) -> f64 {
    if submitted == 0 {
        0.0
    } else {
        landed as f64 / submitted as f64
    }
}

/// Retries that subsequently landed / total retries, `[0,1]`.
pub fn retry_success_rate(retries_landed: u64, retries_total: u64) -> f64 {
    if retries_total == 0 {
        0.0
    } else {
        retries_landed as f64 / retries_total as f64
    }
}

/// Landings per lamport tipped (higher is more efficient). No tips ⇒ `0.0`.
pub fn tip_efficiency_ratio(landed: u64, total_tip_lamports: u64) -> f64 {
    if total_tip_lamports == 0 {
        0.0
    } else {
        landed as f64 / total_tip_lamports as f64
    }
}

/// Total lamports (tips + fees) per successful landing, or `None` if nothing landed.
pub fn cost_per_successful_landing(total_lamports: u64, landed: u64) -> Option<f64> {
    if landed == 0 {
        None
    } else {
        Some(total_lamports as f64 / landed as f64)
    }
}
