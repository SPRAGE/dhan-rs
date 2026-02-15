//! Option Chain endpoints — full chain data, expiry list.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::option_chain::*;

impl DhanClient {
    /// Retrieve real-time Option Chain for a given underlying and expiry.
    ///
    /// Returns OI, Greeks, Volume, LTP, Best Bid/Ask and IV across all strikes.
    ///
    /// Rate limit: 1 unique request every 3 seconds.
    ///
    /// **Endpoint:** `POST /v2/optionchain`
    pub async fn get_option_chain(
        &self,
        req: &OptionChainRequest,
    ) -> Result<OptionChainResponse> {
        self.post("/v2/optionchain", req).await
    }

    /// Retrieve all active expiry dates for an underlying instrument.
    ///
    /// Rate limit: 1 unique request every 3 seconds.
    ///
    /// **Endpoint:** `POST /v2/optionchain/expirylist`
    pub async fn get_expiry_list(
        &self,
        req: &ExpiryListRequest,
    ) -> Result<ExpiryListResponse> {
        self.post("/v2/optionchain/expirylist", req).await
    }
}
