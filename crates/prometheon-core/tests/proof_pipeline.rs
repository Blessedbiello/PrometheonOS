//! End-to-end regression for the proof telemetry pipeline (the bounty's headline deliverable).
//!
//! This is the test that was MISSING: it drives the submit→track→emit driver
//! ([`prometheon_core::proof_run::track_and_emit`]) over a capturing sink with scripted stream
//! events, then feeds the captured telemetry through the SAME assembler the `export-log` binary uses
//! ([`prometheon_telemetry::export::build_log`]). If the proof path stops emitting `Bundle` /
//! `Lifecycle` / `Failure` events — the gap that previously made the exported log come out empty —
//! this test fails. No network, no DB: pure wiring verification.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::Instant;

use prometheon_core::proof_run::{track_and_emit, SubmittedBundle};
use prometheon_core::sinks::EventSink;
use prometheon_faultinject::FaultScenario;
use prometheon_ingest::{IngestMessage, SlotObservation, TxStatus};
use prometheon_telemetry::export::build_log;
use prometheon_telemetry::TelemetryEvent;
use prometheon_types::{SlotStatus, SlotUpdate};

/// A test sink that just records every emitted event for later assertions.
#[derive(Default)]
struct CapturingSink {
    events: Mutex<Vec<TelemetryEvent>>,
}

impl EventSink for CapturingSink {
    async fn emit(&self, event: &TelemetryEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
}

fn slot_msg(slot: u64, status: SlotStatus, at: DateTime<Utc>) -> IngestMessage {
    IngestMessage::Slot {
        update: SlotUpdate::new(slot, Some(slot - 1), status, at),
        observation: SlotObservation::Progressed {
            slot,
            skipped: vec![],
        },
    }
}

/// Bucket captured events by `kind`, exactly as `export.rs` does after reading them back from
/// Postgres, then run the canonical assembler.
fn export(events: &[TelemetryEvent]) -> Vec<prometheon_telemetry::export::LifecycleLogEntry> {
    let (mut bundles, mut lifecycles, mut failures) = (vec![], vec![], vec![]);
    for ev in events {
        let v: Value = serde_json::to_value(ev).unwrap();
        match v["kind"].as_str() {
            Some("bundle") => bundles.push(v),
            Some("lifecycle") => lifecycles.push(v),
            Some("failure") => failures.push(v),
            _ => {}
        }
    }
    build_log(&bundles, &lifecycles, &failures)
}

#[tokio::test]
async fn proof_run_emits_a_populated_lifecycle_log_with_failures() {
    // 12 bundles: 10 land (submitted→processed→confirmed→finalized), 1 low-tip failure, 1 expiry.
    let floor = 20_000u64;
    let mut submitted = Vec::new();
    for i in 0..10u64 {
        submitted.push(SubmittedBundle {
            bundle_id: format!("bundle-{i}"),
            signature: format!("sig-{i}"),
            tip_lamports: 14_500,
            tip_account: "Tip1111111111111111111111111111111111111111".into(),
            region: "ny".into(),
            submitted_at: ts(0),
            tip_floor_p50_lamports: floor,
            injected: None,
            base_id: format!("bundle-{i}"),
            attempt_no: 1,
            deadline_slot: None,
        });
    }
    submitted.push(SubmittedBundle {
        bundle_id: "bundle-10".into(),
        signature: "sig-10".into(),
        tip_lamports: 1_000, // below the floor → FeeTooLow
        tip_account: "Tip1111111111111111111111111111111111111111".into(),
        region: "ny".into(),
        submitted_at: ts(0),
        tip_floor_p50_lamports: floor,
        injected: Some(FaultScenario::LowTip {
            tip_lamports: 1_000,
        }),
        base_id: "bundle-10".into(),
        attempt_no: 1,
        deadline_slot: None,
    });
    submitted.push(SubmittedBundle {
        bundle_id: "bundle-11".into(),
        signature: "sig-11".into(),
        tip_lamports: 14_500,
        tip_account: "Tip1111111111111111111111111111111111111111".into(),
        region: "ny".into(),
        submitted_at: ts(0),
        tip_floor_p50_lamports: floor,
        injected: Some(FaultScenario::BlockhashExpiry),
        base_id: "bundle-11".into(),
        attempt_no: 1,
        deadline_slot: None,
    });

    // Script the stream: each landing bundle's signature lands in a distinct slot which then reaches
    // confirmed and finalized. The two injected bundles get no events (they never land).
    let (tx, mut rx) = mpsc::channel(1024);
    for i in 0..10u64 {
        let slot = 1_000 + i;
        tx.send(IngestMessage::Transaction(TxStatus {
            signature: format!("sig-{i}"),
            slot,
            failed: false,
            ts: ts(1),
        }))
        .await
        .unwrap();
        tx.send(slot_msg(slot, SlotStatus::Confirmed, ts(2)))
            .await
            .unwrap();
        tx.send(slot_msg(slot, SlotStatus::Finalized, ts(14)))
            .await
            .unwrap();
    }
    drop(tx); // closing the channel ends the drain loop deterministically

    let sink = CapturingSink::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let summary = track_and_emit(&sink, submitted, &mut rx, deadline).await;

    assert_eq!(summary.total, 12);
    assert_eq!(summary.landed, 10, "10 bundles should reach confirmed");
    assert_eq!(summary.failed, 2, "2 injected bundles should be failures");

    let log = export(&sink.events.lock().unwrap());
    assert_eq!(log.len(), 12, "every submitted bundle appears in the log");

    let landed = log.iter().filter(|e| e.landed()).count();
    assert!(landed >= 10, "≥10 landed (bounty floor), got {landed}");
    let with_failure = log.iter().filter(|e| e.failure_class.is_some()).count();
    assert!(
        with_failure >= 2,
        "≥2 classified failures (bounty floor), got {with_failure}"
    );

    // A landed bundle carries the full commitment progression, a real slot, and a latency.
    let l0 = log.iter().find(|e| e.bundle_id == "bundle-0").unwrap();
    assert_eq!(l0.final_stage.as_deref(), Some("finalized"));
    assert_eq!(l0.first_slot, Some(1_000));
    assert_eq!(l0.confirmed_latency_ms, Some(2_000)); // submitted→processed (1s) + →confirmed (1s)
    assert!(l0.landed());

    // The injected failures are classified correctly and do not count as landed.
    let low_tip = log.iter().find(|e| e.bundle_id == "bundle-10").unwrap();
    assert!(!low_tip.landed());
    assert_eq!(low_tip.failure_class.as_deref(), Some("fee_too_low"));
    let expired = log.iter().find(|e| e.bundle_id == "bundle-11").unwrap();
    assert!(!expired.landed());
    assert_eq!(expired.failure_class.as_deref(), Some("expired_blockhash"));
}
