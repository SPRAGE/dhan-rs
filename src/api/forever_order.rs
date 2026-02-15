//! Forever Order endpoints.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::forever_order::*;
use crate::types::orders::OrderResponse;

impl DhanClient {
    /// Create a new forever order.
    ///
    /// **Endpoint:** `POST /v2/forever/orders`
    pub async fn create_forever_order(
        &self,
        req: &CreateForeverOrderRequest,
    ) -> Result<OrderResponse> {
        self.post("/v2/forever/orders", req).await
    }

    /// Modify an existing forever order.
    ///
    /// **Endpoint:** `PUT /v2/forever/orders/{order-id}`
    pub async fn modify_forever_order(
        &self,
        order_id: &str,
        req: &ModifyForeverOrderRequest,
    ) -> Result<OrderResponse> {
        self.put(&format!("/v2/forever/orders/{order_id}"), req)
            .await
    }

    /// Delete a pending forever order.
    ///
    /// **Endpoint:** `DELETE /v2/forever/orders/{order-id}`
    pub async fn delete_forever_order(&self, order_id: &str) -> Result<OrderResponse> {
        self.delete(&format!("/v2/forever/orders/{order_id}")).await
    }

    /// Retrieve all existing forever orders.
    ///
    /// **Endpoint:** `GET /v2/forever/all`
    pub async fn get_all_forever_orders(&self) -> Result<Vec<ForeverOrderDetail>> {
        self.get("/v2/forever/all").await
    }
}
