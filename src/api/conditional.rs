//! Conditional Trigger endpoints.

use crate::client::{DhanClient, required_path_segment};
use crate::error::{DhanError, Result};
use crate::types::conditional::*;

impl DhanClient {
    /// Place multiple orders without an alert condition.
    ///
    /// **Endpoint:** `POST /v2/alerts/multi/orders`
    pub async fn place_multi_order(&self, req: &MultiOrderRequest) -> Result<MultiOrderResponse> {
        req.validate()
            .map_err(|message| DhanError::InvalidArgument(message.into()))?;
        self.post("/v2/alerts/multi/orders", req).await
    }

    /// Place a new conditional trigger.
    ///
    /// **Endpoint:** `POST /v2/alerts/orders`
    pub async fn place_conditional_trigger(
        &self,
        req: &ConditionalTriggerRequest,
    ) -> Result<ConditionalTriggerResponse> {
        self.post("/v2/alerts/orders", req).await
    }

    /// Modify an existing conditional trigger.
    ///
    /// **Endpoint:** `PUT /v2/alerts/orders/{alertId}`
    pub async fn modify_conditional_trigger(
        &self,
        alert_id: &str,
        req: &ConditionalTriggerRequest,
    ) -> Result<ConditionalTriggerResponse> {
        let alert_id = required_path_segment("alert_id", alert_id)?;
        self.put(&format!("/v2/alerts/orders/{alert_id}"), req)
            .await
    }

    /// Delete a conditional trigger.
    ///
    /// **Endpoint:** `DELETE /v2/alerts/orders/{alertId}`
    pub async fn delete_conditional_trigger(
        &self,
        alert_id: &str,
    ) -> Result<ConditionalTriggerResponse> {
        let alert_id = required_path_segment("alert_id", alert_id)?;
        self.delete(&format!("/v2/alerts/orders/{alert_id}")).await
    }

    /// Get a specific conditional trigger by its ID.
    ///
    /// **Endpoint:** `GET /v2/alerts/orders/{alertId}`
    pub async fn get_conditional_trigger(
        &self,
        alert_id: &str,
    ) -> Result<ConditionalTriggerDetail> {
        let alert_id = required_path_segment("alert_id", alert_id)?;
        self.get(&format!("/v2/alerts/orders/{alert_id}")).await
    }

    /// Get all conditional triggers.
    ///
    /// **Endpoint:** `GET /v2/alerts/orders`
    pub async fn get_all_conditional_triggers(&self) -> Result<Vec<ConditionalTriggerDetail>> {
        self.get("/v2/alerts/orders").await
    }
}
