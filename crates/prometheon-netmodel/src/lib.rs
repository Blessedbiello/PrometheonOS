//! `prometheon-netmodel`
//!
//! The network-condition intelligence layer: rolling-window aggregation of slot/latency/tip data
//! into bounded health and execution-quality metrics (congestion, slot stability, landing
//! probability, expiry risk, tip efficiency, cost per landing). These feed the AI strategist and
//! the dashboard. All metric math is pure and unit-tested.

pub mod metrics;
pub mod model;
pub mod window;

pub use metrics::{
    bundle_landing_probability, congestion_score, cost_per_successful_landing, expiry_risk_score,
    retry_success_rate, slot_stability_score, tip_efficiency_ratio, CongestionWeights,
};
pub use model::{HealthSnapshot, NetworkHealthModel};
pub use window::RollingWindow;
