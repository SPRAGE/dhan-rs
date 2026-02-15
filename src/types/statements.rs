#![allow(missing_docs)]
//! Statement types — Ledger Report, Trade History.

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Ledger Entry
// ---------------------------------------------------------------------------

/// A single ledger entry from the trading account.
///
/// Returned by `GET /v2/ledger?from-date={}&to-date={}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    pub dhan_client_id: Option<String>,
    pub narration: Option<String>,
    pub voucherdate: Option<String>,
    pub exchange: Option<String>,
    pub voucherdesc: Option<String>,
    pub vouchernumber: Option<String>,
    pub debit: Option<String>,
    pub credit: Option<String>,
    pub runbal: Option<String>,
}

// ---------------------------------------------------------------------------
// Trade History Entry
// ---------------------------------------------------------------------------

/// A single historical trade entry.
///
/// Returned by `GET /v2/trades/{from-date}/{to-date}/{page}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeHistoryEntry {
    pub dhan_client_id: Option<String>,
    pub order_id: Option<String>,
    pub exchange_order_id: Option<String>,
    pub exchange_trade_id: Option<String>,
    pub transaction_type: Option<String>,
    pub exchange_segment: Option<String>,
    pub product_type: Option<String>,
    pub order_type: Option<String>,
    pub trading_symbol: Option<String>,
    pub custom_symbol: Option<String>,
    pub security_id: Option<String>,
    #[serde(default)]
    pub traded_quantity: Option<i64>,
    #[serde(default)]
    pub traded_price: Option<f64>,
    pub isin: Option<String>,
    pub instrument: Option<String>,
    #[serde(default)]
    pub sebi_tax: Option<f64>,
    #[serde(default)]
    pub stt: Option<f64>,
    #[serde(default)]
    pub brokerage_charges: Option<f64>,
    #[serde(default)]
    pub service_tax: Option<f64>,
    #[serde(default)]
    pub exchange_transaction_charges: Option<f64>,
    #[serde(default)]
    pub stamp_duty: Option<f64>,
    pub create_time: Option<String>,
    pub update_time: Option<String>,
    pub exchange_time: Option<String>,
    pub drv_expiry_date: Option<String>,
    pub drv_option_type: Option<String>,
    #[serde(default)]
    pub drv_strike_price: Option<f64>,
}
