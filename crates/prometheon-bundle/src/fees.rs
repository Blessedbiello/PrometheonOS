//! Solana fee math (pure; no Solana dependency).
//!
//! A transaction's total fee = base fee (per signature) + priority fee. The priority fee is set
//! via the Compute Budget program: `priority = ceil(compute_unit_price × compute_unit_limit / 1e6)`
//! where `compute_unit_price` is in micro-lamports per CU. Both knobs are derived from live
//! conditions (the AI agent sets the CU price alongside the tip), never hardcoded.

/// Base fee charged per signature, in lamports.
pub const BASE_FEE_LAMPORTS_PER_SIG: u64 = 5_000;

/// Micro-lamports per lamport (the unit of `compute_unit_price`).
pub const MICRO_LAMPORTS_PER_LAMPORT: u128 = 1_000_000;

/// Priority fee in lamports for a given CU price (µlamports/CU) and CU limit, rounded up.
pub fn priority_fee_lamports(compute_unit_price_micro: u64, compute_unit_limit: u32) -> u64 {
    let micro_total = compute_unit_price_micro as u128 * compute_unit_limit as u128;
    // ceil division by 1e6.
    let lamports = micro_total.div_ceil(MICRO_LAMPORTS_PER_LAMPORT);
    lamports as u64
}

/// Total transaction fee in lamports = priority fee + base fee × signatures.
pub fn total_fee_lamports(priority_fee: u64, num_signatures: u64) -> u64 {
    priority_fee + BASE_FEE_LAMPORTS_PER_SIG * num_signatures
}
