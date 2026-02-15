#![allow(missing_docs)]
//! Option Chain types — chain data, greeks, expiry list.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// Request body for fetching the option chain.
///
/// Used by `POST /v2/optionchain`.
///
/// Note: field names use PascalCase in the API.
#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct OptionChainRequest {
    /// Security ID of the underlying instrument.
    pub UnderlyingScrip: u64,
    /// Exchange & segment of the underlying.
    pub UnderlyingSeg: String,
    /// Expiry date (YYYY-MM-DD).
    pub Expiry: String,
}

/// Request body for fetching the expiry list.
///
/// Used by `POST /v2/optionchain/expirylist`.
#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct ExpiryListRequest {
    /// Security ID of the underlying instrument.
    pub UnderlyingScrip: u64,
    /// Exchange & segment of the underlying.
    pub UnderlyingSeg: String,
}

// ---------------------------------------------------------------------------
// Greeks
// ---------------------------------------------------------------------------

/// Option greeks for a single strike.
#[derive(Debug, Clone, Deserialize)]
pub struct Greeks {
    pub delta: f64,
    pub theta: f64,
    pub gamma: f64,
    pub vega: f64,
}

// ---------------------------------------------------------------------------
// Option Data (per CE/PE)
// ---------------------------------------------------------------------------

/// Data for a single call or put at a given strike.
#[derive(Debug, Clone, Deserialize)]
pub struct OptionData {
    #[serde(default)]
    pub average_price: Option<f64>,
    pub greeks: Option<Greeks>,
    #[serde(default)]
    pub implied_volatility: Option<f64>,
    pub last_price: f64,
    #[serde(default)]
    pub oi: Option<i64>,
    #[serde(default)]
    pub previous_close_price: Option<f64>,
    #[serde(default)]
    pub previous_oi: Option<i64>,
    #[serde(default)]
    pub previous_volume: Option<i64>,
    #[serde(default)]
    pub security_id: Option<u64>,
    #[serde(default)]
    pub top_ask_price: Option<f64>,
    #[serde(default)]
    pub top_ask_quantity: Option<i64>,
    #[serde(default)]
    pub top_bid_price: Option<f64>,
    #[serde(default)]
    pub top_bid_quantity: Option<i64>,
    #[serde(default)]
    pub volume: Option<i64>,
}

// ---------------------------------------------------------------------------
// Strike Data
// ---------------------------------------------------------------------------

/// Call and Put data at a given strike price.
#[derive(Debug, Clone, Deserialize)]
pub struct StrikeData {
    /// Call option data (may be absent if no CE at this strike).
    pub ce: Option<OptionData>,
    /// Put option data (may be absent if no PE at this strike).
    pub pe: Option<OptionData>,
}

// ---------------------------------------------------------------------------
// Option Chain Response
// ---------------------------------------------------------------------------

/// Inner data envelope of the option chain response.
#[derive(Debug, Clone, Deserialize)]
pub struct OptionChainData {
    /// LTP of the underlying.
    pub last_price: f64,
    /// Strike-wise option chain. Keys are strike prices as strings (e.g. `"25650.000000"`).
    pub oc: HashMap<String, StrikeData>,
}

/// Response from `POST /v2/optionchain`.
#[derive(Debug, Clone, Deserialize)]
pub struct OptionChainResponse {
    pub data: OptionChainData,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Expiry List Response
// ---------------------------------------------------------------------------

/// Response from `POST /v2/optionchain/expirylist`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExpiryListResponse {
    /// List of expiry dates (YYYY-MM-DD).
    pub data: Vec<String>,
    pub status: String,
}
