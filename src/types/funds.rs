//! Funds & Margin types.

use serde::{Deserialize, Serialize};

use crate::types::enums::*;

// ---------------------------------------------------------------------------
// Margin Calculator (single)
// ---------------------------------------------------------------------------

/// Request body for calculating margin for a single order.
///
/// Used by `POST /v2/margincalculator`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginCalculatorRequest {
    pub dhan_client_id: String,
    pub exchange_segment: ExchangeSegment,
    pub transaction_type: TransactionType,
    pub quantity: u64,
    pub product_type: ProductType,
    pub security_id: String,
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
}

/// Response from single margin calculation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginCalculatorResponse {
    #[serde(default)]
    pub total_margin: Option<f64>,
    #[serde(default)]
    pub span_margin: Option<f64>,
    #[serde(default)]
    pub exposure_margin: Option<f64>,
    #[serde(default)]
    pub available_balance: Option<f64>,
    #[serde(default)]
    pub variable_margin: Option<f64>,
    #[serde(default)]
    pub insufficient_balance: Option<f64>,
    #[serde(default)]
    pub brokerage: Option<f64>,
    #[serde(default)]
    pub leverage: Option<String>,
}

// ---------------------------------------------------------------------------
// Multi Order Margin Calculator
// ---------------------------------------------------------------------------

/// A single script entry within a multi-margin request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginScript {
    pub exchange_segment: ExchangeSegment,
    pub transaction_type: TransactionType,
    pub quantity: u64,
    pub product_type: ProductType,
    pub security_id: String,
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
}

/// Request body for calculating margin for multiple scripts.
///
/// Used by `POST /v2/margincalculator/multi`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiMarginRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_position: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_orders: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhan_client_id: Option<String>,
    #[serde(alias = "scripList")]
    pub scripts: Vec<MarginScript>,
}

/// Response from multi-margin calculation.
///
/// Note: field names use snake_case in the API response.
#[derive(Debug, Clone, Deserialize)]
pub struct MultiMarginResponse {
    pub total_margin: Option<String>,
    pub span_margin: Option<String>,
    pub exposure_margin: Option<String>,
    pub equity_margin: Option<String>,
    pub fo_margin: Option<String>,
    pub commodity_margin: Option<String>,
    pub currency: Option<String>,
    pub hedge_benefit: Option<String>,
}

// ---------------------------------------------------------------------------
// Fund Limit
// ---------------------------------------------------------------------------

/// Fund limit details for the trading account.
///
/// Returned by `GET /v2/fundlimit`.
///
/// Note: The API misspells `availabelBalance` (missing 'l' in 'available').
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundLimit {
    pub dhan_client_id: Option<String>,
    /// Available amount to trade.
    /// Note: field is misspelled in the API as `availabelBalance`.
    #[serde(alias = "availabelBalance")]
    pub available_balance: Option<f64>,
    #[serde(default)]
    pub sod_limit: Option<f64>,
    #[serde(default)]
    pub collateral_amount: Option<f64>,
    #[serde(default)]
    pub receiveable_amount: Option<f64>,
    #[serde(default)]
    pub utilized_amount: Option<f64>,
    #[serde(default)]
    pub blocked_payout_amount: Option<f64>,
    #[serde(default)]
    pub withdrawable_balance: Option<f64>,
}
