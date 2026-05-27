//! Live Yellowstone gRPC (Dragon's Mouth) client.
//!
//! Connects to a Yellowstone endpoint, subscribes to slot/leader/transaction updates, feeds the
//! [`SlotTracker`], and forwards normalized [`IngestMessage`]s to consumers over a bounded channel.
//! It owns the operational concerns the bounty grades:
//!
//! - **Reconnection** — a supervise loop reconnects with exponential backoff and attempts
//!   `from_slot` replay from the tracker checkpoint (falling back to live-from-tip if the desired
//!   slot predates the server's replay buffer).
//! - **Backpressure** — the receive loop never blocks; if consumers fall behind, messages are
//!   dropped (newest) and counted, rather than stalling the stream and getting force-disconnected
//!   (see [`crate::backpressure`] for the policy core this mirrors).
//! - **Keepalive** — a periodic client ping keeps NAT/load-balancer connections alive, and the
//!   server's pings are answered.
//!
//! The pure request-building seam ([`build_subscribe_request`]) is unit-tested; the network I/O is
//! exercised against a real endpoint (gated; needs SolInfra Yellowstone credentials).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, SubscribeRequest, SubscribeRequestFilterSlots,
    SubscribeRequestFilterTransactions, SubscribeRequestPing,
};

use prometheon_types::{Commitment, Slot, SlotUpdate};

use crate::slot_tracker::{SlotObservation, SlotTracker};
use crate::status_map::{commitment_to_code, slot_status_from_code};

/// Connection + subscription configuration.
#[derive(Clone, Debug)]
pub struct YellowstoneConfig {
    /// gRPC endpoint, e.g. `https://yellowstone.example.com:443`.
    pub endpoint: String,
    /// `x-token` auth metadata (provider-issued), if required.
    pub x_token: Option<String>,
    /// Request-global commitment for the subscription. Subscribe low, track progression locally.
    pub commitment: Commitment,
    /// Bounded channel capacity for forwarding to consumers (backpressure bound).
    pub channel_capacity: usize,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    /// Raise to fit full blocks/accounts; Triton's example uses 64 MiB.
    pub max_decoding_message_size: usize,
    /// Client keepalive ping interval.
    pub keepalive_interval: Duration,
}

impl Default for YellowstoneConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            x_token: None,
            commitment: Commitment::Confirmed,
            channel_capacity: 8192,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_decoding_message_size: 64 * 1024 * 1024,
            keepalive_interval: Duration::from_secs(30),
        }
    }
}

/// What to subscribe to.
#[derive(Clone, Debug, Default)]
pub struct SubscriptionSpec {
    /// Subscribe to the slots stream (progression + leader-window detection).
    pub track_slots: bool,
    /// Emit transactions touching any of these accounts (e.g. our payer + tip accounts).
    pub tx_account_include: Vec<String>,
    /// Emit transactions matching these exact signatures (per-signature filter).
    pub tx_signatures: Vec<String>,
}

/// A normalized transaction status observed on the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxStatus {
    pub signature: String,
    pub slot: Slot,
    /// Whether the transaction carried an error (failed on-chain).
    pub failed: bool,
    pub ts: DateTime<Utc>,
}

/// Messages emitted by the ingestion layer to downstream consumers.
#[derive(Clone, Debug)]
pub enum IngestMessage {
    /// A slot update plus its [`SlotObservation`] classification.
    Slot {
        update: SlotUpdate,
        observation: SlotObservation,
    },
    /// A transaction status update.
    Transaction(TxStatus),
    /// The stream (re)connected; carries the `from_slot` we requested (if any).
    StreamConnected { from_slot: Option<Slot> },
    /// The stream errored or ended; the supervise loop will reconnect.
    StreamError { error: String },
}

/// Lifetime counters for telemetry (lock-free).
#[derive(Debug, Default)]
pub struct IngestCounters {
    pub received: AtomicU64,
    pub dropped: AtomicU64,
    pub reconnects: AtomicU64,
}

impl IngestCounters {
    /// `(received, dropped, reconnects)` snapshot.
    pub fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.received.load(Ordering::Relaxed),
            self.dropped.load(Ordering::Relaxed),
            self.reconnects.load(Ordering::Relaxed),
        )
    }

    /// Fraction of forwarded messages dropped due to backpressure, `[0.0, 1.0]`.
    pub fn drop_rate(&self) -> f64 {
        let received = self.received.load(Ordering::Relaxed);
        if received == 0 {
            0.0
        } else {
            self.dropped.load(Ordering::Relaxed) as f64 / received as f64
        }
    }
}

/// Handle to a running ingestion task.
pub struct IngestHandle {
    /// Receiver for normalized ingest messages.
    pub rx: mpsc::Receiver<IngestMessage>,
    /// Shared telemetry counters.
    pub counters: Arc<IngestCounters>,
    /// The supervise task; aborts on drop if not awaited.
    pub task: JoinHandle<()>,
}

/// Build the `SubscribeRequest` for a spec, commitment, and optional `from_slot` replay point.
///
/// Pure and unit-tested. Uses `..Default::default()` on proto filters so it is robust to additive
/// proto field changes across minor versions.
pub fn build_subscribe_request(
    spec: &SubscriptionSpec,
    commitment: Commitment,
    from_slot: Option<Slot>,
) -> SubscribeRequest {
    let mut slots = HashMap::new();
    if spec.track_slots {
        slots.insert(
            "slots".to_string(),
            SubscribeRequestFilterSlots {
                // We want every status transition (FirstShredReceived..Finalized/Dead) to track
                // progression and detect skips/gaps, so do NOT throttle to a single commitment.
                filter_by_commitment: Some(false),
                ..Default::default()
            },
        );
    }

    let mut transactions = HashMap::new();
    if !spec.tx_account_include.is_empty() {
        transactions.insert(
            "tx-accounts".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                // `failed: None` → include both successful and failed txs (we must see failures).
                account_include: spec.tx_account_include.clone(),
                ..Default::default()
            },
        );
    }
    for (i, sig) in spec.tx_signatures.iter().enumerate() {
        transactions.insert(
            format!("tx-sig-{i}"),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                signature: Some(sig.clone()),
                ..Default::default()
            },
        );
    }

    SubscribeRequest {
        slots,
        transactions,
        commitment: Some(commitment_to_code(commitment)),
        from_slot,
        ..Default::default()
    }
}

/// Spawn the supervised ingestion loop. Returns immediately with a handle; the task reconnects on
/// failure until the receiver is dropped.
pub fn spawn(config: YellowstoneConfig, spec: SubscriptionSpec) -> IngestHandle {
    let (tx, rx) = mpsc::channel(config.channel_capacity.max(1));
    let counters = Arc::new(IngestCounters::default());
    let task_counters = counters.clone();

    let task = tokio::spawn(async move {
        let mut tracker = SlotTracker::default();
        let mut backoff = Duration::from_millis(250);
        let max_backoff = Duration::from_secs(10);

        loop {
            match run_once(&config, &spec, &mut tracker, &tx, &task_counters).await {
                Ok(()) => backoff = Duration::from_millis(250),
                Err(err) => {
                    let _ = tx.try_send(IngestMessage::StreamError {
                        error: err.to_string(),
                    });
                    task_counters.reconnects.fetch_add(1, Ordering::Relaxed);
                }
            }
            if tx.is_closed() {
                break; // all consumers gone
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(max_backoff);
        }
    });

    IngestHandle { rx, counters, task }
}

/// One connect→subscribe→drain cycle. Returns `Ok(())` when the stream ends cleanly; `Err` on a
/// transport error (the supervise loop reconnects).
async fn run_once(
    config: &YellowstoneConfig,
    spec: &SubscriptionSpec,
    tracker: &mut SlotTracker,
    tx: &mpsc::Sender<IngestMessage>,
    counters: &IngestCounters,
) -> anyhow::Result<()> {
    let mut builder = GeyserGrpcClient::build_from_shared(config.endpoint.clone())?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect_timeout(config.connect_timeout)
        .timeout(config.request_timeout)
        .max_decoding_message_size(config.max_decoding_message_size);
    if let Some(token) = config.x_token.clone() {
        builder = builder.x_token(Some(token))?;
    }
    let mut client = builder.connect().await?;

    // Optimistic replay from our checkpoint; if the server rejects it (slot too old), fall back to
    // a live-from-tip subscription.
    let from_slot = tracker.checkpoint().map(|c| c + 1);
    let request = build_subscribe_request(spec, config.commitment, from_slot);
    let (mut sink, mut stream) = match client.subscribe_with_request(Some(request)).await {
        Ok(pair) => pair,
        Err(_) if from_slot.is_some() => {
            let fallback = build_subscribe_request(spec, config.commitment, None);
            client.subscribe_with_request(Some(fallback)).await?
        }
        Err(e) => return Err(e.into()),
    };

    let _ = tx.try_send(IngestMessage::StreamConnected { from_slot });

    let mut keepalive = tokio::time::interval(config.keepalive_interval);
    keepalive.tick().await; // consume the immediate first tick
    let mut ping_id: i32 = 0;

    loop {
        tokio::select! {
            _ = keepalive.tick() => {
                ping_id = ping_id.wrapping_add(1);
                let ping = SubscribeRequest { ping: Some(SubscribeRequestPing { id: ping_id }), ..Default::default() };
                if sink.send(ping).await.is_err() {
                    break; // sink closed; reconnect
                }
            }
            item = stream.next() => {
                match item {
                    Some(Ok(update)) => {
                        handle_update(update, tracker, tx, counters, &mut sink, &mut ping_id).await;
                    }
                    Some(Err(status)) => return Err(anyhow::anyhow!("stream error: {status}")),
                    None => break, // stream ended
                }
            }
        }
    }
    Ok(())
}

/// Route one `SubscribeUpdate`: map slots through the tracker, forward tx statuses, answer pings.
async fn handle_update<S>(
    update: yellowstone_grpc_proto::geyser::SubscribeUpdate,
    tracker: &mut SlotTracker,
    tx: &mpsc::Sender<IngestMessage>,
    counters: &IngestCounters,
    sink: &mut S,
    ping_id: &mut i32,
) where
    S: futures::Sink<SubscribeRequest> + Unpin,
{
    match update.update_oneof {
        Some(UpdateOneof::Slot(s)) => {
            if let Some(status) = slot_status_from_code(s.status) {
                let su = SlotUpdate::new(s.slot, s.parent, status, Utc::now());
                let observation = tracker.observe(&su);
                forward(
                    tx,
                    counters,
                    IngestMessage::Slot {
                        update: su,
                        observation,
                    },
                );
            }
        }
        Some(UpdateOneof::Transaction(t)) => {
            if let Some(info) = t.transaction {
                let failed = info.meta.as_ref().map(|m| m.err.is_some()).unwrap_or(false);
                forward(
                    tx,
                    counters,
                    IngestMessage::Transaction(TxStatus {
                        signature: bs58::encode(info.signature).into_string(),
                        slot: t.slot,
                        failed,
                        ts: Utc::now(),
                    }),
                );
            }
        }
        Some(UpdateOneof::Ping(_)) => {
            // Answer the server's liveness probe.
            *ping_id = ping_id.wrapping_add(1);
            let pong = SubscribeRequest {
                ping: Some(SubscribeRequestPing { id: *ping_id }),
                ..Default::default()
            };
            let _ = sink.send(pong).await;
        }
        _ => {}
    }
}

/// Forward a message to consumers without ever blocking the receive loop (DropNewest + accounting).
fn forward(tx: &mpsc::Sender<IngestMessage>, counters: &IngestCounters, msg: IngestMessage) {
    counters.received.fetch_add(1, Ordering::Relaxed);
    match tx.try_send(msg) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            counters.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_filter_is_added_with_all_statuses_and_commitment() {
        let spec = SubscriptionSpec {
            track_slots: true,
            ..Default::default()
        };
        let req = build_subscribe_request(&spec, Commitment::Confirmed, None);
        assert!(req.slots.contains_key("slots"));
        assert_eq!(req.slots["slots"].filter_by_commitment, Some(false));
        assert_eq!(
            req.commitment,
            Some(commitment_to_code(Commitment::Confirmed))
        );
        assert_eq!(req.from_slot, None);
        assert!(req.transactions.is_empty());
    }

    #[test]
    fn from_slot_is_threaded_into_the_request() {
        let spec = SubscriptionSpec {
            track_slots: true,
            ..Default::default()
        };
        let req = build_subscribe_request(&spec, Commitment::Processed, Some(12_345));
        assert_eq!(req.from_slot, Some(12_345));
    }

    #[test]
    fn account_and_signature_tx_filters_are_built() {
        let spec = SubscriptionSpec {
            track_slots: false,
            tx_account_include: vec!["Acc1".to_string()],
            tx_signatures: vec!["Sig1".to_string(), "Sig2".to_string()],
        };
        let req = build_subscribe_request(&spec, Commitment::Confirmed, None);
        assert!(req.slots.is_empty());
        assert_eq!(
            req.transactions["tx-accounts"].account_include,
            vec!["Acc1".to_string()]
        );
        assert_eq!(
            req.transactions["tx-sig-0"].signature,
            Some("Sig1".to_string())
        );
        assert_eq!(
            req.transactions["tx-sig-1"].signature,
            Some("Sig2".to_string())
        );
    }
}
