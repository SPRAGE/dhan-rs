#![allow(missing_docs)]
//! Super Order types.

use serde::{Deserialize, Serialize};

use crate::types::enums::*;

// ---------------------------------------------------------------------------
// Place Super Order
// ---------------------------------------------------------------------------

/// Request body for placing a new super order.
///
/// Used by `POST /v2/super/orders`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceSuperOrderRequest {
    pub dhan_client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub transaction_type: TransactionType,
    pub exchange_segment: ExchangeSegment,
    pub product_type: ProductType,
    pub order_type: OrderType,
    pub security_id: String,
    pub quantity: u64,
    pub price: f64,
    pub target_price: f64,
    pub stop_loss_price: f64,
    pub trailing_jump: f64,
}

// ---------------------------------------------------------------------------
// Modify Super Order
// ---------------------------------------------------------------------------

/// Request body for modifying a super order.
///
/// Used by `PUT /v2/super/orders/{order-id}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifySuperOrderRequest {
    pub dhan_client_id: String,
    pub order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_type: Option<OrderType>,
    pub leg_name: LegName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_jump: Option<f64>,
}

// ---------------------------------------------------------------------------
// Super Order Detail
// ---------------------------------------------------------------------------

/// Leg detail within a super order.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegDetail {
    pub order_id: Option<String>,
    pub leg_name: Option<String>,
    pub transaction_type: Option<String>,
    #[serde(default, alias = "totalQuatity")]
    pub total_quantity: Option<u64>,
    #[serde(default)]
    pub remaining_quantity: Option<u64>,
    #[serde(default)]
    pub triggered_quantity: Option<u64>,
    #[serde(default)]
    pub price: Option<f64>,
    pub order_status: Option<String>,
    #[serde(default)]
    pub trailing_jump: Option<f64>,
}

/// Full super order detail as returned by the super order list.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuperOrderDetail {
    pub dhan_client_id: Option<String>,
    pub order_id: Option<String>,
    pub correlation_id: Option<String>,
    pub order_status: Option<String>,
    pub transaction_type: Option<String>,
    pub exchange_segment: Option<String>,
    pub product_type: Option<String>,
    pub order_type: Option<String>,
    pub validity: Option<String>,
    pub trading_symbol: Option<String>,
    pub security_id: Option<String>,
    #[serde(default)]
    pub quantity: Option<u64>,
    #[serde(default)]
    pub remaining_quantity: Option<u64>,
    #[serde(default)]
    pub ltp: Option<f64>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub after_market_order: Option<bool>,
    pub leg_name: Option<String>,
    pub exchange_order_id: Option<String>,
    pub create_time: Option<String>,
    pub update_time: Option<String>,
    pub exchange_time: Option<String>,
    pub oms_error_description: Option<String>,
    #[serde(default)]
    pub average_traded_price: Option<f64>,
    #[serde(default)]
    pub filled_qty: Option<u64>,
    #[serde(default)]
    pub triggered_quantity: Option<u64>,
    #[serde(default)]
    pub leg_details: Vec<LegDetail>,
}
