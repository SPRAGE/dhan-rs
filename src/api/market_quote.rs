//! Market Quote endpoints — LTP, OHLC, Market Depth snapshots.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::market_quote::*;

impl DhanClient {
    /// Retrieve LTP (Last Traded Price) for a list of instruments.
    ///
    /// Up to 1000 instruments per request (rate limit: 1 req/sec).
    ///
    /// **Endpoint:** `POST /v2/marketfeed/ltp`
    pub async fn get_ltp(
        &self,
        instruments: &MarketQuoteRequest,
    ) -> Result<MarketQuoteResponse<TickerData>> {
        self.post("/v2/marketfeed/ltp", instruments).await
    }

    /// Retrieve OHLC + LTP for a list of instruments.
    ///
    /// Up to 1000 instruments per request (rate limit: 1 req/sec).
    ///
    /// **Endpoint:** `POST /v2/marketfeed/ohlc`
    pub async fn get_ohlc(
        &self,
        instruments: &MarketQuoteRequest,
    ) -> Result<MarketQuoteResponse<OhlcData>> {
        self.post("/v2/marketfeed/ohlc", instruments).await
    }

    /// Retrieve full market depth (quote) data for a list of instruments.
    ///
    /// Includes depth, OHLC, OI, volume and circuit limits.
    /// Up to 1000 instruments per request (rate limit: 1 req/sec).
    ///
    /// **Endpoint:** `POST /v2/marketfeed/quote`
    pub async fn get_quote(
        &self,
        instruments: &MarketQuoteRequest,
    ) -> Result<MarketQuoteResponse<QuoteData>> {
        self.post("/v2/marketfeed/quote", instruments).await
    }
}
