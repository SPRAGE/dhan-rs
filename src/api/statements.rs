//! Statement endpoints — Ledger Report, Trade History.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::statements::*;

impl DhanClient {
    /// Retrieve Trading Account Ledger Report for a date range.
    ///
    /// Dates should be in `YYYY-MM-DD` format.
    ///
    /// **Endpoint:** `GET /v2/ledger?from-date={from}&to-date={to}`
    pub async fn get_ledger(&self, from_date: &str, to_date: &str) -> Result<Vec<LedgerEntry>> {
        let path = format!("/v2/ledger?from-date={from_date}&to-date={to_date}");
        self.get(&path).await
    }

    /// Retrieve historical trade data for a date range.
    ///
    /// Dates should be in `YYYY-MM-DD` format. Use `page = 0` as default.
    /// The response is paginated.
    ///
    /// **Endpoint:** `GET /v2/trades/{from-date}/{to-date}/{page}`
    pub async fn get_trade_history(
        &self,
        from_date: &str,
        to_date: &str,
        page: u32,
    ) -> Result<Vec<TradeHistoryEntry>> {
        let path = format!("/v2/trades/{from_date}/{to_date}/{page}");
        self.get(&path).await
    }
}
