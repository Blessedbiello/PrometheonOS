//! The proof saga — assemble a real Jito bundle against live mainnet data, then either **simulate**
//! it (dry-run, free) or **submit** it and track its lifecycle via the stream.
//!
//! Split, as everywhere, into a pure tested core and thin live I/O:
//! - [`PendingBundles`] is the **stream-confirmed landing** correlator: it maps the signatures we
//!   submitted to their [`TransactionLifecycle`] and advances them from tx-status + slot-status
//!   stream events. Fully unit-tested — this is the logic the bounty's "confirm landing using
//!   stream subscriptions" requirement turns on.
//! - [`prepare_attempt`] does the live assembly (blockhash → tip accounts → live-data tip → sign);
//!   `simulate`/`submit` are one-liners over the RPC / Block Engine clients.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use prometheon_bundle::{
    build_tip_bundle_tx, compute_tip, self_transfer_ix, serialize_tx_base64, BlockEngineClient,
    BundleParams, Percentile, TipStrategy,
};
use prometheon_lifecycle::{LifecycleStage, TransactionLifecycle, TransitionOutcome};
use prometheon_telemetry::Decision;
use prometheon_types::{Slot, SlotStatus};
use solana_sdk::{hash::Hash, pubkey::Pubkey, signature::Keypair, signer::Signer};

use crate::rpc::RpcClient;
use crate::submission::resolve_tip;

/// Compute-unit limit for the proof tx (two transfers + compute-budget ixs; padded generously).
pub const CU_LIMIT: u32 = 5_000;

/// Map a slot's commitment status to the lifecycle stage it implies for a bundle landed in it.
pub fn slot_status_to_stage(status: SlotStatus) -> Option<LifecycleStage> {
    match status {
        SlotStatus::Processed => Some(LifecycleStage::Processed),
        SlotStatus::Confirmed => Some(LifecycleStage::Confirmed),
        SlotStatus::Finalized => Some(LifecycleStage::Finalized),
        SlotStatus::Dead => Some(LifecycleStage::Failed),
        _ => None,
    }
}

/// Priority-fee CU price (µlamports/CU) scaled by congestion. Modest — for Jito bundles the tip is
/// the auction lever; the priority fee is a small secondary nudge. Dynamic (never hardcoded flat).
pub fn priority_cu_price_micro(congestion: f64) -> u64 {
    let c = congestion.clamp(0.0, 1.0);
    (500.0 + 4_500.0 * c).round() as u64
}

/// The default tip policy (the AI agent / config may propose a value; the tip is always derived from
/// live floor data, never hardcoded). The `min_lamports` floor is a **competitive** 200k floor (well
/// above the 1000-lamport Jito minimum), so a sub-floor AI proposal is lifted to it and still lands —
/// which means in practice the floor, not the model's exact number, sets the tip when the AI
/// under-prices (see the committed proof run).
pub fn default_tip_strategy() -> TipStrategy {
    TipStrategy {
        // Target P95: the landed-tip distribution is heavily skewed and the floor endpoint is volatile
        // (P50/P75 routinely collapse to the ~1000-lamport Jito noise floor minute-to-minute). Anchor
        // on P95 and enforce a competitive safety floor (`min_lamports`) so a bundle reliably wins
        // inclusion even when the live floor momentarily reads near-zero. The deterministic core thus
        // *guarantees* a landing-grade tip; the AI proposes within this band.
        target: Percentile::P95,
        congestion_boost: 0.5,
        min_lamports: 200_000,
        max_lamports: 1_000_000,
    }
}

/// Bound a resolved tip to the policy's `[min, max]`. The AI strategist *proposes* the tip; the
/// deterministic core *caps* the absurd-high side and *lifts* a sub-floor proposal up to the
/// competitive `min_lamports` floor — so a malformed or manipulated decision can never make us overpay,
/// and an under-priced proposal still lands (the floor, not the model's number, then sets the tip).
/// Defense-in-depth against decision poisoning.
pub fn bounded_tip(resolved_lamports: u64, strategy: &TipStrategy) -> u64 {
    resolved_lamports.clamp(strategy.min_lamports, strategy.max_lamports)
}

/// Apply the tip policy to a resolved tip. Normally this is the full `[min, max]` clamp. When
/// `bypass_clamp` is set — used **only** for the low-tip fault injection — only the absurd-high cap
/// applies, so the deliberately sub-floor fault tip is honored (not raised to the competitive
/// `min_lamports`, which would defeat the injection and make it land).
pub fn apply_tip_policy(resolved_lamports: u64, bypass_clamp: bool, strategy: &TipStrategy) -> u64 {
    if bypass_clamp {
        resolved_lamports.min(strategy.max_lamports)
    } else {
        bounded_tip(resolved_lamports, strategy)
    }
}

/// A fully-assembled, signed attempt ready to simulate or submit.
#[derive(Debug, Clone)]
pub struct AttemptPlan {
    pub attempt: u32,
    pub tx_base64: String,
    pub signature: String,
    pub blockhash: String,
    pub last_valid_block_height: u64,
    pub tip_lamports: u64,
    pub tip_account: String,
    pub cu_limit: u32,
    pub cu_price_micro: u64,
    pub decision: Option<Decision>,
}

/// Assemble + sign one attempt against live data. No broadcast — caller chooses simulate vs submit.
///
/// `tip_override` forces a specific tip (the AI-chosen tip in the live path, or a fault-injection
/// value); `blockhash_override` substitutes a caller-supplied blockhash (used to submit on a
/// deliberately stale/expired one). Both `None` for a normal attempt.
///
/// `bypass_tip_clamp` skips the `[min_lamports, max_lamports]` policy clamp (capping only the
/// absurd-high side). It is set **only** for the low-tip fault injection, whose whole purpose is to
/// submit a deliberately sub-floor tip that must NOT be raised to the competitive minimum. A normal
/// or AI-chosen tip is always clamped to the policy band.
#[allow(clippy::too_many_arguments)]
pub async fn prepare_attempt(
    rpc: &RpcClient,
    jito: &BlockEngineClient,
    wallet: &Keypair,
    congestion: f64,
    decision: Option<Decision>,
    attempt: u32,
    transfer_lamports: u64,
    tip_override: Option<u64>,
    bypass_tip_clamp: bool,
    blockhash_override: Option<crate::rpc::BlockhashInfo>,
) -> anyhow::Result<AttemptPlan> {
    let bh = match blockhash_override {
        Some(b) => b,
        None => rpc.latest_blockhash().await?,
    };
    let tip_accounts = jito.get_tip_accounts().await?;
    let tip_account = tip_accounts
        .pick(attempt as u64)
        .ok_or_else(|| anyhow::anyhow!("no tip accounts returned"))?
        .to_string();
    let floor = jito.get_tip_floor().await?;

    let strategy = default_tip_strategy();
    let fallback = compute_tip(&floor, &strategy, congestion);
    // Resolve the tip: explicit override (AI-chosen tip or fault value) → AI decision → live-data
    // fallback. Then clamp to the policy band — UNLESS this is the low-tip fault (`bypass_tip_clamp`),
    // which must stay deliberately sub-floor to guarantee a non-landing (clamping it up to the
    // competitive `min_lamports` would defeat the fault). The clamp still protects every normal/AI
    // tip from a malformed/poisoned decision overpaying or under-tipping below the competitive floor.
    let resolved =
        tip_override.unwrap_or_else(|| resolve_tip(decision.as_ref(), fallback.lamports));
    let tip_lamports = apply_tip_policy(resolved, bypass_tip_clamp, &strategy);
    let cu_price_micro = priority_cu_price_micro(congestion);

    let params = BundleParams {
        payer: wallet,
        recent_blockhash: Hash::from_str(&bh.blockhash)
            .map_err(|e| anyhow::anyhow!("bad blockhash {}: {e}", bh.blockhash))?,
        compute_unit_limit: CU_LIMIT,
        compute_unit_price_micro: cu_price_micro,
        tip_account: Pubkey::from_str(&tip_account)
            .map_err(|e| anyhow::anyhow!("bad tip account {tip_account}: {e}"))?,
        tip_lamports,
        strategy_ixs: vec![self_transfer_ix(&wallet.pubkey(), transfer_lamports)],
    };
    let tx = build_tip_bundle_tx(&params);
    let signature = tx
        .signatures
        .first()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let tx_base64 = serialize_tx_base64(&tx)?;

    Ok(AttemptPlan {
        attempt,
        tx_base64,
        signature,
        blockhash: bh.blockhash,
        last_valid_block_height: bh.last_valid_block_height,
        tip_lamports,
        tip_account,
        cu_limit: CU_LIMIT,
        cu_price_micro,
        decision,
    })
}

/// Probe **real** on-chain / Jito error data for a non-landed bundle so the failure is classified from
/// observed signals (block height vs `lastValidBlockHeight`, `isBlockhashValid`, decoded
/// `getBundleStatuses.err`, or an inflight terminal failure) — not the injection tag. Shared by the
/// proof's `LiveSubmitter` and the product `EngineSubmitter`; `cached_blockhash` is the base's last
/// blockhash (each caller looks it up in its own per-base cache).
pub async fn probe_failure_signals(
    rpc: &RpcClient,
    jito: &BlockEngineClient,
    sb: &crate::proof_run::SubmittedBundle,
    floor_p50: u64,
    cached_blockhash: Option<crate::rpc::BlockhashInfo>,
) -> prometheon_failure::FailureSignals {
    let blockhash_valid = match &cached_blockhash {
        Some(b) => rpc.is_blockhash_valid(&b.blockhash).await.ok(),
        None => None,
    };
    let block_height = rpc.block_height().await.ok();
    let last_valid_block_height = cached_blockhash.as_ref().map(|b| b.last_valid_block_height);

    let (on_chain_error, bundle_status_failed) = match jito
        .get_bundle_statuses(std::slice::from_ref(&sb.bundle_id))
        .await
    {
        Ok(statuses) => match statuses.value.iter().find(|e| e.bundle_id == sb.bundle_id) {
            Some(e) if e.has_error() => (
                e.err.as_ref().map(crate::proof_run::decode_on_chain_error),
                false,
            ),
            _ => {
                let failed = jito
                    .get_inflight_bundle_statuses(std::slice::from_ref(&sb.bundle_id))
                    .await
                    .ok()
                    .map(|inf| {
                        inf.value
                            .iter()
                            .any(|e| e.bundle_id == sb.bundle_id && e.status.is_terminal_failure())
                    })
                    .unwrap_or(false);
                (None, failed)
            }
        },
        Err(_) => (None, false),
    };

    crate::proof_run::signals_from_observation(&crate::proof_run::FailureObservation {
        tip_lamports: sb.tip_lamports,
        tip_floor_p50_lamports: floor_p50,
        blockhash_valid,
        block_height,
        last_valid_block_height,
        on_chain_error,
        bundle_status_failed,
    })
}

/// A correlated lifecycle advance produced by a stream event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correlated {
    pub bundle_id: String,
    pub stage: LifecycleStage,
    pub outcome: TransitionOutcome,
}

/// One submitted bundle being tracked to a terminal lifecycle stage.
#[derive(Debug, Clone)]
pub struct PendingBundle {
    pub bundle_id: String,
    pub signatures: HashSet<String>,
    pub tip_lamports: u64,
    pub lifecycle: TransactionLifecycle,
    pub landed_slot: Option<Slot>,
}

/// The stream-confirmed landing correlator: signatures we submitted → their lifecycles, advanced by
/// tx-status (landed in a slot) and slot-status (that slot reaching confirmed/finalized) events.
#[derive(Debug, Default)]
pub struct PendingBundles {
    by_id: HashMap<String, PendingBundle>,
    sig_to_id: HashMap<String, String>,
}

impl PendingBundles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a freshly-submitted bundle (its lifecycle starts at `Submitted`).
    pub fn track(
        &mut self,
        bundle_id: impl Into<String>,
        signatures: Vec<String>,
        tip_lamports: u64,
        submitted_at: DateTime<Utc>,
    ) {
        let id = bundle_id.into();
        for s in &signatures {
            self.sig_to_id.insert(s.clone(), id.clone());
        }
        self.by_id.insert(
            id.clone(),
            PendingBundle {
                lifecycle: TransactionLifecycle::new(id.clone(), submitted_at),
                bundle_id: id,
                signatures: signatures.into_iter().collect(),
                tip_lamports,
                landed_slot: None,
            },
        );
    }

    /// A tracked signature appeared on the stream → it's in a block (`Processed`) or failed.
    pub fn on_tx_status(
        &mut self,
        signature: &str,
        slot: Slot,
        failed: bool,
        ts: DateTime<Utc>,
    ) -> Option<Correlated> {
        let id = self.sig_to_id.get(signature)?.clone();
        let b = self.by_id.get_mut(&id)?;
        let stage = if failed {
            LifecycleStage::Failed
        } else {
            LifecycleStage::Processed
        };
        let outcome = b.lifecycle.advance(stage, Some(slot), ts);
        if matches!(outcome, TransitionOutcome::Advanced) && stage == LifecycleStage::Processed {
            b.landed_slot = Some(slot);
        }
        Some(Correlated {
            bundle_id: id,
            stage,
            outcome,
        })
    }

    /// A slot advanced in commitment → advance any bundle that landed in that slot.
    pub fn on_slot_status(
        &mut self,
        slot: Slot,
        status: SlotStatus,
        ts: DateTime<Utc>,
    ) -> Vec<Correlated> {
        let Some(stage) = slot_status_to_stage(status) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for b in self.by_id.values_mut() {
            if b.landed_slot == Some(slot) && !b.lifecycle.is_terminal() {
                let outcome = b.lifecycle.advance(stage, Some(slot), ts);
                if matches!(outcome, TransitionOutcome::Advanced) {
                    out.push(Correlated {
                        bundle_id: b.bundle_id.clone(),
                        stage,
                        outcome,
                    });
                }
            }
        }
        out
    }

    pub fn get(&self, bundle_id: &str) -> Option<&PendingBundle> {
        self.by_id.get(bundle_id)
    }

    pub fn all_terminal(&self) -> bool {
        !self.by_id.is_empty() && self.by_id.values().all(|b| b.lifecycle.is_terminal())
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn cu_price_scales_with_congestion_and_is_never_flat() {
        assert_eq!(priority_cu_price_micro(0.0), 500);
        assert_eq!(priority_cu_price_micro(1.0), 5_000);
        assert_eq!(priority_cu_price_micro(0.5), 2_750);
        assert_eq!(priority_cu_price_micro(2.0), 5_000); // clamped
    }

    #[test]
    fn ai_tip_is_clamped_to_policy_bounds() {
        let s = default_tip_strategy(); // min 200_000 (competitive floor), max 1_000_000
                                        // A near-noise-floor tip is raised to the competitive minimum so the bundle reliably lands
                                        // (the landed-tip distribution is skewed; P50 sits at the ~1000-lamport Jito floor).
        assert_eq!(bounded_tip(50, &s), 200_000);
        assert_eq!(bounded_tip(5_000_000, &s), 1_000_000); // absurdly high → capped
        assert_eq!(bounded_tip(300_000, &s), 300_000); // within range → untouched
    }

    #[test]
    fn injected_low_tip_bypasses_the_competitive_floor() {
        let s = default_tip_strategy(); // min 200_000, max 1_000_000
                                        // Normal/AI tip: clamped UP to the competitive floor so the bundle lands.
        assert_eq!(apply_tip_policy(1_000, false, &s), 200_000);
        // Low-tip fault (bypass): the deliberately sub-floor tip is honored, guaranteeing a
        // non-landing — it must NOT be raised to the floor.
        assert_eq!(apply_tip_policy(1_000, true, &s), 1_000);
        // Even with bypass, an absurd-high tip is still capped (no accidental overpay).
        assert_eq!(apply_tip_policy(5_000_000, true, &s), 1_000_000);
    }

    #[test]
    fn slot_status_maps_to_lifecycle_stages() {
        assert_eq!(
            slot_status_to_stage(SlotStatus::Processed),
            Some(LifecycleStage::Processed)
        );
        assert_eq!(
            slot_status_to_stage(SlotStatus::Confirmed),
            Some(LifecycleStage::Confirmed)
        );
        assert_eq!(
            slot_status_to_stage(SlotStatus::Finalized),
            Some(LifecycleStage::Finalized)
        );
        assert_eq!(
            slot_status_to_stage(SlotStatus::Dead),
            Some(LifecycleStage::Failed)
        );
        assert_eq!(slot_status_to_stage(SlotStatus::Completed), None);
    }

    #[test]
    fn bundle_threads_submitted_to_finalized_via_stream() {
        let mut p = PendingBundles::new();
        p.track("bundleA", vec!["sigA".into()], 14_500, ts(0));
        assert_eq!(
            p.get("bundleA").unwrap().lifecycle.current_stage(),
            LifecycleStage::Submitted
        );

        // tx-status: our signature landed in slot 100 → Processed.
        let c = p.on_tx_status("sigA", 100, false, ts(1)).unwrap();
        assert_eq!(c.stage, LifecycleStage::Processed);
        assert_eq!(c.outcome, TransitionOutcome::Advanced);
        assert_eq!(p.get("bundleA").unwrap().landed_slot, Some(100));

        // slot 100 reaches confirmed, then finalized.
        let conf = p.on_slot_status(100, SlotStatus::Confirmed, ts(2));
        assert_eq!(conf.len(), 1);
        assert_eq!(conf[0].stage, LifecycleStage::Confirmed);
        let fin = p.on_slot_status(100, SlotStatus::Finalized, ts(14));
        assert_eq!(fin[0].stage, LifecycleStage::Finalized);

        let lc = &p.get("bundleA").unwrap().lifecycle;
        assert_eq!(lc.current_stage(), LifecycleStage::Finalized);
        assert!(lc.is_success());
        assert!(p.all_terminal());
    }

    #[test]
    fn failed_tx_status_marks_the_bundle_failed() {
        let mut p = PendingBundles::new();
        p.track("b", vec!["sig".into()], 1_000, ts(0));
        let c = p.on_tx_status("sig", 50, true, ts(1)).unwrap();
        assert_eq!(c.stage, LifecycleStage::Failed);
        assert!(p.get("b").unwrap().lifecycle.is_terminal());
    }

    #[test]
    fn untracked_signature_and_empty_slot_are_noops() {
        let mut p = PendingBundles::new();
        p.track("b", vec!["mine".into()], 1_000, ts(0));
        assert!(p.on_tx_status("someone-else", 10, false, ts(1)).is_none());
        // slot 999 has no landed bundle.
        assert!(p
            .on_slot_status(999, SlotStatus::Confirmed, ts(2))
            .is_empty());
    }
}
