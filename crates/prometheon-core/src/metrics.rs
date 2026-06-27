//! Prometheus `/metrics` exporter.
//!
//! Deliberately dependency-light: a pure renderer ([`render_prometheus`]) that emits the text
//! exposition format, plus a tiny Tokio HTTP endpoint that serves the latest engine state. We avoid
//! the `metrics`/recorder ecosystem (and its global state) because the engine already owns the
//! [`NetworkHealthModel`] snapshot and a handful of counters — there's nothing to instrument
//! indirectly.

use std::net::SocketAddr;
use std::sync::Arc;

use prometheon_netmodel::HealthSnapshot;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Cumulative engine counters surfaced as Prometheus counters.
///
/// Scoped to what the read-only engine actually produces. (Bundle submit/land/retry counts live in
/// the proof saga's `ProofSummary` and the per-bundle telemetry, not here — the engine never submits,
/// so exposing always-zero submit gauges would be misleading.)
#[derive(Debug, Default, Clone)]
pub struct EngineCounters {
    pub slots_total: u64,
    pub telemetry_events_total: u64,
    pub stream_reconnects_total: u64,
}

#[derive(Default)]
struct Inner {
    health: Option<HealthSnapshot>,
    counters: EngineCounters,
}

/// Shared, cheaply-cloneable metrics state: the engine writes, the endpoint reads.
#[derive(Clone, Default)]
pub struct MetricsState {
    inner: Arc<Mutex<Inner>>,
}

impl MetricsState {
    pub async fn set_health(&self, health: HealthSnapshot) {
        self.inner.lock().await.health = Some(health);
    }

    pub async fn set_counters(&self, counters: EngineCounters) {
        self.inner.lock().await.counters = counters;
    }

    pub async fn render(&self) -> String {
        let g = self.inner.lock().await;
        render_prometheus(g.health.as_ref(), &g.counters)
    }
}

fn gauge(out: &mut String, name: &str, help: &str, value: f64) {
    out.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n"
    ));
}

fn counter(out: &mut String, name: &str, help: &str, value: u64) {
    out.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} counter\n{name} {value}\n"
    ));
}

/// Render the Prometheus text exposition (v0.0.4) for the current engine state. Pure.
pub fn render_prometheus(health: Option<&HealthSnapshot>, c: &EngineCounters) -> String {
    let mut out = String::new();

    if let Some(h) = health {
        gauge(
            &mut out,
            "prometheon_congestion_score",
            "Network congestion score [0,1].",
            h.congestion_score,
        );
        gauge(
            &mut out,
            "prometheon_slot_stability_score",
            "Slot stability score [0,1].",
            h.slot_stability_score,
        );
        gauge(
            &mut out,
            "prometheon_bundle_landing_probability",
            "Observed bundle landing probability [0,1].",
            h.bundle_landing_probability,
        );
        gauge(
            &mut out,
            "prometheon_retry_success_rate",
            "Fraction of retries that subsequently landed.",
            h.retry_success_rate,
        );
        gauge(
            &mut out,
            "prometheon_tip_floor_lamports",
            "Latest Jito tip-floor p50 (lamports).",
            h.tip_floor_lamports as f64,
        );
        if let Some(ms) = h.avg_confirmed_latency_ms {
            gauge(
                &mut out,
                "prometheon_avg_confirmed_latency_ms",
                "Mean submit->confirmed latency (ms).",
                ms,
            );
        }
    }

    counter(
        &mut out,
        "prometheon_slots_total",
        "Slot updates observed from the stream.",
        c.slots_total,
    );
    counter(
        &mut out,
        "prometheon_telemetry_events_total",
        "Telemetry events emitted.",
        c.telemetry_events_total,
    );
    counter(
        &mut out,
        "prometheon_stream_reconnects_total",
        "Yellowstone stream reconnects.",
        c.stream_reconnects_total,
    );

    out
}

/// Serve `/metrics` (any path returns the metrics) on `addr` until the task is aborted.
pub async fn serve(addr: SocketAddr, state: MetricsState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "prometheus /metrics exporter listening");
    loop {
        let (mut sock, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await; // drain the request line; we serve metrics for any path
            let body = state.render().await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn snapshot() -> HealthSnapshot {
        HealthSnapshot {
            ts: Utc::now(),
            congestion_score: 0.123,
            slot_stability_score: 0.9,
            bundle_landing_probability: 0.8,
            retry_success_rate: 0.5,
            tip_efficiency_ratio: 0.0,
            cost_per_successful_landing: None,
            avg_confirmed_latency_ms: Some(842.0),
            confirm_latency_variance_ms: None,
            tip_floor_lamports: 2856,
        }
    }

    #[test]
    fn renders_gauges_and_counters_in_exposition_format() {
        let c = EngineCounters {
            slots_total: 1234,
            telemetry_events_total: 1300,
            ..Default::default()
        };
        let out = render_prometheus(Some(&snapshot()), &c);
        assert!(out.contains("# TYPE prometheon_congestion_score gauge"));
        assert!(out.contains("prometheon_congestion_score 0.123"));
        assert!(out.contains("prometheon_tip_floor_lamports 2856"));
        assert!(out.contains("prometheon_avg_confirmed_latency_ms 842"));
        assert!(out.contains("# TYPE prometheon_slots_total counter"));
        assert!(out.contains("prometheon_slots_total 1234"));
    }

    #[test]
    fn renders_counters_without_a_health_snapshot() {
        let out = render_prometheus(None, &EngineCounters::default());
        assert!(!out.contains("prometheon_congestion_score"));
        assert!(out.contains("prometheon_slots_total 0"));
    }
}
