//! Conditional Trigger types.

use serde::{Deserialize, Serialize};

use crate::types::enums::*;

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
