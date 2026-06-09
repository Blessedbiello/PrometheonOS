//! The engine orchestration loop.
//!
//! Phase 11c wires the **read-only spine**: stream Yellowstone slots → maintain the
//! [`NetworkHealthModel`] → publish telemetry (`telemetry.slot`, `telemetry.health`) over NATS.
//! It runs against any configured stream — including the free SolInfra mainnet feed — without a
//! wallet or any on-chain writes, so it's the cheapest way to validate the whole pipeline live.
//!
//! Bundle submission, stream-confirmed lifecycle, failure classification, retry, and the AI
//! decision request/reply land on top of this loop in 11d.

use std::time::Duration;

use chrono::Utc;
use prometheon_bundle::{BlockEngineClient, BlockEngineConfig, Percentile};
use prometheon_ingest::{
    yellowstone::{self, IngestMessage, SubscriptionSpec, YellowstoneConfig},
    SlotObservation,
};
use prometheon_netmodel::NetworkHealthModel;
use prometheon_telemetry::{TelemetryBus, TelemetryEvent};

use crate::config::Config;

const LATENCY_WINDOW: usize = 256;
const HEALTH_INTERVAL: Duration = Duration::from_secs(2);
const TIP_FLOOR_INTERVAL: Duration = Duration::from_secs(30);

/// Apply a slot observation to the health model's slot counters.
///
/// Only the *first sighting* of a scheduled slot affects produced/skipped counts: `Progressed`
/// records its leader-skipped predecessors as skips plus the produced slot, and `Dead` (a forked-out
/// bank) counts as a skip. `StatusAdvanced`/`Gap`/`Redundant` are not new scheduled slots, so they
/// leave the counters untouched — double-counting them would corrupt the skip-rate / stability
/// metrics that feed congestion.
pub fn apply_slot_observation(model: &mut NetworkHealthModel, obs: &SlotObservation) {
    match obs {
        SlotObservation::Progressed { skipped, .. } => {
            for _ in skipped {
                model.record_slot(false);
            }
            model.record_slot(true);
        }
        SlotObservation::Dead { .. } => model.record_slot(false),
        SlotObservation::StatusAdvanced { .. }
        | SlotObservation::Gap { .. }
        | SlotObservation::Redundant { .. } => {}
    }
}

/// Build the Jito client used for the tip-floor congestion feed (best-effort; `None` if it fails).
fn tip_floor_client(config: &Config) -> Option<BlockEngineClient> {
    BlockEngineClient::new(BlockEngineConfig {
        base_url: config.jito_block_engine_url.clone(),
        tip_floor_url: config.jito_tip_floor_url.clone(),
        auth_uuid: config.jito_auth_uuid.clone(),
        ..Default::default()
    })
    .ok()
}

/// Run the read-only telemetry pipeline until the ingest stream closes.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let endpoint = config
        .yellowstone_endpoint
        .clone()
        .ok_or_else(|| anyhow::anyhow!("YELLOWSTONE_ENDPOINT not set — fill .env (SolInfra)"))?;

    tracing::info!(
        network = config.network.as_str(),
        nats = %config.nats_url,
        "starting read-only telemetry pipeline"
    );

    let bus = TelemetryBus::connect(&config.nats_url).await?;

    let yc = YellowstoneConfig {
        endpoint,
        x_token: config.yellowstone_x_token.clone(),
        commitment: config.yellowstone_commitment,
        channel_capacity: config.ingest_channel_capacity,
        ..Default::default()
    };
    let spec = SubscriptionSpec {
        track_slots: true,
        ..Default::default()
    };
    let mut handle = yellowstone::spawn(yc, spec);

    let block_engine = tip_floor_client(&config);
    let mut model = NetworkHealthModel::new(LATENCY_WINDOW);
    let mut health_tick = tokio::time::interval(HEALTH_INTERVAL);
    let mut tipfloor_tick = tokio::time::interval(TIP_FLOOR_INTERVAL);
    let mut slots_seen: u64 = 0;

    loop {
        tokio::select! {
            maybe = handle.rx.recv() => match maybe {
                Some(msg) => {
                    if matches!(msg, IngestMessage::Slot { .. }) {
                        slots_seen += 1;
                    }
                    handle_message(&bus, &mut model, msg).await;
                }
                None => {
                    tracing::warn!("ingest channel closed; stopping pipeline");
                    break;
                }
            },
            _ = health_tick.tick() => {
                let snap = model.snapshot(Utc::now());
                tracing::debug!(
                    slots = slots_seen,
                    stability = snap.slot_stability_score,
                    congestion = snap.congestion_score,
                    "health snapshot"
                );
                if let Err(e) = bus.publish(&TelemetryEvent::Health(snap)).await {
                    tracing::warn!(error = %e, "health publish failed");
                }
            },
            _ = tipfloor_tick.tick() => {
                if let Some(client) = &block_engine {
                    match client.get_tip_floor().await {
                        Ok(floor) => {
                            model.set_tip_floor_lamports(floor.percentile_lamports(Percentile::P50));
                        }
                        Err(e) => tracing::debug!(error = %e, "tip-floor refresh failed"),
                    }
                }
            },
        }
    }

    Ok(())
}

async fn handle_message(bus: &TelemetryBus, model: &mut NetworkHealthModel, msg: IngestMessage) {
    match msg {
        IngestMessage::Slot {
            update,
            observation,
        } => {
            apply_slot_observation(model, &observation);
            if let Err(e) = bus.publish(&TelemetryEvent::Slot(update)).await {
                tracing::debug!(error = %e, "slot publish failed");
            }
        }
        IngestMessage::Transaction(_tx) => {
            // Lifecycle wiring (stream-confirmed landing) lands in 11d.
        }
        IngestMessage::StreamConnected { from_slot } => {
            tracing::info!(?from_slot, "yellowstone stream connected");
        }
        IngestMessage::StreamError { error } => {
            tracing::warn!(%error, "yellowstone stream error; supervisor will reconnect");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap_stability(model: &NetworkHealthModel) -> f64 {
        model.snapshot(Utc::now()).slot_stability_score
    }

    #[test]
    fn progressed_counts_skips_then_the_produced_slot() {
        let mut model = NetworkHealthModel::new(16);
        // 8 produced, 2 skipped → skip rate 0.2 → stability 0.8.
        apply_slot_observation(
            &mut model,
            &SlotObservation::Progressed {
                slot: 100,
                skipped: vec![98, 99],
            },
        );
        for s in 101..=107 {
            apply_slot_observation(
                &mut model,
                &SlotObservation::Progressed {
                    slot: s,
                    skipped: vec![],
                },
            );
        }
        assert!((snap_stability(&model) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn status_advance_and_redundant_do_not_double_count() {
        use prometheon_types::SlotStatus;
        let mut model = NetworkHealthModel::new(16);
        apply_slot_observation(
            &mut model,
            &SlotObservation::Progressed {
                slot: 100,
                skipped: vec![],
            },
        );
        // Several more status updates for the SAME slot must not change counts.
        apply_slot_observation(
            &mut model,
            &SlotObservation::StatusAdvanced {
                slot: 100,
                from: SlotStatus::Processed,
                to: SlotStatus::Confirmed,
            },
        );
        apply_slot_observation(
            &mut model,
            &SlotObservation::Redundant {
                slot: 100,
                status: SlotStatus::Confirmed,
            },
        );
        // 1 produced, 0 skipped → perfect stability.
        assert!((snap_stability(&model) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dead_slot_counts_as_a_skip() {
        let mut model = NetworkHealthModel::new(16);
        apply_slot_observation(
            &mut model,
            &SlotObservation::Progressed {
                slot: 100,
                skipped: vec![],
            },
        );
        apply_slot_observation(&mut model, &SlotObservation::Dead { slot: 101 });
        // 1 produced, 1 skipped → stability 0.5.
        assert!((snap_stability(&model) - 0.5).abs() < 1e-9);
    }
}
