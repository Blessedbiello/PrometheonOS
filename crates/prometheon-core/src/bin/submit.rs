//! `submit` — the callable product surface as a CLI.
//!
//! ```text
//! # one-shot: submit a self-transfer strategy and print the Receipt JSON
//! NETWORK=mainnet cargo run -p prometheon-core --bin submit -- --transfer-lamports 1 --max-attempts 3
//! # serve the loopback HTTP endpoint (POST /submit → Receipt JSON)
//! NETWORK=mainnet cargo run -p prometheon-core --bin submit -- --serve
//! curl -s 127.0.0.1:9180/submit -d '{"transfer_lamports":1,"max_attempts":3,"deadline_secs":180}'
//! ```
//!
//! Engine-custody: the engine signs with the configured wallet, tips, tracks the lifecycle, and
//! autonomously retries (see `crates/prometheon-core/src/submit.rs`). The HTTP endpoint binds
//! loopback-only and is unauthenticated by design — it signs with a funded wallet, so localhost is
//! the trust boundary (exactly like the Prometheus `/metrics` exporter and the dashboard).

use std::time::Duration;

use prometheon_core::config::Config;
use prometheon_core::submit::{self, SignerSource, SubmitRequest, SubmitStrategy};

fn arg_u64(args: &[String], flag: &str, default: u64) -> u64 {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
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
    let config = Config::from_env()?;

    // `--serve`: run the loopback HTTP endpoint (POST /submit → Receipt JSON).
    if args.iter().any(|a| a == "--serve") {
        let addr = std::env::var("SUBMIT_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:9180".to_string())
            .parse()?;
        eprintln!(
            "PrometheonOS submit endpoint on http://{addr}  (POST /submit · loopback only · network={})",
            config.network.as_str()
        );
        return submit::serve_submit(addr, config).await;
    }

    // One-shot: submit and print the receipt.
    let request = SubmitRequest {
        strategy: SubmitStrategy::SelfTransfer {
            lamports: arg_u64(&args, "--transfer-lamports", 1),
        },
        signer: SignerSource::ConfigWallet,
        max_attempts: arg_u64(&args, "--max-attempts", 3) as u32,
        deadline: Duration::from_secs(arg_u64(&args, "--deadline-secs", 180)),
    };
    let receipt = submit::submit(&config, request).await?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
