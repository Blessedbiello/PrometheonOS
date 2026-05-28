//! Behavioural spec for fault injection (Phase 6, test-first).
//!
//! Each scenario deterministically perturbs the inputs the engine reasons over, so we can observe
//! adaptation under chaos. The headline test chains injection → classification → retry decision,
//! proving the mandatory blockhash-expiry recovery loop end-to-end (deterministic core).

use prometheon_faultinject::{normal_signals, FaultInjector, FaultScenario};

#[test]
fn blockhash_expiry_injection_marks_the_signals_expired() {
    let mut s = normal_signals();
    assert!(!s.block_height_exceeded);
    FaultScenario::BlockhashExpiry.apply(&mut s);
    assert!(s.block_height_exceeded);
    assert!(!s.landed);
    assert_eq!(s.blockhash_valid, Some(false));
}

#[test]
fn low_tip_injection_sets_tip_below_floor() {
    let mut s = normal_signals();
    FaultScenario::LowTip { tip_lamports: 100 }.apply(&mut s);
    assert_eq!(s.tip_lamports, 100);
    assert!(s.tip_lamports < s.tip_floor_p50_lamports);
    assert!(!s.landed);
}

#[test]
fn congestion_injection_is_reported_for_the_health_model() {
    let scenario = FaultScenario::Congestion { score: 0.9 };
    assert_eq!(scenario.congestion_override(), Some(0.9));
    assert_eq!(FaultScenario::BlockhashExpiry.congestion_override(), None);
}

#[test]
fn delayed_submission_reports_its_delay() {
    assert_eq!(
        FaultScenario::DelayedSubmission { delay_ms: 5_000 }.submission_delay_ms(),
        Some(5_000)
    );
    assert_eq!(
        FaultScenario::LowTip { tip_lamports: 1 }.submission_delay_ms(),
        None
    );
}

#[test]
fn dropped_stream_events_drops_every_nth() {
    let injector = FaultInjector::new(vec![FaultScenario::DroppedStreamEvents { drop_every: 3 }]);
    // Events 3, 6, 9, ... are dropped; others pass.
    let dropped: Vec<bool> = (1..=6).map(|i| injector.should_drop_event(i)).collect();
    assert_eq!(dropped, vec![false, false, true, false, false, true]);
}

#[test]
fn no_drop_scenario_never_drops() {
    let injector = FaultInjector::new(vec![FaultScenario::BlockhashExpiry]);
    assert!((1..=10).all(|i| !injector.should_drop_event(i)));
}
