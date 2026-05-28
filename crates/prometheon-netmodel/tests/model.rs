//! Behavioural spec for the composed network-health model (Phase 4, test-first).

use chrono::Utc;
use prometheon_netmodel::model::NetworkHealthModel;

#[test]
fn snapshot_composes_metrics_from_recorded_events() {
    let mut m = NetworkHealthModel::new(64);

    // 10 scheduled slots, 1 skipped → stability 0.9.
    for _ in 0..9 {
        m.record_slot(true);
    }
    m.record_slot(false);

    // 10 submissions, 7 landed (tip 20k, total cost 25k each).
    for _ in 0..10 {
        m.record_submission();
    }
    for _ in 0..7 {
        m.record_landing(20_000, 25_000);
    }

    // Confirm latencies.
    m.record_confirmed_latency_ms(600.0);
    m.record_confirmed_latency_ms(800.0);

    // Live tip floor.
    m.set_tip_floor_lamports(50_000);

    let snap = m.snapshot(Utc::now());

    assert!((snap.slot_stability_score - 0.9).abs() < 1e-9);
    assert!((snap.bundle_landing_probability - 0.7).abs() < 1e-9);
    assert_eq!(snap.avg_confirmed_latency_ms, Some(700.0));
    assert_eq!(snap.cost_per_successful_landing, Some(25_000.0));
    // 7 landings / 140_000 lamports tipped.
    assert!((snap.tip_efficiency_ratio - 7.0 / 140_000.0).abs() < 1e-12);
    assert_eq!(snap.tip_floor_lamports, 50_000);
    // Mild congestion: low skip, sub-second latency, modest tip floor.
    assert!(snap.congestion_score > 0.0 && snap.congestion_score < 0.3);
}

#[test]
fn retry_success_rate_is_tracked() {
    let mut m = NetworkHealthModel::new(16);
    m.record_retry(true);
    m.record_retry(true);
    m.record_retry(false);
    let snap = m.snapshot(Utc::now());
    assert!((snap.retry_success_rate - 2.0 / 3.0).abs() < 1e-9);
}

#[test]
fn fresh_model_snapshot_has_safe_defaults() {
    let m = NetworkHealthModel::new(16);
    let snap = m.snapshot(Utc::now());
    assert_eq!(snap.slot_stability_score, 1.0); // no evidence ⇒ assume stable
    assert_eq!(snap.bundle_landing_probability, 0.0);
    assert_eq!(snap.avg_confirmed_latency_ms, None);
    assert_eq!(snap.cost_per_successful_landing, None);
}
