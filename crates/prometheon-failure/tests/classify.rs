//! Behavioural spec for the failure classifier (Phase 3, test-first).
//!
//! Maps observable signals to the failure taxonomy with a confidence score. Observable (O) classes
//! (on-chain error, block-height expiry) score high; Inferred (I) classes (fee-too-low, leader
//! miss) score moderately and scale with signal strength. On-chain errors take precedence over
//! inference.

use prometheon_failure::classify::{
    classify, EvidenceGrade, FailureClass, FailureSignals, OnChainError,
};

/// A baseline "non-landed, nothing known" signal set; tests flip individual fields.
fn base() -> FailureSignals {
    FailureSignals {
        on_chain_error: None,
        bundle_status_failed: false,
        block_height_exceeded: false,
        blockhash_valid: None,
        landed: false,
        tip_lamports: 100_000,
        tip_floor_p50_lamports: 50_000,
        leader_missed: false,
        slot_skipped: false,
        confirmation_timeout: false,
    }
}

#[test]
fn compute_exceeded_is_observable_high_confidence_not_retryable() {
    let mut s = base();
    s.on_chain_error = Some(OnChainError::ComputeBudgetExceeded);
    let c = classify(&s);
    assert_eq!(c.class, FailureClass::ComputeExceeded);
    assert_eq!(c.grade, EvidenceGrade::Observable);
    assert!(c.confidence >= 0.9);
    assert!(!c.retryable);
}

#[test]
fn expired_blockhash_from_block_height_exceeded() {
    let mut s = base();
    s.block_height_exceeded = true;
    let c = classify(&s);
    assert_eq!(c.class, FailureClass::ExpiredBlockhash);
    assert_eq!(c.grade, EvidenceGrade::Observable);
    assert!(c.confidence >= 0.9);
    assert!(c.retryable);
}

#[test]
fn expired_blockhash_from_isblockhashvalid_false() {
    let mut s = base();
    s.blockhash_valid = Some(false);
    assert_eq!(classify(&s).class, FailureClass::ExpiredBlockhash);
}

#[test]
fn fee_too_low_is_inferred_when_tip_below_floor_and_not_landed() {
    let mut s = base();
    s.blockhash_valid = Some(true); // window still open, so not expiry
    s.tip_lamports = 10_000;
    s.tip_floor_p50_lamports = 50_000; // tip is 1/5 of the median
    let c = classify(&s);
    assert_eq!(c.class, FailureClass::FeeTooLow);
    assert_eq!(c.grade, EvidenceGrade::Inferred);
    assert!(c.retryable);
    assert!(c.confidence > 0.5 && c.confidence < 0.95);
}

#[test]
fn fee_too_low_confidence_rises_as_tip_falls_further_below_floor() {
    let mut s = base();
    s.blockhash_valid = Some(true);
    s.tip_floor_p50_lamports = 100_000;

    s.tip_lamports = 90_000; // barely below
    let low = classify(&s).confidence;
    s.tip_lamports = 1_000; // far below
    let high = classify(&s).confidence;
    assert!(
        high > low,
        "bigger shortfall ⇒ higher confidence it's fee-related"
    );
}

#[test]
fn on_chain_error_takes_precedence_over_low_tip_inference() {
    let mut s = base();
    s.tip_lamports = 1; // would look like fee-too-low
    s.tip_floor_p50_lamports = 50_000;
    s.on_chain_error = Some(OnChainError::ComputeBudgetExceeded);
    // But it actually landed-and-failed on compute — observable wins.
    assert_eq!(classify(&s).class, FailureClass::ComputeExceeded);
}

#[test]
fn duplicate_signature_and_instruction_error_are_not_retryable() {
    let mut s = base();
    s.on_chain_error = Some(OnChainError::DuplicateSignature);
    let c = classify(&s);
    assert_eq!(c.class, FailureClass::DuplicateSignature);
    assert!(!c.retryable);

    s.on_chain_error = Some(OnChainError::InstructionError);
    assert_eq!(classify(&s).class, FailureClass::InstructionError);
}

#[test]
fn bundle_failure_when_inflight_failed_without_onchain_error() {
    let mut s = base();
    s.bundle_status_failed = true;
    assert_eq!(classify(&s).class, FailureClass::BundleFailure);
}

#[test]
fn sub_floor_tip_beats_probe_time_expiry_and_generic_inflight_failure() {
    // The low-tip fault, classified from real probe data: by probe time (after the give-up wait) the
    // blockhash has naturally expired and Jito reports a generic inflight `Failed` — neither names the
    // cause. The sub-floor tip is the decisive, time-invariant root cause (could never win the
    // auction), so it must win over both.
    let mut s = base();
    s.tip_lamports = 1_000;
    s.tip_floor_p50_lamports = 9_360;
    s.blockhash_valid = Some(false); // expired during the unavoidable wait
    s.bundle_status_failed = true; // Jito's generic "Failed"
    assert_eq!(classify(&s).class, FailureClass::FeeTooLow);
}

#[test]
fn adequate_tip_with_expiry_is_expired_blockhash_not_generic_failure() {
    // The stale-blockhash fault: the tip is competitive (≥ floor), so the expired blockhash is the
    // cause — and it must win over the generic inflight-`Failed` signal.
    let mut s = base();
    s.tip_lamports = 200_000;
    s.tip_floor_p50_lamports = 9_360;
    s.blockhash_valid = Some(false);
    s.bundle_status_failed = true;
    assert_eq!(classify(&s).class, FailureClass::ExpiredBlockhash);
}

#[test]
fn skipped_slot_and_leader_miss_are_distinguished_by_grade() {
    let mut s = base();
    s.blockhash_valid = Some(true);
    s.slot_skipped = true;
    let skipped = classify(&s);
    assert_eq!(skipped.class, FailureClass::SkippedSlot);
    assert_eq!(skipped.grade, EvidenceGrade::Observable);

    let mut s2 = base();
    s2.blockhash_valid = Some(true);
    s2.leader_missed = true;
    let miss = classify(&s2);
    assert_eq!(miss.class, FailureClass::LeaderMiss);
    assert_eq!(miss.grade, EvidenceGrade::Inferred);
    assert!(miss.retryable);
}

#[test]
fn confirmation_timeout_when_window_open_and_nothing_else() {
    let mut s = base();
    s.blockhash_valid = Some(true);
    s.confirmation_timeout = true;
    let c = classify(&s);
    assert_eq!(c.class, FailureClass::ConfirmationTimeout);
    assert!(c.retryable);
}

#[test]
fn unclassified_when_no_signal_matches() {
    let mut s = base();
    s.blockhash_valid = Some(true);
    s.tip_lamports = 100_000; // at/above floor, so not fee-too-low
    let c = classify(&s);
    assert_eq!(c.class, FailureClass::Unclassified);
    assert!(!c.retryable);
    assert!(c.confidence < 0.5);
}
