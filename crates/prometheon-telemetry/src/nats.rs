//! NATS transport for the telemetry contract.
//!
//! Two responsibilities:
//! 1. **Publish** [`TelemetryEvent`]s to their mapped subjects (the dashboard + persistence sinks
//!    subscribe). Fire-and-forget JSON — telemetry must never block the engine.
//! 2. **Request a decision** from the TypeScript AI strategist over `decision.request.<type>` and
//!    parse the returned [`Decision`]. This is the one place the deterministic Rust core *waits* on
//!    the agent — and only for non-hot-path events (tip policy, retry reasoning), per the
//!    AI-as-strategist design.
//!
//! The wire shapes (request body, reply parsing) are pure and unit-tested; the connection/publish
//! is thin I/O exercised by an env-gated integration test against a live NATS server.

use std::time::Duration;

use async_nats::Client;
use serde_json::Value;

use crate::decision::{Decision, DecisionType};
use crate::events::TelemetryEvent;

/// A handle over a NATS connection for publishing telemetry + requesting decisions.
#[derive(Clone)]
pub struct TelemetryBus {
    client: Client,
}

impl TelemetryBus {
    /// Connect to a NATS server.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let client = async_nats::connect(url)
            .await
            .map_err(|e| anyhow::anyhow!("nats connect {url}: {e}"))?;
        Ok(Self { client })
    }

    /// Wrap an already-established client (useful for sharing one connection).
    pub fn from_client(client: Client) -> Self {
        Self { client }
    }

    /// Borrow the underlying client (e.g. to subscribe).
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Publish a telemetry event to its mapped subject as JSON.
    pub async fn publish(&self, event: &TelemetryEvent) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(event)?;
        self.client
            .publish(event.subject().to_string(), payload.into())
            .await
            .map_err(|e| anyhow::anyhow!("nats publish {}: {e}", event.subject()))?;
        Ok(())
    }

    /// Flush pending publishes to the server.
    pub async fn flush(&self) -> anyhow::Result<()> {
        self.client.flush().await?;
        Ok(())
    }

    /// Request a decision from the AI agent and parse its reply.
    ///
    /// Publishes `{ decisionType, context }` to `decision.request.<type>` (the subject the agent
    /// subscribes to) and waits up to `timeout` for the assembled [`Decision`].
    pub async fn request_decision(
        &self,
        decision_type: DecisionType,
        context: Value,
        timeout: Duration,
    ) -> anyhow::Result<Decision> {
        let subject = request_subject(decision_type);
        let body = decision_request_body(decision_type, context);
        let payload = serde_json::to_vec(&body)?;
        let msg = tokio::time::timeout(timeout, self.client.request(subject, payload.into()))
            .await
            .map_err(|_| anyhow::anyhow!("decision request timed out after {timeout:?}"))?
            .map_err(|e| anyhow::anyhow!("decision request failed: {e}"))?;
        parse_decision_reply(&msg.payload)
    }
}

/// The agent-facing subject for a decision type.
pub fn request_subject(decision_type: DecisionType) -> String {
    format!("decision.request.{}", decision_type_str(decision_type))
}

/// The wire string for a decision type (matches the TS `decisionTypeSchema` enum).
pub fn decision_type_str(decision_type: DecisionType) -> &'static str {
    match decision_type {
        DecisionType::Tip => "tip",
        DecisionType::Timing => "timing",
        DecisionType::Retry => "retry",
    }
}

/// Build the request payload the TS agent expects: `{ decisionType, context }`.
pub fn decision_request_body(decision_type: DecisionType, context: Value) -> Value {
    serde_json::json!({
        "decisionType": decision_type_str(decision_type),
        "context": context,
    })
}

/// Parse the agent's reply: either a serialized [`Decision`] or `{ "error": "..." }`.
pub fn parse_decision_reply(bytes: &[u8]) -> anyhow::Result<Decision> {
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|e| anyhow::anyhow!("decision reply not JSON: {e}"))?;
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        anyhow::bail!("agent returned error: {err}");
    }
    serde_json::from_value(v).map_err(|e| anyhow::anyhow!("decision reply schema mismatch: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_decision_json() -> Value {
        serde_json::json!({
            "decision_type": "tip",
            "action": "tip 12000->18000 lamports",
            "reasoning": "congestion rising; target 75th pct",
            "confidence": 0.81,
            "inputs_considered": { "p50": 14200 },
            "before": { "tip": 12000 },
            "after": { "tip": 18000 },
            "provider": "anthropic",
            "latency_ms": 740,
            "ts": Utc::now().to_rfc3339(),
        })
    }

    #[test]
    fn request_subject_matches_agent_subscription() {
        assert_eq!(request_subject(DecisionType::Tip), "decision.request.tip");
        assert_eq!(
            request_subject(DecisionType::Timing),
            "decision.request.timing"
        );
        assert_eq!(
            request_subject(DecisionType::Retry),
            "decision.request.retry"
        );
    }

    #[test]
    fn request_body_uses_camelcase_decision_type_and_nested_context() {
        let body = decision_request_body(
            DecisionType::Tip,
            serde_json::json!({ "congestion_score": 0.74 }),
        );
        assert_eq!(body["decisionType"], "tip");
        assert_eq!(body["context"]["congestion_score"], 0.74);
    }

    #[test]
    fn parse_reply_decodes_a_decision() {
        let bytes = serde_json::to_vec(&sample_decision_json()).unwrap();
        let d = parse_decision_reply(&bytes).unwrap();
        assert_eq!(d.decision_type, DecisionType::Tip);
        assert_eq!(d.confidence, 0.81);
        assert_eq!(d.provider, "anthropic");
    }

    #[test]
    fn parse_reply_surfaces_agent_errors() {
        let bytes = br#"{"error":"missing ANTHROPIC_API_KEY"}"#;
        let err = parse_decision_reply(bytes).unwrap_err().to_string();
        assert!(err.contains("missing ANTHROPIC_API_KEY"), "got: {err}");
    }

    #[test]
    fn parse_reply_rejects_garbage() {
        assert!(parse_decision_reply(b"not json").is_err());
        assert!(parse_decision_reply(br#"{"decision_type":"tip"}"#).is_err()); // incomplete
    }
}
