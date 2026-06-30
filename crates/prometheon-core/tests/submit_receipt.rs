//! End-to-end regression for the `submit → Receipt` surface (the callable product).
//!
//! Drives [`prometheon_core::run_submit`] over the same no-network saga doubles as the saga test,
//! and asserts the returned [`Receipt`] is the honest view of the lifecycle the saga observed:
//! a landing reports the **landed** attempt's slot (never an earlier failed attempt's); an autonomous
//! recovery reports `attempts: 2`; an abandoned bundle reports `Failed` with the real-signal class and
//! does NOT pay a second tip. No network, no DB.

mod common;
use common::*;

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::Instant;

use prometheon_core::saga::{BaseBundle, SagaConfig};
use prometheon_core::{run_submit, Receipt};
use prometheon_faultinject::FaultScenario;
use prometheon_types::SlotStatus;

fn cfg(secs: u64) -> SagaConfig {
    SagaConfig {
        max_attempts: 3,
        global_deadline: Instant::now() + Duration::from_secs(secs),
    }
}

#[tokio::test]
async fn landed_submit_returns_landed_with_slot() {
    let bases = vec![BaseBundle {
        base_id: "b0".into(),
        injected: None,
        tip_context: json!({ "congestionScore": 0.4, "tipFloorP50Lamports": 20_000 }),
    }];
    let (tx, mut rx) = mpsc::channel(64);
    tx.send(tx_msg("b0-s1", 100, ts(1))).await.unwrap();
    tx.send(slot_msg(100, SlotStatus::Confirmed, ts(2)))
        .await
        .unwrap();
    tx.send(slot_msg(100, SlotStatus::Finalized, ts(14)))
        .await
        .unwrap();
    drop(tx);

    let sink = CapturingSink::default();
    let receipts = run_submit(&sink, &FakeDecider, &FakeSubmitter, bases, &mut rx, cfg(10)).await;

    assert_eq!(
        receipts,
        vec![Receipt::Landed {
            slot: 100,
            final_stage: "finalized".into(),
            attempts: 1,
        }]
    );
}

#[tokio::test]
async fn injected_failure_recovers_returns_landed_attempts_2() {
    // One bundle with an injected blockhash expiry: attempt 1 fails, the AI retries, attempt 2 lands.
    let bases = vec![BaseBundle {
        base_id: "b0".into(),
        injected: Some(FaultScenario::BlockhashExpiry),
        tip_context: json!({ "congestionScore": 0.4, "tipFloorP50Lamports": 20_000 }),
    }];
    let (tx, mut rx) = mpsc::channel(64);
    // Advance past attempt-1's give-up watermark (1_000) → autonomous retry.
    tx.send(slot_msg(2_000, SlotStatus::Confirmed, ts(5)))
        .await
        .unwrap();
    // The recovered attempt-2 lands in a DIFFERENT slot.
    tx.send(tx_msg("b0-s2", 3_000, ts(6))).await.unwrap();
    tx.send(slot_msg(3_000, SlotStatus::Confirmed, ts(7)))
        .await
        .unwrap();
    tx.send(slot_msg(3_000, SlotStatus::Finalized, ts(19)))
        .await
        .unwrap();
    drop(tx);

    let sink = CapturingSink::default();
    let receipts = run_submit(&sink, &FakeDecider, &FakeSubmitter, bases, &mut rx, cfg(10)).await;

    // The receipt reports the RECOVERED landing — attempt 2's slot, not attempt 1's failure.
    assert_eq!(
        receipts,
        vec![Receipt::Landed {
            slot: 3_000,
            final_stage: "finalized".into(),
            attempts: 2,
        }]
    );

    // And the failed first attempt is classified from real signals as expired_blockhash in the log.
    let log = export(&sink.snapshot());
    let a1 = log.iter().find(|e| e.bundle_id == "b0#a1").unwrap();
    assert_eq!(a1.failure_class.as_deref(), Some("expired_blockhash"));
    assert!(!a1.landed());
}

#[tokio::test]
async fn abandoned_returns_failed_without_paying_a_second_tip() {
    // A bundle that never lands and never hits a watermark: at the deadline it is recorded as Failed,
    // and the submitter is invoked exactly once (no untracked, tip-paying resubmit).
    let bases = vec![BaseBundle {
        base_id: "b".into(),
        injected: None,
        tip_context: json!({}),
    }];
    let (tx, mut rx) = mpsc::channel(8);
    drop(tx); // no stream events → never confirmed

    let sub = CountingSubmitter::default();
    let sink = CapturingSink::default();
    let cfg = SagaConfig {
        max_attempts: 3,
        global_deadline: Instant::now() + Duration::from_millis(200),
    };
    let receipts = run_submit(&sink, &FakeDecider, &sub, bases, &mut rx, cfg).await;

    assert_eq!(
        sub.n.load(Ordering::SeqCst),
        1,
        "only attempt 1 is submitted"
    );
    match &receipts[..] {
        [Receipt::Failed {
            last_class,
            attempts,
            ..
        }] => {
            assert_eq!(last_class.as_deref(), Some("confirmation_timeout"));
            assert_eq!(*attempts, 1);
        }
        other => panic!("expected one Failed receipt, got {other:?}"),
    }
}

#[tokio::test]
async fn receipt_reflects_real_probe_class_on_recovery() {
    // ProbeSubmitter reports a REAL expiry (block height past lastValidBlockHeight) regardless of the
    // injection tag; the bundle recovers on attempt 2.
    let bases = vec![BaseBundle {
        base_id: "px".into(),
        injected: None,
        tip_context: json!({}),
    }];
    let (tx, mut rx) = mpsc::channel(64);
    tx.send(slot_msg(2_000, SlotStatus::Confirmed, ts(5)))
        .await
        .unwrap();
    tx.send(tx_msg("px-s2", 3_000, ts(6))).await.unwrap();
    tx.send(slot_msg(3_000, SlotStatus::Confirmed, ts(7)))
        .await
        .unwrap();
    tx.send(slot_msg(3_000, SlotStatus::Finalized, ts(19)))
        .await
        .unwrap();
    drop(tx);

    let sink = CapturingSink::default();
    let receipts = run_submit(
        &sink,
        &FakeDecider,
        &ProbeSubmitter,
        bases,
        &mut rx,
        cfg(10),
    )
    .await;

    assert_eq!(
        receipts,
        vec![Receipt::Landed {
            slot: 3_000,
            final_stage: "finalized".into(),
            attempts: 2,
        }]
    );
    // The recovery was driven by the real probe class, not the (absent) injection tag.
    let log = export(&sink.snapshot());
    let a1 = log.iter().find(|e| e.bundle_id == "px#a1").unwrap();
    assert_eq!(a1.failure_class.as_deref(), Some("expired_blockhash"));
}
