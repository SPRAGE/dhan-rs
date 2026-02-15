#![allow(missing_docs)]
//! Market Quote types — LTP, OHLC, Market Depth (REST snapshots).

use std::collections::HashMap;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// Request body for market quote endpoints.
///
/// The body is a map of exchange segment name → list of security IDs.
/// Example: `{ "NSE_EQ": [11536], "NSE_FNO": [49081, 49082] }`
pub type MarketQuoteRequest = HashMap<String, Vec<u64>>;

// ---------------------------------------------------------------------------
// Ticker (LTP) response
// ---------------------------------------------------------------------------

/// Single security LTP data.
#[derive(Debug, Clone, Deserialize)]
pub struct TickerData {
    pub last_price: f64,
}

/// Response from `POST /v2/marketfeed/ltp`.
#[derive(Debug, Clone, Deserialize)]
pub struct MarketQuoteResponse<T> {
    pub data: HashMap<String, HashMap<String, T>>,
    pub status: String,
}

// ---------------------------------------------------------------------------
// OHLC response
// ---------------------------------------------------------------------------

/// OHLC values.
#[derive(Debug, Clone, Deserialize)]
pub struct OhlcValues {
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
}

/// Single security OHLC data.
#[derive(Debug, Clone, Deserialize)]
pub struct OhlcData {
    pub last_price: f64,
    pub ohlc: OhlcValues,
}

// ---------------------------------------------------------------------------
// Full Quote (Market Depth) response
// ---------------------------------------------------------------------------

/// A single level of market depth.
#[derive(Debug, Clone, Deserialize)]
pub struct DepthLevel {
    pub quantity: i64,
    pub orders: i64,
    pub price: f64,
}

/// Buy and sell depth.
#[derive(Debug, Clone, Deserialize)]
pub struct DepthData {
    pub buy: Vec<DepthLevel>,
    pub sell: Vec<DepthLevel>,
}

/// Full quote data for a single security.
#[derive(Debug, Clone, Deserialize)]
pub struct QuoteData {
    #[serde(default)]
    pub average_price: Option<f64>,
    #[serde(default)]
    pub buy_quantity: Option<i64>,
    #[serde(default)]
    pub sell_quantity: Option<i64>,
    pub depth: Option<DepthData>,
    pub last_price: f64,
    #[serde(default)]
    pub last_quantity: Option<i64>,
    pub last_trade_time: Option<String>,
    #[serde(default)]
    pub lower_circuit_limit: Option<f64>,
    #[serde(default)]
    pub upper_circuit_limit: Option<f64>,
    #[serde(default)]
    pub net_change: Option<f64>,
    pub ohlc: Option<OhlcValues>,
    #[serde(default)]
    pub oi: Option<i64>,
    #[serde(default)]
    pub oi_day_high: Option<i64>,
    #[serde(default)]
    pub oi_day_low: Option<i64>,
    #[serde(default)]
    pub volume: Option<i64>,
}
