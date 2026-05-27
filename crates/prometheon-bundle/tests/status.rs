//! Behavioural spec for Jito bundle-status parsing (Phase 2, test-first).
//!
//! Two endpoints, two purposes:
//! - `getInflightBundleStatuses` — fast, last ~5 min, statuses Invalid|Pending|Failed|Landed.
//! - `getBundleStatuses` — on-chain confirmation detail (confirmation_status + err + sigs + slot).
//!
//! Robust landing detection uses inflight first, then confirms via getBundleStatuses.

use prometheon_bundle::status::{
    parse_bundle_statuses, parse_inflight_statuses, BundleConfirmation, InflightStatus,
};

const INFLIGHT_JSON: &str = r#"{
  "context": { "slot": 280624000 },
  "value": [
    { "bundle_id": "b_landed", "status": "Landed", "landed_slot": 280623950 },
    { "bundle_id": "b_pending", "status": "Pending", "landed_slot": null },
    { "bundle_id": "b_failed", "status": "Failed", "landed_slot": null },
    { "bundle_id": "b_invalid", "status": "Invalid", "landed_slot": null }
  ]
}"#;

const BUNDLE_STATUSES_JSON: &str = r#"{
  "context": { "slot": 280624010 },
  "value": [
    {
      "bundle_id": "b_landed",
      "transactions": ["5sig1", "5sig2"],
      "slot": 280623950,
      "confirmation_status": "confirmed",
      "err": null
    }
  ]
}"#;

#[test]
fn inflight_statuses_parse_with_landed_slot_and_enum() {
    let res = parse_inflight_statuses(INFLIGHT_JSON).expect("parse");
    assert_eq!(res.context_slot, 280624000);
    assert_eq!(res.value.len(), 4);

    let landed = &res.value[0];
    assert_eq!(landed.bundle_id, "b_landed");
    assert_eq!(landed.status, InflightStatus::Landed);
    assert_eq!(landed.landed_slot, Some(280623950));

    assert_eq!(res.value[1].status, InflightStatus::Pending);
    assert_eq!(res.value[1].landed_slot, None);
    assert_eq!(res.value[2].status, InflightStatus::Failed);
    assert_eq!(res.value[3].status, InflightStatus::Invalid);
}

#[test]
fn inflight_status_landed_is_terminal_success_others_classified() {
    assert!(InflightStatus::Landed.is_landed());
    assert!(!InflightStatus::Pending.is_landed());
    // Pending is the only non-terminal state worth continuing to poll.
    assert!(InflightStatus::Pending.is_pending());
    assert!(!InflightStatus::Landed.is_pending());
    // Failed and Invalid are terminal non-success.
    assert!(InflightStatus::Failed.is_terminal_failure());
    assert!(InflightStatus::Invalid.is_terminal_failure());
    assert!(!InflightStatus::Landed.is_terminal_failure());
}

#[test]
fn bundle_statuses_parse_confirmation_sigs_and_no_error() {
    let res = parse_bundle_statuses(BUNDLE_STATUSES_JSON).expect("parse");
    assert_eq!(res.context_slot, 280624010);
    let s = &res.value[0];
    assert_eq!(s.bundle_id, "b_landed");
    assert_eq!(
        s.transactions,
        vec!["5sig1".to_string(), "5sig2".to_string()]
    );
    assert_eq!(s.slot, 280623950);
    assert_eq!(s.confirmation_status, Some(BundleConfirmation::Confirmed));
    assert!(!s.has_error());
}

#[test]
fn bundle_status_with_error_is_flagged() {
    let json = r#"{
      "context": { "slot": 1 },
      "value": [{
        "bundle_id": "b_err",
        "transactions": ["sigX"],
        "slot": 100,
        "confirmation_status": "processed",
        "err": { "Ok": null, "code": 1 }
      }]
    }"#;
    let res = parse_bundle_statuses(json).unwrap();
    let s = &res.value[0];
    assert!(s.has_error());
    assert_eq!(s.confirmation_status, Some(BundleConfirmation::Processed));
}

#[test]
fn empty_bundle_status_value_means_not_found() {
    let json = r#"{ "context": { "slot": 1 }, "value": [] }"#;
    let res = parse_bundle_statuses(json).unwrap();
    assert!(res.value.is_empty());
}
