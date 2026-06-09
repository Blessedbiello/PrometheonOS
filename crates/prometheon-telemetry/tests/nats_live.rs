//! Live NATS round-trip — env-gated so the default `cargo test` (and CI without a broker) skips it.
//!
//! Run against the local docker-compose NATS:
//! ```text
//! NATS_TEST_URL=nats://localhost:4222 cargo test -p prometheon-telemetry --test nats_live
//! ```

use std::time::Duration;

use futures::StreamExt;
use prometheon_telemetry::{TelemetryBus, TelemetryEvent};
use prometheon_types::{SlotStatus, SlotUpdate};

fn test_url() -> Option<String> {
    std::env::var("NATS_TEST_URL")
        .ok()
        .filter(|v| !v.is_empty())
}

#[tokio::test]
async fn publish_then_receive_roundtrips_a_telemetry_event() {
    let Some(url) = test_url() else {
        eprintln!("skipping: NATS_TEST_URL not set");
        return;
    };

    let bus = TelemetryBus::connect(&url).await.expect("connect");
    let mut sub = bus
        .client()
        .subscribe("telemetry.slot")
        .await
        .expect("subscribe");

    let event = TelemetryEvent::Slot(SlotUpdate::new(
        425_350_212,
        Some(425_350_211),
        SlotStatus::Confirmed,
        chrono::Utc::now(),
    ));
    bus.publish(&event).await.expect("publish");
    bus.flush().await.expect("flush");

    let msg = tokio::time::timeout(Duration::from_secs(3), sub.next())
        .await
        .expect("timed out waiting for message")
        .expect("subscription closed");
    let decoded: TelemetryEvent = serde_json::from_slice(&msg.payload).expect("decode");
    assert_eq!(decoded, event);
}
