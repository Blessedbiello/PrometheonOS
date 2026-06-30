//! Shared test doubles for the saga / submit integration tests — fake `DecisionSource`, `Submitter`,
//! and `EventSink` impls plus stream-scripting helpers, so the AI-in-the-loop saga and the
//! `submit → Receipt` surface are both exercised with NO network.

#![allow(dead_code)] // not every test binary uses every double

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use prometheon_core::proof_run::{signals_from_observation, FailureObservation, SubmittedBundle};
use prometheon_core::saga::{AttemptSpec, DecisionSource, Submitter};
use prometheon_core::sinks::EventSink;
use prometheon_faultinject::FaultScenario;
use prometheon_ingest::{IngestMessage, SlotObservation, TxStatus};
use prometheon_telemetry::export::{build_log, LifecycleLogEntry};
use prometheon_telemetry::{Decision, DecisionType, TelemetryEvent};
use prometheon_types::{SlotStatus, SlotUpdate};

pub fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
}

pub fn slot_msg(slot: u64, status: SlotStatus, at: DateTime<Utc>) -> IngestMessage {
    IngestMessage::Slot {
        update: SlotUpdate::new(slot, Some(slot.saturating_sub(1)), status, at),
        observation: SlotObservation::Progressed {
            slot,
            skipped: vec![],
        },
    }
}

pub fn tx_msg(signature: &str, slot: u64, at: DateTime<Utc>) -> IngestMessage {
    IngestMessage::Transaction(TxStatus {
        signature: signature.to_string(),
        slot,
        failed: false,
        ts: at,
    })
}

/// A sink that records every emitted event for later assertions.
#[derive(Default)]
pub struct CapturingSink {
    pub events: Mutex<Vec<TelemetryEvent>>,
}

impl CapturingSink {
    pub fn snapshot(&self) -> Vec<TelemetryEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl EventSink for CapturingSink {
    async fn emit(&self, event: &TelemetryEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

/// Always answers: a tip decision (tip 25k) and a retry decision (refresh + re-price to 30k).
pub struct FakeDecider;
impl DecisionSource for FakeDecider {
    async fn decide(&self, dtype: DecisionType, context: Value) -> Option<Decision> {
        let (action, reasoning, after) = match dtype {
            DecisionType::Tip => (
                "tip 25000 lamports",
                "floor + congestion → target the competitive P75–P95 band (≥ floor)",
                json!({ "tip": 25_000 }),
            ),
            DecisionType::Retry => (
                "refresh blockhash, re-price 25000->30000",
                "blockhash expired (height past lastValidBlockHeight); refresh and bump tip as congestion rose",
                json!({ "refresh_blockhash": true, "tip": 30_000 }),
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

/// Deterministic submitter: bundle_id `{base}#a{n}`, signature `{base}-s{n}`. Injected attempt 1 gets
/// a sub-floor tip (low-tip) or a give-up watermark (stale-blockhash). Retries (attempt ≥2) carry no
/// injection and no watermark, so they land via the scripted stream.
pub struct FakeSubmitter;
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
            deadline_slot: spec.injected.map(|_| 1_000),
        })
    }
}

/// Counts how many times the submitter is invoked (no watermark → relies on the global deadline).
pub struct CountingSubmitter {
    pub n: AtomicUsize,
}
impl Default for CountingSubmitter {
    fn default() -> Self {
        Self {
            n: AtomicUsize::new(0),
        }
    }
}
impl Submitter for CountingSubmitter {
    async fn submit(&self, spec: &AttemptSpec) -> anyhow::Result<SubmittedBundle> {
        self.n.fetch_add(1, Ordering::SeqCst);
        Ok(SubmittedBundle {
            bundle_id: format!("{}#a{}", spec.base_id, spec.attempt_no),
            signature: format!("{}-s{}", spec.base_id, spec.attempt_no),
            tip_lamports: spec.tip_lamports.unwrap_or(14_500),
            tip_account: "Tip".into(),
            region: "ny".into(),
            submitted_at: ts(0),
            tip_floor_p50_lamports: 20_000,
            injected: spec.injected,
            base_id: spec.base_id.clone(),
            attempt_no: spec.attempt_no,
            deadline_slot: None,
        })
    }
}

/// A submitter whose `probe_failure` returns REAL expiry signals (block height past
/// lastValidBlockHeight) regardless of the injection tag — modelling the live RPC/Jito probe.
pub struct ProbeSubmitter;
impl Submitter for ProbeSubmitter {
    async fn submit(&self, spec: &AttemptSpec) -> anyhow::Result<SubmittedBundle> {
        Ok(SubmittedBundle {
            bundle_id: format!("{}#a{}", spec.base_id, spec.attempt_no),
            signature: format!("{}-s{}", spec.base_id, spec.attempt_no),
            tip_lamports: 25_000, // competitive tip (≥ floor) → only the probe's expiry signal is the cause
            tip_account: "Tip".into(),
            region: "ny".into(),
            submitted_at: ts(spec.attempt_no as i64),
            tip_floor_p50_lamports: 20_000,
            injected: None, // NOT tagged as expiry — only the probe knows it expired
            base_id: spec.base_id.clone(),
            attempt_no: spec.attempt_no,
            deadline_slot: if spec.attempt_no == 1 {
                Some(1_000)
            } else {
                None
            },
        })
    }
    async fn probe_failure(
        &self,
        _sb: &SubmittedBundle,
    ) -> Option<prometheon_failure::FailureSignals> {
        Some(signals_from_observation(&FailureObservation {
            tip_lamports: 25_000,
            tip_floor_p50_lamports: 20_000,
            blockhash_valid: Some(false),
            block_height: Some(1_000),
            last_valid_block_height: Some(900),
            ..Default::default()
        }))
    }
}

/// Bucket captured events by `kind` and run the canonical assembler — the export path.
pub fn export(events: &[TelemetryEvent]) -> Vec<LifecycleLogEntry> {
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
