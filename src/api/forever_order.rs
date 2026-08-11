//! Forever Order endpoints.

use crate::client::{DhanClient, required_path_segment};
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
        let order_id = required_path_segment("order_id", order_id)?;
        self.put(&format!("/v2/forever/orders/{order_id}"), req)
            .await
    }

    /// Delete a pending forever order.
    ///
    /// **Endpoint:** `DELETE /v2/forever/orders/{order-id}`
    pub async fn delete_forever_order(&self, order_id: &str) -> Result<OrderResponse> {
        let order_id = required_path_segment("order_id", order_id)?;
        self.delete(&format!("/v2/forever/orders/{order_id}")).await
    }

    /// Retrieve all existing forever orders.
    ///
    /// **Endpoint:** `GET /v2/forever/all`
    pub async fn get_all_forever_orders(&self) -> Result<Vec<ForeverOrderDetail>> {
        self.get("/v2/forever/all").await
    }

    /// Retrieve all forever orders using the route in Dhan's linked OpenAPI.
    ///
    /// Dhan's HTML endpoint example currently uses `/v2/forever/all`, exposed
    /// by [`Self::get_all_forever_orders`], while its summary and linked
    /// OpenAPI use this `/v2/forever/orders` route. Both are explicit so the
    /// caller can select the contract available to its account/environment.
    ///
    /// **Endpoint:** `GET /v2/forever/orders`
    pub async fn get_all_forever_orders_openapi(&self) -> Result<Vec<ForeverOrderDetail>> {
        self.get("/v2/forever/orders").await
    }
}
