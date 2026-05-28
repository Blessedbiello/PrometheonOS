//! Behavioural spec for the network-health metric functions (Phase 4, test-first).
//!
//! Each metric is a pure function with an explicit, deterministic definition so the AI agent's
//! inputs are reproducible and explainable.

use prometheon_netmodel::metrics::{
    bundle_landing_probability, congestion_score, cost_per_successful_landing, expiry_risk_score,
    retry_success_rate, slot_stability_score, tip_efficiency_ratio, CongestionWeights,
};

#[test]
fn slot_stability_is_fraction_of_slots_produced() {
    // 9 produced of 10 scheduled → 0.9 stability.
    assert_eq!(slot_stability_score(9, 1), 0.9);
    // No skips → perfect.
    assert_eq!(slot_stability_score(10, 0), 1.0);
    // No evidence yet → assume stable.
    assert_eq!(slot_stability_score(0, 0), 1.0);
}

#[test]
fn congestion_blends_skip_latency_and_tip_components() {
    let w = CongestionWeights::default();
    // Calm network: no skips, low latency, low tip floor → near 0.
    let calm = congestion_score(0.0, 200.0, 5_000, &w);
    assert!(calm < 0.1, "calm congestion should be low, got {calm}");

    // Hot network: 50% skip, high latency, high tip floor → high.
    let hot = congestion_score(0.5, 3_000.0, 1_000_000, &w);
    assert!(hot > 0.7, "hot congestion should be high, got {hot}");

    // Monotonic: more skipping raises congestion, all else equal.
    let a = congestion_score(0.1, 500.0, 10_000, &w);
    let b = congestion_score(0.4, 500.0, 10_000, &w);
    assert!(b > a);
}

#[test]
fn congestion_is_clamped_to_unit_range() {
    let w = CongestionWeights::default();
    let extreme = congestion_score(1.0, 100_000.0, 100_000_000, &w);
    assert!((0.0..=1.0).contains(&extreme));
    assert!(extreme > 0.95);
}

#[test]
fn expiry_risk_is_confirm_time_over_time_remaining() {
    // 150 blocks left × 400ms = 60s remaining; 600ms confirm → ~0.01 risk.
    let low = expiry_risk_score(150, 600.0, 400);
    assert!(low < 0.05, "lots of runway ⇒ low risk, got {low}");

    // 2 blocks left × 400ms = 800ms; 600ms confirm → 0.75 risk.
    assert!((expiry_risk_score(2, 600.0, 400) - 0.75).abs() < 1e-9);

    // No blocks left → certain expiry.
    assert_eq!(expiry_risk_score(0, 600.0, 400), 1.0);

    // Risk rises as blocks deplete.
    assert!(expiry_risk_score(5, 600.0, 400) > expiry_risk_score(50, 600.0, 400));
}

#[test]
fn landing_probability_and_retry_success_are_ratios() {
    assert_eq!(bundle_landing_probability(7, 10), 0.7);
    assert_eq!(bundle_landing_probability(0, 0), 0.0); // no evidence
    assert_eq!(retry_success_rate(3, 4), 0.75);
    assert_eq!(retry_success_rate(0, 0), 0.0);
}

#[test]
fn tip_efficiency_and_cost_per_landing() {
    // 5 landings for 1_000_000 lamports tipped → 5e-6 landings/lamport.
    assert!((tip_efficiency_ratio(5, 1_000_000) - 0.000005).abs() < 1e-12);
    assert_eq!(tip_efficiency_ratio(5, 0), 0.0);

    // 1_000_000 lamports / 5 landings = 200_000 lamports per landing.
    assert_eq!(cost_per_successful_landing(1_000_000, 5), Some(200_000.0));
    // No landings → undefined cost.
    assert_eq!(cost_per_successful_landing(1_000_000, 0), None);
}
