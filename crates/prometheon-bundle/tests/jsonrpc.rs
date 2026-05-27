//! Behavioural spec for the JSON-RPC seam of the Block Engine client (Phase 2, test-first).
//!
//! These are the pure pieces of the HTTP client: building the JSON-RPC request envelopes Jito
//! expects, unwrapping the `result`/`error`, and the retry-status decision. The async I/O wraps
//! these and is exercised live (gated).

use prometheon_bundle::jsonrpc::{
    bundle_ids_params, is_retryable_status, jsonrpc_body, send_bundle_params, unwrap_result,
    JsonRpcError,
};
use serde_json::json;

#[test]
fn jsonrpc_body_has_the_2_0_envelope() {
    let body = jsonrpc_body(1, "getTipAccounts", json!([]));
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert_eq!(body["method"], "getTipAccounts");
    assert_eq!(body["params"], json!([]));
}

#[test]
fn send_bundle_params_wrap_txs_and_request_base64_encoding() {
    let txs = vec!["AAAA".to_string(), "BBBB".to_string()];
    let params = send_bundle_params(&txs);
    // params = [[tx1, tx2], {"encoding": "base64"}]
    assert_eq!(params[0], json!(["AAAA", "BBBB"]));
    assert_eq!(params[1], json!({ "encoding": "base64" }));
}

#[test]
fn bundle_ids_params_wrap_ids_in_a_nested_array() {
    let ids = vec!["id1".to_string(), "id2".to_string()];
    assert_eq!(bundle_ids_params(&ids), json!([["id1", "id2"]]));
}

#[test]
fn unwrap_result_returns_the_result_value_on_success() {
    let body = r#"{"jsonrpc":"2.0","result":{"context":{"slot":1},"value":[]},"id":1}"#;
    let result = unwrap_result(body).expect("ok");
    assert_eq!(result["context"]["slot"], 1);
}

#[test]
fn unwrap_result_surfaces_a_jsonrpc_error() {
    let body = r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"bad params"},"id":1}"#;
    let err = unwrap_result(body).unwrap_err();
    match err {
        JsonRpcError::Rpc { code, message } => {
            assert_eq!(code, -32602);
            assert_eq!(message, "bad params");
        }
        other => panic!("expected Rpc error, got {other:?}"),
    }
}

#[test]
fn unwrap_result_errors_when_neither_result_nor_error_present() {
    let body = r#"{"jsonrpc":"2.0","id":1}"#;
    assert!(unwrap_result(body).is_err());
}

#[test]
fn retryable_statuses_are_429_and_5xx_only() {
    assert!(is_retryable_status(429)); // rate limited
    assert!(is_retryable_status(500));
    assert!(is_retryable_status(503));
    assert!(!is_retryable_status(400)); // bad request — do not retry
    assert!(!is_retryable_status(404));
    assert!(!is_retryable_status(200));
}
