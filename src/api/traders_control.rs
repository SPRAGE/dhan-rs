//! Trader's Control endpoints — Kill Switch, P&L Based Exit.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::traders_control::*;

impl DhanClient {
    /// Activate or deactivate the kill switch for the current trading day.
    ///
    /// Pass `"ACTIVATE"` or `"DEACTIVATE"` as the `status` parameter.
    ///
    /// **Endpoint:** `POST /v2/killswitch?killSwitchStatus={status}`
    pub async fn manage_kill_switch(&self, status: &str) -> Result<KillSwitchResponse> {
        let path = format!("/v2/killswitch?killSwitchStatus={status}");
        // POST with no body — send an empty JSON object.
        self.post(&path, &serde_json::json!({})).await
    }

    /// Retrieve current kill switch status.
    ///
    /// **Endpoint:** `GET /v2/killswitch`
    pub async fn get_kill_switch_status(&self) -> Result<KillSwitchResponse> {
        self.get("/v2/killswitch").await
    }

    /// Configure P&L-based auto-exit for the current trading day.
    ///
    /// **Endpoint:** `PUT /v2/pnlExit`
    pub async fn set_pnl_exit(&self, req: &PnlExitRequest) -> Result<PnlExitResponse> {
        self.put("/v2/pnlExit", req).await
    }

    /// Disable the active P&L-based exit configuration.
    ///
    /// **Endpoint:** `DELETE /v2/pnlExit`
    pub async fn stop_pnl_exit(&self) -> Result<PnlExitResponse> {
        self.delete("/v2/pnlExit").await
    }

    /// Fetch the currently active P&L-based exit configuration.
    ///
    /// **Endpoint:** `GET /v2/pnlExit`
    pub async fn get_pnl_exit(&self) -> Result<PnlExitConfig> {
        self.get("/v2/pnlExit").await
    }
}
