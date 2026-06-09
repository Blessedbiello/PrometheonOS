//! Live Postgres sink — env-gated so the default `cargo test`/CI skips it.
//!
//! ```text
//! DATABASE_TEST_URL=postgres://prometheon:prometheon@localhost:55432/prometheon \
//!   cargo test -p prometheon-telemetry --test postgres_live
//! ```

use chrono::Utc;
use prometheon_telemetry::{
    decision::{Decision, DecisionType},
    PostgresSink, TelemetryEvent,
};
use prometheon_types::{SlotStatus, SlotUpdate};

fn test_url() -> Option<String> {
    std::env::var("DATABASE_TEST_URL")
        .ok()
        .filter(|v| !v.is_empty())
}

#[tokio::test]
async fn records_events_and_projects_them_through_views() {
    let Some(url) = test_url() else {
        eprintln!("skipping: DATABASE_TEST_URL not set");
        return;
    };

    let sink = PostgresSink::connect(&url)
        .await
        .expect("connect + migrate");

    // A unique marker so the assertions are independent of any prior rows.
    let marker = format!(
        "itest-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );

    sink.record(&TelemetryEvent::Slot(SlotUpdate::new(
        425_350_212,
        Some(425_350_211),
        SlotStatus::Confirmed,
        Utc::now(),
    )))
    .await
    .expect("record slot");

    sink.record(&TelemetryEvent::Decision(Decision {
        decision_type: DecisionType::Tip,
        action: marker.clone(),
        reasoning: "integration test".into(),
        confidence: 0.81,
        inputs_considered: serde_json::json!({}),
        before: None,
        after: Some(serde_json::json!({ "tip": 14500 })),
        provider: "mock".into(),
        latency_ms: 5,
        ts: Utc::now(),
    }))
    .await
    .expect("record decision");

    // The decision is visible through the typed view with its fields projected from jsonb.
    let row: (String, f64, String) =
        sqlx::query_as("SELECT action, confidence, provider FROM v_decision WHERE action = $1")
            .bind(&marker)
            .fetch_one(sink.pool())
            .await
            .expect("query v_decision");
    assert_eq!(row.0, marker);
    assert!((row.1 - 0.81).abs() < 1e-9);
    assert_eq!(row.2, "mock");

    // And the raw hypertable holds the slot event.
    let (slots,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM telemetry_event WHERE kind = 'slot'")
            .fetch_one(sink.pool())
            .await
            .expect("count slots");
    assert!(slots >= 1, "expected at least one slot row");
}
