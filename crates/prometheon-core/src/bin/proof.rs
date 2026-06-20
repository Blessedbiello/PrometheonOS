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

use std::time::Duration;

use prometheon_bundle::{BlockEngineClient, BlockEngineConfig, Percentile, TipFloor};
use prometheon_core::{
    config::Config,
    leader::LeaderWindow,
    proof,
    proof_run::{self, SubmittedBundle},
    rpc::{BlockhashInfo, RpcClient},
    sinks::Sinks,
    wallet,
};
use prometheon_faultinject::FaultScenario;
use prometheon_ingest::yellowstone::{self, SubscriptionSpec, YellowstoneConfig};
use prometheon_telemetry::{PostgresSink, TelemetryBus};
use prometheon_types::Commitment;
use solana_sdk::signer::Signer;

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
    println!(
        "tip floor p50={} ema50={} lamports -> congestion≈{:.3}\n",
        floor_p50,
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
            &rpc,
            &jito,
            &payer,
            congestion,
            transfer,
            floor_p50,
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

/// Live: open one shared stream, submit every bundle (incl. injected failures), then track them all
/// to terminal/timeout while emitting telemetry to NATS + Postgres for the lifecycle-log export.
#[allow(clippy::too_many_arguments)]
async fn run_live(
    config: &Config,
    rpc: &RpcClient,
    jito: &BlockEngineClient,
    payer: &solana_sdk::signature::Keypair,
    congestion: f64,
    transfer: u64,
    floor_p50: u64,
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
    let sinks = Sinks::new(bus, pg);

    // ONE Yellowstone stream for the whole run (open before submitting so we miss no early events).
    let endpoint = config
        .yellowstone_endpoint
        .clone()
        .ok_or_else(|| anyhow::anyhow!("YELLOWSTONE_ENDPOINT required for live tracking"))?;
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

    let region = proof_run::region_from_url(&config.jito_block_engine_url);

    // Capture a blockhash once for any stale-blockhash injection.
    let needs_expiry = injections
        .iter()
        .any(|i| matches!(i, Some(FaultScenario::BlockhashExpiry)));
    let expiry_source = if needs_expiry {
        Some(rpc.latest_blockhash().await?)
    } else {
        None
    };

    let mut submitted = Vec::new();
    for (i, inj) in injections.iter().enumerate() {
        let attempt = (i + 1) as u32;
        let (tip_override, bh_override) = match inj {
            Some(FaultScenario::LowTip { tip_lamports }) => (Some(*tip_lamports), None),
            Some(FaultScenario::BlockhashExpiry) => {
                let src = expiry_source.clone().expect("captured when needs_expiry");
                println!("   …waiting for blockhash to expire (guarantees a non-landing)…");
                wait_until_expired(rpc, &src).await;
                (None, Some(src))
            }
            _ => (None, None),
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
            bh_override,
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

        // Send; an expired/invalid bundle may be rejected outright — still track it (by signature)
        // so it appears in the log as a classified failure.
        let bundle_id = match jito
            .send_bundle(std::slice::from_ref(&plan.tx_base64))
            .await
        {
            Ok(id) => id,
            Err(e) => {
                println!("   send rejected ({e}); tracking as failure");
                plan.signature.clone()
            }
        };

        submitted.push(SubmittedBundle {
            bundle_id,
            signature: plan.signature.clone(),
            tip_lamports: plan.tip_lamports,
            tip_account: plan.tip_account.clone(),
            region: region.clone(),
            submitted_at: chrono::Utc::now(),
            tip_floor_p50_lamports: floor_p50,
            injected: *inj,
        });
    }

    // Track every bundle on the shared stream until terminal or the blockhash window closes.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let summary = proof_run::track_and_emit(&sinks, submitted, &mut handle.rx, deadline).await;
    handle.task.abort();
    let _ = sinks.bus.flush().await;

    println!(
        "\nsummary: {} landed, {} failed, of {} attempts (telemetry persisted; run export-log)",
        summary.landed, summary.failed, summary.total
    );
    Ok(())
}
