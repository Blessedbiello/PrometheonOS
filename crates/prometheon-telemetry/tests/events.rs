//! Behavioural spec for the telemetry event envelope + subject mapping (Phase 4, test-first).
//!
//! `TelemetryEvent` is the single tagged envelope that flows over NATS and into persistence. Every
//! variant must round-trip through JSON (the wire format shared with the TS agent + dashboard) and
//! map to a stable NATS subject.

use chrono::Utc;
use prometheon_failure::{classify, FailureSignals};
use prometheon_lifecycle::lifecycle::LifecycleEvent;
use prometheon_lifecycle::LifecycleStage;
use prometheon_netmodel::NetworkHealthModel;
use prometheon_telemetry::decision::{Decision, DecisionType};
use prometheon_telemetry::events::{
    BundleEvent, BundlePhase, FailureRecord, LifecycleRecord, TelemetryEvent,
};
use prometheon_types::{SlotStatus, SlotUpdate};
use serde_json::json;

fn roundtrip(ev: &TelemetryEvent) -> TelemetryEvent {
    let s = serde_json::to_string(ev).expect("serialize");
    serde_json::from_str(&s).expect("deserialize")
}

#[test]
fn slot_event_roundtrips_and_maps_subject() {
    let ev = TelemetryEvent::Slot(SlotUpdate::new(
        100,
        Some(99),
        SlotStatus::Confirmed,
        Utc::now(),
    ));
    assert_eq!(roundtrip(&ev), ev);
    assert_eq!(ev.subject(), "telemetry.slot");
}

#[test]
fn bundle_event_roundtrips_and_maps_subject() {
    let ev = TelemetryEvent::Bundle(BundleEvent {
        bundle_id: "b1".into(),
        tip_lamports: 18_000,
        tip_account: "TipAcct".into(),
        region: "ny".into(),
        signatures: vec!["sig1".into()],
        phase: BundlePhase::Submitted,
        ts: Utc::now(),
    });
    assert_eq!(roundtrip(&ev), ev);
    assert_eq!(ev.subject(), "telemetry.bundle");
}

#[test]
fn lifecycle_and_failure_events_roundtrip() {
    let lc = TelemetryEvent::Lifecycle(LifecycleRecord {
        id: "b1".into(),
        event: LifecycleEvent {
            stage: LifecycleStage::Confirmed,
            slot: Some(100),
            ts: Utc::now(),
            delta_ms_from_prev: Some(700),
        },
    });
    assert_eq!(roundtrip(&lc), lc);
    assert_eq!(lc.subject(), "telemetry.lifecycle");

    let classification = classify(&FailureSignals {
        on_chain_error: None,
        bundle_status_failed: false,
        block_height_exceeded: true,
        blockhash_valid: None,
        landed: false,
        tip_lamports: 10_000,
        tip_floor_p50_lamports: 50_000,
        leader_missed: false,
        slot_skipped: false,
        confirmation_timeout: false,
    });
    let fe = TelemetryEvent::Failure(FailureRecord {
        id: "b1".into(),
        classification,
    });
    assert_eq!(roundtrip(&fe), fe);
    assert_eq!(fe.subject(), "telemetry.failure");
}

#[test]
fn health_event_roundtrips_and_maps_subject() {
    let snap = NetworkHealthModel::new(8).snapshot(Utc::now());
    let ev = TelemetryEvent::Health(snap);
    assert_eq!(roundtrip(&ev), ev);
    assert_eq!(ev.subject(), "telemetry.health");
}

#[test]
fn decision_event_subject_depends_on_decision_type() {
    let make = |dt: DecisionType| {
        TelemetryEvent::Decision(Decision {
            decision_type: dt,
            action: "tip 12000->18000".into(),
            reasoning: "congestion rising; 3 recent 12k bundles missed".into(),
            confidence: 0.81,
            inputs_considered: json!({ "congestion": 0.74, "floor_p50": 14200 }),
            before: Some(json!({ "tip": 12000 })),
            after: Some(json!({ "tip": 18000 })),
            provider: "anthropic".into(),
            latency_ms: 850,
            ts: Utc::now(),
        })
    };
    let tip = make(DecisionType::Tip);
    assert_eq!(roundtrip(&tip), tip);
    assert_eq!(tip.subject(), "decision.tip");
    assert_eq!(make(DecisionType::Timing).subject(), "decision.timing");
    assert_eq!(make(DecisionType::Retry).subject(), "decision.retry");
}
