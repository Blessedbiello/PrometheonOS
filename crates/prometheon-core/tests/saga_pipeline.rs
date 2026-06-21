//! End-to-end regression for the AI-in-the-loop saga (the bounty's headline: "Autonomous Retry with
//! Fault Injection"). Drives `run_saga` over fake `DecisionSource` + `Submitter` with a scripted
//! stream, then feeds the captured telemetry through the same `build_log` the export uses. Proves —
//! with NO network — that the agent makes a tip decision per bundle, that an injected blockhash
//! expiry is detected, reasoned about (a retry `Decision` with real reasoning), refreshed + re-priced
//! and **resubmitted to a landing**, and that both the failed attempt and the recovered one appear in
//! the lifecycle log.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::Instant;

use prometheon_core::proof_run::SubmittedBundle;
use prometheon_core::saga::{
    run_saga, AttemptSpec, BaseBundle, DecisionSource, SagaConfig, Submitter,
};
use prometheon_core::sinks::EventSink;
use prometheon_faultinject::FaultScenario;
use prometheon_ingest::{IngestMessage, SlotObservation, TxStatus};
use prometheon_telemetry::export::build_log;
use prometheon_telemetry::{Decision, DecisionType, TelemetryEvent};
use prometheon_types::{SlotStatus, SlotUpdate};

fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
}

fn slot_msg(slot: u64, status: SlotStatus, at: DateTime<Utc>) -> IngestMessage {
    IngestMessage::Slot {
        update: SlotUpdate::new(slot, Some(slot.saturating_sub(1)), status, at),
        observation: SlotObservation::Progressed {
            slot,
            skipped: vec![],
        },
    }
}

fn tx_msg(signature: &str, slot: u64, at: DateTime<Utc>) -> IngestMessage {
    IngestMessage::Transaction(TxStatus {
        signature: signature.to_string(),
        slot,
        failed: false,
        ts: at,
    })
}

// ── Fakes ────────────────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct CapturingSink {
    events: Mutex<Vec<TelemetryEvent>>,
}
impl EventSink for CapturingSink {
    async fn emit(&self, event: &TelemetryEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

/// Always answers: a tip decision (tip 15k) and a retry decision (refresh + re-price to 25k).
struct FakeDecider;
impl DecisionSource for FakeDecider {
    async fn decide(&self, dtype: DecisionType, context: Value) -> Option<Decision> {
        let (action, reasoning, after) = match dtype {
            DecisionType::Tip => (
                "tip 15000 lamports",
                "floor + congestion → target ~p50",
                json!({ "tip": 15_000 }),
            ),
            DecisionType::Retry => (
                "refresh blockhash, re-price 15000->25000",
                "blockhash expired (height past lastValidBlockHeight); refresh and bump tip as congestion rose",
                json!({ "refresh_blockhash": true, "tip": 25_000 }),
            ),
            DecisionType::Timing => ("submit now", "leader window open", json!({})),
        };
        Some(Decision {
            decision_type: dtype,
            action: action.into(),
            reasoning: reasoning.into(),
            confidence: 0.85,
            inputs_considered: context,
            before: None,
            after: Some(after),
            provider: "anthropic".into(),
            latency_ms: 700,
            ts: ts(0),
        })
    }
}

/// Deterministic submitter: bundle_id `{base}#a{n}`, signature `{base}-s{n}`. Injected attempts get a
/// sub-floor tip (low-tip) or a give-up watermark (stale-blockhash). Retries (attempt ≥2) carry no
/// injection and no watermark, so they land via the scripted stream.
struct FakeSubmitter;
impl Submitter for FakeSubmitter {
    async fn submit(&self, spec: &AttemptSpec) -> anyhow::Result<SubmittedBundle> {
        let tip = match spec.injected {
            Some(FaultScenario::LowTip { tip_lamports }) => tip_lamports,
            _ => spec
                .tip_lamports
                .unwrap_or(if spec.attempt_no == 1 { 14_500 } else { 25_000 }),
        };
        Ok(SubmittedBundle {
            bundle_id: format!("{}#a{}", spec.base_id, spec.attempt_no),
            signature: format!("{}-s{}", spec.base_id, spec.attempt_no),
            tip_lamports: tip,
            tip_account: "Tip1111111111111111111111111111111111111111".into(),
            region: "ny".into(),
            submitted_at: ts(spec.attempt_no as i64),
            tip_floor_p50_lamports: 20_000,
            injected: spec.injected,
            base_id: spec.base_id.clone(),
            attempt_no: spec.attempt_no,
            // Injected attempt 1 has an already-passed give-up watermark (born expired / under-tipped).
            deadline_slot: spec.injected.map(|_| 1_000),
        })
    }
}

fn export(events: &[TelemetryEvent]) -> Vec<prometheon_telemetry::export::LifecycleLogEntry> {
    let (mut b, mut l, mut f) = (vec![], vec![], vec![]);
    for ev in events {
        let v: Value = serde_json::to_value(ev).unwrap();
        match v["kind"].as_str() {
            Some("bundle") => b.push(v),
            Some("lifecycle") => l.push(v),
            Some("failure") => f.push(v),
            _ => {}
        }
    }
    build_log(&b, &l, &f)
}

fn decisions(events: &[TelemetryEvent]) -> Vec<Decision> {
    events
        .iter()
        .filter_map(|e| match e {
            TelemetryEvent::Decision(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn saga_runs_ai_tip_decision_and_autonomous_retry_to_landing() {
    // 10 normal bundles + 1 low-tip + 1 stale-blockhash. The two injected ones fail attempt 1 and the
    // AI recovers them on attempt 2.
    let mut bases = Vec::new();
    let ctx = json!({ "congestionScore": 0.4, "tipFloorP50Lamports": 20_000, "slotStabilityScore": 0.95 });
    for i in 0..10u64 {
        bases.push(BaseBundle {
            base_id: format!("bundle-{i}"),
            injected: None,
            tip_context: ctx.clone(),
        });
    }
    bases.push(BaseBundle {
        base_id: "bundle-10".into(),
        injected: Some(FaultScenario::LowTip {
            tip_lamports: 1_000,
        }),
        tip_context: ctx.clone(),
    });
    bases.push(BaseBundle {
        base_id: "bundle-11".into(),
        injected: Some(FaultScenario::BlockhashExpiry),
        tip_context: ctx.clone(),
    });

    // Script the stream.
    let (tx, mut rx) = mpsc::channel(2048);
    for i in 0..10u64 {
        let slot = 100 + i;
        tx.send(tx_msg(&format!("bundle-{i}-s1"), slot, ts(1)))
            .await
            .unwrap();
        tx.send(slot_msg(slot, SlotStatus::Confirmed, ts(2)))
            .await
            .unwrap();
        tx.send(slot_msg(slot, SlotStatus::Finalized, ts(14)))
            .await
            .unwrap();
    }
    // Advance the chain past the injected attempts' give-up watermark → triggers autonomous retry.
    tx.send(slot_msg(2_000, SlotStatus::Confirmed, ts(20)))
        .await
        .unwrap();
    // The recovered attempt-2 submissions land.
    for (base, slot) in [("bundle-10", 3_000u64), ("bundle-11", 3_001u64)] {
        tx.send(tx_msg(&format!("{base}-s2"), slot, ts(21)))
            .await
            .unwrap();
        tx.send(slot_msg(slot, SlotStatus::Confirmed, ts(22)))
            .await
            .unwrap();
        tx.send(slot_msg(slot, SlotStatus::Finalized, ts(34)))
            .await
            .unwrap();
    }
    drop(tx);

    let sink = CapturingSink::default();
    let cfg = SagaConfig {
        max_attempts: 3,
        global_deadline: Instant::now() + Duration::from_secs(10),
    };
    let summary = run_saga(&sink, &FakeDecider, &FakeSubmitter, bases, &mut rx, cfg).await;

    // 12 attempt-1s + 2 recovery attempt-2s = 14 submissions; 12 landed; 2 failed attempts.
    assert_eq!(summary.total, 14, "submissions (attempts)");
    assert_eq!(summary.landed, 12, "landed attempts");
    assert_eq!(summary.failed, 2, "failed attempts (the injected ones)");

    let events = sink.events.lock().unwrap();
    let ds = decisions(&events);
    let tips = ds
        .iter()
        .filter(|d| d.decision_type == DecisionType::Tip)
        .count();
    let retries: Vec<&Decision> = ds
        .iter()
        .filter(|d| d.decision_type == DecisionType::Retry)
        .collect();
    assert_eq!(tips, 12, "one AI tip decision per bundle");
    assert_eq!(
        retries.len(),
        2,
        "one AI retry decision per recovered failure"
    );
    assert!(
        retries.iter().all(|d| !d.reasoning.is_empty()),
        "retry decisions must carry visible reasoning"
    );

    let log = export(&events);
    assert_eq!(log.len(), 14, "every attempt appears in the lifecycle log");
    let failed = log.iter().filter(|e| e.failure_class.is_some()).count();
    assert!(
        failed >= 2,
        "≥2 classified failures in the log, got {failed}"
    );

    // The stale-blockhash bundle: attempt 1 expired, attempt 2 finalized (AI recovery).
    let a1 = log.iter().find(|e| e.bundle_id == "bundle-11#a1").unwrap();
    assert_eq!(a1.failure_class.as_deref(), Some("expired_blockhash"));
    assert!(!a1.landed());
    let a2 = log.iter().find(|e| e.bundle_id == "bundle-11#a2").unwrap();
    assert_eq!(a2.final_stage.as_deref(), Some("finalized"));
    assert!(a2.landed());

    // The low-tip bundle: attempt 1 fee-too-low, attempt 2 landed.
    let lt1 = log.iter().find(|e| e.bundle_id == "bundle-10#a1").unwrap();
    assert_eq!(lt1.failure_class.as_deref(), Some("fee_too_low"));
    let lt2 = log.iter().find(|e| e.bundle_id == "bundle-10#a2").unwrap();
    assert!(lt2.landed());
}
