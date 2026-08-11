//! Super Order endpoints.

use crate::client::{DhanClient, required_path_segment};
use crate::error::{DhanError, Result};
use crate::types::orders::OrderResponse;
use crate::types::super_order::*;

impl DhanClient {
    /// Place a new super order.
    ///
    /// **Endpoint:** `POST /v2/super/orders`
    pub async fn place_super_order(&self, req: &PlaceSuperOrderRequest) -> Result<OrderResponse> {
        self.post("/v2/super/orders", req).await
    }

    /// Modify a pending super order.
    ///
    /// **Endpoint:** `PUT /v2/super/orders/{order-id}`
    pub async fn modify_super_order(
        &self,
        order_id: &str,
        req: &ModifySuperOrderRequest,
    ) -> Result<OrderResponse> {
        let order_id = required_path_segment("order_id", order_id)?;
        self.put(&format!("/v2/super/orders/{order_id}"), req).await
    }

    /// Cancel a super order leg.
    ///
    /// Cancelling the `ENTRY_LEG` cancels all legs.
    ///
    /// **Endpoint:** `DELETE /v2/super/orders/{order-id}/{order-leg}`
    pub async fn cancel_super_order(&self, order_id: &str, leg: &str) -> Result<OrderResponse> {
        let order_id = required_path_segment("order_id", order_id)?;
        validate_super_order_leg(leg)?;
        self.delete(&format!("/v2/super/orders/{order_id}/{leg}"))
            .await
    }

    /// Cancel a super order leg when the server follows the HTML contract and
    /// returns a successful empty response.
    ///
    /// Dhan's linked OpenAPI describes a `200` JSON [`OrderResponse`] for this
    /// operation, while the HTML page describes `202 Accepted` with no body.
    /// [`Self::cancel_super_order`] models the OpenAPI form; this method models
    /// the HTML form without attempting to deserialize an empty response.
    ///
    /// **Endpoint:** `DELETE /v2/super/orders/{order-id}/{order-leg}`
    pub async fn cancel_super_order_no_content(&self, order_id: &str, leg: &str) -> Result<()> {
        let order_id = required_path_segment("order_id", order_id)?;
        validate_super_order_leg(leg)?;
        self.delete_no_content(&format!("/v2/super/orders/{order_id}/{leg}"))
            .await
    }

    /// Retrieve all super orders for the day.
    ///
    /// **Endpoint:** `GET /v2/super/orders`
    pub async fn get_super_orders(&self) -> Result<Vec<SuperOrderDetail>> {
        self.get("/v2/super/orders").await
    }
}

fn validate_super_order_leg(leg: &str) -> Result<()> {
    if matches!(leg, "ENTRY_LEG" | "TARGET_LEG" | "STOP_LOSS_LEG") {
        Ok(())
    } else {
        Err(DhanError::InvalidArgument(
            "leg must be ENTRY_LEG, TARGET_LEG, or STOP_LOSS_LEG".into(),
        ))
    }
}
