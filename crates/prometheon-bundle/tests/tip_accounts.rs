//! Behavioural spec for tip-account handling (Phase 2, test-first).
//!
//! `getTipAccounts` returns 8 tip accounts; we fetch them live (never hardcode) and pick one per
//! bundle, ideally rotating/randomizing to avoid write-lock contention on a single account.

use prometheon_bundle::tip_accounts::{parse_tip_accounts, TipAccounts};

const TIP_ACCOUNTS_JSON: &str = r#"{
  "jsonrpc": "2.0",
  "result": [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT"
  ],
  "id": 1
}"#;

#[test]
fn tip_accounts_parse_from_jsonrpc_result() {
    let accounts = parse_tip_accounts(TIP_ACCOUNTS_JSON).expect("parse");
    assert_eq!(accounts.len(), 8);
    assert_eq!(
        accounts.all()[0],
        "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5"
    );
}

#[test]
fn pick_index_is_deterministic_modulo_len_and_rotates() {
    let accounts = parse_tip_accounts(TIP_ACCOUNTS_JSON).unwrap();
    // Rotating seed spreads picks across all 8 accounts (no contention on one).
    assert_eq!(accounts.pick(0), Some(accounts.all()[0].as_str()));
    assert_eq!(accounts.pick(7), Some(accounts.all()[7].as_str()));
    assert_eq!(accounts.pick(8), Some(accounts.all()[0].as_str())); // wraps
    assert_eq!(accounts.pick(10), Some(accounts.all()[2].as_str()));
}

#[test]
fn empty_tip_accounts_pick_is_none() {
    let empty = TipAccounts::new(vec![]);
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.pick(0), None);
    assert!(empty.is_empty());
}

#[test]
fn parse_rejects_empty_result_list() {
    let json = r#"{ "jsonrpc": "2.0", "result": [], "id": 1 }"#;
    assert!(parse_tip_accounts(json).is_err());
}
