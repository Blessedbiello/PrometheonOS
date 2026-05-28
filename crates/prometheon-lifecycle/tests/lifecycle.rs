//! Behavioural spec for the per-submission lifecycle state machine (Phase 3, test-first).
//!
//! Tracks one bundle/transaction attempt through Submitted → Processed → Confirmed → Finalized
//! (with failure branches), capturing slot numbers, timestamps, latency deltas between stages, and
//! the processed→confirmed delta that answers README Q1. Retry/abandon is the retry orchestrator's
//! concern (saga level, Phase 6), not this per-attempt machine.

use chrono::{TimeZone, Utc};
use prometheon_lifecycle::lifecycle::{LifecycleStage, TransactionLifecycle, TransitionOutcome};

fn at(secs: i64, millis: u32) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + secs, millis * 1_000_000)
        .unwrap()
}

#[test]
fn starts_in_submitted_with_no_latencies() {
    let lc = TransactionLifecycle::new("bundle1", at(0, 0));
    assert_eq!(lc.current_stage(), LifecycleStage::Submitted);
    assert_eq!(lc.processed_latency_ms(), None);
    assert_eq!(lc.confirmed_latency_ms(), None);
    assert!(!lc.is_terminal());
}

#[test]
fn happy_path_records_stages_slots_and_latency_deltas() {
    let mut lc = TransactionLifecycle::new("bundle1", at(0, 0));
    assert_eq!(
        lc.advance(LifecycleStage::Processed, Some(100), at(0, 500)),
        TransitionOutcome::Advanced
    );
    assert_eq!(
        lc.advance(LifecycleStage::Confirmed, Some(100), at(1, 200)),
        TransitionOutcome::Advanced
    );
    assert_eq!(
        lc.advance(LifecycleStage::Finalized, Some(100), at(13, 300)),
        TransitionOutcome::Advanced
    );

    // submit→processed = 500ms; submit→confirmed = 1200ms; submit→finalized = 13300ms.
    assert_eq!(lc.processed_latency_ms(), Some(500));
    assert_eq!(lc.confirmed_latency_ms(), Some(1_200));
    assert_eq!(lc.finalized_latency_ms(), Some(13_300));
    // processed→confirmed delta (README Q1 health signal) = 700ms.
    assert_eq!(lc.processed_to_confirmed_ms(), Some(700));

    assert_eq!(lc.current_stage(), LifecycleStage::Finalized);
    assert!(lc.is_terminal());
    assert!(lc.is_success());

    // Each recorded event carries the slot and the delta from the previous event.
    let events = lc.events();
    assert_eq!(events.len(), 4); // Submitted + 3 advances
    assert_eq!(events[1].stage, LifecycleStage::Processed);
    assert_eq!(events[1].slot, Some(100));
    assert_eq!(events[1].delta_ms_from_prev, Some(500));
    assert_eq!(events[2].delta_ms_from_prev, Some(700)); // processed→confirmed
}

#[test]
fn illegal_transition_is_rejected_without_mutating() {
    let mut lc = TransactionLifecycle::new("bundle1", at(0, 0));
    // Submitted cannot jump straight to Finalized.
    assert_eq!(
        lc.advance(LifecycleStage::Finalized, Some(100), at(1, 0)),
        TransitionOutcome::Illegal {
            from: LifecycleStage::Submitted,
            to: LifecycleStage::Finalized,
        }
    );
    assert_eq!(lc.current_stage(), LifecycleStage::Submitted);
    assert_eq!(lc.events().len(), 1); // only the initial Submitted event
}

#[test]
fn redundant_same_stage_is_ignored() {
    let mut lc = TransactionLifecycle::new("bundle1", at(0, 0));
    lc.advance(LifecycleStage::Processed, Some(100), at(0, 400));
    // The stream may resend Processed for the same slot — ignore, don't error or double-count.
    assert_eq!(
        lc.advance(LifecycleStage::Processed, Some(100), at(0, 450)),
        TransitionOutcome::Redundant
    );
    assert_eq!(lc.events().len(), 2); // Submitted + one Processed
}

#[test]
fn failure_branch_from_submitted_is_terminal_and_not_success() {
    let mut lc = TransactionLifecycle::new("bundle1", at(0, 0));
    assert_eq!(
        lc.advance(LifecycleStage::Expired, None, at(65, 0)),
        TransitionOutcome::Advanced
    );
    assert!(lc.is_terminal());
    assert!(!lc.is_success());
    assert_eq!(lc.current_stage(), LifecycleStage::Expired);
}

#[test]
fn processed_can_fail_on_dead_slot() {
    let mut lc = TransactionLifecycle::new("bundle1", at(0, 0));
    lc.advance(LifecycleStage::Processed, Some(100), at(0, 400));
    // Slot went DEAD (forked out) after we saw it processed.
    assert_eq!(
        lc.advance(LifecycleStage::Failed, Some(100), at(1, 0)),
        TransitionOutcome::Advanced
    );
    assert!(lc.is_terminal());
    assert!(!lc.is_success());
}

#[test]
fn cannot_advance_out_of_a_terminal_stage() {
    let mut lc = TransactionLifecycle::new("bundle1", at(0, 0));
    lc.advance(LifecycleStage::Failed, None, at(1, 0));
    assert_eq!(
        lc.advance(LifecycleStage::Processed, Some(100), at(2, 0)),
        TransitionOutcome::Illegal {
            from: LifecycleStage::Failed,
            to: LifecycleStage::Processed,
        }
    );
}
