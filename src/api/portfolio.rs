//! Portfolio endpoints — Holdings, Positions, Convert Position, Exit All.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::portfolio::*;

impl DhanClient {
    /// Retrieve all holdings in the demat account.
    ///
    /// **Endpoint:** `GET /v2/holdings`
    pub async fn get_holdings(&self) -> Result<Vec<Holding>> {
        self.get("/v2/holdings").await
    }

    /// Retrieve all open positions for the day.
    ///
    /// **Endpoint:** `GET /v2/positions`
    pub async fn get_positions(&self) -> Result<Vec<Position>> {
        self.get("/v2/positions").await
    }

    /// Convert a position's product type (e.g. intraday → delivery).
    ///
    /// Returns `202 Accepted` on success.
    ///
    /// **Endpoint:** `POST /v2/positions/convert`
    pub async fn convert_position(&self, req: &ConvertPositionRequest) -> Result<()> {
        self.post_no_content("/v2/positions/convert", req).await
    }

    /// Exit all active positions and cancel all open orders.
    ///
    /// **Endpoint:** `DELETE /v2/positions`
    pub async fn exit_all_positions(&self) -> Result<ExitAllResponse> {
        self.delete("/v2/positions").await
    }
}
