//! The engine orchestration loop.
//!
//! Wires the **read-only spine**: stream Yellowstone slots → maintain the [`NetworkHealthModel`] →
//! fan each telemetry event out to the sinks — NATS (always), Postgres/TimescaleDB (when
//! configured) — while a Prometheus `/metrics` exporter serves the live health + counters. It runs
//! against any configured stream — including the free SolInfra mainnet feed — without a wallet or
//! any on-chain writes, so it's the cheapest way to validate the whole pipeline live.
//!
//! The live on-chain submit + stream-confirmed lifecycle correlation layers on top of this loop in
//! the Phase 8 proof run.

use std::net::SocketAddr;
use std::time::Duration;

use chrono::Utc;
use prometheon_bundle::{BlockEngineClient, BlockEngineConfig, Percentile};
use prometheon_ingest::{
    yellowstone::{self, IngestMessage, SubscriptionSpec, YellowstoneConfig},
    SlotObservation,
};
use prometheon_netmodel::NetworkHealthModel;
use prometheon_telemetry::{PostgresSink, TelemetryBus, TelemetryEvent};

use crate::config::Config;
use crate::metrics::{self, EngineCounters, MetricsState};

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

/// The sinks a telemetry event fans out to: NATS (always) and Postgres (when configured). One
/// `emit` call site keeps every event both published and persisted, and bumps the event counter.
struct Sinks {
    bus: TelemetryBus,
    pg: Option<PostgresSink>,
}

impl Sinks {
    /// Publish to NATS and persist to Postgres — both best-effort: telemetry must never block or
    /// crash the engine, so failures are logged and swallowed.
    async fn emit(&self, event: &TelemetryEvent, counters: &mut EngineCounters) {
        counters.telemetry_events_total += 1;
        if let Err(e) = self.bus.publish(event).await {
            tracing::debug!(error = %e, subject = event.subject(), "nats publish failed");
        }
        if let Some(pg) = &self.pg {
            if let Err(e) = pg.record(event).await {
                tracing::debug!(error = %e, "postgres record failed");
            }
        }
    }
}

/// Run the telemetry pipeline until the ingest stream closes.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let endpoint = config
        .yellowstone_endpoint
        .clone()
        .ok_or_else(|| anyhow::anyhow!("YELLOWSTONE_ENDPOINT not set — fill .env (SolInfra)"))?;

    tracing::info!(
        network = config.network.as_str(),
        nats = %config.nats_url,
        db = config.db_enabled(),
        metrics = %config.prometheus_metrics_addr,
        "starting telemetry pipeline"
    );

    let bus = TelemetryBus::connect(&config.nats_url).await?;

    // Optional Postgres sink — the engine runs fine without it (NATS + Prometheus still work).
    let pg = if config.db_enabled() {
        match PostgresSink::connect(&config.database_url).await {
            Ok(sink) => {
                tracing::info!("postgres sink connected");
                Some(sink)
            }
            Err(e) => {
                tracing::warn!(error = %e, "postgres sink unavailable; continuing without persistence");
                None
            }
        }
    } else {
        None
    };
    let sinks = Sinks { bus, pg };

    // Prometheus /metrics exporter (best-effort; engine continues if the address is unusable).
    let metrics = MetricsState::default();
    match config.prometheus_metrics_addr.parse::<SocketAddr>() {
        Ok(addr) => {
            let m = metrics.clone();
            tokio::spawn(async move {
                if let Err(e) = metrics::serve(addr, m).await {
                    tracing::warn!(error = %e, "metrics exporter stopped");
                }
            });
        }
        Err(_) => tracing::warn!(
            addr = %config.prometheus_metrics_addr,
            "invalid PROMETHEUS_METRICS_ADDR; exporter disabled"
        ),
    }

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
    let mut counters = EngineCounters::default();
    let mut health_tick = tokio::time::interval(HEALTH_INTERVAL);
    let mut tipfloor_tick = tokio::time::interval(TIP_FLOOR_INTERVAL);

    loop {
        tokio::select! {
            maybe = handle.rx.recv() => match maybe {
                Some(msg) => handle_message(&sinks, &mut model, &mut counters, msg).await,
                None => {
                    tracing::warn!("ingest channel closed; stopping pipeline");
                    break;
                }
            },
            _ = health_tick.tick() => {
                let snap = model.snapshot(Utc::now());
                tracing::debug!(
                    slots = counters.slots_total,
                    stability = snap.slot_stability_score,
                    congestion = snap.congestion_score,
                    "health snapshot"
                );
                metrics.set_health(snap.clone()).await;
                sinks.emit(&TelemetryEvent::Health(snap), &mut counters).await;
                counters.stream_reconnects_total = handle.counters.snapshot().2;
                metrics.set_counters(counters.clone()).await;
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

async fn handle_message(
    sinks: &Sinks,
    model: &mut NetworkHealthModel,
    counters: &mut EngineCounters,
    msg: IngestMessage,
) {
    match msg {
        IngestMessage::Slot {
            update,
            observation,
        } => {
            apply_slot_observation(model, &observation);
            counters.slots_total += 1;
            sinks.emit(&TelemetryEvent::Slot(update), counters).await;
        }
        IngestMessage::Transaction(_tx) => {
            // Stream-confirmed lifecycle correlation lands with the live submit driver (Phase 8).
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
