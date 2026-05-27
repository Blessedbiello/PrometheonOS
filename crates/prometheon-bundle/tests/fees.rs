//! Behavioural spec for priority-fee math (Phase 2, test-first).
//!
//! Priority fee = ceil(compute_unit_price[µlamports/CU] × compute_unit_limit / 1e6), added on top
//! of the 5000-lamport-per-signature base fee. Derived, never hardcoded.

use prometheon_bundle::fees::{
    priority_fee_lamports, total_fee_lamports, BASE_FEE_LAMPORTS_PER_SIG,
};

#[test]
fn priority_fee_is_price_times_limit_over_one_million() {
    // 1000 µlamports/CU × 200_000 CU = 200_000_000 µlamports = 200 lamports.
    assert_eq!(priority_fee_lamports(1_000, 200_000), 200);
    // Exactly divisible, no rounding.
    assert_eq!(priority_fee_lamports(5_000, 200_000), 1_000);
}

#[test]
fn priority_fee_rounds_up_partial_lamports() {
    // 1 µlamport/CU × 1 CU = 1 µlamport → ceil to 1 lamport.
    assert_eq!(priority_fee_lamports(1, 1), 1);
    // 1_000_001 µlamports total → ceil(1.000001) = 2 lamports.
    assert_eq!(priority_fee_lamports(1, 1_000_001), 2);
}

#[test]
fn zero_price_or_limit_yields_zero_priority_fee() {
    assert_eq!(priority_fee_lamports(0, 200_000), 0);
    assert_eq!(priority_fee_lamports(1_000, 0), 0);
}

#[test]
fn priority_fee_does_not_overflow_at_max_compute() {
    // Max CU limit 1.4M with a large price must not overflow (u128 internally).
    // 10_000_000 µlamports/CU × 1_400_000 CU = 14e12 µlamports ÷ 1e6 = 14_000_000 lamports.
    let fee = priority_fee_lamports(10_000_000, 1_400_000);
    assert_eq!(fee, 14_000_000);
}

#[test]
fn total_fee_adds_base_fee_per_signature() {
    assert_eq!(BASE_FEE_LAMPORTS_PER_SIG, 5_000);
    // 1 signature + 200-lamport priority fee.
    assert_eq!(total_fee_lamports(200, 1), 5_200);
    // 2 signatures.
    assert_eq!(total_fee_lamports(0, 2), 10_000);
}
