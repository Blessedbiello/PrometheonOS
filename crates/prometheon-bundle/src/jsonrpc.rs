//! JSON-RPC seam for the Jito Block Engine client (pure; no network).
//!
//! The Block Engine speaks JSON-RPC 2.0 over HTTP. This module builds the request envelopes,
//! unwraps `result`/`error`, and classifies HTTP statuses for retry — all unit-tested so the async
//! client ([`crate::client`]) is a thin I/O wrapper over verified logic.

use serde_json::{json, Value};

/// A JSON-RPC error surfaced by the Block Engine, or a malformed response.
#[derive(Debug, thiserror::Error)]
pub enum JsonRpcError {
    #[error("response was not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("JSON-RPC error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("response had neither `result` nor `error`")]
    Malformed,
}

/// Build a JSON-RPC 2.0 request envelope.
pub fn jsonrpc_body(id: u64, method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

/// `sendBundle` params: `[[<base64 txs>], {"encoding": "base64"}]`.
pub fn send_bundle_params(txs_base64: &[String]) -> Value {
    json!([txs_base64, { "encoding": "base64" }])
}

/// `getBundleStatuses` / `getInflightBundleStatuses` params: `[[<bundle_id>, ...]]`.
pub fn bundle_ids_params(bundle_ids: &[String]) -> Value {
    json!([bundle_ids])
}

/// Extract the `result` value from a JSON-RPC response body, or map its `error`.
pub fn unwrap_result(body: &str) -> Result<Value, JsonRpcError> {
    let mut v: Value = serde_json::from_str(body)?;
    if let Some(err) = v.get("error") {
        if !err.is_null() {
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            return Err(JsonRpcError::Rpc { code, message });
        }
    }
    match v.get_mut("result") {
        Some(result) => Ok(result.take()),
        None => Err(JsonRpcError::Malformed),
    }
}

/// Whether an HTTP status warrants a retry: rate-limit (429) or server errors (5xx). Other 4xx are
/// client errors and must not be retried blindly.
pub fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}
