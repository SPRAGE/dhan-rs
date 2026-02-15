//! Super Order endpoints.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::orders::OrderResponse;
use crate::types::super_order::*;

impl DhanClient {
    /// Place a new super order.
    ///
    /// **Endpoint:** `POST /v2/super/orders`
    pub async fn place_super_order(
        &self,
        req: &PlaceSuperOrderRequest,
    ) -> Result<OrderResponse> {
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
        self.put(&format!("/v2/super/orders/{order_id}"), req).await
    }

    /// Cancel a super order leg.
    ///
    /// Cancelling the `ENTRY_LEG` cancels all legs.
    ///
    /// **Endpoint:** `DELETE /v2/super/orders/{order-id}/{order-leg}`
    pub async fn cancel_super_order(
        &self,
        order_id: &str,
        leg: &str,
    ) -> Result<OrderResponse> {
        self.delete(&format!("/v2/super/orders/{order_id}/{leg}"))
            .await
    }

    /// Retrieve all super orders for the day.
    ///
    /// **Endpoint:** `GET /v2/super/orders`
    pub async fn get_super_orders(&self) -> Result<Vec<SuperOrderDetail>> {
        self.get("/v2/super/orders").await
    }
}
