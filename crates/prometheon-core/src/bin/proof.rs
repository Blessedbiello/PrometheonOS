//! Proof runner — build and either **dry-run simulate** (free) or **live submit + stream-track** N
//! Jito bundles on the configured network.
//!
//! ```text
//! # dry-run (free): validate the whole assembly path against live mainnet, no broadcast
//! NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --count 12
//! # live (needs a funded mainnet wallet): submit + stream-confirm the lifecycle log
//! NETWORK=mainnet cargo run -p prometheon-core --bin proof -- --live --count 12
//! ```
//!
//! Dry-run on an *unfunded* wallet is expected to report "assembled OK — only funding blocks it":
//! that proves blockhash, tip-account selection, live-data tip, compute budget, signing, and the
//! RPC round-trip are all correct, leaving only `sendBundle` (and the SOL) for the live run.

use std::time::Duration;

use prometheon_bundle::{BlockEngineClient, BlockEngineConfig, Percentile, TipFloor};
use prometheon_core::{
    config::Config,
    proof::{self, PendingBundles},
    rpc::RpcClient,
    wallet,
};
use prometheon_ingest::yellowstone::{self, IngestMessage, SubscriptionSpec, YellowstoneConfig};
use prometheon_types::Commitment;
use solana_sdk::signer::Signer;

fn arg_u64(args: &[String], flag: &str, default: u64) -> u64 {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
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
        "PrometheonOS proof — network={} mode={} count={}",
        config.network.as_str(),
        if live { "LIVE" } else { "DRY-RUN" },
        count
    );
    println!("payer: {}", payer.pubkey());

    let floor = jito.get_tip_floor().await?;
    let congestion = congestion_proxy(&floor);
    println!(
        "tip floor p50={} ema50={} lamports -> congestion≈{:.3}\n",
        floor.percentile_lamports(Percentile::P50),
        floor.ema50_lamports(),
        congestion
    );

    let mut landed = 0u32;
    let mut failed = 0u32;

    for attempt in 1..=count {
        let plan = proof::prepare_attempt(&rpc, &jito, &payer, congestion, None, attempt, transfer)
            .await?;
        println!(
            "#{:<2} tip={:>7} lamports  cu_price={:<5}  acct={}  bh={}  sig={}",
            attempt,
            plan.tip_lamports,
            plan.cu_price_micro,
            short(&plan.tip_account),
            short(&plan.blockhash),
            short(&plan.signature)
        );

        if live {
            match submit_and_track(&jito, &config, &payer, &plan).await {
                Ok(true) => {
                    landed += 1;
                    println!("   ✓ landed");
                }
                Ok(false) => {
                    failed += 1;
                    println!("   ✗ did not land");
                }
                Err(e) => {
                    failed += 1;
                    println!("   ✗ error: {e}");
                }
            }
        } else {
            let sim = rpc.simulate_transaction(&plan.tx_base64).await?;
            if sim.succeeded() {
                landed += 1;
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
    }

    println!("\nsummary: {landed} ok, {failed} failed, of {count} attempts");
    Ok(())
}

/// Live: submit the bundle, then stream-confirm its lifecycle (tx-status → slot-status) to terminal
/// or timeout. Returns whether it landed (reached at least `confirmed`).
async fn submit_and_track(
    jito: &BlockEngineClient,
    config: &Config,
    payer: &solana_sdk::signature::Keypair,
    plan: &proof::AttemptPlan,
) -> anyhow::Result<bool> {
    let bundle_id = jito
        .send_bundle(std::slice::from_ref(&plan.tx_base64))
        .await?;
    let submitted_at = chrono::Utc::now();

    let mut pending = PendingBundles::new();
    pending.track(
        bundle_id.clone(),
        vec![plan.signature.clone()],
        plan.tip_lamports,
        submitted_at,
    );

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

    // Track until the bundle is terminal or the blockhash window (~90s) closes.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, handle.rx.recv()).await {
            Ok(Some(IngestMessage::Transaction(tx))) => {
                pending.on_tx_status(&tx.signature, tx.slot, tx.failed, tx.ts);
            }
            Ok(Some(IngestMessage::Slot { update, .. })) => {
                pending.on_slot_status(update.slot, update.status, update.ts);
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
        if pending.all_terminal() {
            break;
        }
    }
    handle.task.abort();

    let b = pending
        .get(&bundle_id)
        .ok_or_else(|| anyhow::anyhow!("bundle vanished from tracker"))?;
    Ok(b.lifecycle.is_success() || b.landed_slot.is_some())
}
