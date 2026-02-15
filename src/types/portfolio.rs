#![allow(missing_docs)]
//! Portfolio types — Holdings, Positions, Convert Position.

use serde::{Deserialize, Serialize};

use crate::types::enums::*;

// ---------------------------------------------------------------------------
// Holdings
// ---------------------------------------------------------------------------

/// A single holding in the demat account.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub exchange: Option<String>,
    pub trading_symbol: Option<String>,
    pub security_id: Option<String>,
    pub isin: Option<String>,
    #[serde(default)]
    pub total_qty: Option<i64>,
    #[serde(default)]
    pub dp_qty: Option<i64>,
    #[serde(default)]
    pub t1_qty: Option<i64>,
    #[serde(default)]
    pub available_qty: Option<i64>,
    #[serde(default)]
    pub collateral_qty: Option<i64>,
    #[serde(default)]
    pub avg_cost_price: Option<f64>,
}

// ---------------------------------------------------------------------------
// Positions
// ---------------------------------------------------------------------------

/// A single open position.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub dhan_client_id: Option<String>,
    pub trading_symbol: Option<String>,
    pub security_id: Option<String>,
    pub position_type: Option<String>,
    pub exchange_segment: Option<String>,
    pub product_type: Option<String>,
    #[serde(default)]
    pub buy_avg: Option<f64>,
    #[serde(default)]
    pub buy_qty: Option<i64>,
    #[serde(default)]
    pub cost_price: Option<f64>,
    #[serde(default)]
    pub sell_avg: Option<f64>,
    #[serde(default)]
    pub sell_qty: Option<i64>,
    #[serde(default)]
    pub net_qty: Option<i64>,
    #[serde(default)]
    pub realized_profit: Option<f64>,
    #[serde(default)]
    pub unrealized_profit: Option<f64>,
    #[serde(default)]
    pub rbi_reference_rate: Option<f64>,
    #[serde(default)]
    pub multiplier: Option<i64>,
    #[serde(default)]
    pub carry_forward_buy_qty: Option<i64>,
    #[serde(default)]
    pub carry_forward_sell_qty: Option<i64>,
    #[serde(default)]
    pub carry_forward_buy_value: Option<f64>,
    #[serde(default)]
    pub carry_forward_sell_value: Option<f64>,
    #[serde(default)]
    pub day_buy_qty: Option<i64>,
    #[serde(default)]
    pub day_sell_qty: Option<i64>,
    #[serde(default)]
    pub day_buy_value: Option<f64>,
    #[serde(default)]
    pub day_sell_value: Option<f64>,
    pub drv_expiry_date: Option<String>,
    pub drv_option_type: Option<String>,
    #[serde(default)]
    pub drv_strike_price: Option<f64>,
    #[serde(default)]
    pub cross_currency: Option<bool>,
}

// ---------------------------------------------------------------------------
// Convert Position
// ---------------------------------------------------------------------------

/// Request body for converting a position product type.
///
/// Used by `POST /v2/positions/convert`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertPositionRequest {
    pub dhan_client_id: String,
    pub from_product_type: ProductType,
    pub exchange_segment: ExchangeSegment,
    pub position_type: PositionType,
    pub security_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trading_symbol: Option<String>,
    pub convert_qty: u64,
    pub to_product_type: ProductType,
}

// ---------------------------------------------------------------------------
// Exit All Positions
// ---------------------------------------------------------------------------

/// Response from exiting all positions.
#[derive(Debug, Clone, Deserialize)]
pub struct ExitAllResponse {
    pub status: String,
    pub message: String,
}
