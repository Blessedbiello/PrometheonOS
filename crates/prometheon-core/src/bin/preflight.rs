//! Preflight connectivity checker.
//!
//! One command to answer "is my infrastructure ready?" — it validates each external dependency the
//! engine needs and prints a clear ✓/✗ report. Run after filling `.env` (especially the SolInfra
//! Yellowstone endpoint):
//!
//! ```text
//! cargo run -p prometheon-core --bin preflight
//! ```
//!
//! Checks: Solana RPC health + wallet balance, Jito tip-floor reachability, and (if configured) a
//! live Yellowstone slot stream. Exits non-zero if any required check fails.

use std::time::Duration;

use prometheon_bundle::{BlockEngineClient, BlockEngineConfig};
use prometheon_ingest::yellowstone::{self, IngestMessage, SubscriptionSpec, YellowstoneConfig};
use prometheon_types::Commitment;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn pass(label: &str, detail: impl std::fmt::Display) {
    println!("  \x1b[32m✓\x1b[0m {label}: {detail}");
}
fn fail(label: &str, detail: impl std::fmt::Display) {
    println!("  \x1b[31m✗\x1b[0m {label}: {detail}");
}
fn skip(label: &str, detail: impl std::fmt::Display) {
    println!("  \x1b[33m–\x1b[0m {label}: {detail}");
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let mut ok = true;

    let network = env("NETWORK").unwrap_or_else(|| "testnet".into());
    println!("PrometheonOS preflight — network: {network}\n");

    // ── 1. Solana RPC ─────────────────────────────────────────────────────────────────────────
    println!("Solana RPC:");
    let rpc_url = env("RPC_URL").unwrap_or_else(|| "https://api.testnet.solana.com".into());
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("http client");

    match rpc_call(&http, &rpc_url, "getHealth", serde_json::json!([])).await {
        Ok(v) if v.get("result").is_some() => pass("getHealth", &rpc_url),
        Ok(v) => {
            fail("getHealth", format!("unexpected response: {v}"));
            ok = false;
        }
        Err(e) => {
            fail("getHealth", e);
            ok = false;
        }
    }

    // Wallet balance (read the keypair file's pubkey via the wallet path is overkill here; we ask
    // the user-visible address indirectly by reading the configured pubkey if present).
    if let Some(pubkey) = wallet_pubkey() {
        match rpc_call(
            &http,
            &rpc_url,
            "getBalance",
            serde_json::json!([pubkey, { "commitment": "confirmed" }]),
        )
        .await
        {
            Ok(v) => {
                let lamports = v["result"]["value"].as_u64().unwrap_or(0);
                let sol = lamports as f64 / 1e9;
                if lamports == 0 {
                    fail(
                        "wallet balance",
                        format!("{pubkey} has 0 SOL — fund via faucet (see README)"),
                    );
                    ok = false;
                } else {
                    pass("wallet balance", format!("{sol} SOL ({pubkey})"));
                }
            }
            Err(e) => {
                fail("getBalance", e);
                ok = false;
            }
        }
    } else {
        skip("wallet balance", "no wallet pubkey resolved");
    }

    // ── 2. Jito tip floor ─────────────────────────────────────────────────────────────────────
    println!("\nJito Block Engine:");
    let cfg = BlockEngineConfig {
        tip_floor_url: env("JITO_TIP_FLOOR_URL")
            .unwrap_or_else(|| "https://bundles.jito.wtf/api/v1/bundles/tip_floor".into()),
        ..Default::default()
    };
    match BlockEngineClient::new(cfg) {
        Ok(client) => match client.get_tip_floor().await {
            Ok(floor) => pass(
                "tip floor",
                format!(
                    "p50={} lamports, ema50={} lamports",
                    floor.percentile_lamports(prometheon_bundle::Percentile::P50),
                    floor.ema50_lamports()
                ),
            ),
            Err(e) => {
                fail("tip floor", e);
                ok = false;
            }
        },
        Err(e) => {
            fail("client", e);
            ok = false;
        }
    }

    // ── 3. Yellowstone gRPC (optional until SolInfra endpoint is set) ──────────────────────────
    println!("\nYellowstone gRPC:");
    match env("YELLOWSTONE_ENDPOINT") {
        None => skip(
            "stream",
            "YELLOWSTONE_ENDPOINT not set — claim SolInfra credits and fill .env",
        ),
        Some(endpoint) => {
            let config = YellowstoneConfig {
                endpoint,
                x_token: env("YELLOWSTONE_X_TOKEN"),
                commitment: Commitment::Confirmed,
                channel_capacity: 1024,
                ..Default::default()
            };
            let spec = SubscriptionSpec {
                track_slots: true,
                ..Default::default()
            };
            let mut handle = yellowstone::spawn(config, spec);
            // Wait up to 20s for the first slot update.
            let got = tokio::time::timeout(Duration::from_secs(20), async {
                while let Some(msg) = handle.rx.recv().await {
                    if let IngestMessage::Slot { update, .. } = msg {
                        return Some(update.slot);
                    }
                }
                None
            })
            .await;
            handle.task.abort();
            match got {
                Ok(Some(slot)) => pass("stream", format!("received slot {slot}")),
                Ok(None) => {
                    fail("stream", "stream closed before any slot");
                    ok = false;
                }
                Err(_) => {
                    fail("stream", "no slot within 20s (check endpoint / x-token)");
                    ok = false;
                }
            }
        }
    }

    println!();
    if ok {
        println!("\x1b[32mPreflight OK\x1b[0m");
    } else {
        println!("\x1b[31mPreflight FAILED — see ✗ above\x1b[0m");
        std::process::exit(1);
    }
}

/// Make a JSON-RPC 2.0 call to an RPC endpoint and return the parsed response.
async fn rpc_call(
    http: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let body = serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
    let resp = http.post(url).json(&body).send().await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {status}");
    }
    Ok(resp.json().await?)
}

/// Resolve the wallet pubkey from the configured keypair file, if readable.
///
/// Reads the standard Solana keypair JSON (a 64-byte array) and derives the base58 pubkey from the
/// last 32 bytes (the ed25519 public key half), avoiding a solana-keypair dependency here.
fn wallet_pubkey() -> Option<String> {
    let path = env("WALLET_KEYPAIR_PATH")?;
    let bytes = std::fs::read_to_string(&path).ok()?;
    let arr: Vec<u8> = serde_json::from_str(&bytes).ok()?;
    if arr.len() != 64 {
        return None;
    }
    Some(bs58::encode(&arr[32..64]).into_string())
}
