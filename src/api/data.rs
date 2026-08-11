//! Data API endpoints for rolling options, technical metrics, market movers,
//! and company information.

use crate::client::DhanClient;
use crate::error::{DhanError, Result};
use crate::types::data::*;

impl DhanClient {
    /// Fetch continuous expired-option chart data.
    ///
    /// **Endpoint:** `POST /v2/charts/rollingoption`
    pub async fn get_rolling_option_data(
        &self,
        req: &RollingOptionRequest,
    ) -> Result<RollingOptionResponse> {
        req.validate()
            .map_err(|message| DhanError::InvalidArgument(message.into()))?;
        self.post("/v2/charts/rollingoption", req).await
    }

    /// Fetch point-in-time technical indicators for the latest closed candle.
    ///
    /// **Endpoint:** `POST /v2/data/technical`
    pub async fn get_technical_metrics(
        &self,
        req: &TechnicalMetricsRequest,
    ) -> Result<TechnicalMetricsResponse> {
        req.validate()
            .map_err(|message| DhanError::InvalidArgument(message.into()))?;
        self.post("/v2/data/technical", req).await
    }

    /// Fetch instruments ranked by OI, volume, or price movement.
    ///
    /// **Endpoint:** `POST /v2/data/marketmovers`
    pub async fn get_market_movers(
        &self,
        req: &MarketMoversRequest,
    ) -> Result<MarketMoversResponse> {
        req.validate()
            .map_err(|message| DhanError::InvalidArgument(message.into()))?;
        self.post("/v2/data/marketmovers", req).await
    }

    /// Fetch selected company overview, ratio, or shareholding sections.
    ///
    /// **Endpoint:** `POST /v2/data/companyinfo`
    pub async fn get_company_info(&self, req: &CompanyInfoRequest) -> Result<CompanyInfoResponse> {
        req.validate()
            .map_err(|message| DhanError::InvalidArgument(message.into()))?;
        self.post("/v2/data/companyinfo", req).await
    }
}
