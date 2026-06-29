//! The integrated proof driver: track submitted bundles over one shared stream and **emit the
//! telemetry the lifecycle-log export reads** (`Bundle` on submit, `Lifecycle` per commitment
//! advance, `Failure` for anything that didn't land).
//!
//! This is the seam that connects the (previously memory-only) submit path to the
//! `telemetry_event` table and the `export-log` binary. The driver is generic over [`EventSink`] so
//! the full submit→emit→export pipeline is exercised without a network or database (see
//! `tests/proof_pipeline.rs`); the live binary passes the real NATS+Postgres [`Sinks`].

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use tokio::time::Instant;

use prometheon_failure::{classify, FailureClassification, FailureSignals, OnChainError};
use prometheon_faultinject::{normal_signals, FaultScenario};
use prometheon_ingest::IngestMessage;
use prometheon_lifecycle::lifecycle::LifecycleEvent;
use prometheon_telemetry::{
    BundleEvent, BundlePhase, FailureRecord, LifecycleRecord, TelemetryEvent,
};
use serde_json::Value;

use crate::proof::PendingBundles;
use crate::sinks::EventSink;

/// One bundle that has been sent to the Block Engine and is now being tracked.
#[derive(Debug, Clone)]
pub struct SubmittedBundle {
    pub bundle_id: String,
    pub signature: String,
    pub tip_lamports: u64,
    pub tip_account: String,
    pub region: String,
    pub submitted_at: DateTime<Utc>,
    /// The tip-floor median at submit time — lets the classifier infer `FeeTooLow` on a non-landing.
    pub tip_floor_p50_lamports: u64,
    /// Set when this attempt was a deliberately injected fault; drives the failure classification.
    pub injected: Option<FaultScenario>,
    /// The logical bundle this attempt belongs to (retries share a `base_id`).
    pub base_id: String,
    /// 1-indexed attempt number for this `base_id`.
    pub attempt_no: u32,
    /// Observed-slot watermark past which the saga gives up on this attempt and treats it as failed
    /// (the give-up signal that drives autonomous retry). `None` → only fails via a failed tx-status
    /// or the global deadline.
    pub deadline_slot: Option<u64>,
}

/// Outcome counts for a proof run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProofSummary {
    pub total: usize,
    pub landed: usize,
    pub failed: usize,
}

/// Derive a region label from a Block Engine base URL (e.g. `https://ny.mainnet.block-engine…` → `ny`).
pub fn region_from_url(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('.')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

/// The `Bundle` telemetry event announcing a freshly-submitted bundle.
pub fn bundle_submitted_event(b: &SubmittedBundle) -> TelemetryEvent {
    TelemetryEvent::Bundle(BundleEvent {
        bundle_id: b.bundle_id.clone(),
        tip_lamports: b.tip_lamports,
        tip_account: b.tip_account.clone(),
        region: b.region.clone(),
        signatures: vec![b.signature.clone()],
        phase: BundlePhase::Submitted,
        ts: b.submitted_at,
    })
}

/// A `Lifecycle` telemetry event for one recorded commitment transition.
pub fn lifecycle_event(id: &str, event: LifecycleEvent) -> TelemetryEvent {
    TelemetryEvent::Lifecycle(LifecycleRecord {
        id: id.to_string(),
        event,
    })
}

/// A `Failure` telemetry event carrying a classification.
pub fn failure_event(id: &str, classification: FailureClassification) -> TelemetryEvent {
    TelemetryEvent::Failure(FailureRecord {
        id: id.to_string(),
        classification,
    })
}

/// Real on-chain / Jito error observations for a non-landed bundle, gathered live (block height vs
/// `lastValidBlockHeight`, `isBlockhashValid`, and the bundle's decoded `err`). Mapping this into
/// [`FailureSignals`] lets the classifier decide the class from **real error data** rather than the
/// injected fault tag — what the bounty asks for.
#[derive(Debug, Clone, Default)]
pub struct FailureObservation {
    pub tip_lamports: u64,
    pub tip_floor_p50_lamports: u64,
    /// `isBlockhashValid` result (if the probe could fetch it).
    pub blockhash_valid: Option<bool>,
    /// Current block height (if fetched).
    pub block_height: Option<u64>,
    /// The submitted blockhash's `lastValidBlockHeight` (if known).
    pub last_valid_block_height: Option<u64>,
    /// Decoded on-chain error from `getBundleStatuses.err`, if the bundle landed-and-reverted.
    pub on_chain_error: Option<OnChainError>,
    /// Inflight status reported a terminal failure across regions.
    pub bundle_status_failed: bool,
}

/// Build [`FailureSignals`] from real observations. A `confirmation_timeout` baseline is set (we only
/// probe non-lands); the classifier's priority order means a decoded on-chain error, an inflight
/// failure, a real blockhash expiry, or a sub-floor tip each override it.
pub fn signals_from_observation(obs: &FailureObservation) -> FailureSignals {
    let mut s = normal_signals();
    s.landed = false;
    s.confirmation_timeout = true;
    s.tip_lamports = obs.tip_lamports;
    s.tip_floor_p50_lamports = obs.tip_floor_p50_lamports;
    s.blockhash_valid = obs.blockhash_valid;
    s.block_height_exceeded = matches!(
        (obs.block_height, obs.last_valid_block_height),
        (Some(h), Some(lvbh)) if h > lvbh
    );
    s.on_chain_error = obs.on_chain_error.clone();
    s.bundle_status_failed = obs.bundle_status_failed;
    s
}

/// Decode a Jito `getBundleStatuses.err` JSON value into an [`OnChainError`] (best-effort string
/// match against the common Solana error shapes).
pub fn decode_on_chain_error(err: &Value) -> OnChainError {
    let s = err.to_string().to_lowercase();
    if s.contains("computebudgetexceeded") || s.contains("computebudget") {
        OnChainError::ComputeBudgetExceeded
    } else if s.contains("alreadyprocessed") || s.contains("duplicate") {
        OnChainError::DuplicateSignature
    } else if s.contains("instructionerror") {
        OnChainError::InstructionError
    } else {
        OnChainError::Other(err.to_string())
    }
}

/// Classify why a tracked bundle did not land — from the injected scenario when present, otherwise
/// from the observed signals (tip vs floor, confirmation timeout). The live path prefers
/// [`signals_from_observation`] (real error data) via [`crate::saga::Submitter::probe_failure`].
pub fn classify_nonland(b: &SubmittedBundle) -> FailureClassification {
    let mut signals: FailureSignals = normal_signals();
    signals.landed = false;
    signals.tip_lamports = b.tip_lamports;
    signals.tip_floor_p50_lamports = b.tip_floor_p50_lamports;

    match b.injected {
        // A deliberately expired blockhash is observable and decisive.
        Some(FaultScenario::BlockhashExpiry) => FaultScenario::BlockhashExpiry.apply(&mut signals),
        // A deliberately sub-floor tip: leave the (already-low) tip vs floor to drive `FeeTooLow`.
        Some(FaultScenario::LowTip { .. }) | None => {
            if signals.tip_lamports >= signals.tip_floor_p50_lamports {
                // Not obviously under-tipped → it simply never confirmed in our window.
                signals.confirmation_timeout = true;
            }
        }
        // Other scenarios don't perturb the landing signals on their own.
        Some(_) => signals.confirmation_timeout = true,
    }
    classify(&signals)
}

/// Track every submitted bundle over one shared ingest stream, emitting telemetry as each progresses,
/// until all are terminal or the deadline passes. A `Failure` event is emitted for any bundle that
/// did not reach `confirmed`.
///
/// Generic over [`EventSink`]: the live driver passes NATS+Postgres [`crate::sinks::Sinks`]; tests
/// pass a capturing sink. The stream is a plain `mpsc::Receiver<IngestMessage>` — the real one from
/// `yellowstone::spawn`, or a pre-loaded channel in tests.
pub async fn track_and_emit<S: EventSink>(
    sink: &S,
    submitted: Vec<SubmittedBundle>,
    rx: &mut mpsc::Receiver<IngestMessage>,
    deadline: Instant,
) -> ProofSummary {
    let meta: HashMap<String, SubmittedBundle> = submitted
        .iter()
        .map(|b| (b.bundle_id.clone(), b.clone()))
        .collect();

    let mut pending = PendingBundles::new();
    for b in &submitted {
        sink.emit(&bundle_submitted_event(b)).await;
        pending.track(
            b.bundle_id.clone(),
            vec![b.signature.clone()],
            b.tip_lamports,
            b.submitted_at,
        );
        emit_last_lifecycle(sink, &pending, &b.bundle_id).await; // the initial `Submitted` event
    }

    loop {
        if pending.all_terminal() {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(IngestMessage::Transaction(tx))) => {
                if let Some(c) = pending.on_tx_status(&tx.signature, tx.slot, tx.failed, tx.ts) {
                    emit_last_lifecycle(sink, &pending, &c.bundle_id).await;
                }
            }
            Ok(Some(IngestMessage::Slot { update, .. })) => {
                for c in pending.on_slot_status(update.slot, update.status, update.ts) {
                    emit_last_lifecycle(sink, &pending, &c.bundle_id).await;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }

    let mut summary = ProofSummary {
        total: submitted.len(),
        ..Default::default()
    };
    for b in &submitted {
        let landed = pending
            .get(&b.bundle_id)
            .map(|pb| pb.lifecycle.reached_confirmed())
            .unwrap_or(false);
        if landed {
            summary.landed += 1;
        } else {
            summary.failed += 1;
            let classification = classify_nonland(&meta[&b.bundle_id]);
            sink.emit(&failure_event(&b.bundle_id, classification))
                .await;
        }
    }
    summary
}

/// Emit a `Lifecycle` event for a bundle's most recently recorded transition.
async fn emit_last_lifecycle<S: EventSink>(sink: &S, pending: &PendingBundles, id: &str) {
    if let Some(pb) = pending.get(id) {
        if let Some(ev) = pb.lifecycle.events().last().cloned() {
            sink.emit(&lifecycle_event(id, ev)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheon_failure::FailureClass;

    fn mk(injected: Option<FaultScenario>, tip: u64) -> SubmittedBundle {
        SubmittedBundle {
            bundle_id: "b".into(),
            signature: "s".into(),
            tip_lamports: tip,
            tip_account: "acct".into(),
            region: "ny".into(),
            submitted_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            tip_floor_p50_lamports: 20_000,
            injected,
            base_id: "b".into(),
            attempt_no: 1,
            deadline_slot: None,
        }
    }

    #[test]
    fn region_parsing() {
        assert_eq!(
            region_from_url("https://ny.mainnet.block-engine.jito.wtf"),
            "ny"
        );
        assert_eq!(region_from_url("garbage"), "garbage");
    }

    #[test]
    fn nonland_classification_uses_injected_scenario() {
        // The stale-blockhash fault carries a competitive tip (≥ floor 20_000), so the expiry — not the
        // fee — is the cause; a sub-floor tip would (correctly) classify as FeeTooLow instead.
        assert_eq!(
            classify_nonland(&mk(Some(FaultScenario::BlockhashExpiry), 25_000)).class,
            FailureClass::ExpiredBlockhash
        );
        assert_eq!(
            classify_nonland(&mk(
                Some(FaultScenario::LowTip {
                    tip_lamports: 1_000
                }),
                1_000
            ))
            .class,
            FailureClass::FeeTooLow
        );
    }

    #[test]
    fn nonland_without_injection_is_a_timeout() {
        // Adequately tipped but never confirmed in our window.
        assert_eq!(
            classify_nonland(&mk(None, 50_000)).class,
            FailureClass::ConfirmationTimeout
        );
    }

    #[test]
    fn real_signals_classify_from_data_not_the_injected_tag() {
        use prometheon_failure::OnChainError;
        // Expired: real block height past lastValidBlockHeight, with an adequate tip (so expiry, not
        // fee, is the cause — no injection tag involved).
        let expired = signals_from_observation(&FailureObservation {
            tip_lamports: 25_000,
            tip_floor_p50_lamports: 20_000,
            blockhash_valid: Some(false),
            block_height: Some(1_000),
            last_valid_block_height: Some(900),
            ..Default::default()
        });
        assert_eq!(classify(&expired).class, FailureClass::ExpiredBlockhash);

        // Fee too low: blockhash still valid, tip below the floor.
        let low = signals_from_observation(&FailureObservation {
            tip_lamports: 1_000,
            tip_floor_p50_lamports: 20_000,
            blockhash_valid: Some(true),
            block_height: Some(900),
            last_valid_block_height: Some(1_000),
            ..Default::default()
        });
        assert_eq!(classify(&low).class, FailureClass::FeeTooLow);

        // Landed-and-reverted compute error decoded from real getBundleStatuses.err.
        let compute = signals_from_observation(&FailureObservation {
            tip_lamports: 14_500,
            tip_floor_p50_lamports: 20_000,
            on_chain_error: Some(OnChainError::ComputeBudgetExceeded),
            ..Default::default()
        });
        assert_eq!(classify(&compute).class, FailureClass::ComputeExceeded);

        // Unknown non-land, valid blockhash, adequate tip → confirmation timeout.
        let timeout = signals_from_observation(&FailureObservation {
            tip_lamports: 50_000,
            tip_floor_p50_lamports: 20_000,
            blockhash_valid: Some(true),
            block_height: Some(900),
            last_valid_block_height: Some(1_000),
            ..Default::default()
        });
        assert_eq!(classify(&timeout).class, FailureClass::ConfirmationTimeout);
    }

    #[test]
    fn decode_on_chain_error_maps_known_shapes() {
        use prometheon_failure::OnChainError;
        use serde_json::json;
        assert_eq!(
            decode_on_chain_error(&json!("ComputeBudgetExceeded")),
            OnChainError::ComputeBudgetExceeded
        );
        assert!(matches!(
            decode_on_chain_error(&json!({ "InstructionError": [0, "X"] })),
            OnChainError::InstructionError
        ));
        assert!(matches!(
            decode_on_chain_error(&json!({ "weird": 1 })),
            OnChainError::Other(_)
        ));
    }
}
