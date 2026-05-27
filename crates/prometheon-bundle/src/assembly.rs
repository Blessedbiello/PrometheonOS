//! Bundle transaction assembly.
//!
//! Builds a single signed transaction that carries the strategy logic **and** the Jito tip,
//! following Jito's construction rules:
//! - Compute-budget instructions (limit + price) first, so the priority fee is explicit.
//! - The tip is a real `SystemProgram::transfer` to a tip account, **co-located in the same
//!   transaction** as the strategy logic — if the strategy reverts, the whole bundle is dropped and
//!   we never pay the tip.
//! - We use a legacy [`Transaction`], so every account (including the tip account) is a **static
//!   key** — satisfying the rule that tip accounts must never be referenced via an Address Lookup
//!   Table.
//!
//! A bundle is up to 5 such transactions; this builds one. Pure construction (no network); the
//! caller supplies a fresh blockhash and the dynamically-computed tip/CU price.

use base64::Engine;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_sdk::{
    hash::Hash, instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer,
    transaction::Transaction,
};
use solana_system_interface::instruction::transfer;

/// Inputs to assemble one bundle transaction.
pub struct BundleParams<'a> {
    /// Fee payer + signer.
    pub payer: &'a Keypair,
    /// Recent blockhash (fetch at `confirmed`/`processed`, never `finalized` — see README Q2).
    pub recent_blockhash: Hash,
    /// Compute-unit limit (set tight to actual usage to keep the priority fee cheap).
    pub compute_unit_limit: u32,
    /// Compute-unit price in micro-lamports/CU (the dynamic priority-fee knob).
    pub compute_unit_price_micro: u64,
    /// Tip account (one of the 8 from `getTipAccounts`, fetched live).
    pub tip_account: Pubkey,
    /// Tip amount in lamports (computed from live tip-floor data — never hardcoded).
    pub tip_lamports: u64,
    /// The strategy instructions (the real work). For the proof run this is a self-transfer.
    pub strategy_ixs: Vec<Instruction>,
}

/// Build and sign one bundle transaction per the rules above.
pub fn build_tip_bundle_tx(params: &BundleParams) -> Transaction {
    let mut ixs = Vec::with_capacity(3 + params.strategy_ixs.len());
    // Compute budget first.
    ixs.push(ComputeBudgetInstruction::set_compute_unit_limit(
        params.compute_unit_limit,
    ));
    ixs.push(ComputeBudgetInstruction::set_compute_unit_price(
        params.compute_unit_price_micro,
    ));
    // Strategy logic.
    ixs.extend(params.strategy_ixs.iter().cloned());
    // Tip last, co-located in the same transaction as the strategy.
    ixs.push(transfer(
        &params.payer.pubkey(),
        &params.tip_account,
        params.tip_lamports,
    ));

    Transaction::new_signed_with_payer(
        &ixs,
        Some(&params.payer.pubkey()),
        &[params.payer],
        params.recent_blockhash,
    )
}

/// A self-transfer instruction — the benign strategy used for the lifecycle proof run.
pub fn self_transfer_ix(payer: &Pubkey, lamports: u64) -> Instruction {
    transfer(payer, payer, lamports)
}

/// Serialize a signed transaction to base64 (the encoding `sendBundle` expects).
pub fn serialize_tx_base64(tx: &Transaction) -> Result<String, bincode::Error> {
    let bytes = bincode::serialize(tx)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}
