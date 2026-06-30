//! The `submit → Receipt` surface — PrometheonOS as a callable product, not just a proof runner.
//!
//! A caller hands the engine a strategy to land; the engine builds, signs (engine-custody — see
//! below), tips, tracks the full lifecycle, and **autonomously retries** on a non-landing, then
//! returns a [`Receipt`]. This wraps the same tested saga ([`crate::saga::run_saga`]) the mainnet
//! proof uses — the library fn ([`submit`]), the `submit` CLI, and the loopback HTTP endpoint
//! ([`serve_submit`]) are three faces of it.
//!
//! **Engine-custody (honest framing).** The saga refreshes the blockhash and **re-signs** on a
//! retry, which needs the signing key — so v1 is engine-custody: the engine holds the wallet and
//! signs. A caller-signed `submit(signed_tx)` that still supports refresh-on-expiry requires a
//! durable nonce (or a re-sign callback) and is deliberately left as future work, so the autonomous
//! retry we advertise is always real.
//!
//! **The receipt is the lifecycle log.** A [`Receipt`] is derived from the *same* `Bundle` /
//! `Lifecycle` / `Failure` telemetry the saga emits, run through the canonical
//! [`prometheon_telemetry::export::build_log`] assembler — so a receipt is provably reconcilable
//! with the committed lifecycle log, not a separate codepath that could drift.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;

use prometheon_bundle::{BlockEngineClient, BlockEngineConfig, Percentile, TipFloor};
use prometheon_failure::FailureSignals;
use prometheon_ingest::yellowstone::{self, SubscriptionSpec, YellowstoneConfig};
use prometheon_ingest::IngestMessage;
use prometheon_telemetry::export::{build_log, LifecycleLogEntry};
use prometheon_telemetry::{Decision, DecisionType, PostgresSink, TelemetryBus, TelemetryEvent};
use prometheon_types::Commitment;

use crate::config::Config;
use crate::proof;
use crate::proof_run::{self, SubmittedBundle};
use crate::rpc::{BlockhashInfo, RpcClient};
use crate::saga::{run_saga, AttemptSpec, BaseBundle, DecisionSource, SagaConfig, Submitter};
use crate::sinks::{EventSink, Sinks};

/// The outcome of a [`submit`], derived from the lifecycle the saga observed. Serializes to JSON for
/// the CLI / HTTP response. Carries **no** wallet, key, or signature material — slot/stage/attempts/
/// class only.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Receipt {
    /// The bundle landed. `final_stage` is honest about the observed commitment bar — `"confirmed"`
    /// (the landing bar) or `"finalized"` (rooted) — since finalization may lag the caller's deadline.
    Landed {
        slot: u64,
        final_stage: String,
        attempts: u32,
    },
    /// No attempt landed within the deadline / attempt cap. `last_class` is the real-signal failure
    /// classification of the final attempt (e.g. `"expired_blockhash"`, `"fee_too_low"`).
    Failed {
        reason: String,
        last_class: Option<String>,
        attempts: u32,
    },
}

/// Bucket captured [`TelemetryEvent`]s by `kind` and run the canonical lifecycle-log assembler — the
/// exact path `export.rs` uses after reading the events back from Postgres, so an in-process receipt
/// and the exported log are built from one assembler.
pub fn log_from_events(events: &[TelemetryEvent]) -> Vec<LifecycleLogEntry> {
    let (mut bundles, mut lifecycles, mut failures) = (vec![], vec![], vec![]);
    for ev in events {
        let v: Value = match serde_json::to_value(ev) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v["kind"].as_str() {
            Some("bundle") => bundles.push(v),
            Some("lifecycle") => lifecycles.push(v),
            Some("failure") => failures.push(v),
            _ => {}
        }
    }
    build_log(&bundles, &lifecycles, &failures)
}

/// Map an assembled lifecycle log to the [`Receipt`] for one logical bundle (`base_id`).
///
/// A logical bundle may have several attempts (retries share `base_id`). If any attempt landed, the
/// receipt is [`Receipt::Landed`] reporting the **landed attempt's** slot/stage (not an earlier
/// failed attempt's); otherwise [`Receipt::Failed`] with the final attempt's real-signal class.
/// `attempts` is the highest attempt number seen (how many tries it took).
pub fn receipt_from_log(log: &[LifecycleLogEntry], base_id: &str) -> Receipt {
    // Entries for this logical bundle: match on base_id, falling back to bundle_id for a single
    // attempt that predates base_id linkage.
    let mut mine: Vec<&LifecycleLogEntry> = log
        .iter()
        .filter(|e| e.base_id.as_deref() == Some(base_id))
        .collect();
    if mine.is_empty() {
        mine = log.iter().filter(|e| e.bundle_id == base_id).collect();
    }
    if mine.is_empty() {
        return Receipt::Failed {
            reason: format!("no telemetry recorded for bundle `{base_id}`"),
            last_class: None,
            attempts: 0,
        };
    }

    let attempts = mine
        .iter()
        .filter_map(|e| e.attempt)
        .max()
        .unwrap_or(mine.len() as u32)
        .max(1);

    // Prefer the highest-attempt landed entry — the recovered resubmission, not the failed attempt.
    if let Some(landed) = mine
        .iter()
        .filter(|e| e.landed())
        .max_by_key(|e| e.attempt.unwrap_or(1))
    {
        return Receipt::Landed {
            slot: landed.first_slot.unwrap_or(0).max(0) as u64,
            final_stage: landed
                .final_stage
                .clone()
                .unwrap_or_else(|| "confirmed".into()),
            attempts,
        };
    }

    let last = mine.iter().max_by_key(|e| e.attempt.unwrap_or(1));
    let last_class = last.and_then(|e| e.failure_class.clone());
    let reason = last_class
        .clone()
        .map(|c| format!("did not land; final attempt classified `{c}`"))
        .unwrap_or_else(|| "did not land within the deadline".into());
    Receipt::Failed {
        reason,
        last_class,
        attempts,
    }
}

/// An [`EventSink`] that forwards every event to an inner sink **and** captures a copy, so the saga's
/// real telemetry still flows to NATS/Postgres in production while we derive a [`Receipt`] from the
/// captured stream in-process. The inner sink sees exactly what it would without the tee.
struct TeeSink<'a, E: EventSink> {
    inner: &'a E,
    captured: Arc<Mutex<Vec<TelemetryEvent>>>,
}

impl<'a, E: EventSink> TeeSink<'a, E> {
    fn new(inner: &'a E) -> Self {
        Self {
            inner,
            captured: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn captured(&self) -> Vec<TelemetryEvent> {
        self.captured.lock().unwrap().clone()
    }
}

impl<E: EventSink + Sync> EventSink for TeeSink<'_, E> {
    async fn emit(&self, event: &TelemetryEvent) {
        // Scope the lock so the guard is dropped before the await (keeps the future `Send`).
        {
            self.captured.lock().unwrap().push(event.clone());
        }
        self.inner.emit(event).await;
    }
}

/// Run the autonomous saga over `bases` and return one [`Receipt`] per logical bundle (in `bases`
/// order). Telemetry still fans out through `sink` (NATS/Postgres in production); the receipts are
/// derived from the *same* events via [`log_from_events`] + [`receipt_from_log`], so a receipt is the
/// in-process view of what the exported lifecycle log will show. Generic over the same three traits
/// as [`run_saga`], so the no-network saga test doubles drive it directly.
pub async fn run_submit<E: EventSink + Sync, D: DecisionSource, S: Submitter>(
    sink: &E,
    decider: &D,
    submitter: &S,
    bases: Vec<BaseBundle>,
    rx: &mut mpsc::Receiver<IngestMessage>,
    cfg: SagaConfig,
) -> Vec<Receipt> {
    let base_ids: Vec<String> = bases.iter().map(|b| b.base_id.clone()).collect();
    let tee = TeeSink::new(sink);
    let _ = run_saga(&tee, decider, submitter, bases, rx, cfg).await;
    let log = log_from_events(&tee.captured());
    base_ids
        .iter()
        .map(|id| receipt_from_log(&log, id))
        .collect()
}

// ── The callable surface: SubmitRequest → Receipt (engine-custody) ───────────────────────────────

/// What the caller wants landed. The ENGINE builds, signs (with `signer`), tips, tracks, and retries
/// — engine-custody, because the saga re-signs on a refresh-on-expiry retry (see the module docs for
/// why a caller-signed tx with retry needs a durable nonce).
#[derive(Debug, Clone)]
pub struct SubmitRequest {
    pub strategy: SubmitStrategy,
    pub signer: SignerSource,
    /// Max attempts before abandoning (≥1). Maps to `SagaConfig.max_attempts`.
    pub max_attempts: u32,
    /// Wall-clock budget for the whole submit, including any autonomous retries.
    pub deadline: Duration,
}

/// The strategy to land. v1 ships the mainnet-proven self-transfer; arbitrary instructions
/// (`Custom`) are future work (thread `strategy_ixs` through `proof::prepare_attempt`).
#[derive(Debug, Clone)]
pub enum SubmitStrategy {
    SelfTransfer { lamports: u64 },
}

/// Where the signing key comes from. Loaded **server-side**; never crosses the wire or a log.
#[derive(Debug, Clone)]
pub enum SignerSource {
    /// A Solana CLI keypair JSON path.
    KeypairPath(String),
    /// The `NETWORK`-selected `Config.wallet_keypair_path`.
    ConfigWallet,
}

/// Congestion proxy from the live tip floor: when the EMA sits above the p50, tips are rising.
fn congestion_proxy(floor: &TipFloor) -> f64 {
    let p50 = floor.percentile_lamports(Percentile::P50) as f64;
    let ema = floor.ema50_lamports() as f64;
    if p50 <= 0.0 {
        0.0
    } else {
        ((ema / p50) - 1.0).clamp(0.0, 1.0)
    }
}

/// AI decision source backed by the TS agent over NATS; `None` on timeout/error so the saga falls
/// back to the deterministic policy (and still emits a visible decision trace).
struct NatsDecisionSource {
    bus: TelemetryBus,
    timeout: Duration,
}

impl DecisionSource for NatsDecisionSource {
    async fn decide(&self, decision_type: DecisionType, context: Value) -> Option<Decision> {
        self.bus
            .request_decision(decision_type, context, self.timeout)
            .await
            .ok()
    }
}

/// The clean product submitter: assemble (honoring `refresh_blockhash`), sign with the engine wallet,
/// and `sendBundle` — **no fault injection** (that lives only in the proof's `LiveSubmitter`). Uses
/// the shared real-signal [`proof::probe_failure_signals`] so non-landings are classified from
/// observed RPC/Jito data.
struct EngineSubmitter {
    rpc: RpcClient,
    jito: BlockEngineClient,
    payer: Keypair,
    congestion: f64,
    transfer: u64,
    region: String,
    floor_p50: u64,
    /// Per-base blockhash of the last attempt, so a non-refresh retry REUSES it (honoring
    /// `AttemptSpec.refresh_blockhash`) instead of always re-fetching.
    blockhash_cache: AsyncMutex<HashMap<String, BlockhashInfo>>,
}

impl Submitter for EngineSubmitter {
    async fn submit(&self, spec: &AttemptSpec) -> anyhow::Result<SubmittedBundle> {
        let current_slot = self.rpc.get_slot().await.ok();
        // Honor the retry refresh flag: reuse the base's cached blockhash only when not refreshing AND
        // it is still valid on-chain (a non-refresh retry can arrive after the give-up window, by which
        // point the cached blockhash may have expired — reusing it then guarantees a Jito 400).
        let cached = if spec.refresh_blockhash {
            None
        } else {
            self.blockhash_cache
                .lock()
                .await
                .get(&spec.base_id)
                .cloned()
        };
        let reuse = match cached {
            Some(b) => match self.rpc.is_blockhash_valid(&b.blockhash).await {
                Ok(true) => Some(b),
                _ => None,
            },
            None => None,
        };
        let plan = proof::prepare_attempt(
            &self.rpc,
            &self.jito,
            &self.payer,
            self.congestion,
            None,
            spec.attempt_no,
            self.transfer,
            spec.tip_lamports, // AI-chosen tip (or None → live-floor fallback); always clamped (no bypass)
            false,
            reuse,
        )
        .await?;
        self.blockhash_cache.lock().await.insert(
            spec.base_id.clone(),
            BlockhashInfo {
                blockhash: plan.blockhash.clone(),
                last_valid_block_height: plan.last_valid_block_height,
            },
        );
        let bundle_id = match self
            .jito
            .send_bundle(std::slice::from_ref(&plan.tx_base64))
            .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(error = %e, "send rejected; tracking by signature");
                plan.signature.clone()
            }
        };
        Ok(SubmittedBundle {
            bundle_id,
            signature: plan.signature.clone(),
            tip_lamports: plan.tip_lamports,
            tip_account: plan.tip_account.clone(),
            region: self.region.clone(),
            submitted_at: chrono::Utc::now(),
            tip_floor_p50_lamports: self.floor_p50,
            injected: None,
            base_id: spec.base_id.clone(),
            attempt_no: spec.attempt_no,
            deadline_slot: current_slot.map(|s| s.saturating_add(150)),
        })
    }

    async fn probe_failure(&self, sb: &SubmittedBundle) -> Option<FailureSignals> {
        let bh = self.blockhash_cache.lock().await.get(&sb.base_id).cloned();
        Some(proof::probe_failure_signals(&self.rpc, &self.jito, sb, self.floor_p50, bh).await)
    }
}

/// **Submit a strategy and get a [`Receipt`]** — the callable product surface. The engine assembles,
/// signs, tips, tracks the full lifecycle over one Yellowstone stream, and autonomously retries on a
/// non-landing (the AI agent over NATS, falling back to the deterministic policy). Telemetry fans out
/// to NATS + Postgres exactly as the proof run does, and the returned receipt is derived from that
/// same telemetry. Requires a configured Yellowstone endpoint + NATS (the bounty `docker-compose`),
/// and a funded wallet to land on mainnet.
pub async fn submit(config: &Config, request: SubmitRequest) -> anyhow::Result<Receipt> {
    let payer = match &request.signer {
        SignerSource::KeypairPath(p) => crate::wallet::load_keypair(p)?,
        SignerSource::ConfigWallet => crate::wallet::load_keypair(&config.wallet_keypair_path)?,
    };
    let rpc = RpcClient::new(&config.rpc_url)?;
    let jito = BlockEngineClient::new(BlockEngineConfig {
        base_url: config.jito_block_engine_url.clone(),
        tip_floor_url: config.jito_tip_floor_url.clone(),
        auth_uuid: config.jito_auth_uuid.clone(),
        ..Default::default()
    })?;

    let floor = jito.get_tip_floor().await?;
    let congestion = congestion_proxy(&floor);
    let floor_p50 = floor.percentile_lamports(Percentile::P50);
    let floor_p75 = floor.percentile_lamports(Percentile::P75);
    let floor_p95 = floor.percentile_lamports(Percentile::P95);

    // Telemetry sinks + AI decision source — the SAME wiring the proof's run_live uses.
    let bus = TelemetryBus::connect(&config.nats_url).await?;
    let pg = if config.db_enabled() {
        PostgresSink::connect(&config.database_url).await.ok()
    } else {
        None
    };
    let decider = NatsDecisionSource {
        bus: bus.clone(),
        timeout: Duration::from_secs(8),
    };
    let sinks = Sinks::new(bus, pg);

    // ONE Yellowstone stream for the whole submit (opened before sending so we miss no early events).
    let endpoint = config.yellowstone_endpoint.clone().ok_or_else(|| {
        anyhow::anyhow!("YELLOWSTONE_ENDPOINT required for submit lifecycle tracking")
    })?;
    let yc = YellowstoneConfig {
        endpoint,
        x_token: config.yellowstone_x_token.clone(),
        commitment: Commitment::Confirmed,
        channel_capacity: 1024,
        ..Default::default()
    };
    let spec = SubscriptionSpec {
        track_slots: true,
        tx_account_include: vec![payer.pubkey().to_string()],
        ..Default::default()
    };
    let mut handle = yellowstone::spawn(yc, spec);

    let SubmitStrategy::SelfTransfer { lamports } = request.strategy;
    let submitter = EngineSubmitter {
        rpc,
        jito,
        payer,
        congestion,
        transfer: lamports,
        region: proof_run::region_from_url(&config.jito_block_engine_url),
        floor_p50,
        blockhash_cache: AsyncMutex::new(HashMap::new()),
    };

    let base = BaseBundle {
        base_id: "submit".into(),
        injected: None,
        tip_context: json!({
            "congestionScore": congestion,
            "tipFloorP50Lamports": floor_p50,
            "tipFloorP75Lamports": floor_p75,
            "tipFloorP95Lamports": floor_p95,
        }),
    };
    let cfg = SagaConfig {
        max_attempts: request.max_attempts.max(1),
        global_deadline: tokio::time::Instant::now() + request.deadline,
    };

    let receipts = run_submit(
        &sinks,
        &decider,
        &submitter,
        vec![base],
        &mut handle.rx,
        cfg,
    )
    .await;
    handle.task.abort();
    let _ = sinks.bus.flush().await;

    Ok(receipts.into_iter().next().unwrap_or(Receipt::Failed {
        reason: "no receipt produced".into(),
        last_class: None,
        attempts: 0,
    }))
}

// ── Loopback HTTP endpoint ───────────────────────────────────────────────────────────────────────

/// Parsed `/submit` request body. All fields optional with sane defaults, so an empty `{}` body works.
struct SubmitArgs {
    transfer_lamports: u64,
    max_attempts: u32,
    deadline_secs: u64,
}

/// Parse the JSON request body `{transfer_lamports, max_attempts, deadline_secs}` → [`SubmitArgs`],
/// falling back to defaults on any missing field or malformed body. Pure (unit-tested).
fn parse_submit_body(body: &str) -> SubmitArgs {
    let v: Value = serde_json::from_str(body.trim()).unwrap_or(Value::Null);
    SubmitArgs {
        transfer_lamports: v["transfer_lamports"].as_u64().unwrap_or(1),
        max_attempts: v["max_attempts"].as_u64().unwrap_or(3) as u32,
        deadline_secs: v["deadline_secs"].as_u64().unwrap_or(180),
    }
}

/// Render a minimal HTTP/1.1 JSON response. Pure (unit-tested).
fn http_json(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// Serve `POST /submit` on a **loopback** address: each request runs a real engine-custody [`submit`]
/// and returns the [`Receipt`] JSON. Refuses to bind a non-loopback host — this endpoint signs with a
/// funded wallet, so (exactly like the Prometheus `/metrics` exporter and the dashboard) localhost is
/// the trust boundary; it is unauthenticated by design and must not be exposed. Reuses the
/// dependency-light raw-`TcpListener` pattern of [`crate::metrics::serve`] (no HTTP framework).
pub async fn serve_submit(addr: SocketAddr, config: Config) -> anyhow::Result<()> {
    if !addr.ip().is_loopback() {
        anyhow::bail!(
            "submit endpoint must bind a loopback address (got {addr}); it signs with the engine \
             wallet and is unauthenticated — do not expose it"
        );
    }
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "submit endpoint listening (loopback only)");
    let config = Arc::new(config);
    loop {
        let (mut sock, _) = listener.accept().await?;
        let config = config.clone();
        tokio::spawn(async move {
            // The request body is tiny (a few fields); one read suffices.
            let mut tmp = [0u8; 4096];
            let n = sock.read(&mut tmp).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&tmp[..n]);
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
            let args = parse_submit_body(body);
            let request = SubmitRequest {
                strategy: SubmitStrategy::SelfTransfer {
                    lamports: args.transfer_lamports,
                },
                signer: SignerSource::ConfigWallet,
                max_attempts: args.max_attempts,
                deadline: Duration::from_secs(args.deadline_secs),
            };
            let resp = match submit(&config, request).await {
                Ok(r) => http_json("200 OK", &serde_json::to_string(&r).unwrap_or_default()),
                Err(e) => {
                    let msg = e.to_string().replace('"', "'");
                    http_json(
                        "500 Internal Server Error",
                        &format!("{{\"error\":\"{msg}\"}}"),
                    )
                }
            };
            let _ = sock.write_all(resp.as_bytes()).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Build a lifecycle log from raw event payloads, exactly as the export path does.
    fn log(
        bundles: Vec<Value>,
        lifecycles: Vec<Value>,
        failures: Vec<Value>,
    ) -> Vec<LifecycleLogEntry> {
        build_log(&bundles, &lifecycles, &failures)
    }

    fn landed_lifecycles(id: &str, slot: i64) -> Vec<Value> {
        vec![
            json!({"kind":"lifecycle","id":id,"event":{"stage":"submitted","slot":null,"ts":"2026-06-29T00:00:00.000Z","delta_ms_from_prev":null}}),
            json!({"kind":"lifecycle","id":id,"event":{"stage":"processed","slot":slot,"ts":"2026-06-29T00:00:00.400Z","delta_ms_from_prev":400}}),
            json!({"kind":"lifecycle","id":id,"event":{"stage":"confirmed","slot":slot,"ts":"2026-06-29T00:00:00.800Z","delta_ms_from_prev":400}}),
            json!({"kind":"lifecycle","id":id,"event":{"stage":"finalized","slot":slot,"ts":"2026-06-29T00:00:13.000Z","delta_ms_from_prev":12200}}),
        ]
    }

    #[test]
    fn landed_single_attempt_maps_to_landed() {
        let bundles = vec![
            json!({"kind":"bundle","bundle_id":"a1","base_id":"b1","attempt":1,"tip_lamports":200000,"tip_account":"T","region":"ny","signatures":["sig"],"ts":"2026-06-29T00:00:00.000Z"}),
        ];
        let r = receipt_from_log(
            &log(bundles, landed_lifecycles("a1", 429572113), vec![]),
            "b1",
        );
        assert_eq!(
            r,
            Receipt::Landed {
                slot: 429_572_113,
                final_stage: "finalized".into(),
                attempts: 1
            }
        );
    }

    #[test]
    fn recovered_maps_to_landed_with_attempt_2_slot() {
        // attempt 1 (fee_too_low, never landed) + attempt 2 (finalized in a DIFFERENT slot).
        let bundles = vec![
            json!({"kind":"bundle","bundle_id":"a1","base_id":"b1","attempt":1,"tip_lamports":1000,"tip_account":"T","region":"ny","signatures":["sigFail"],"ts":"2026-06-29T00:00:00.000Z"}),
            json!({"kind":"bundle","bundle_id":"a2","base_id":"b1","attempt":2,"tip_lamports":200000,"tip_account":"T","region":"ny","signatures":["sigLand"],"ts":"2026-06-29T00:01:00.000Z"}),
        ];
        let failures = vec![
            json!({"kind":"failure","id":"a1","classification":{"class":"fee_too_low","confidence":0.80}}),
        ];
        let r = receipt_from_log(
            &log(bundles, landed_lifecycles("a2", 429572096), failures),
            "b1",
        );
        // Must report attempt 2's landing slot — never attempt 1's (which never landed).
        assert_eq!(
            r,
            Receipt::Landed {
                slot: 429_572_096,
                final_stage: "finalized".into(),
                attempts: 2
            }
        );
    }

    #[test]
    fn never_landed_maps_to_failed_with_last_class() {
        let bundles = vec![
            json!({"kind":"bundle","bundle_id":"a1","base_id":"b1","attempt":1,"tip_lamports":1000,"tip_account":"T","region":"ny","signatures":["sig"],"ts":"2026-06-29T00:00:00.000Z"}),
        ];
        let failures = vec![
            json!({"kind":"failure","id":"a1","classification":{"class":"fee_too_low","confidence":0.80}}),
        ];
        let r = receipt_from_log(&log(bundles, vec![], failures), "b1");
        match r {
            Receipt::Failed {
                last_class,
                attempts,
                ..
            } => {
                assert_eq!(last_class.as_deref(), Some("fee_too_low"));
                assert_eq!(attempts, 1);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn unknown_base_maps_to_failed_zero_attempts() {
        let r = receipt_from_log(&[], "nope");
        assert_eq!(
            r,
            Receipt::Failed {
                reason: "no telemetry recorded for bundle `nope`".into(),
                last_class: None,
                attempts: 0
            }
        );
    }

    #[test]
    fn receipt_serializes_without_wallet_material() {
        // A regression guard: the public receipt must never carry key/secret/signer material.
        for r in [
            Receipt::Landed {
                slot: 429_572_113,
                final_stage: "finalized".into(),
                attempts: 2,
            },
            Receipt::Failed {
                reason: "did not land".into(),
                last_class: Some("expired_blockhash".into()),
                attempts: 3,
            },
        ] {
            let s = serde_json::to_string(&r).unwrap().to_lowercase();
            for forbidden in ["key", "secret", "signer", "keypair", "privkey", "mnemonic"] {
                assert!(
                    !s.contains(forbidden),
                    "receipt JSON leaked `{forbidden}`: {s}"
                );
            }
        }
        // Shape is stable + self-describing for the HTTP/CLI consumer.
        let s = serde_json::to_string(&Receipt::Landed {
            slot: 429_572_113,
            final_stage: "finalized".into(),
            attempts: 2,
        })
        .unwrap();
        assert!(s.contains("\"outcome\":\"landed\""));
        assert!(s.contains("\"slot\":429572113"));
    }

    #[test]
    fn log_from_events_matches_export_buckets() {
        use chrono::DateTime;
        use prometheon_telemetry::{BundleEvent, BundlePhase, TelemetryEvent};
        let ts = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let events = vec![TelemetryEvent::Bundle(BundleEvent {
            bundle_id: "a1".into(),
            tip_lamports: 200_000,
            tip_account: "T".into(),
            region: "ny".into(),
            signatures: vec!["sig".into()],
            phase: BundlePhase::Submitted,
            ts,
            base_id: Some("b1".into()),
            attempt: Some(1),
        })];
        let built = log_from_events(&events);
        assert_eq!(built.len(), 1);
        assert_eq!(built[0].base_id.as_deref(), Some("b1"));
        assert_eq!(built[0].tip_lamports, 200_000);
    }

    #[test]
    fn parse_submit_body_uses_overrides_and_defaults() {
        let a = parse_submit_body(r#"{"transfer_lamports":5,"max_attempts":2,"deadline_secs":60}"#);
        assert_eq!(a.transfer_lamports, 5);
        assert_eq!(a.max_attempts, 2);
        assert_eq!(a.deadline_secs, 60);
        // Empty body → defaults (so `curl` with no body still works).
        let d = parse_submit_body("{}");
        assert_eq!(d.transfer_lamports, 1);
        assert_eq!(d.max_attempts, 3);
        assert_eq!(d.deadline_secs, 180);
        // Malformed body → defaults, never a panic.
        let bad = parse_submit_body("not json at all");
        assert_eq!(bad.transfer_lamports, 1);
        assert_eq!(bad.max_attempts, 3);
    }

    #[test]
    fn http_json_renders_headers_and_body() {
        let receipt = Receipt::Landed {
            slot: 429_572_113,
            final_stage: "finalized".into(),
            attempts: 2,
        };
        let body = serde_json::to_string(&receipt).unwrap();
        let resp = http_json("200 OK", &body);
        assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(resp.contains("Content-Type: application/json"));
        assert!(resp.contains(&format!("Content-Length: {}", body.len())));
        assert!(resp.ends_with(&body));
        assert!(resp.contains(r#""outcome":"landed""#));
    }
}
