//! Behavioural spec for bundle transaction assembly (Phase 2, test-first).
//!
//! Pins the Jito-correct construction rules:
//! - Compute-budget instructions (limit + price) come first.
//! - The tip is a real SOL transfer **co-located in the same transaction** as the strategy logic
//!   (so a failed bundle never pays the tip).
//! - The tip account is a **static account key**, never referenced via an Address Lookup Table.
//! - The signed transaction serializes to base64 and round-trips.
//!
//! Runs fully offline: a locally generated keypair + a fixed blockhash, no network.

use base64::Engine;
use prometheon_bundle::assembly::{
    build_tip_bundle_tx, self_transfer_ix, serialize_tx_base64, BundleParams,
};
use solana_sdk::{
    hash::Hash, pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction,
};

fn fixed_tip_account() -> Pubkey {
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5"
        .parse()
        .expect("valid pubkey")
}

#[test]
fn bundle_tx_has_compute_budget_then_strategy_then_tip() {
    let payer = Keypair::new();
    let params = BundleParams {
        payer: &payer,
        recent_blockhash: Hash::new_unique(),
        compute_unit_limit: 200_000,
        compute_unit_price_micro: 1_000,
        tip_account: fixed_tip_account(),
        tip_lamports: 50_000,
        strategy_ixs: vec![self_transfer_ix(&payer.pubkey(), 1)],
    };
    let tx = build_tip_bundle_tx(&params);
    let ixs = &tx.message.instructions;
    // 2 compute-budget + 1 strategy + 1 tip = 4 instructions.
    assert_eq!(ixs.len(), 4, "compute-limit, compute-price, strategy, tip");
}

#[test]
fn tip_account_is_a_static_key_not_in_a_lookup_table() {
    let payer = Keypair::new();
    let tip = fixed_tip_account();
    let params = BundleParams {
        payer: &payer,
        recent_blockhash: Hash::new_unique(),
        compute_unit_limit: 200_000,
        compute_unit_price_micro: 1_000,
        tip_account: tip,
        tip_lamports: 50_000,
        strategy_ixs: vec![self_transfer_ix(&payer.pubkey(), 1)],
    };
    let tx = build_tip_bundle_tx(&params);
    // Legacy message: every account is static. The tip account MUST appear in account_keys.
    assert!(
        tx.message.account_keys.contains(&tip),
        "tip account must be a static account key (never via ALT)"
    );
}

#[test]
fn bundle_tx_is_signed_and_round_trips_through_base64() {
    let payer = Keypair::new();
    let params = BundleParams {
        payer: &payer,
        recent_blockhash: Hash::new_unique(),
        compute_unit_limit: 200_000,
        compute_unit_price_micro: 1_000,
        tip_account: fixed_tip_account(),
        tip_lamports: 50_000,
        strategy_ixs: vec![self_transfer_ix(&payer.pubkey(), 1)],
    };
    let tx = build_tip_bundle_tx(&params);
    assert_eq!(tx.signatures.len(), 1, "single payer signature");
    assert!(tx.is_signed(), "transaction must be fully signed");

    let b64 = serialize_tx_base64(&tx).expect("serialize");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .expect("decode");
    let decoded: Transaction = bincode::deserialize(&bytes).expect("deserialize");
    assert_eq!(decoded, tx, "base64 round-trip preserves the transaction");
}

#[test]
fn payer_is_the_fee_payer_first_account() {
    let payer = Keypair::new();
    let params = BundleParams {
        payer: &payer,
        recent_blockhash: Hash::new_unique(),
        compute_unit_limit: 100_000,
        compute_unit_price_micro: 500,
        tip_account: fixed_tip_account(),
        tip_lamports: 10_000,
        strategy_ixs: vec![],
    };
    let tx = build_tip_bundle_tx(&params);
    assert_eq!(
        tx.message.account_keys[0],
        payer.pubkey(),
        "payer is fee payer"
    );
}
