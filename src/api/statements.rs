//! Statement endpoints — Ledger Report, Trade History.

use crate::client::{DhanClient, required_path_segment, required_query_value};
use crate::error::Result;
use crate::types::statements::*;

impl DhanClient {
    /// Retrieve Trading Account Ledger Report for a date range.
    ///
    /// Dates should be in `YYYY-MM-DD` format.
    ///
    /// **Endpoint:** `GET /v2/ledger?from-date={from}&to-date={to}`
    pub async fn get_ledger_response(
        &self,
        from_date: &str,
        to_date: &str,
    ) -> Result<LedgerResponse> {
        let from_date = required_query_value("from_date", from_date)?;
        let to_date = required_query_value("to_date", to_date)?;
        let path = format!("/v2/ledger?from-date={from_date}&to-date={to_date}");
        self.get(&path).await
    }

    /// Retrieve ledger entries while preserving the pre-0.1.7 return type.
    ///
    /// New callers that need the documented client metadata should use
    /// [`Self::get_ledger_response`].
    pub async fn get_ledger(&self, from_date: &str, to_date: &str) -> Result<Vec<LedgerEntry>> {
        Ok(self
            .get_ledger_response(from_date, to_date)
            .await?
            .into_entries())
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
        let from_date = required_path_segment("from_date", from_date)?;
        let to_date = required_path_segment("to_date", to_date)?;
        let path = format!("/v2/trades/{from_date}/{to_date}/{page}");
        self.get(&path).await
    }
}
