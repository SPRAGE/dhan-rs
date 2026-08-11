#![allow(missing_docs)]
#![allow(non_camel_case_types)]
//! Types for the linked DhanHQ v2 Global Stocks API.
//!
//! These types deliberately do not reuse the domestic-market exchange segment
//! types: the Global Stocks OpenAPI operations use a distinct wire contract.

use serde::{Deserialize, Serialize};

/// Buy or sell side used by Global Stocks requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobalStockTransactionType {
    BUY,
    SELL,
}

/// Order type accepted by Global Stocks orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobalStockOrderType {
    MARKET,
    LIMIT,
    STOP_LOSS,
    STOP_LOSS_MARKET,
    AMOUNT,
}

/// Leg selector for a Global Stocks super order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobalStockLegName {
    ENTRY_LEG,
    STOP_LOSS_LEG,
    TARGET_LEG,
    NA,
}

/// Known Global Stocks order states, with forward-compatible unknown values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalStockOrderStatus {
    TRANSIT,
    PENDING,
    REJECTED,
    CANCELLED,
    PART_TRADED,
    TRADED,
    EXPIRED,
    MODIFIED,
    TRIGGERED,
    INACTIVE,
    Unknown(String),
}

impl GlobalStockOrderStatus {
    fn as_str(&self) -> &str {
        match self {
            Self::TRANSIT => "TRANSIT",
            Self::PENDING => "PENDING",
            Self::REJECTED => "REJECTED",
            Self::CANCELLED => "CANCELLED",
            Self::PART_TRADED => "PART_TRADED",
            Self::TRADED => "TRADED",
            Self::EXPIRED => "EXPIRED",
            Self::MODIFIED => "MODIFIED",
            Self::TRIGGERED => "TRIGGERED",
            Self::INACTIVE => "INACTIVE",
            Self::Unknown(value) => value,
        }
    }

    fn from_wire(value: String) -> Self {
        match value.as_str() {
            "TRANSIT" => Self::TRANSIT,
            "PENDING" => Self::PENDING,
            "REJECTED" => Self::REJECTED,
            "CANCELLED" => Self::CANCELLED,
            "PART_TRADED" => Self::PART_TRADED,
            "TRADED" => Self::TRADED,
            "EXPIRED" => Self::EXPIRED,
            "MODIFIED" => Self::MODIFIED,
            "TRIGGERED" => Self::TRIGGERED,
            "INACTIVE" => Self::INACTIVE,
            _ => Self::Unknown(value),
        }
    }
}

impl Serialize for GlobalStockOrderStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for GlobalStockOrderStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_wire)
    }
}

/// Request body for `POST /v2/globalstocks/orders`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockOrderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhan_client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub transaction_type: GlobalStockTransactionType,
    pub order_type: GlobalStockOrderType,
    pub security_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_market_order: Option<bool>,
}

/// Request body for `PUT /v2/globalstocks/orders/{order-id}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockModifyOrderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhan_client_id: Option<String>,
    pub order_type: GlobalStockOrderType,
    pub transaction_type: GlobalStockTransactionType,
    pub security_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leg_name: Option<GlobalStockLegName>,
}

/// Request body for the Global Stocks transaction estimator and margin calculator.
///
/// The official schema defines `price` and `quantity` as JSON strings, so the
/// type preserves that wire representation while endpoint methods validate
/// that both strings contain positive finite decimal values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockEstimatorRequest {
    pub security_id: String,
    pub price: String,
    pub quantity: String,
    pub transaction_type: GlobalStockTransactionType,
}

/// Order submission, modification, or cancellation acknowledgement.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockOrderStatusResponse {
    pub order_id: Option<String>,
    pub order_status: Option<GlobalStockOrderStatus>,
}

/// A Global Stocks order-book entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockOrder {
    pub dhan_client_id: Option<String>,
    pub order_id: Option<String>,
    pub exchange_order_id: Option<String>,
    pub correlation_id: Option<String>,
    pub transaction_type: Option<String>,
    pub exchange_segment: Option<String>,
    pub product_type: Option<String>,
    pub order_type: Option<String>,
    pub validity: Option<String>,
    pub trading_symbol: Option<String>,
    pub display_name: Option<String>,
    pub security_id: Option<String>,
    pub quantity: Option<f64>,
    pub remaining_quantity: Option<f64>,
    pub traded_qty: Option<f64>,
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub avg_traded_price: Option<f64>,
    pub order_status: Option<GlobalStockOrderStatus>,
    pub create_time: Option<String>,
    pub exchange_time: Option<String>,
    pub update_time: Option<String>,
    pub oms_error_code: Option<String>,
    pub oms_error_description: Option<String>,
    pub after_market_order: Option<bool>,
    pub amount: Option<f64>,
    pub lot_size: Option<i32>,
    pub fractional_flag: Option<bool>,
    pub leg_name: Option<GlobalStockLegName>,
    pub child_orders: Option<Vec<GlobalStockOrder>>,
}

/// Charge estimate returned by `POST /v2/globalstocks/transEstimate`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockEstimatorResponse {
    pub brokerage: Option<f64>,
    pub order_charges: Option<f64>,
    pub exchange_charges: Option<f64>,
    pub turn_over_fee: Option<f64>,
    pub gst_charges: Option<f64>,
    pub other_charges: Option<f64>,
}

/// Margin result returned by `POST /v2/globalstocks/margincalculator`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockMarginResponse {
    pub insufficient_bal: Option<f64>,
    pub brokerage: Option<f64>,
    pub leverage: Option<String>,
    pub total_margin: Option<f64>,
    pub available_bal: Option<f64>,
}

/// A Global Stocks trade-book entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockTrade {
    pub dhan_client_id: Option<String>,
    pub order_id: Option<String>,
    pub exchange_order_id: Option<String>,
    pub transaction_type: Option<String>,
    pub exchange_segment: Option<String>,
    pub product_type: Option<String>,
    pub order_type: Option<String>,
    pub trading_symbol: Option<String>,
    pub security_id: Option<String>,
    pub traded_quantity: Option<f64>,
    pub traded_price: Option<f64>,
    pub trade_date: Option<String>,
    pub create_time: Option<String>,
    pub exchange_time: Option<String>,
    pub order_status: Option<GlobalStockOrderStatus>,
    pub brokerage: Option<f64>,
    pub other_charges: Option<f64>,
    pub trade_value: Option<f64>,
}

/// Current Global Stocks market status.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockMarketStatus {
    pub market_close_time: Option<String>,
    pub market_open_time: Option<String>,
    pub holiday_flag: Option<bool>,
    pub status: Option<String>,
}

/// A Global Stocks holding.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockHolding {
    pub dhan_client_id: Option<String>,
    pub trading_symbol: Option<String>,
    pub display_name: Option<String>,
    pub security_id: Option<String>,
    pub exchange: Option<String>,
    pub quantity: Option<f64>,
    pub avg_cost_price: Option<f64>,
    pub cost_value: Option<f64>,
    pub current_value: Option<f64>,
    pub gain_value: Option<f64>,
    pub ltp: Option<f64>,
    pub prev_close: Option<f64>,
    pub long_term_flag: Option<String>,
    pub last_updated: Option<String>,
}

/// Global Stocks cash and margin limits.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStockFundLimit {
    pub dhan_client_id: Option<String>,
    pub available_cash: Option<f64>,
    pub cash_on_account: Option<f64>,
    pub actual_cash: Option<f64>,
    pub settled_cash: Option<f64>,
    pub unsettled_cash: Option<f64>,
    pub margin_utilized: Option<f64>,
}
