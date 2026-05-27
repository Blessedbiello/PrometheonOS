//! Dynamic tip computation — the deterministic policy the hot path runs.
//!
//! The tip is always a function of live tip-floor data ([`crate::tip_floor::TipFloor`]) and the
//! current congestion score; there are no hardcoded tip amounts. The AI strategist (Phase 5) owns
//! the *policy* — it sets [`TipStrategy`] (target percentile, congestion boost, bounds) based on
//! reasoning — while this function applies that policy deterministically and fast.

use serde::{Deserialize, Serialize};

use crate::tip_floor::TipFloor;

/// Landed-tip percentile tiers exposed by the Jito tip-floor API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Percentile {
    P25,
    P50,
    P75,
    P95,
    P99,
}

/// Tip policy parameters. Set by the AI agent / config; never the tip value itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipStrategy {
    /// Which floor percentile to target as the baseline tip.
    pub target: Percentile,
    /// Fractional uplift applied at full congestion (e.g. `0.5` → up to +50% when congestion = 1).
    pub congestion_boost: f64,
    /// Lower safety bound (lamports). The tip never goes below this.
    pub min_lamports: u64,
    /// Upper safety bound (lamports). The tip never exceeds this (cost guard).
    pub max_lamports: u64,
}

/// The outcome of a tip computation, with the inputs that produced it — for telemetry and for the
/// AI agent's reasoning traces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipDecision {
    /// Final tip in lamports (post-boost, post-clamp).
    pub lamports: u64,
    /// The percentile baseline used.
    pub percentile: Percentile,
    /// The floor percentile value before the congestion boost (lamports).
    pub base_lamports: u64,
    /// The congestion multiplier applied (`1.0 + boost * congestion`).
    pub congestion_multiplier: f64,
    /// Whether the result hit the upper bound.
    pub clamped_high: bool,
    /// Whether the result hit the lower bound.
    pub clamped_low: bool,
}

/// Compute a tip from live floor data + congestion, applying the configured policy.
///
/// `congestion` is clamped to `[0.0, 1.0]`. The baseline is the target floor percentile; it is
/// scaled by `1.0 + congestion_boost * congestion` and then clamped to the configured bounds.
pub fn compute_tip(floor: &TipFloor, strategy: &TipStrategy, congestion: f64) -> TipDecision {
    let congestion = congestion.clamp(0.0, 1.0);
    let base_lamports = floor.percentile_lamports(strategy.target);
    let congestion_multiplier = 1.0 + strategy.congestion_boost * congestion;

    let boosted = (base_lamports as f64 * congestion_multiplier).round() as u64;

    let (lamports, clamped_high, clamped_low) = if boosted > strategy.max_lamports {
        (strategy.max_lamports, true, false)
    } else if boosted < strategy.min_lamports {
        (strategy.min_lamports, false, true)
    } else {
        (boosted, false, false)
    };

    TipDecision {
        lamports,
        percentile: strategy.target,
        base_lamports,
        congestion_multiplier,
        clamped_high,
        clamped_low,
    }
}
