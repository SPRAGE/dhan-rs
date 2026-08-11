//! Global Stocks REST API endpoint implementations.

use crate::client::{DhanClient, required_path_segment};
use crate::error::{DhanError, Result};
use crate::types::global_stocks::*;

fn require_non_empty(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(DhanError::InvalidArgument(format!(
            "{name} must not be empty"
        )));
    }
    Ok(())
}

fn require_positive_decimal(name: &str, value: &str) -> Result<()> {
    require_non_empty(name, value)?;
    let decimal = value.parse::<f64>().map_err(|_| {
        DhanError::InvalidArgument(format!("{name} must be a positive finite decimal string"))
    })?;
    if !decimal.is_finite() || decimal <= 0.0 {
        return Err(DhanError::InvalidArgument(format!(
            "{name} must be a positive finite decimal string"
        )));
    }
    Ok(())
}

fn validate_optional_positive(name: &str, value: Option<f64>) -> Result<()> {
    if let Some(value) = value {
        if !value.is_finite() || value <= 0.0 {
            return Err(DhanError::InvalidArgument(format!(
                "{name} must be a positive finite number"
            )));
        }
    }
    Ok(())
}

fn validate_order_request(request: &GlobalStockOrderRequest) -> Result<()> {
    require_non_empty("security_id", &request.security_id)?;
    if let Some(correlation_id) = &request.correlation_id {
        if correlation_id.chars().count() > 30 {
            return Err(DhanError::InvalidArgument(
                "correlation_id must not exceed 30 characters".into(),
            ));
        }
    }
    validate_optional_positive("quantity", request.quantity)?;
    validate_optional_positive("price", request.price)?;
    validate_optional_positive("trigger_price", request.trigger_price)?;
    validate_optional_positive("stop_loss_price", request.stop_loss_price)?;
    validate_optional_positive("target_price", request.target_price)?;
    validate_optional_positive("amount", request.amount)?;
    Ok(())
}

fn validate_modify_request(request: &GlobalStockModifyOrderRequest) -> Result<()> {
    require_non_empty("security_id", &request.security_id)?;
    validate_optional_positive("quantity", request.quantity)?;
    validate_optional_positive("price", request.price)
}

fn validate_estimator_request(request: &GlobalStockEstimatorRequest) -> Result<()> {
    require_non_empty("security_id", &request.security_id)?;
    require_positive_decimal("price", &request.price)?;
    require_positive_decimal("quantity", &request.quantity)
}

impl DhanClient {
    /// Retrieve all Global Stocks orders.
    ///
    /// **Endpoint:** `GET /v2/globalstocks/orders`
    pub async fn get_global_stock_orders(&self) -> Result<Vec<GlobalStockOrder>> {
        self.get("/v2/globalstocks/orders").await
    }

    /// Place a Global Stocks order.
    ///
    /// **Endpoint:** `POST /v2/globalstocks/orders`
    pub async fn place_global_stock_order(
        &self,
        request: &GlobalStockOrderRequest,
    ) -> Result<GlobalStockOrderStatusResponse> {
        validate_order_request(request)?;
        self.post("/v2/globalstocks/orders", request).await
    }

    /// Retrieve a Global Stocks order by ID.
    ///
    /// **Endpoint:** `GET /v2/globalstocks/orders/{order-id}`
    pub async fn get_global_stock_order(&self, order_id: &str) -> Result<GlobalStockOrder> {
        let order_id = required_path_segment("order_id", order_id)?;
        self.get(&format!("/v2/globalstocks/orders/{order_id}"))
            .await
    }

    /// Modify a Global Stocks order.
    ///
    /// **Endpoint:** `PUT /v2/globalstocks/orders/{order-id}`
    pub async fn modify_global_stock_order(
        &self,
        order_id: &str,
        request: &GlobalStockModifyOrderRequest,
    ) -> Result<GlobalStockOrderStatusResponse> {
        let order_id = required_path_segment("order_id", order_id)?;
        validate_modify_request(request)?;
        self.put(&format!("/v2/globalstocks/orders/{order_id}"), request)
            .await
    }

    /// Cancel a Global Stocks order.
    ///
    /// **Endpoint:** `DELETE /v2/globalstocks/orders/{order-id}`
    pub async fn cancel_global_stock_order(
        &self,
        order_id: &str,
    ) -> Result<GlobalStockOrderStatusResponse> {
        let order_id = required_path_segment("order_id", order_id)?;
        self.delete(&format!("/v2/globalstocks/orders/{order_id}"))
            .await
    }

    /// Estimate charges for a Global Stocks transaction.
    ///
    /// **Endpoint:** `POST /v2/globalstocks/transEstimate`
    pub async fn estimate_global_stock_order(
        &self,
        request: &GlobalStockEstimatorRequest,
    ) -> Result<GlobalStockEstimatorResponse> {
        validate_estimator_request(request)?;
        self.post("/v2/globalstocks/transEstimate", request).await
    }

    /// Calculate Global Stocks margin requirements.
    ///
    /// **Endpoint:** `POST /v2/globalstocks/margincalculator`
    pub async fn calculate_global_stock_margin(
        &self,
        request: &GlobalStockEstimatorRequest,
    ) -> Result<GlobalStockMarginResponse> {
        validate_estimator_request(request)?;
        self.post("/v2/globalstocks/margincalculator", request)
            .await
    }

    /// Retrieve all Global Stocks trades.
    ///
    /// **Endpoint:** `GET /v2/globalstocks/trades`
    pub async fn get_global_stock_trades(&self) -> Result<Vec<GlobalStockTrade>> {
        self.get("/v2/globalstocks/trades").await
    }

    /// Retrieve Global Stocks trades for one security.
    ///
    /// **Endpoint:** `GET /v2/globalstocks/trades/{security-id}`
    pub async fn get_global_stock_trades_for_security(
        &self,
        security_id: &str,
    ) -> Result<Vec<GlobalStockTrade>> {
        let security_id = required_path_segment("security_id", security_id)?;
        self.get(&format!("/v2/globalstocks/trades/{security_id}"))
            .await
    }

    /// Retrieve Global Stocks market status.
    ///
    /// **Endpoint:** `GET /v2/globalstocks/marketstatus`
    pub async fn get_global_stock_market_status(&self) -> Result<GlobalStockMarketStatus> {
        self.get("/v2/globalstocks/marketstatus").await
    }

    /// Retrieve Global Stocks holdings.
    ///
    /// **Endpoint:** `GET /v2/globalstocks/holdings`
    pub async fn get_global_stock_holdings(&self) -> Result<Vec<GlobalStockHolding>> {
        self.get("/v2/globalstocks/holdings").await
    }

    /// Retrieve Global Stocks cash and margin limits.
    ///
    /// **Endpoint:** `GET /v2/globalstocks/fundlimit`
    pub async fn get_global_stock_fund_limit(&self) -> Result<GlobalStockFundLimit> {
        self.get("/v2/globalstocks/fundlimit").await
    }
}
