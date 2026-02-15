//! Historical Data endpoints — Daily and Intraday candle data.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::historical::*;

impl DhanClient {
    /// Retrieve daily OHLCV candle data for an instrument.
    ///
    /// Data is available back to the instrument's inception date.
    ///
    /// **Endpoint:** `POST /v2/charts/historical`
    pub async fn get_daily_historical(
        &self,
        req: &HistoricalDataRequest,
    ) -> Result<CandleData> {
        self.post("/v2/charts/historical", req).await
    }

    /// Retrieve intraday OHLCV candle data for an instrument.
    ///
    /// Supports 1, 5, 15, 25, and 60-minute intervals.
    /// Only 90 days of data can be polled per request.
    ///
    /// **Endpoint:** `POST /v2/charts/intraday`
    pub async fn get_intraday_historical(
        &self,
        req: &IntradayDataRequest,
    ) -> Result<CandleData> {
        self.post("/v2/charts/intraday", req).await
    }
}
