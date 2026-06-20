//! Shared telemetry emitter.
//!
//! Both the read-only engine ([`crate::engine`]) and the bundle submit driver
//! ([`crate::proof_run`]) fan their [`TelemetryEvent`]s through one [`EventSink`], so the submission
//! path persists the SAME `Bundle` / `Lifecycle` / `Failure` events the lifecycle-log export reads.
//! (Previously the proof tracked landings in memory only, and the exported log came out empty.)
//!
//! The trait abstracts the live [`Sinks`] (NATS + Postgres) from test doubles that capture events,
//! which keeps the submit→emit→export pipeline unit-testable without any network or database.

use prometheon_telemetry::{PostgresSink, TelemetryBus, TelemetryEvent};

/// Something telemetry events fan out to. The live [`Sinks`] publishes to NATS and persists to
/// Postgres; tests implement this to capture events for assertions.
///
/// Declared with an explicit `impl Future` return (rather than `async fn`) so the trait stays
/// `-D warnings`-clean under the `async_fn_in_trait` lint while remaining `Send` for spawning.
pub trait EventSink {
    fn emit(&self, event: &TelemetryEvent) -> impl std::future::Future<Output = ()> + Send;
}

/// Live sinks: publish to NATS (always) and persist to Postgres (when configured).
///
/// Both are **best-effort** — telemetry must never block or crash the caller, so failures are logged
/// at debug and swallowed.
#[derive(Clone)]
pub struct Sinks {
    pub bus: TelemetryBus,
    pub pg: Option<PostgresSink>,
}

impl Sinks {
    pub fn new(bus: TelemetryBus, pg: Option<PostgresSink>) -> Self {
        Self { bus, pg }
    }
}

impl EventSink for Sinks {
    async fn emit(&self, event: &TelemetryEvent) {
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
