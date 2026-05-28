//! Behavioural spec for the retry orchestrator (Phase 6, test-first).
//!
//! On a classified failure the orchestrator decides whether to retry and *what to change* before
//! resubmitting (refresh the blockhash, recalculate the tip), with exponential backoff and an
//! attempt cap. Every retry is justified by the failure class — there is no hardcoded retry flow.

use prometheon_failure::FailureClass;
use prometheon_retry::backoff::{backoff_ms, with_jitter};
use prometheon_retry::orchestrator::RetryOrchestrator;
use prometheon_retry::policy::{decide_retry, RetryAction};

#[test]
fn backoff_is_exponential_and_capped() {
    // base 250ms, doubling, capped at 10_000ms. Attempts are 1-indexed.
    assert_eq!(backoff_ms(1, 250, 10_000), 250);
    assert_eq!(backoff_ms(2, 250, 10_000), 500);
    assert_eq!(backoff_ms(3, 250, 10_000), 1_000);
    assert_eq!(backoff_ms(6, 250, 10_000), 8_000);
    assert_eq!(backoff_ms(7, 250, 10_000), 10_000); // 16_000 capped
    assert_eq!(backoff_ms(100, 250, 10_000), 10_000); // no overflow
}

#[test]
fn jitter_adds_up_to_the_configured_fraction() {
    assert_eq!(with_jitter(1_000, 0.5, 0.0), 1_000); // no jitter at rand=0
    assert_eq!(with_jitter(1_000, 0.5, 0.5), 1_250);
    assert_eq!(with_jitter(1_000, 0.5, 1.0), 1_500); // full +50%
}

#[test]
fn expired_blockhash_retry_refreshes_and_reprices() {
    let action = decide_retry(FailureClass::ExpiredBlockhash, 1, 3);
    match action {
        RetryAction::Retry {
            refresh_blockhash,
            recalc_tip,
            next_attempt,
        } => {
            assert!(refresh_blockhash, "expiry must refresh the blockhash");
            assert!(recalc_tip, "re-price for current conditions");
            assert_eq!(next_attempt, 2);
        }
        other => panic!("expected Retry, got {other:?}"),
    }
}

#[test]
fn fee_too_low_retry_reprices_without_refreshing_blockhash() {
    match decide_retry(FailureClass::FeeTooLow, 1, 3) {
        RetryAction::Retry {
            refresh_blockhash,
            recalc_tip,
            ..
        } => {
            assert!(!refresh_blockhash, "blockhash still valid for fee-too-low");
            assert!(recalc_tip, "raise the tip");
        }
        other => panic!("expected Retry, got {other:?}"),
    }
}

#[test]
fn non_retryable_classes_are_abandoned() {
    assert!(matches!(
        decide_retry(FailureClass::ComputeExceeded, 1, 3),
        RetryAction::Abandon { .. }
    ));
    assert!(matches!(
        decide_retry(FailureClass::DuplicateSignature, 1, 3),
        RetryAction::Abandon { .. }
    ));
}

#[test]
fn attempt_cap_abandons_even_retryable_classes() {
    // 3rd failed attempt with max 3 → no more retries.
    assert!(matches!(
        decide_retry(FailureClass::ExpiredBlockhash, 3, 3),
        RetryAction::Abandon { .. }
    ));
}

#[test]
fn orchestrator_tracks_attempts_per_saga_until_cap() {
    let mut orch = RetryOrchestrator::new(3, 250, 10_000);

    // First failure → retry with attempt 2, backoff ~250ms.
    let (a1, delay1) = orch.on_failure("bundleA", FailureClass::ExpiredBlockhash);
    assert!(matches!(
        a1,
        RetryAction::Retry {
            next_attempt: 2,
            ..
        }
    ));
    assert_eq!(delay1, Some(250));

    // Second failure → retry attempt 3, backoff ~500ms.
    let (a2, delay2) = orch.on_failure("bundleA", FailureClass::ExpiredBlockhash);
    assert!(matches!(
        a2,
        RetryAction::Retry {
            next_attempt: 3,
            ..
        }
    ));
    assert_eq!(delay2, Some(500));

    // Third failure → cap reached → abandon, no backoff.
    let (a3, delay3) = orch.on_failure("bundleA", FailureClass::ExpiredBlockhash);
    assert!(matches!(a3, RetryAction::Abandon { .. }));
    assert_eq!(delay3, None);

    // A different saga is tracked independently.
    let (b1, _) = orch.on_failure("bundleB", FailureClass::FeeTooLow);
    assert!(matches!(
        b1,
        RetryAction::Retry {
            next_attempt: 2,
            ..
        }
    ));
}
