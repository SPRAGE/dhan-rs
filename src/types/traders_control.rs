#![allow(missing_docs)]
//! Trader's Control types — Kill Switch, P&L Based Exit.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

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
    pub profit_value: PnlThreshold,
    /// Target loss amount to trigger exit.
    pub loss_value: PnlThreshold,
    /// Product types to apply exit to.
    pub product_type: Vec<PnlProductType>,
    /// Whether to enable kill switch after exit.
    pub enable_kill_switch: bool,
}

impl PnlExitRequest {
    pub(crate) fn validate(&self) -> std::result::Result<(), &'static str> {
        if !self.profit_value.is_valid() || !self.loss_value.is_valid() {
            return Err("P&L exit thresholds must be finite numeric values");
        }
        if self.product_type.is_empty() {
            return Err("P&L exit product_type cannot be empty");
        }
        Ok(())
    }
}

/// Threshold wire value. Dhan's example uses numeric strings while its linked
/// OpenAPI declares JSON numbers, so both official forms are supported.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum PnlThreshold {
    Number(f64),
    Text(String),
}

impl PnlThreshold {
    fn is_valid(&self) -> bool {
        match self {
            Self::Number(value) => value.is_finite(),
            Self::Text(value) => value.parse::<f64>().is_ok_and(|value| value.is_finite()),
        }
    }
}

impl From<f64> for PnlThreshold {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<String> for PnlThreshold {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for PnlThreshold {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PnlProductType {
    Intraday,
    Delivery,
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
    #[serde(default, deserialize_with = "deserialize_optional_scalar")]
    pub profit: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_scalar")]
    pub loss: Option<String>,
    /// Product types / segments (may come as `segments` in the wire format).
    #[serde(default, alias = "segments")]
    pub product_type: Option<Vec<PnlProductType>>,
    #[serde(default, alias = "enable_kill_switch")]
    pub enable_kill_switch: Option<bool>,
}

fn deserialize_optional_scalar<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        Some(other) => Err(D::Error::custom(format!(
            "expected a string, number, or null, got {other}"
        ))),
    }
}
