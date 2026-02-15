#![allow(missing_docs)]
//! Historical Data types — Daily and Intraday candles.

use serde::{Deserialize, Serialize};

use crate::types::enums::*;

// ---------------------------------------------------------------------------
// Daily Historical Data Request
// ---------------------------------------------------------------------------

/// Request body for daily historical data.
///
/// Used by `POST /v2/charts/historical`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalDataRequest {
    pub security_id: String,
    pub exchange_segment: ExchangeSegment,
    pub instrument: Instrument,
    /// Expiry code for derivatives (`0` = Near, `1` = Next, `2` = Far).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_code: Option<u8>,
    /// Include open interest data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oi: Option<bool>,
    /// Start date (YYYY-MM-DD).
    pub from_date: String,
    /// End date (YYYY-MM-DD, non-inclusive).
    pub to_date: String,
}

// ---------------------------------------------------------------------------
// Intraday Historical Data Request
// ---------------------------------------------------------------------------

/// Request body for intraday historical data.
///
/// Used by `POST /v2/charts/intraday`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntradayDataRequest {
    pub security_id: String,
    pub exchange_segment: ExchangeSegment,
    pub instrument: Instrument,
    /// Minute interval: 1, 5, 15, 25, or 60.
    pub interval: String,
    /// Include open interest data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oi: Option<bool>,
    /// Start date/time (YYYY-MM-DD HH:MM:SS).
    pub from_date: String,
    /// End date/time (YYYY-MM-DD HH:MM:SS).
    pub to_date: String,
}

// ---------------------------------------------------------------------------
// Candle Data Response
// ---------------------------------------------------------------------------

/// OHLCV candle data returned by both daily and intraday endpoints.
///
/// Each field is a parallel array — index `i` across all arrays corresponds
/// to the same candle.
#[derive(Debug, Clone, Deserialize)]
pub struct CandleData {
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<i64>,
    /// Epoch timestamps (seconds).
    pub timestamp: Vec<i64>,
    /// Open interest (empty or zeroed for equity).
    #[serde(default)]
    pub open_interest: Vec<i64>,
}
