//! Behavioural spec for dynamic tip computation (Phase 2, test-first).
//!
//! The bounty forbids hardcoded tip values: the tip MUST be derived from live tip-floor data and
//! current network conditions. These tests pin that contract — every tip is a function of the
//! parsed floor percentiles and the congestion score, with only safety *bounds* configured.

use prometheon_bundle::tip::{compute_tip, Percentile, TipStrategy};
use prometheon_bundle::tip_floor::TipFloor;

/// Real-shape tip-floor payload (the endpoint returns a single-element array; units are SOL).
const TIP_FLOOR_JSON: &str = r#"[{
    "time": "2026-05-27T00:00:00Z",
    "landed_tips_25th_percentile": 0.000005,
    "landed_tips_50th_percentile": 0.00001,
    "landed_tips_75th_percentile": 0.0001,
    "landed_tips_95th_percentile": 0.001,
    "landed_tips_99th_percentile": 0.01,
    "ema_landed_tips_50th_percentile": 0.0000123
}]"#;

#[test]
fn tip_floor_parses_and_converts_sol_to_lamports() {
    let floor = TipFloor::from_response_json(TIP_FLOOR_JSON).expect("parse");
    // 0.00001 SOL = 10_000 lamports; 0.0001 SOL = 100_000; 0.001 = 1_000_000.
    assert_eq!(floor.percentile_lamports(Percentile::P50), 10_000);
    assert_eq!(floor.percentile_lamports(Percentile::P75), 100_000);
    assert_eq!(floor.percentile_lamports(Percentile::P95), 1_000_000);
    assert_eq!(floor.percentile_lamports(Percentile::P25), 5_000);
    assert_eq!(floor.percentile_lamports(Percentile::P99), 10_000_000);
    assert_eq!(floor.ema50_lamports(), 12_300);
}

#[test]
fn tip_is_the_target_percentile_when_network_is_calm() {
    let floor = TipFloor::from_response_json(TIP_FLOOR_JSON).unwrap();
    let strategy = TipStrategy {
        target: Percentile::P75,
        congestion_boost: 0.5,
        min_lamports: 1_000,
        max_lamports: 5_000_000,
    };
    // congestion 0.0 → no boost → exactly the 75th percentile (100_000 lamports).
    let decision = compute_tip(&floor, &strategy, 0.0);
    assert_eq!(decision.lamports, 100_000);
    assert_eq!(decision.base_lamports, 100_000);
    assert_eq!(decision.percentile, Percentile::P75);
    assert!((decision.congestion_multiplier - 1.0).abs() < 1e-9);
}

#[test]
fn congestion_scales_the_tip_up_within_the_boost_range() {
    let floor = TipFloor::from_response_json(TIP_FLOOR_JSON).unwrap();
    let strategy = TipStrategy {
        target: Percentile::P75,
        congestion_boost: 0.5, // up to +50% at full congestion
        min_lamports: 1_000,
        max_lamports: 5_000_000,
    };
    // congestion 1.0 → multiplier 1.5 → 100_000 * 1.5 = 150_000.
    let decision = compute_tip(&floor, &strategy, 1.0);
    assert_eq!(decision.lamports, 150_000);
    assert!((decision.congestion_multiplier - 1.5).abs() < 1e-9);

    // congestion 0.5 → multiplier 1.25 → 125_000.
    assert_eq!(compute_tip(&floor, &strategy, 0.5).lamports, 125_000);
}

#[test]
fn tip_is_clamped_to_the_configured_bounds() {
    let floor = TipFloor::from_response_json(TIP_FLOOR_JSON).unwrap();
    // Upper clamp: 99th pct (10_000_000) boosted, capped at max 2_000_000.
    let capped = TipStrategy {
        target: Percentile::P99,
        congestion_boost: 0.5,
        min_lamports: 1_000,
        max_lamports: 2_000_000,
    };
    let d = compute_tip(&floor, &capped, 1.0);
    assert_eq!(d.lamports, 2_000_000);
    assert!(d.clamped_high);

    // Lower clamp: 25th pct (5_000) but min floor 8_000 (e.g. Jito's practical minimum).
    let floored = TipStrategy {
        target: Percentile::P25,
        congestion_boost: 0.0,
        min_lamports: 8_000,
        max_lamports: 2_000_000,
    };
    let d = compute_tip(&floor, &floored, 0.0);
    assert_eq!(d.lamports, 8_000);
    assert!(d.clamped_low);
}

#[test]
fn congestion_score_is_clamped_to_unit_range() {
    let floor = TipFloor::from_response_json(TIP_FLOOR_JSON).unwrap();
    let strategy = TipStrategy {
        target: Percentile::P75,
        congestion_boost: 0.5,
        min_lamports: 1_000,
        max_lamports: 5_000_000,
    };
    // Out-of-range congestion is clamped: 2.0 behaves like 1.0, -1.0 like 0.0.
    assert_eq!(
        compute_tip(&floor, &strategy, 2.0).lamports,
        compute_tip(&floor, &strategy, 1.0).lamports
    );
    assert_eq!(
        compute_tip(&floor, &strategy, -1.0).lamports,
        compute_tip(&floor, &strategy, 0.0).lamports
    );
}
