//! Postgres / TimescaleDB sink.
//!
//! Every [`TelemetryEvent`] is persisted to one `telemetry_event` hypertable as `(kind, subject,
//! payload jsonb)`. Storing the canonical JSON keeps the sink a single insert path (no per-type
//! schema churn) while TimescaleDB gives time-partitioned retention; typed SQL **views** project
//! the jsonb back into decisions / bundles / lifecycle / failures for the lifecycle-log export and
//! ad-hoc queries.
//!
//! The event→row mapping ([`event_row`]) is pure and unit-tested; the connection + insert is thin
//! I/O exercised by an env-gated integration test against the docker Postgres.

use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::events::TelemetryEvent;

/// Schema: the durable hypertable + projection views. Idempotent (safe to run on every startup).
const MIGRATION: &str = r#"
CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE TABLE IF NOT EXISTS telemetry_event (
    id          BIGSERIAL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind        TEXT        NOT NULL,
    subject     TEXT        NOT NULL,
    payload     JSONB       NOT NULL
);
SELECT create_hypertable('telemetry_event', 'recorded_at', if_not_exists => TRUE, migrate_data => TRUE);
CREATE INDEX IF NOT EXISTS telemetry_event_kind_idx ON telemetry_event (kind, recorded_at DESC);

CREATE OR REPLACE VIEW v_decision AS
  SELECT recorded_at,
         payload->>'decision_type'        AS decision_type,
         payload->>'action'               AS action,
         payload->>'reasoning'            AS reasoning,
         (payload->>'confidence')::float8 AS confidence,
         payload->>'provider'             AS provider,
         (payload->>'latency_ms')::bigint AS latency_ms,
         payload->>'ts'                   AS event_ts
  FROM telemetry_event WHERE kind = 'decision';

CREATE OR REPLACE VIEW v_bundle AS
  SELECT recorded_at,
         payload->>'bundle_id'              AS bundle_id,
         (payload->>'tip_lamports')::bigint AS tip_lamports,
         payload->>'tip_account'            AS tip_account,
         payload->>'region'                 AS region,
         payload->>'phase'                  AS phase,
         payload->'signatures'              AS signatures,
         payload->>'ts'                     AS event_ts
  FROM telemetry_event WHERE kind = 'bundle';

CREATE OR REPLACE VIEW v_lifecycle AS
  SELECT recorded_at,
         payload->>'id'                                  AS id,
         payload->'event'->>'stage'                      AS stage,
         (payload->'event'->>'slot')::bigint             AS slot,
         payload->'event'->>'ts'                         AS event_ts,
         (payload->'event'->>'delta_ms_from_prev')::bigint AS delta_ms_from_prev
  FROM telemetry_event WHERE kind = 'lifecycle';

CREATE OR REPLACE VIEW v_failure AS
  SELECT recorded_at,
         payload->>'id'                              AS id,
         payload->'classification'->>'class'         AS class,
         (payload->'classification'->>'confidence')::float8 AS confidence
  FROM telemetry_event WHERE kind = 'failure';
"#;

/// A connection pool + the persistence logic.
#[derive(Clone)]
pub struct PostgresSink {
    pool: PgPool,
}

impl PostgresSink {
    /// Connect, pool, and run the (idempotent) migration.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(|e| anyhow::anyhow!("postgres connect: {e}"))?;
        let sink = Self { pool };
        sink.migrate().await?;
        Ok(sink)
    }

    /// Run the schema migration (CREATE … IF NOT EXISTS — safe to repeat).
    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::raw_sql(MIGRATION)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("postgres migrate: {e}"))?;
        Ok(())
    }

    /// Persist one telemetry event.
    pub async fn record(&self, event: &TelemetryEvent) -> anyhow::Result<()> {
        let (kind, subject, payload) = event_row(event)?;
        sqlx::query(
            "INSERT INTO telemetry_event (kind, subject, payload) VALUES ($1, $2, $3::jsonb)",
        )
        .bind(kind)
        .bind(subject)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|e| anyhow::anyhow!("postgres insert: {e}"))?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// Map an event to its `(kind, subject, payload-json)` row. Pure — the canonical JSON is the same
/// wire format published to NATS, so persistence and the bus never diverge.
pub fn event_row(event: &TelemetryEvent) -> anyhow::Result<(String, String, String)> {
    let payload = serde_json::to_string(event)?;
    let kind = serde_json::from_str::<serde_json::Value>(&payload)?
        .get("kind")
        .and_then(|k| k.as_str())
        .unwrap_or("unknown")
        .to_string();
    Ok((kind, event.subject().to_string(), payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{Decision, DecisionType};
    use chrono::Utc;

    #[test]
    fn event_row_extracts_kind_subject_and_payload() {
        let decision = Decision {
            decision_type: DecisionType::Tip,
            action: "tip 14500 lamports".into(),
            reasoning: "congestion rising".into(),
            confidence: 0.7,
            inputs_considered: serde_json::json!({ "congestionScore": 0.9 }),
            before: Some(serde_json::json!({ "tip": 12500 })),
            after: Some(serde_json::json!({ "tip": 14500 })),
            provider: "anthropic".into(),
            latency_ms: 740,
            ts: Utc::now(),
        };
        let (kind, subject, payload) = event_row(&TelemetryEvent::Decision(decision)).unwrap();
        assert_eq!(kind, "decision");
        assert_eq!(subject, "decision.tip");
        assert!(payload.contains("\"decision_type\":\"tip\""));
        assert!(payload.contains("14500"));
    }

    #[test]
    fn event_row_kind_matches_subject_family_for_slots() {
        use prometheon_types::{SlotStatus, SlotUpdate};
        let ev = TelemetryEvent::Slot(SlotUpdate::new(
            425_350_212,
            Some(425_350_211),
            SlotStatus::Confirmed,
            Utc::now(),
        ));
        let (kind, subject, _) = event_row(&ev).unwrap();
        assert_eq!(kind, "slot");
        assert_eq!(subject, "telemetry.slot");
    }
}
