//! Forever Order (GTT) types.

use serde::{Deserialize, Serialize};

use crate::types::enums::*;

// ---------------------------------------------------------------------------
// Create Forever Order
// ---------------------------------------------------------------------------

/// Request body for creating a new forever order.
///
/// Used by `POST /v2/forever/orders`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateForeverOrderRequest {
    pub dhan_client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// `SINGLE` for forever order, `OCO` for OCO order.
    pub order_flag: OrderFlag,
    pub transaction_type: TransactionType,
    pub exchange_segment: ExchangeSegment,
    pub product_type: ProductType,
    pub order_type: OrderType,
    pub validity: Validity,
    pub security_id: String,
    pub quantity: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disclosed_quantity: Option<u64>,
    pub price: f64,
    pub trigger_price: f64,
    /// Target price for OCO order (second leg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price1: Option<f64>,
    /// Target trigger price for OCO order (second leg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price1: Option<f64>,
    /// Target quantity for OCO order (second leg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity1: Option<u64>,
}

// ---------------------------------------------------------------------------
// Modify Forever Order
// ---------------------------------------------------------------------------

/// Request body for modifying an existing forever order.
///
/// Used by `PUT /v2/forever/orders/{order-id}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyForeverOrderRequest {
    pub dhan_client_id: String,
    pub order_id: String,
    pub order_flag: OrderFlag,
    pub order_type: OrderType,
    /// `TARGET_LEG` for SINGLE and first leg of OCO,
    /// `STOP_LOSS_LEG` for second leg of OCO.
    pub leg_name: LegName,
    pub quantity: u64,
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disclosed_quantity: Option<u64>,
    pub trigger_price: f64,
    pub validity: Validity,
}

// ---------------------------------------------------------------------------
// Forever Order Detail
// ---------------------------------------------------------------------------

/// Full forever order detail as returned by the forever order list.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeverOrderDetail {
    pub dhan_client_id: Option<String>,
    pub order_id: Option<String>,
    pub order_status: Option<String>,
    pub transaction_type: Option<String>,
    pub exchange_segment: Option<String>,
    pub product_type: Option<String>,
    /// `SINGLE` or `OCO`.
    pub order_type: Option<String>,
    pub trading_symbol: Option<String>,
    pub security_id: Option<String>,
    #[serde(default)]
    pub quantity: Option<u64>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub trigger_price: Option<f64>,
    pub leg_name: Option<String>,
    pub create_time: Option<String>,
    pub update_time: Option<String>,
    pub exchange_time: Option<String>,
    pub drv_expiry_date: Option<String>,
    pub drv_option_type: Option<String>,
    #[serde(default)]
    pub drv_strike_price: Option<f64>,
}
