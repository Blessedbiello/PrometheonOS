//! Tip-account fetching and selection.
//!
//! `getTipAccounts` returns 8 tip accounts. We fetch them **live** (never hardcode — they rotate)
//! and select one per bundle. Selection rotates/randomizes across the 8 to avoid serializing our
//! bundles on a single account's write-lock (the 8-account design exists for parallel execution).

use serde::Deserialize;

/// Error parsing a `getTipAccounts` response.
#[derive(Debug, thiserror::Error)]
pub enum TipAccountsError {
    #[error("getTipAccounts response was not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("getTipAccounts returned no accounts")]
    Empty,
}

/// The set of Jito tip accounts (typically 8), fetched live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TipAccounts {
    accounts: Vec<String>,
}

impl TipAccounts {
    /// Wrap a list of tip-account pubkeys.
    pub fn new(accounts: Vec<String>) -> Self {
        Self { accounts }
    }

    /// All accounts (in returned order).
    pub fn all(&self) -> &[String] {
        &self.accounts
    }

    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Pick an account by a rotating/random `seed` (taken mod the count). `None` if empty.
    ///
    /// Callers pass a rotating counter or random value per bundle so picks spread across all
    /// accounts, avoiding write-lock contention on any single one.
    pub fn pick(&self, seed: u64) -> Option<&str> {
        if self.accounts.is_empty() {
            return None;
        }
        let idx = (seed % self.accounts.len() as u64) as usize;
        Some(self.accounts[idx].as_str())
    }
}

/// Parse a `getTipAccounts` JSON-RPC response (`{ "result": [..8 pubkeys..] }`).
pub fn parse_tip_accounts(body: &str) -> Result<TipAccounts, TipAccountsError> {
    #[derive(Deserialize)]
    struct Resp {
        result: Vec<String>,
    }
    let resp: Resp = serde_json::from_str(body)?;
    if resp.result.is_empty() {
        return Err(TipAccountsError::Empty);
    }
    Ok(TipAccounts::new(resp.result))
}
