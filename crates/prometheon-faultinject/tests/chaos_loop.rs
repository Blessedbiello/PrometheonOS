//! The headline chaos test: inject a fault, classify the resulting failure, and decide the retry —
//! proving the autonomous-recovery loop deterministically (the mandatory blockhash-expiry scenario,
//! plus low-tip). The live version (Phase 8) replaces the deterministic policy with the AI agent's
//! reasoned decision over the same signals.

use prometheon_failure::{classify, FailureClass};
use prometheon_faultinject::{normal_signals, FaultScenario};
use prometheon_retry::policy::{decide_retry, RetryAction};

#[test]
fn injected_blockhash_expiry_is_classified_and_recovered() {
    // 1. Inject the fault.
    let mut signals = normal_signals();
    FaultScenario::BlockhashExpiry.apply(&mut signals);

    // 2. Classify (what the engine observes).
    let classification = classify(&signals);
    assert_eq!(classification.class, FailureClass::ExpiredBlockhash);
    assert!(classification.confidence >= 0.9);
    assert!(classification.retryable);

    // 3. Decide the retry: refresh the blockhash and re-price, then resubmit.
    match decide_retry(classification.class, 1, 3) {
        RetryAction::Retry {
            refresh_blockhash,
            recalc_tip,
            next_attempt,
        } => {
            assert!(refresh_blockhash);
            assert!(recalc_tip);
            assert_eq!(next_attempt, 2);
        }
        other => panic!("expected Retry, got {other:?}"),
    }
}

#[test]
fn injected_low_tip_is_classified_and_repriced() {
    let mut signals = normal_signals();
    signals.blockhash_valid = Some(true); // window still open
    FaultScenario::LowTip { tip_lamports: 500 }.apply(&mut signals);

    let classification = classify(&signals);
    assert_eq!(classification.class, FailureClass::FeeTooLow);
    assert!(classification.retryable);

    match decide_retry(classification.class, 1, 3) {
        RetryAction::Retry {
            refresh_blockhash,
            recalc_tip,
            ..
        } => {
            assert!(!refresh_blockhash, "blockhash still valid");
            assert!(recalc_tip, "raise the tip");
        }
        other => panic!("expected Retry, got {other:?}"),
    }
}
