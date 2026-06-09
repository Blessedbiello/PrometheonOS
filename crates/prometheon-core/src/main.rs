//! `prometheon-core` — the orchestrator binary.
//!
//! Loads `.env` config, brings up the engine, and runs the orchestration loop. The wiring itself
//! lives in [`prometheon_core::engine`] so it stays testable; this entrypoint is intentionally thin.

use prometheon_core::{engine, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LOG_LEVEL")
                .or_else(|_| tracing_subscriber::EnvFilter::try_new("info"))
                .unwrap(),
        )
        .init();

    let config = Config::from_env()?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        network = config.network.as_str(),
        yellowstone = config.yellowstone_ready(),
        "PrometheonOS — Autonomous Solana Execution Intelligence Engine"
    );

    engine::run(config).await
}
