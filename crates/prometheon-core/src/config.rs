//! Engine configuration, resolved from the environment (`.env`).
//!
//! `NETWORK` selects the *active set*: on `mainnet` the `*_MAINNET` variants of the RPC, Jito Block
//! Engine, and wallet path are used; otherwise the testnet/devnet values. Keeping one active set
//! (rather than threading a network flag through every call site) means the rest of the engine is
//! network-agnostic — it just reads `config.rpc_url` / `config.jito_block_engine_url`.
//!
//! The selection + defaulting logic is pure ([`resolve`]) and unit-tested; [`Config::from_env`] is
//! the thin Figment glue that reads process env into [`RawConfig`].

use anyhow::{anyhow, Context};
use figment::{providers::Env, Figment};
use serde::Deserialize;

use prometheon_types::Commitment;

/// Which Solana cluster the engine targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Devnet,
    Testnet,
    Mainnet,
}

impl Network {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "mainnet" | "mainnet-beta" => Network::Mainnet,
            "devnet" => Network::Devnet,
            _ => Network::Testnet,
        }
    }

    pub fn is_mainnet(self) -> bool {
        matches!(self, Network::Mainnet)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Network::Devnet => "devnet",
            Network::Testnet => "testnet",
            Network::Mainnet => "mainnet",
        }
    }
}

fn parse_commitment(s: &str) -> Commitment {
    match s.trim().to_lowercase().as_str() {
        "processed" => Commitment::Processed,
        "finalized" => Commitment::Finalized,
        _ => Commitment::Confirmed,
    }
}

/// Raw environment shape — every field optional. Resolution applies defaults + network selection.
#[derive(Debug, Default, Deserialize)]
pub struct RawConfig {
    pub network: Option<String>,
    pub rpc_url: Option<String>,
    pub rpc_url_mainnet: Option<String>,
    pub rpc_ws_url: Option<String>,
    pub yellowstone_endpoint: Option<String>,
    pub yellowstone_x_token: Option<String>,
    pub yellowstone_commitment: Option<String>,
    pub jito_block_engine_url: Option<String>,
    pub jito_block_engine_url_mainnet: Option<String>,
    pub jito_tip_floor_url: Option<String>,
    pub jito_auth_uuid: Option<String>,
    pub wallet_keypair_path: Option<String>,
    pub wallet_keypair_path_mainnet: Option<String>,
    pub nats_url: Option<String>,
    pub database_url: Option<String>,
    pub prometheus_metrics_addr: Option<String>,
    pub ingest_channel_capacity: Option<usize>,
    pub llm_provider: Option<String>,
}

/// Fully-resolved engine configuration (active set chosen by [`Network`]).
#[derive(Debug, Clone)]
pub struct Config {
    pub network: Network,
    pub rpc_url: String,
    pub rpc_ws_url: Option<String>,
    pub yellowstone_endpoint: Option<String>,
    pub yellowstone_x_token: Option<String>,
    pub yellowstone_commitment: Commitment,
    pub jito_block_engine_url: String,
    pub jito_tip_floor_url: String,
    pub jito_auth_uuid: Option<String>,
    pub wallet_keypair_path: String,
    pub nats_url: String,
    /// Empty when no DB is configured (Postgres sink disabled).
    pub database_url: String,
    pub prometheus_metrics_addr: String,
    pub ingest_channel_capacity: usize,
    pub llm_provider: String,
}

/// Trim and treat blank strings as absent — `.env` keys are often present-but-empty.
fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

const DEFAULT_TIP_FLOOR: &str = "https://bundles.jito.wtf/api/v1/bundles/tip_floor";
const DEFAULT_TESTNET_RPC: &str = "https://api.testnet.solana.com";
const DEFAULT_TESTNET_JITO: &str = "https://testnet.block-engine.jito.wtf";
const DEFAULT_TESTNET_WALLET: &str = "./wallets/payer.testnet.json";
const DEFAULT_NATS: &str = "nats://localhost:4222";
const DEFAULT_METRICS_ADDR: &str = "127.0.0.1:9100";
const DEFAULT_CHANNEL_CAPACITY: usize = 8192;

/// Resolve raw env values into an active [`Config`], applying network selection + defaults.
///
/// On `mainnet` the `*_MAINNET` endpoints are *required* (no silent fall-through to testnet, which
/// would be a dangerous footgun — submitting "mainnet" bundles against a testnet engine).
pub fn resolve(raw: RawConfig) -> anyhow::Result<Config> {
    let network = Network::parse(raw.network.as_deref().unwrap_or("testnet"));

    let rpc_url = if network.is_mainnet() {
        non_empty(raw.rpc_url_mainnet).context("RPC_URL_MAINNET required when NETWORK=mainnet")?
    } else {
        non_empty(raw.rpc_url).unwrap_or_else(|| DEFAULT_TESTNET_RPC.to_string())
    };

    let jito_block_engine_url = if network.is_mainnet() {
        non_empty(raw.jito_block_engine_url_mainnet)
            .context("JITO_BLOCK_ENGINE_URL_MAINNET required when NETWORK=mainnet")?
    } else {
        non_empty(raw.jito_block_engine_url).unwrap_or_else(|| DEFAULT_TESTNET_JITO.to_string())
    };

    let wallet_keypair_path = if network.is_mainnet() {
        non_empty(raw.wallet_keypair_path_mainnet)
            .context("WALLET_KEYPAIR_PATH_MAINNET required when NETWORK=mainnet")?
    } else {
        non_empty(raw.wallet_keypair_path).unwrap_or_else(|| DEFAULT_TESTNET_WALLET.to_string())
    };

    Ok(Config {
        network,
        rpc_url,
        rpc_ws_url: non_empty(raw.rpc_ws_url),
        yellowstone_endpoint: non_empty(raw.yellowstone_endpoint),
        yellowstone_x_token: non_empty(raw.yellowstone_x_token),
        yellowstone_commitment: parse_commitment(
            raw.yellowstone_commitment.as_deref().unwrap_or("confirmed"),
        ),
        jito_block_engine_url,
        jito_tip_floor_url: non_empty(raw.jito_tip_floor_url)
            .unwrap_or_else(|| DEFAULT_TIP_FLOOR.to_string()),
        jito_auth_uuid: non_empty(raw.jito_auth_uuid),
        wallet_keypair_path,
        nats_url: non_empty(raw.nats_url).unwrap_or_else(|| DEFAULT_NATS.to_string()),
        database_url: non_empty(raw.database_url).unwrap_or_default(),
        prometheus_metrics_addr: non_empty(raw.prometheus_metrics_addr)
            .unwrap_or_else(|| DEFAULT_METRICS_ADDR.to_string()),
        ingest_channel_capacity: raw
            .ingest_channel_capacity
            .unwrap_or(DEFAULT_CHANNEL_CAPACITY),
        llm_provider: non_empty(raw.llm_provider).unwrap_or_else(|| "anthropic".to_string()),
    })
}

impl Config {
    /// Load from the process environment (after the caller has loaded `.env`).
    pub fn from_env() -> anyhow::Result<Config> {
        let raw: RawConfig = Figment::new()
            .merge(Env::raw())
            .extract()
            .map_err(|e| anyhow!("config: {e}"))?;
        resolve(raw)
    }

    /// True when the Yellowstone stream is configured (endpoint present).
    pub fn yellowstone_ready(&self) -> bool {
        self.yellowstone_endpoint.is_some()
    }

    /// True when a Postgres/Timescale sink is configured.
    pub fn db_enabled(&self) -> bool {
        !self.database_url.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_parse_is_case_insensitive_and_defaults_to_testnet() {
        assert_eq!(Network::parse("mainnet"), Network::Mainnet);
        assert_eq!(Network::parse("MAINNET-BETA"), Network::Mainnet);
        assert_eq!(Network::parse("Devnet"), Network::Devnet);
        assert_eq!(Network::parse("testnet"), Network::Testnet);
        assert_eq!(Network::parse("nonsense"), Network::Testnet);
    }

    #[test]
    fn testnet_defaults_fill_in_when_absent() {
        let cfg = resolve(RawConfig::default()).unwrap();
        assert_eq!(cfg.network, Network::Testnet);
        assert_eq!(cfg.rpc_url, DEFAULT_TESTNET_RPC);
        assert_eq!(cfg.jito_block_engine_url, DEFAULT_TESTNET_JITO);
        assert_eq!(cfg.wallet_keypair_path, DEFAULT_TESTNET_WALLET);
        assert_eq!(cfg.nats_url, DEFAULT_NATS);
        assert_eq!(cfg.ingest_channel_capacity, DEFAULT_CHANNEL_CAPACITY);
        assert_eq!(cfg.yellowstone_commitment, Commitment::Confirmed);
        assert_eq!(cfg.llm_provider, "anthropic");
        assert!(!cfg.yellowstone_ready());
        assert!(!cfg.db_enabled());
    }

    #[test]
    fn mainnet_selects_the_mainnet_active_set() {
        let raw = RawConfig {
            network: Some("mainnet".into()),
            rpc_url: Some("https://api.testnet.solana.com".into()), // must NOT be picked
            rpc_url_mainnet: Some("https://fra.rpc.solinfra.dev/sol?api_key=x".into()),
            jito_block_engine_url: Some(DEFAULT_TESTNET_JITO.into()),
            jito_block_engine_url_mainnet: Some("https://ny.mainnet.block-engine.jito.wtf".into()),
            wallet_keypair_path: Some(DEFAULT_TESTNET_WALLET.into()),
            wallet_keypair_path_mainnet: Some("./wallets/payer.mainnet.json".into()),
            ..Default::default()
        };
        let cfg = resolve(raw).unwrap();
        assert_eq!(cfg.network, Network::Mainnet);
        assert_eq!(cfg.rpc_url, "https://fra.rpc.solinfra.dev/sol?api_key=x");
        assert_eq!(
            cfg.jito_block_engine_url,
            "https://ny.mainnet.block-engine.jito.wtf"
        );
        assert_eq!(cfg.wallet_keypair_path, "./wallets/payer.mainnet.json");
    }

    #[test]
    fn mainnet_without_mainnet_rpc_is_an_error() {
        let raw = RawConfig {
            network: Some("mainnet".into()),
            rpc_url: Some(DEFAULT_TESTNET_RPC.into()),
            ..Default::default()
        };
        let err = resolve(raw).unwrap_err().to_string();
        assert!(err.contains("RPC_URL_MAINNET"), "unexpected error: {err}");
    }

    #[test]
    fn blank_env_values_are_treated_as_absent() {
        let raw = RawConfig {
            yellowstone_endpoint: Some("   ".into()),
            yellowstone_x_token: Some(String::new()),
            jito_auth_uuid: Some("".into()),
            database_url: Some("  ".into()),
            ..Default::default()
        };
        let cfg = resolve(raw).unwrap();
        assert_eq!(cfg.yellowstone_endpoint, None);
        assert_eq!(cfg.yellowstone_x_token, None);
        assert_eq!(cfg.jito_auth_uuid, None);
        assert!(!cfg.db_enabled());
    }

    #[test]
    fn commitment_parsing() {
        let mk = |c: &str| {
            resolve(RawConfig {
                yellowstone_commitment: Some(c.into()),
                ..Default::default()
            })
            .unwrap()
            .yellowstone_commitment
        };
        assert_eq!(mk("processed"), Commitment::Processed);
        assert_eq!(mk("CONFIRMED"), Commitment::Confirmed);
        assert_eq!(mk("finalized"), Commitment::Finalized);
        assert_eq!(mk("weird"), Commitment::Confirmed);
    }
}
