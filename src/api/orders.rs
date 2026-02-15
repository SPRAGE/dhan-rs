//! Order management endpoints.

use crate::client::DhanClient;
use crate::error::Result;
use crate::types::orders::*;

impl DhanClient {
    /// Place a new order.
    ///
    /// **Endpoint:** `POST /v2/orders`
    pub async fn place_order(&self, req: &PlaceOrderRequest) -> Result<OrderResponse> {
        self.post("/v2/orders", req).await
    }

    /// Modify a pending order.
    ///
    /// **Endpoint:** `PUT /v2/orders/{order-id}`
    pub async fn modify_order(
        &self,
        order_id: &str,
        req: &ModifyOrderRequest,
    ) -> Result<OrderResponse> {
        self.put(&format!("/v2/orders/{order_id}"), req).await
    }

    /// Cancel a pending order.
    ///
    /// **Endpoint:** `DELETE /v2/orders/{order-id}`
    pub async fn cancel_order(&self, order_id: &str) -> Result<OrderResponse> {
        self.delete(&format!("/v2/orders/{order_id}")).await
    }

    /// Slice an order into multiple legs (for quantities over freeze limit).
    ///
    /// **Endpoint:** `POST /v2/orders/slicing`
    pub async fn slice_order(&self, req: &PlaceOrderRequest) -> Result<Vec<OrderResponse>> {
        self.post("/v2/orders/slicing", req).await
    }

    /// Retrieve all orders for the day.
    ///
    /// **Endpoint:** `GET /v2/orders`
    pub async fn get_orders(&self) -> Result<Vec<OrderDetail>> {
        self.get("/v2/orders").await
    }

    /// Retrieve a specific order by its ID.
    ///
    /// **Endpoint:** `GET /v2/orders/{order-id}`
    pub async fn get_order(&self, order_id: &str) -> Result<OrderDetail> {
        self.get(&format!("/v2/orders/{order_id}")).await
    }

    /// Retrieve an order by its correlation ID.
    ///
    /// **Endpoint:** `GET /v2/orders/external/{correlation-id}`
    pub async fn get_order_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<OrderDetail> {
        self.get(&format!("/v2/orders/external/{correlation_id}"))
            .await
    }

    /// Retrieve all trades for the day.
    ///
    /// **Endpoint:** `GET /v2/trades`
    pub async fn get_trades(&self) -> Result<Vec<TradeDetail>> {
        self.get("/v2/trades").await
    }

    /// Retrieve trades for a specific order.
    ///
    /// **Endpoint:** `GET /v2/trades/{order-id}`
    pub async fn get_trades_for_order(&self, order_id: &str) -> Result<Vec<TradeDetail>> {
        self.get(&format!("/v2/trades/{order_id}")).await
    }
}
