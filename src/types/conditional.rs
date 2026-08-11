#![allow(missing_docs)]
//! Conditional Trigger types.

use serde::{Deserialize, Serialize};

use crate::types::enums::*;

// ---------------------------------------------------------------------------
// Multi Order
// ---------------------------------------------------------------------------

/// Exchange segments accepted by the REST multi-order endpoint.
///
/// This is deliberately separate from [`ExchangeSegment`]. `NSE_COMM` is
/// documented for this JSON API, but Dhan has not documented a corresponding
/// standard market-feed binary segment code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum MultiOrderExchangeSegment {
    NSE_EQ,
    NSE_FNO,
    NSE_COMM,
    BSE_EQ,
    BSE_FNO,
    MCX_COMM,
}

/// Product types accepted specifically by the REST multi-order operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum MultiOrderProductType {
    CNC,
    INTRADAY,
    MARGIN,
    MTF,
}

/// One order in a multi-order request.
///
/// Used in [`MultiOrderRequest`] for `POST /v2/alerts/multi/orders`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiOrderItemRequest {
    /// Caller-selected identifier for correlating a response to this item.
    pub sequence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub transaction_type: TransactionType,
    pub exchange_segment: MultiOrderExchangeSegment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<MultiOrderProductType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_type: Option<OrderType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_market_order: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amo_time: Option<AmoTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disclosed_quantity: Option<u32>,
}

/// Request body for placing a batch of orders without a condition.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiOrderRequest {
    pub dhan_client_id: String,
    pub orders: Vec<MultiOrderItemRequest>,
}

impl MultiOrderRequest {
    /// Validate the documented batch and correlation fields before sending.
    pub fn validate(&self) -> std::result::Result<(), &'static str> {
        if self.dhan_client_id.trim().is_empty() {
            return Err("multi order dhan_client_id cannot be empty");
        }
        if self.orders.is_empty() {
            return Err("multi order orders cannot be empty");
        }
        if self.orders.iter().any(|order| {
            order
                .correlation_id
                .as_ref()
                .is_some_and(|value| value.chars().count() > 30)
        }) {
            return Err("multi order correlation_id cannot exceed 30 characters");
        }
        if self.orders.iter().any(|order| {
            order.quantity.is_some_and(|value| value > i32::MAX as u32)
                || order
                    .disclosed_quantity
                    .is_some_and(|value| value > i32::MAX as u32)
        }) {
            return Err("multi order quantities cannot exceed the documented int32 range");
        }
        Ok(())
    }
}

/// Response for one item in a multi-order response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiOrderItemResponse {
    pub order_id: Option<String>,
    pub sequence: Option<String>,
    pub order_status: Option<String>,
}

/// Response from placing a batch of orders.
#[derive(Debug, Clone, Deserialize)]
pub struct MultiOrderResponse {
    #[serde(default)]
    pub orders: Vec<MultiOrderItemResponse>,
}

// ---------------------------------------------------------------------------
// Alert Condition
// ---------------------------------------------------------------------------

/// Condition configuration for a conditional trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertCondition {
    /// Type of comparison (e.g. `TECHNICAL_WITH_VALUE`).
    pub comparison_type: String,
    /// Exchange where condition is evaluated.
    pub exchange_segment: ExchangeSegment,
    /// Security ID of the instrument.
    pub security_id: String,
    /// Technical indicator name (e.g. `SMA_5`, `LTP`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator_name: Option<String>,
    /// Timeframe for indicator evaluation (`DAY`, `ONE_MIN`, `FIVE_MIN`, `FIFTEEN_MIN`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_frame: Option<String>,
    /// Condition operator (e.g. `CROSSING_UP`, `GREATER_THAN`).
    pub operator: String,
    /// Value to compare indicator/price against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparing_value: Option<serde_json::Value>,
    /// Second indicator name for indicator-vs-indicator comparisons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparing_indicator_name: Option<String>,
    /// Alert expiry date (YYYY-MM-DD). Defaults to 1 year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp_date: Option<String>,
    /// Trigger frequency (e.g. `ONCE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
    /// User-provided note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,
}

// ---------------------------------------------------------------------------
// Alert Order
// ---------------------------------------------------------------------------

/// Order to execute when the alert condition is met.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertOrder {
    pub transaction_type: TransactionType,
    pub exchange_segment: ExchangeSegment,
    pub product_type: ProductType,
    pub order_type: OrderType,
    pub security_id: String,
    pub quantity: u64,
    pub validity: Validity,
    /// Price at which order is placed (as string in API).
    pub price: String,
    /// Disclosed quantity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_quantity: Option<String>,
    /// Trigger price for SL/SL-M.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<String>,
}

// ---------------------------------------------------------------------------
// Place / Modify Conditional Trigger
// ---------------------------------------------------------------------------

/// Request body for placing or modifying a conditional trigger.
///
/// Used by `POST /v2/alerts/orders` and `PUT /v2/alerts/orders/{alertId}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalTriggerRequest {
    pub dhan_client_id: String,
    /// Alert ID (only for modify requests).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_id: Option<String>,
    pub condition: AlertCondition,
    pub orders: Vec<AlertOrder>,
}

// ---------------------------------------------------------------------------
// Conditional Trigger Response
// ---------------------------------------------------------------------------

/// Response from placing, modifying, or deleting a conditional trigger.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalTriggerResponse {
    pub alert_id: String,
    pub alert_status: String,
}

// ---------------------------------------------------------------------------
// Conditional Trigger Detail
// ---------------------------------------------------------------------------

/// Full conditional trigger detail as returned by get endpoints.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalTriggerDetail {
    pub alert_id: Option<String>,
    pub alert_status: Option<String>,
    pub created_time: Option<String>,
    pub triggered_time: Option<String>,
    #[serde(default)]
    pub last_price: Option<serde_json::Value>,
    pub condition: Option<AlertCondition>,
    #[serde(default)]
    pub orders: Vec<AlertOrder>,
}
