//! Jito tip-floor data: the live signal that drives dynamic tip sizing.
//!
//! Source: `GET https://bundles.jito.wtf/api/v1/bundles/tip_floor` returns a single-element JSON
//! array of landed-tip percentiles. **Units are SOL** — we convert to lamports (×1e9) on access so
//! the rest of the engine works in integer lamports.
//!
//! There are NO hardcoded tip values anywhere: the tip is always derived from this live data (see
//! [`crate::tip`]); only safety *bounds* are configured.

use serde::{Deserialize, Serialize};

use crate::tip::Percentile;

/// Lamports per SOL.
pub const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

/// Parsed tip-floor snapshot. Percentiles are stored as SOL exactly as returned; convert via the
/// `*_lamports` accessors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TipFloor {
    /// Server timestamp of the snapshot (opaque string as returned).
    #[serde(default)]
    pub time: String,
    #[serde(rename = "landed_tips_25th_percentile")]
    pub p25_sol: f64,
    #[serde(rename = "landed_tips_50th_percentile")]
    pub p50_sol: f64,
    #[serde(rename = "landed_tips_75th_percentile")]
    pub p75_sol: f64,
    #[serde(rename = "landed_tips_95th_percentile")]
    pub p95_sol: f64,
    #[serde(rename = "landed_tips_99th_percentile")]
    pub p99_sol: f64,
    #[serde(rename = "ema_landed_tips_50th_percentile")]
    pub ema50_sol: f64,
}

/// Error parsing a tip-floor response.
#[derive(Debug, thiserror::Error)]
pub enum TipFloorError {
    #[error("tip floor response was not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("tip floor response array was empty")]
    Empty,
}

impl TipFloor {
    /// Parse the tip-floor endpoint's response body (a single-element array), taking the first
    /// element.
    pub fn from_response_json(body: &str) -> Result<Self, TipFloorError> {
        let arr: Vec<TipFloor> = serde_json::from_str(body)?;
        arr.into_iter().next().ok_or(TipFloorError::Empty)
    }

    /// The SOL value for a percentile.
    pub fn percentile_sol(&self, p: Percentile) -> f64 {
        match p {
            Percentile::P25 => self.p25_sol,
            Percentile::P50 => self.p50_sol,
            Percentile::P75 => self.p75_sol,
            Percentile::P95 => self.p95_sol,
            Percentile::P99 => self.p99_sol,
        }
    }

    /// The lamport value for a percentile (rounded).
    pub fn percentile_lamports(&self, p: Percentile) -> u64 {
        sol_to_lamports(self.percentile_sol(p))
    }

    /// The EMA of the median landed tip, in lamports.
    pub fn ema50_lamports(&self) -> u64 {
        sol_to_lamports(self.ema50_sol)
    }
}

/// Convert a SOL amount to lamports, rounded to the nearest lamport (never negative).
pub fn sol_to_lamports(sol: f64) -> u64 {
    if sol <= 0.0 {
        0
    } else {
        (sol * LAMPORTS_PER_SOL).round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sol_to_lamports_rounds_and_floors_at_zero() {
        assert_eq!(sol_to_lamports(1.0), 1_000_000_000);
        assert_eq!(sol_to_lamports(0.0000000004), 0); // rounds down below half a lamport
        assert_eq!(sol_to_lamports(0.0000000006), 1); // rounds up
        assert_eq!(sol_to_lamports(-5.0), 0);
    }
}
