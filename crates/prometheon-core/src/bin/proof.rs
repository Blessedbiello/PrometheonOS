//! Proof runner — build and either **dry-run simulate** (free) or **live submit + stream-track** N
//! Jito bundles on the configured network, deterministically including injected failures, and
//! **persisting the telemetry the lifecycle-log export reads**.
//!
//! ```text
//! # dry-run (free): validate the whole assembly path against live mainnet, no broadcast
//! NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count 12
//! # live (needs a funded mainnet wallet): submit + stream-confirm + persist the lifecycle log
//! NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --live --count 12 \
//!     --inject low-tip:1,stale-blockhash:1
//! ```
//!
//! The live run opens **one** Yellowstone stream (respecting the 1-stream plan), tracks every
//! in-flight bundle on it, and emits `Bundle` / `Lifecycle` / `Failure` events to NATS + Postgres so
//! `export-log` produces the populated, explorer-verifiable log. `--inject` guarantees the bounty's
//! ≥2 failure cases: `low-tip` submits a sub-floor tip (→ `fee_too_low`); `stale-blockhash` waits for
//! a captured blockhash to expire, then submits on it (→ `expired_blockhash`, a guaranteed non-land).

use std::collections::HashMap;
use std::time::Duration;

use prometheon_bundle::{BlockEngineClient, BlockEngineConfig, Percentile, TipFloor};
use prometheon_core::{
    config::Config,
    leader::{timing_hold_slots, LeaderSchedule, LeaderWindow},
    proof,
    proof_run::{self, SubmittedBundle},
    rpc::{BlockhashInfo, RpcClient},
    saga::{run_saga, AttemptSpec, BaseBundle, DecisionSource, SagaConfig, Submitter},
    sinks::{EventSink, Sinks},
    wallet,
};
use prometheon_failure::FailureSignals;
use prometheon_faultinject::FaultScenario;
use prometheon_ingest::yellowstone::{self, SubscriptionSpec, YellowstoneConfig};
use prometheon_telemetry::{Decision, DecisionType, PostgresSink, TelemetryBus, TelemetryEvent};
use prometheon_types::Commitment;
use serde_json::{json, Value};
use solana_sdk::signer::Signer;
use tokio::sync::Mutex;

fn arg_u64(args: &[String], flag: &str, default: u64) -> u64 {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn arg_str(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn short(s: &str) -> String {
    if s.len() <= 12 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..6], &s[s.len() - 4..])
    }
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

/// Expand an inject spec (`"low-tip:1,stale-blockhash:1"`) into a flat list of scenarios.
fn parse_inject_spec(spec: &str) -> Vec<FaultScenario> {
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, n) = match part.split_once(':') {
            Some((name, count)) => (name.trim(), count.trim().parse::<u32>().unwrap_or(1)),
            None => (part, 1),
        };
        let scenario = match name {
            "low-tip" => Some(FaultScenario::LowTip {
                tip_lamports: 1_000,
            }),
            "stale-blockhash" => Some(FaultScenario::BlockhashExpiry),
            _ => {
                eprintln!("warning: unknown inject scenario '{name}' ignored");
                None
            }
        };
        if let Some(s) = scenario {
            for _ in 0..n {
                out.push(s);
            }
        }
    }
    out
}

/// Assign scenarios to the LAST attempts (so early attempts are clean landings). Index = attempt-1.
fn assign_injections(scenarios: Vec<FaultScenario>, count: u32) -> Vec<Option<FaultScenario>> {
    let count = count as usize;
    let mut out = vec![None; count];
    let start = count.saturating_sub(scenarios.len());
    for (i, s) in scenarios.into_iter().enumerate() {
        if start + i < count {
            out[start + i] = Some(s);
        }
    }
    out
}

/// Poll block height until a captured blockhash is past its validity window, guaranteeing any bundle
/// built on it cannot land. Capped so the run never hangs.
async fn wait_until_expired(rpc: &RpcClient, bh: &BlockhashInfo) {
    let cap = tokio::time::Instant::now() + Duration::from_secs(150);
    loop {
        if let Ok(h) = rpc.block_height().await {
            if h > bh.last_valid_block_height {
                break;
            }
        }
        if tokio::time::Instant::now() >= cap {
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

fn inject_label(inj: Option<FaultScenario>) -> &'static str {
    match inj {
        Some(FaultScenario::LowTip { .. }) => " [inject low-tip]",
        Some(FaultScenario::BlockhashExpiry) => " [inject stale-blockhash]",
        _ => "",
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LOG_LEVEL")
                .or_else(|_| tracing_subscriber::EnvFilter::try_new("warn"))
                .unwrap(),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");
    let count = arg_u64(&args, "--count", 10) as u32;
    let transfer = arg_u64(&args, "--transfer-lamports", 1);
    let inject = arg_str(&args, "--inject").unwrap_or_default();
    let injections = assign_injections(parse_inject_spec(&inject), count);

    let config = Config::from_env()?;
    let payer = wallet::load_keypair(&config.wallet_keypair_path)?;
    let rpc = RpcClient::new(&config.rpc_url)?;
    let jito = BlockEngineClient::new(BlockEngineConfig {
        base_url: config.jito_block_engine_url.clone(),
        tip_floor_url: config.jito_tip_floor_url.clone(),
        auth_uuid: config.jito_auth_uuid.clone(),
        ..Default::default()
    })?;

    println!(
        "PrometheonOS proof — network={} mode={} count={} inject=[{}]",
        config.network.as_str(),
        if live { "LIVE" } else { "DRY-RUN" },
        count,
        inject
    );
    println!("payer: {}", payer.pubkey());

    let floor = jito.get_tip_floor().await?;
    let congestion = congestion_proxy(&floor);
    let floor_p50 = floor.percentile_lamports(Percentile::P50);
    let floor_p75 = floor.percentile_lamports(Percentile::P75);
    let floor_p95 = floor.percentile_lamports(Percentile::P95);
    println!(
        "tip floor p50={} p75={} p95={} ema50={} lamports -> congestion≈{:.3}\n",
        floor_p50,
        floor_p75,
        floor_p95,
        floor.ema50_lamports(),
        congestion
    );

    // Best-effort leader-window readout (endpoint shape varies by deployment; tolerate failure).
    match jito.get_next_scheduled_leader().await {
        Ok(next) => {
            let w = LeaderWindow::from_next(&next);
            println!(
                "next Jito leader: slot {} ({} slots out){}\n",
                next.next_leader_slot,
                w.slots_until(),
                next.next_leader_region
                    .map(|r| format!(", region {r}"))
                    .unwrap_or_default()
            );
        }
        Err(e) => println!("next Jito leader: unavailable ({e}) — verify endpoint\n"),
    }

    if live {
        run_live(
            &config,
            rpc.clone(),
            jito.clone(),
            payer,
            congestion,
            transfer,
            floor_p50,
            floor_p75,
            floor_p95,
            &injections,
        )
        .await
    } else {
        run_dry(&rpc, &jito, &payer, congestion, transfer, &injections).await
    }
}

/// Dry-run: assemble + simulate each attempt (free). Validates the whole path bar broadcast.
async fn run_dry(
    rpc: &RpcClient,
    jito: &BlockEngineClient,
    payer: &solana_sdk::signature::Keypair,
    congestion: f64,
    transfer: u64,
    injections: &[Option<FaultScenario>],
) -> anyhow::Result<()> {
    let mut ok = 0u32;
    let mut failed = 0u32;
    for (i, inj) in injections.iter().enumerate() {
        let attempt = (i + 1) as u32;
        // Stale-blockhash injection only meaningfully applies to a live submit; in dry-run we still
        // show the low-tip override.
        let tip_override = match inj {
            Some(FaultScenario::LowTip { tip_lamports }) => Some(*tip_lamports),
            _ => None,
        };
        let plan = proof::prepare_attempt(
            rpc,
            jito,
            payer,
            congestion,
            None,
            attempt,
            transfer,
            tip_override,
            matches!(inj, Some(FaultScenario::LowTip { .. })),
            None,
        )
        .await?;
        println!(
            "#{:<2} tip={:>7} lamports  cu_price={:<5}  acct={}  bh={}  sig={}{}",
            attempt,
            plan.tip_lamports,
            plan.cu_price_micro,
            short(&plan.tip_account),
            short(&plan.blockhash),
            short(&plan.signature),
            inject_label(*inj),
        );
        let sim = rpc.simulate_transaction(&plan.tx_base64).await?;
        if sim.succeeded() {
            ok += 1;
            println!("   ✓ simulate OK (units={:?})", sim.units_consumed);
        } else if sim.is_insufficient_funds() {
            println!(
                "   ⊘ assembled OK — only funding blocks it (expected on unfunded wallet): {}",
                sim.err.as_deref().unwrap_or("")
            );
        } else {
            failed += 1;
            println!("   ✗ simulate error: {}", sim.err.as_deref().unwrap_or(""));
        }
    }
    println!(
        "\nsummary: {ok} ok, {failed} failed, of {} attempts",
        injections.len()
    );
    Ok(())
}

/// Live AI decision source: ask the TS agent over NATS; `None` on timeout/error so the saga falls
/// back to the deterministic policy (and still emits a visible decision trace).
struct LiveDecisionSource {
    bus: TelemetryBus,
    timeout: Duration,
}

impl DecisionSource for LiveDecisionSource {
    async fn decide(&self, decision_type: DecisionType, context: Value) -> Option<Decision> {
        match self
            .bus
            .request_decision(decision_type, context, self.timeout)
            .await
        {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::debug!(error = %e, "decision request failed; policy fallback");
                None
            }
        }
    }
}

/// Live submitter: assemble (with injection / refresh), sign, and `sendBundle`, returning a tracked
/// attempt. Sets a real-slot give-up watermark so the saga retries deterministically.
struct LiveSubmitter {
    rpc: RpcClient,
    jito: BlockEngineClient,
    payer: solana_sdk::signature::Keypair,
    congestion: f64,
    transfer: u64,
    region: String,
    /// Tip-floor median at run start — lets the classifier infer `fee_too_low` on a low-tip non-land.
    floor_p50: u64,
    /// Captured-then-expired blockhash reused for stale-blockhash injection (fetched once).
    stale: Mutex<Option<BlockhashInfo>>,
    /// Per-base blockhash of the last attempt, so a non-refresh retry can REUSE it (honoring
    /// `AttemptSpec.refresh_blockhash`) instead of always re-fetching.
    blockhash_cache: Mutex<HashMap<String, BlockhashInfo>>,
}

impl LiveSubmitter {
    /// Fetch a blockhash once, wait until it has expired, and cache it for injection reuse.
    async fn stale_blockhash(&self) -> anyhow::Result<BlockhashInfo> {
        let mut guard = self.stale.lock().await;
        if guard.is_none() {
            let bh = self.rpc.latest_blockhash().await?;
            eprintln!("   …waiting for blockhash to expire (guarantees a non-landing)…");
            wait_until_expired(&self.rpc, &bh).await;
            *guard = Some(bh);
        }
        Ok(guard.clone().expect("just set"))
    }
}

impl Submitter for LiveSubmitter {
    async fn submit(&self, spec: &AttemptSpec) -> anyhow::Result<SubmittedBundle> {
        // Real-slot give-up watermark; on a transient RPC failure, OMIT it (rely on the global
        // deadline) rather than collapsing it to slot 0, which would instantly false-fail a healthy
        // bundle and churn paid resubmissions.
        let current_slot = self.rpc.get_slot().await.ok();
        // tip override + blockhash + give-up budget by injection / refresh policy.
        let (tip_override, bh_override, give_up_after) = match spec.injected {
            Some(FaultScenario::LowTip { tip_lamports }) => (Some(tip_lamports), None, 64),
            Some(FaultScenario::BlockhashExpiry) => (None, Some(self.stale_blockhash().await?), 2),
            _ => {
                // Honor the retry policy's refresh flag: when a refresh is NOT required (e.g. a
                // fee-too-low retry while the blockhash is still valid), reuse the base's cached
                // blockhash and only re-price; otherwise fetch a fresh one (override = None).
                // BUT a non-refresh retry can land here long after the first attempt (the saga waits
                // a full give-up window for landing before retrying), by which point the cached
                // blockhash may have expired — reusing it then guarantees a Jito "expired blockhash"
                // 400. So reuse only if it's still valid on-chain; otherwise fall back to a fresh one.
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
                        _ => None, // stale/unknown → fetch fresh
                    },
                    None => None,
                };
                (spec.tip_lamports, reuse, 150)
            }
        };
        let plan = proof::prepare_attempt(
            &self.rpc,
            &self.jito,
            &self.payer,
            self.congestion,
            None,
            spec.attempt_no,
            self.transfer,
            tip_override,
            // Only the low-tip fault bypasses the competitive clamp; the AI/normal tip (also passed as
            // `tip_override`) must be clamped to the policy band so it lands.
            matches!(spec.injected, Some(FaultScenario::LowTip { .. })),
            bh_override,
        )
        .await?;
        // Remember the blockhash actually used so a later non-refresh retry of this base reuses it.
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
                eprintln!("   send rejected ({e}); tracking by signature");
                plan.signature.clone()
            }
        };
        println!(
            "  {} a{}  tip={:>7}  bh={}  sig={}{}",
            spec.base_id,
            spec.attempt_no,
            plan.tip_lamports,
            short(&plan.blockhash),
            short(&plan.signature),
            inject_label(spec.injected),
        );
        Ok(SubmittedBundle {
            bundle_id,
            signature: plan.signature.clone(),
            tip_lamports: plan.tip_lamports,
            tip_account: plan.tip_account.clone(),
            region: self.region.clone(),
            submitted_at: chrono::Utc::now(),
            tip_floor_p50_lamports: self.floor_p50,
            injected: spec.injected,
            base_id: spec.base_id.clone(),
            attempt_no: spec.attempt_no,
            deadline_slot: current_slot.map(|s| s.saturating_add(give_up_after)),
        })
    }

    /// Probe real on-chain / Jito error data for a non-landed bundle so the saga classifies the
    /// failure from observed signals — not the injection tag. Delegates to the shared
    /// [`proof::probe_failure_signals`] (the same probe the product `EngineSubmitter` uses).
    async fn probe_failure(&self, sb: &SubmittedBundle) -> Option<FailureSignals> {
        let bh = self.blockhash_cache.lock().await.get(&sb.base_id).cloned();
        Some(proof::probe_failure_signals(&self.rpc, &self.jito, sb, self.floor_p50, bh).await)
    }
}

/// Live: open one shared stream and run the AI-driven saga — the agent makes a tip decision per
/// bundle and a retry decision on each failure (refresh + re-price + resubmit), emitting
/// Bundle/Lifecycle/Failure/Decision telemetry to NATS + Postgres for the lifecycle-log export.
#[allow(clippy::too_many_arguments)]
async fn run_live(
    config: &Config,
    rpc: RpcClient,
    jito: BlockEngineClient,
    payer: solana_sdk::signature::Keypair,
    congestion: f64,
    transfer: u64,
    floor_p50: u64,
    floor_p75: u64,
    floor_p95: u64,
    injections: &[Option<FaultScenario>],
) -> anyhow::Result<()> {
    // Telemetry sinks — the SAME emitter the engine uses, so export-log reads what we persist.
    let bus = TelemetryBus::connect(&config.nats_url).await?;
    let pg = if config.db_enabled() {
        match PostgresSink::connect(&config.database_url).await {
            Ok(sink) => Some(sink),
            Err(e) => {
                eprintln!("postgres unavailable ({e}); telemetry will publish to NATS only");
                None
            }
        }
    } else {
        eprintln!(
            "DATABASE_URL not set — lifecycle-log export needs Postgres; set it to capture the log"
        );
        None
    };
    let decider = LiveDecisionSource {
        bus: bus.clone(),
        timeout: Duration::from_secs(8),
    };
    let sinks = Sinks::new(bus, pg);

    // ONE Yellowstone stream for the whole run (open before submitting so we miss no early events).
    let endpoint = config
        .yellowstone_endpoint
        .clone()
        .ok_or_else(|| anyhow::anyhow!("YELLOWSTONE_ENDPOINT required for live tracking"))?;
    let payer_pubkey = payer.pubkey().to_string();
    let yc = YellowstoneConfig {
        endpoint,
        x_token: config.yellowstone_x_token.clone(),
        commitment: Commitment::Confirmed,
        channel_capacity: 1024,
        ..Default::default()
    };
    let spec = SubscriptionSpec {
        track_slots: true,
        tx_account_include: vec![payer_pubkey],
        ..Default::default()
    };
    let mut handle = yellowstone::spawn(yc, spec);

    // Real leader-window detection via RPC `getSlotLeaders` → a visible AI submission-timing decision
    // (best-effort; the proof still submits — Jito routes the bundle to the next Jito leader).
    let current_slot = rpc.get_slot().await.unwrap_or(0);
    let leaders = rpc
        .get_slot_leaders(current_slot, 16)
        .await
        .unwrap_or_default();
    let schedule = LeaderSchedule::new(current_slot, leaders);
    let timing_ctx = json!({
        "currentSlot": current_slot,
        "currentLeader": schedule.current_leader(),
        "slotsUntilLeaderChange": schedule.slots_until_leader_change(),
        "congestionScore": congestion,
    });
    if let Some(d) = decider.decide(DecisionType::Timing, timing_ctx).await {
        let ai_hold = d
            .after
            .as_ref()
            .and_then(|a| a.get("hold"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        println!(
            "leader window: current_leader={} next_change_in={:?} → timing: {} ({})",
            schedule.current_leader().unwrap_or("?"),
            schedule.slots_until_leader_change(),
            d.action,
            d.reasoning,
        );
        sinks.emit(&TelemetryEvent::Decision(d)).await;
        // Submission-timing GATE: actually hold (bounded) for a fresher leader window when asked,
        // instead of merely printing the decision.
        let hold = timing_hold_slots(schedule.slots_until_leader_change(), ai_hold, 8);
        if hold > 0 {
            println!("   holding ~{hold} slots for a fresher leader window before submitting…");
            tokio::time::sleep(Duration::from_millis(hold.saturating_mul(420))).await;
        }
    }

    let submitter = LiveSubmitter {
        rpc,
        jito,
        payer,
        congestion,
        transfer,
        region: proof_run::region_from_url(&config.jito_block_engine_url),
        floor_p50,
        stale: Mutex::new(None),
        blockhash_cache: Mutex::new(HashMap::new()),
    };

    // One logical bundle per attempt slot; injected faults sit on the LAST attempts (set up earlier).
    // Give the strategist the full landed-tip distribution, not just P50. The distribution is heavily
    // skewed — P50 sits at the ~1000-lamport Jito noise floor, so tipping at P50 rarely wins
    // inclusion; the P75–P95 band is where bundles reliably land. The agent reasons over all three.
    let base_ctx = json!({
        "congestionScore": congestion,
        "tipFloorP50Lamports": floor_p50,
        "tipFloorP75Lamports": floor_p75,
        "tipFloorP95Lamports": floor_p95,
    });
    let bases: Vec<BaseBundle> = injections
        .iter()
        .enumerate()
        .map(|(i, inj)| BaseBundle {
            base_id: format!("b{:02}", i + 1),
            injected: *inj,
            tip_context: base_ctx.clone(),
        })
        .collect();

    let cfg = SagaConfig {
        max_attempts: 3,
        global_deadline: tokio::time::Instant::now() + Duration::from_secs(180),
    };
    let summary = run_saga(&sinks, &decider, &submitter, bases, &mut handle.rx, cfg).await;
    handle.task.abort();
    let _ = sinks.bus.flush().await;

    println!(
        "\nsummary: {} landed, {} failed, of {} submissions (telemetry persisted; run export-log)",
        summary.landed, summary.failed, summary.total
    );
    Ok(())
}
