//! Jito `getNextScheduledLeader` — the next slot led by a Jito-Solana validator.
//!
//! This is how we "detect the correct leader window": only a Jito leader honours bundles, so the
//! engine times submission for the upcoming Jito leader slots. The wire field names vary by
//! deployment (camelCase vs snake_case), so the parser is tolerant; the response shape should be
//! confirmed against the live Block Engine.

use serde_json::Value;

/// The next Jito-Solana leader, relative to the current slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextLeader {
    pub current_slot: u64,
    pub next_leader_slot: u64,
    pub next_leader_identity: Option<String>,
    pub next_leader_region: Option<String>,
}

/// Parse a `getNextScheduledLeader` JSON-RPC response body. Tolerant of camelCase/snake_case and of
/// a bare result object vs. a `{ "result": … }` envelope.
pub fn parse_next_scheduled_leader(body: &str) -> Result<NextLeader, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("rpc error: {err}"));
    }
    let r = v.get("result").unwrap_or(&v);

    let u64_of = |keys: &[&str]| -> Option<u64> {
        keys.iter().find_map(|k| r.get(*k).and_then(Value::as_u64))
    };
    let str_of = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| r.get(*k).and_then(Value::as_str).map(String::from))
    };

    let current_slot = u64_of(&["currentSlot", "current_slot"])
        .ok_or_else(|| "missing currentSlot".to_string())?;
    let next_leader_slot = u64_of(&["nextLeaderSlot", "next_leader_slot"])
        .ok_or_else(|| "missing nextLeaderSlot".to_string())?;

    Ok(NextLeader {
        current_slot,
        next_leader_slot,
        next_leader_identity: str_of(&["nextLeaderIdentity", "next_leader_identity"]),
        next_leader_region: str_of(&["nextLeaderRegion", "next_leader_region"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_camel_case_wrapped_result() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{
            "currentSlot":425350000,"nextLeaderSlot":425350003,
            "nextLeaderIdentity":"Jito1Va...","nextLeaderRegion":"frankfurt"}}"#;
        let n = parse_next_scheduled_leader(body).unwrap();
        assert_eq!(n.current_slot, 425_350_000);
        assert_eq!(n.next_leader_slot, 425_350_003);
        assert_eq!(n.next_leader_region.as_deref(), Some("frankfurt"));
    }

    #[test]
    fn parses_snake_case_bare_object() {
        let body = r#"{"current_slot":100,"next_leader_slot":104}"#;
        let n = parse_next_scheduled_leader(body).unwrap();
        assert_eq!(n.current_slot, 100);
        assert_eq!(n.next_leader_slot, 104);
        assert_eq!(n.next_leader_identity, None);
    }

    #[test]
    fn surfaces_rpc_errors_and_missing_fields() {
        assert!(parse_next_scheduled_leader(r#"{"error":{"message":"x"}}"#).is_err());
        assert!(parse_next_scheduled_leader(r#"{"result":{"currentSlot":1}}"#).is_err());
        assert!(parse_next_scheduled_leader("not json").is_err());
    }
}
