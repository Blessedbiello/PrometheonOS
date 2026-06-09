//! Jito Block Engine HTTP client (async I/O).
//!
//! A thin wrapper over the tested seams in [`crate::jsonrpc`] and the parsers in
//! [`crate::status`] / [`crate::tip_floor`] / [`crate::tip_accounts`]. Responsibilities:
//! - JSON-RPC calls: `getTipAccounts`, `sendBundle`, `getInflightBundleStatuses`,
//!   `getBundleStatuses`.
//! - Tip-floor fetch from the separate `bundles.jito.wtf` host.
//! - **Rate-limit pacing** (~1 req/s/IP/region) — a minimum inter-request interval.
//! - **Region failover** — retry a request against fallback regions on retryable failures (429/5xx).
//! - Optional `x-jito-auth` UUID header for higher limits.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::jsonrpc::{
    bundle_ids_params, is_retryable_status, jsonrpc_body, send_bundle_params, unwrap_result,
    JsonRpcError,
};
use crate::status::{
    parse_bundle_statuses, parse_inflight_statuses, BundleStatuses, InflightStatuses,
};
use crate::tip_accounts::{parse_tip_accounts, TipAccounts};
use crate::tip_floor::TipFloor;

/// Block Engine client configuration.
#[derive(Clone, Debug)]
pub struct BlockEngineConfig {
    /// Primary regional base, e.g. `https://ny.mainnet.block-engine.jito.wtf`.
    pub base_url: String,
    /// Other regional bases tried on retryable failure (closest-first).
    pub fallback_urls: Vec<String>,
    /// Tip-floor endpoint (separate host), e.g. `https://bundles.jito.wtf/api/v1/bundles/tip_floor`.
    pub tip_floor_url: String,
    /// Optional UUID for higher rate limits (sent as `x-jito-auth`).
    pub auth_uuid: Option<String>,
    /// Minimum interval between requests to one region (pacing under the ~1 rps limit).
    pub min_request_interval: Duration,
    /// Max attempts across regions on retryable failures.
    pub max_attempts: u32,
    pub request_timeout: Duration,
}

impl Default for BlockEngineConfig {
    fn default() -> Self {
        Self {
            base_url: "https://ny.mainnet.block-engine.jito.wtf".to_string(),
            fallback_urls: vec![
                "https://amsterdam.mainnet.block-engine.jito.wtf".to_string(),
                "https://frankfurt.mainnet.block-engine.jito.wtf".to_string(),
            ],
            tip_floor_url: "https://bundles.jito.wtf/api/v1/bundles/tip_floor".to_string(),
            auth_uuid: None,
            min_request_interval: Duration::from_millis(1100),
            max_attempts: 3,
            request_timeout: Duration::from_secs(10),
        }
    }
}

/// Errors from Block Engine calls.
#[derive(Debug, thiserror::Error)]
pub enum JitoError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    JsonRpc(#[from] JsonRpcError),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("rate limited (429) after exhausting retries/regions")]
    RateLimited,
    #[error("all regions failed; last status {0}")]
    AllRegionsFailed(u16),
}

/// Async Jito Block Engine client.
#[derive(Clone)]
pub struct BlockEngineClient {
    http: reqwest::Client,
    config: BlockEngineConfig,
    /// Timestamp of the last request, for pacing (shared across clones).
    last_request: Arc<Mutex<Option<Instant>>>,
}

impl BlockEngineClient {
    /// Build a client. Fails only if the underlying HTTP client cannot be constructed.
    pub fn new(config: BlockEngineConfig) -> Result<Self, JitoError> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()?;
        Ok(Self {
            http,
            config,
            last_request: Arc::new(Mutex::new(None)),
        })
    }

    /// All regional bases, primary first.
    fn regions(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.config.base_url).chain(self.config.fallback_urls.iter())
    }

    /// Sleep until the pacing interval has elapsed since the last request, then mark now.
    async fn pace(&self) {
        let mut last = self.last_request.lock().await;
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < self.config.min_request_interval {
                tokio::time::sleep(self.config.min_request_interval - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }

    /// POST a JSON-RPC body to `<region><path>`, trying regions on retryable statuses.
    /// Returns the response body text on HTTP success.
    async fn post_rpc(
        &self,
        path: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<String, JitoError> {
        let body = jsonrpc_body(1, method, params);
        let mut last_status = 0u16;
        let max_attempts = self.config.max_attempts as usize;

        for region in self.regions().take(max_attempts) {
            self.pace().await;

            let url = format!("{region}{path}");
            let mut req = self.http.post(&url).json(&body);
            if let Some(uuid) = &self.config.auth_uuid {
                req = req.header("x-jito-auth", uuid);
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if status == 200 {
                        return Ok(resp.text().await?);
                    }
                    last_status = status;
                    if !is_retryable_status(status) {
                        return Err(JitoError::AllRegionsFailed(status));
                    }
                    // retryable → fall through to next region
                }
                Err(e) if e.is_timeout() || e.is_connect() => {
                    last_status = 503;
                    // try next region
                }
                Err(e) => return Err(JitoError::Http(e)),
            }
        }

        if last_status == 429 {
            Err(JitoError::RateLimited)
        } else {
            Err(JitoError::AllRegionsFailed(last_status))
        }
    }

    /// `getTipAccounts` → the 8 live tip accounts.
    pub async fn get_tip_accounts(&self) -> Result<TipAccounts, JitoError> {
        let body = self
            .post_rpc(
                "/api/v1/getTipAccounts",
                "getTipAccounts",
                serde_json::json!([]),
            )
            .await?;
        parse_tip_accounts(&body).map_err(|e| JitoError::Parse(e.to_string()))
    }

    /// `sendBundle` → the bundle id. `txs_base64` are fully-signed, base64-encoded transactions.
    pub async fn send_bundle(&self, txs_base64: &[String]) -> Result<String, JitoError> {
        let body = self
            .post_rpc(
                "/api/v1/bundles",
                "sendBundle",
                send_bundle_params(txs_base64),
            )
            .await?;
        let result = unwrap_result(&body)?;
        result
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| JitoError::Parse("sendBundle result was not a string".into()))
    }

    /// `getInflightBundleStatuses` → fast in-memory status (last ~5 min).
    pub async fn get_inflight_bundle_statuses(
        &self,
        bundle_ids: &[String],
    ) -> Result<InflightStatuses, JitoError> {
        let body = self
            .post_rpc(
                "/api/v1/getInflightBundleStatuses",
                "getInflightBundleStatuses",
                bundle_ids_params(bundle_ids),
            )
            .await?;
        let result = unwrap_result(&body)?;
        parse_inflight_statuses(&result.to_string()).map_err(|e| JitoError::Parse(e.to_string()))
    }

    /// `getBundleStatuses` → on-chain confirmation detail.
    pub async fn get_bundle_statuses(
        &self,
        bundle_ids: &[String],
    ) -> Result<BundleStatuses, JitoError> {
        let body = self
            .post_rpc(
                "/api/v1/getBundleStatuses",
                "getBundleStatuses",
                bundle_ids_params(bundle_ids),
            )
            .await?;
        let result = unwrap_result(&body)?;
        parse_bundle_statuses(&result.to_string()).map_err(|e| JitoError::Parse(e.to_string()))
    }

    /// Fetch the live tip floor (separate host; a plain GET returning a single-element array).
    pub async fn get_tip_floor(&self) -> Result<TipFloor, JitoError> {
        self.pace().await;
        let body = self
            .http
            .get(&self.config.tip_floor_url)
            .send()
            .await?
            .text()
            .await?;
        TipFloor::from_response_json(&body).map_err(|e| JitoError::Parse(e.to_string()))
    }

    /// `getNextScheduledLeader` → the next Jito-Solana leader slot (the submission window). The exact
    /// path/shape should be confirmed against the live Block Engine; the parser is tolerant.
    pub async fn get_next_scheduled_leader(&self) -> Result<crate::NextLeader, JitoError> {
        let body = self
            .post_rpc(
                "/api/v1/getNextScheduledLeader",
                "getNextScheduledLeader",
                serde_json::json!([]),
            )
            .await?;
        crate::parse_next_scheduled_leader(&body).map_err(JitoError::Parse)
    }
}
