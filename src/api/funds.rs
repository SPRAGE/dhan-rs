//! Funds & Margin endpoints — Margin Calculator, Fund Limit.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::funds::*;

impl DhanClient {
    /// Calculate margin requirement for a single order.
    ///
    /// **Endpoint:** `POST /v2/margincalculator`
    pub async fn calculate_margin(
        &self,
        req: &MarginCalculatorRequest,
    ) -> Result<MarginCalculatorResponse> {
        self.post("/v2/margincalculator", req).await
    }

    /// Calculate margin requirements for multiple scripts in a single request.
    ///
    /// **Endpoint:** `POST /v2/margincalculator/multi`
    pub async fn calculate_multi_margin(
        &self,
        req: &MultiMarginRequest,
    ) -> Result<MultiMarginResponse> {
        self.post("/v2/margincalculator/multi", req).await
    }

    /// Retrieve fund limits for the trading account.
    ///
    /// Returns balance, margin utilised, collateral, and other fund details.
    ///
    /// **Endpoint:** `GET /v2/fundlimit`
    pub async fn get_fund_limit(&self) -> Result<FundLimit> {
        self.get("/v2/fundlimit").await
    }
}
