#![allow(missing_docs)]
//! Trader's Control types — Kill Switch, P&L Based Exit.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Kill Switch
// ---------------------------------------------------------------------------

/// Response from managing or querying the kill switch.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillSwitchResponse {
    pub dhan_client_id: Option<String>,
    pub kill_switch_status: String,
}

// ---------------------------------------------------------------------------
// P&L Based Exit
// ---------------------------------------------------------------------------

/// Request body for configuring P&L-based auto-exit.
///
/// Used by `POST /v2/pnlExit`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlExitRequest {
    /// Target profit amount to trigger exit.
    pub profit_value: String,
    /// Target loss amount to trigger exit.
    pub loss_value: String,
    /// Product types to apply exit to (e.g. `["INTRADAY", "DELIVERY"]`).
    pub product_type: Vec<String>,
    /// Whether to enable kill switch after exit.
    pub enable_kill_switch: bool,
}

/// Response from configuring or stopping P&L-based exit.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlExitResponse {
    pub pnl_exit_status: String,
    pub message: String,
}

/// Current P&L-based exit configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlExitConfig {
    pub pnl_exit_status: Option<String>,
    #[serde(default)]
    pub profit: Option<String>,
    #[serde(default)]
    pub loss: Option<String>,
    /// Product types / segments (may come as `segments` in the wire format).
    #[serde(default, alias = "segments")]
    pub product_type: Option<Vec<String>>,
    #[serde(default)]
    pub enable_kill_switch: Option<bool>,
}
