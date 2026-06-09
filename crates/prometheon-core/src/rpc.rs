//! Minimal Solana JSON-RPC client — the secondary cross-check alongside the Yellowstone stream.
//!
//! The engine confirms landing via the **stream** (mandatory, per the bounty); RPC is the
//! cross-check and the source of the things the stream doesn't carry: a fresh blockhash to build
//! against, the current block height (to measure blockhash expiry by *block height*, not wall
//! clock — see README Q2), and `isBlockhashValid` to decide rebroadcast vs. rebuild.
//!
//! The response *parsers* are pure and unit-tested with fixtures; the methods are thin HTTP.

use std::time::Duration;

use serde_json::{json, Value};

/// A fresh blockhash plus the last block height at which it is still valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockhashInfo {
    pub blockhash: String,
    pub last_valid_block_height: u64,
}

impl BlockhashInfo {
    /// Blocks of validity remaining given the current block height (0 once expired).
    pub fn blocks_remaining(&self, current_block_height: u64) -> u64 {
        self.last_valid_block_height
            .saturating_sub(current_block_height)
    }
}

/// A thin JSON-RPC client over one endpoint.
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
}

impl RpcClient {
    pub fn new(url: impl Into<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            http,
            url: url.into(),
        })
    }

    async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let resp = self.http.post(&self.url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("RPC {method} HTTP {status}");
        }
        Ok(resp.json().await?)
    }

    /// `getLatestBlockhash` at `confirmed` (never `finalized` for time-sensitive tx — README Q2).
    pub async fn latest_blockhash(&self) -> anyhow::Result<BlockhashInfo> {
        let v = self
            .call("getLatestBlockhash", json!([{ "commitment": "confirmed" }]))
            .await?;
        parse_latest_blockhash(&v)
    }

    /// `getBlockHeight` at `confirmed` — the budget against which blockhash expiry is measured.
    pub async fn block_height(&self) -> anyhow::Result<u64> {
        let v = self
            .call("getBlockHeight", json!([{ "commitment": "confirmed" }]))
            .await?;
        parse_block_height(&v)
    }

    /// `isBlockhashValid` — decides rebroadcast (still valid) vs. rebuild (expired).
    pub async fn is_blockhash_valid(&self, blockhash: &str) -> anyhow::Result<bool> {
        let v = self
            .call(
                "isBlockhashValid",
                json!([blockhash, { "commitment": "confirmed" }]),
            )
            .await?;
        parse_is_blockhash_valid(&v)
    }

    /// `simulateTransaction` — the free dry-run: confirm a signed, base64 tx would execute against
    /// current cluster state without broadcasting (or paying) anything.
    pub async fn simulate_transaction(&self, tx_base64: &str) -> anyhow::Result<SimulationResult> {
        let v = self
            .call(
                "simulateTransaction",
                json!([
                    tx_base64,
                    { "encoding": "base64", "sigVerify": true, "commitment": "confirmed", "replaceRecentBlockhash": false }
                ]),
            )
            .await?;
        Ok(parse_simulation(&v))
    }
}

/// Outcome of a `simulateTransaction` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationResult {
    /// `None` if the tx would succeed; otherwise the stringified on-chain error.
    pub err: Option<String>,
    pub logs: Vec<String>,
    pub units_consumed: Option<u64>,
}

impl SimulationResult {
    pub fn succeeded(&self) -> bool {
        self.err.is_none()
    }

    /// True if the only thing blocking the tx is funding — the signal we expect on the *unfunded*
    /// mainnet wallet during a dry-run (everything else assembled + verified correctly).
    pub fn is_insufficient_funds(&self) -> bool {
        self.err
            .as_deref()
            .map(|e| {
                let e = e.to_lowercase();
                e.contains("insufficient")
                    || e.contains("accountnotfound")
                    || e.contains("account not found")
            })
            .unwrap_or(false)
    }
}

fn result<'a>(v: &'a Value, method: &str) -> anyhow::Result<&'a Value> {
    if let Some(err) = v.get("error") {
        anyhow::bail!("{method} error: {err}");
    }
    v.get("result")
        .ok_or_else(|| anyhow::anyhow!("{method}: missing result"))
}

/// Parse a `getLatestBlockhash` response.
pub fn parse_latest_blockhash(v: &Value) -> anyhow::Result<BlockhashInfo> {
    let value = &result(v, "getLatestBlockhash")?["value"];
    let blockhash = value["blockhash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("getLatestBlockhash: missing blockhash"))?
        .to_string();
    let last_valid_block_height = value["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("getLatestBlockhash: missing lastValidBlockHeight"))?;
    Ok(BlockhashInfo {
        blockhash,
        last_valid_block_height,
    })
}

/// Parse a `getBlockHeight` response.
pub fn parse_block_height(v: &Value) -> anyhow::Result<u64> {
    result(v, "getBlockHeight")?
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("getBlockHeight: result not a u64"))
}

/// Parse an `isBlockhashValid` response (`result.value` is the boolean).
pub fn parse_is_blockhash_valid(v: &Value) -> anyhow::Result<bool> {
    result(v, "isBlockhashValid")?["value"]
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("isBlockhashValid: missing value"))
}

/// Parse a `simulateTransaction` response (`result.value` = `{ err, logs, unitsConsumed }`).
/// Tolerant: a missing `result` (RPC-level error) is surfaced as a failed simulation, not a panic.
pub fn parse_simulation(v: &Value) -> SimulationResult {
    let value = match result(v, "simulateTransaction") {
        Ok(r) => &r["value"],
        Err(e) => {
            return SimulationResult {
                err: Some(e.to_string()),
                logs: Vec::new(),
                units_consumed: None,
            }
        }
    };
    let err = match &value["err"] {
        Value::Null => None,
        other => Some(other.to_string()),
    };
    let logs = value["logs"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let units_consumed = value["unitsConsumed"].as_u64();
    SimulationResult {
        err,
        logs,
        units_consumed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_latest_blockhash() {
        let v = json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {
                "context": { "slot": 425_350_000 },
                "value": {
                    "blockhash": "EkSnNWid2cvwEVnVx9aBqaWmtn6q1S4tHrFs5o9eF2",
                    "lastValidBlockHeight": 396_823_001u64
                }
            }
        });
        let info = parse_latest_blockhash(&v).unwrap();
        assert_eq!(info.blockhash, "EkSnNWid2cvwEVnVx9aBqaWmtn6q1S4tHrFs5o9eF2");
        assert_eq!(info.last_valid_block_height, 396_823_001);
        assert_eq!(info.blocks_remaining(396_822_851), 150);
        assert_eq!(info.blocks_remaining(396_823_500), 0); // expired
    }

    #[test]
    fn parses_block_height() {
        let v = json!({ "jsonrpc": "2.0", "id": 1, "result": 396_822_851u64 });
        assert_eq!(parse_block_height(&v).unwrap(), 396_822_851);
    }

    #[test]
    fn parses_successful_simulation() {
        let v = json!({
            "result": { "context": { "slot": 1 },
                "value": { "err": null, "logs": ["Program 11111111111111111111111111111111 success"], "unitsConsumed": 450 } }
        });
        let sim = parse_simulation(&v);
        assert!(sim.succeeded());
        assert_eq!(sim.units_consumed, Some(450));
        assert_eq!(sim.logs.len(), 1);
    }

    #[test]
    fn parses_insufficient_funds_simulation() {
        // The expected dry-run result on an unfunded wallet: assembled fine, only funding blocks it.
        let v = json!({
            "result": { "context": { "slot": 1 },
                "value": { "err": { "InsufficientFundsForRent": { "account_index": 0 } }, "logs": [], "unitsConsumed": 0 } }
        });
        let sim = parse_simulation(&v);
        assert!(!sim.succeeded());
        assert!(sim.is_insufficient_funds());
    }

    #[test]
    fn rpc_level_error_is_a_failed_simulation_not_a_panic() {
        let v = json!({ "error": { "code": -32602, "message": "invalid tx" } });
        let sim = parse_simulation(&v);
        assert!(!sim.succeeded());
    }

    #[test]
    fn parses_is_blockhash_valid() {
        let valid = json!({ "result": { "context": { "slot": 1 }, "value": true } });
        let invalid = json!({ "result": { "context": { "slot": 1 }, "value": false } });
        assert!(parse_is_blockhash_valid(&valid).unwrap());
        assert!(!parse_is_blockhash_valid(&invalid).unwrap());
    }

    #[test]
    fn surfaces_rpc_errors() {
        let v =
            json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32002, "message": "boom" } });
        assert!(parse_block_height(&v).is_err());
        assert!(parse_latest_blockhash(&v).is_err());
    }
}
