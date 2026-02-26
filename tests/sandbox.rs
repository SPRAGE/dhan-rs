//! Integration tests against the DhanHQ Sandbox (`https://sandbox.dhan.co/v2`).
//!
//! # Running
//!
//! These tests require real DhanHQ credentials. Set the following environment
//! variables before running:
//!
//! ```sh
//! export DHAN_CLIENT_ID="your-client-id"
//! export DHAN_ACCESS_TOKEN="your-access-token"
//! cargo test --test sandbox -- --nocapture
//! ```
//!
//! Without these env vars, every test is silently skipped.
//!
//! # Sandbox limitations
//!
//! The sandbox at `https://sandbox.dhan.co` implements only a **subset** of the
//! production API. The following are known to be absent or return stub errors:
//!
//! - **Market Quote** (`/marketfeed/ltp`, `/ohlc`, `/quote`) — not implemented (404)
//! - **Super Orders** (`/super/orders`) — not implemented (404)
//! - **Option Chain** (`/optionchain`) — not implemented (404)
//! - **Kill Switch GET** — sandbox only supports POST, not GET status
//! - **Forever Orders list** — sandbox uses `GET /forever/orders` instead of
//!   production's `GET /forever/all`
//! - **Order Book / Trade Book / Holdings / Positions / Fund Limit** — may
//!   return stub error responses depending on account state
//!
//! Tests for these endpoints are marked to gracefully skip when the sandbox
//! returns unsupported errors.
//!
//! # What is tested
//!
//! - **Profile** — validates token & deserialization
//! - **Orders** — place and cancel
//! - **Historical** — daily & intraday candles
//! - **Statements** — ledger & trade history
//! - **Error handling** — verifies bad requests produce typed `DhanError::Api`
//! - **Sandbox-limited endpoints** — exercised but tolerate known sandbox errors

use std::collections::HashMap;

use dhan_rs::client::DhanClient;
use dhan_rs::error::DhanError;
use dhan_rs::types::enums::*;
use dhan_rs::types::funds::MarginCalculatorRequest;
use dhan_rs::types::historical::{HistoricalDataRequest, IntradayDataRequest};
use dhan_rs::types::orders::{ModifyOrderRequest, PlaceOrderRequest};

const SANDBOX_BASE_URL: &str = "https://sandbox.dhan.co";

/// TCS on NSE — a liquid, well-known security for testing.
const TCS_SECURITY_ID: &str = "11536";

/// Helper: create a sandbox client or skip the test.
fn sandbox_client() -> Option<DhanClient> {
    let client_id = std::env::var("DHAN_CLIENT_ID").ok()?;
    let token = std::env::var("DHAN_ACCESS_TOKEN").ok()?;
    if client_id.is_empty() || token.is_empty() {
        return None;
    }
    Some(DhanClient::with_base_url(
        client_id,
        token,
        SANDBOX_BASE_URL,
    ))
}

/// Macro to skip a test when credentials are missing.
macro_rules! require_client {
    () => {
        match sandbox_client() {
            Some(c) => c,
            None => {
                eprintln!("⏭  Skipped (DHAN_CLIENT_ID / DHAN_ACCESS_TOKEN not set)");
                return;
            }
        }
    };
}

/// Returns `true` if the error is a known sandbox limitation (404, 504, or
/// certain stub error types the sandbox returns for unimplemented features).
fn is_sandbox_limitation(err: &DhanError) -> bool {
    match err {
        DhanError::HttpStatus { status, .. } => {
            let code = status.as_u16();
            // 404 = endpoint not in sandbox, 504 = sandbox gateway timeout
            code == 404 || code == 504
        }
        DhanError::Api(body) => {
            // Sandbox returns these stub error types for endpoints it nominally
            // exposes but doesn't properly implement.
            matches!(
                body.error_type.as_deref(),
                Some(
                    "HOLDING_ERROR"
                        | "CONVERT_POSITION_ERROR"
                        | "FUND_LIMIT_ERROR"
                        | "TRADE_RESOURCE_ERROR"
                        | "Input_Exception"
                )
            ) || matches!(
                body.error_code.as_deref(),
                Some("DH-905" | "DH-906")
            )
        }
        _ => false,
    }
}

/// Macro to skip a test when the result is a known sandbox limitation.
macro_rules! sandbox_ok_or_skip {
    ($result:expr, $label:expr) => {
        match $result {
            Ok(val) => val,
            Err(ref e) if is_sandbox_limitation(e) => {
                eprintln!("⏭  Skipped (sandbox limitation): {} — {e}", $label);
                return;
            }
            Err(e) => panic!("{} failed: {e}", $label),
        }
    };
}

// ===================================================================
// Profile
// ===================================================================

#[tokio::test]
async fn test_profile() {
    let client = require_client!();
    let profile = client.get_profile().await.expect("get_profile failed");
    assert!(
        !profile.dhan_client_id.is_empty(),
        "profile should contain a client id"
    );
    println!(
        "✔ Profile: client_id={}, token_validity={}",
        profile.dhan_client_id, profile.token_validity
    );
}

// ===================================================================
// Orders — full lifecycle
// ===================================================================

#[tokio::test]
async fn test_order_lifecycle() {
    let client = require_client!();

    // 1. Place a LIMIT order at a low price (won't execute)
    let req = PlaceOrderRequest {
        dhan_client_id: client.client_id().to_owned(),
        correlation_id: Some("dhan-rs-test-001".into()),
        transaction_type: TransactionType::BUY,
        exchange_segment: ExchangeSegment::NSE_EQ,
        product_type: ProductType::INTRADAY,
        order_type: OrderType::LIMIT,
        validity: Validity::DAY,
        security_id: TCS_SECURITY_ID.into(),
        quantity: 1,
        disclosed_quantity: None,
        price: Some(100.0), // well below market — won't fill
        trigger_price: None,
        after_market_order: Some(false),
        amo_time: None,
        bo_profit_value: None,
        bo_stop_loss_value: None,
    };

    let place_resp = sandbox_ok_or_skip!(client.place_order(&req).await, "place_order");
    let order_id = &place_resp.order_id;
    println!(
        "✔ Placed order: id={order_id}, status={}",
        place_resp.order_status
    );

    // 2. Get order by ID (sandbox may not support this)
    match client.get_order(order_id).await {
        Ok(detail) => {
            assert_eq!(detail.order_id.as_deref(), Some(order_id.as_str()));
            println!("✔ Get order: status={:?}", detail.order_status);
        }
        Err(ref e) if is_sandbox_limitation(e) => {
            eprintln!("⏭  get_order skipped (sandbox limitation): {e}");
        }
        Err(e) => panic!("get_order failed: {e}"),
    }

    // 3. Get order by correlation ID (sandbox may not support this)
    match client
        .get_order_by_correlation_id("dhan-rs-test-001")
        .await
    {
        Ok(corr) => {
            assert_eq!(corr.correlation_id.as_deref(), Some("dhan-rs-test-001"));
            println!("✔ Get order by correlationId: found");
        }
        Err(ref e) if is_sandbox_limitation(e) => {
            eprintln!("⏭  get_order_by_correlation_id skipped (sandbox limitation): {e}");
        }
        Err(e) => panic!("get_order_by_correlation_id failed: {e}"),
    }

    // 4. Modify the order (change price)
    let modify_req = ModifyOrderRequest {
        dhan_client_id: client.client_id().to_owned(),
        order_id: order_id.clone(),
        order_type: OrderType::LIMIT,
        leg_name: None,
        quantity: Some(1),
        price: Some(110.0),
        disclosed_quantity: None,
        trigger_price: None,
        validity: Validity::DAY,
    };
    match client.modify_order(order_id, &modify_req).await {
        Ok(resp) => println!("✔ Modified order: status={}", resp.order_status),
        Err(ref e) if is_sandbox_limitation(e) => {
            eprintln!("⏭  modify_order skipped (sandbox limitation): {e}");
        }
        Err(e) => panic!("modify_order failed: {e}"),
    }

    // 5. Cancel the order
    match client.cancel_order(order_id).await {
        Ok(resp) => println!("✔ Cancelled order: status={}", resp.order_status),
        Err(ref e) if is_sandbox_limitation(e) => {
            eprintln!("⏭  cancel_order skipped (sandbox limitation): {e}");
        }
        Err(e) => panic!("cancel_order failed: {e}"),
    }
}

// ===================================================================
// Order Book & Trade Book
// ===================================================================

#[tokio::test]
async fn test_order_book() {
    let client = require_client!();
    let orders = sandbox_ok_or_skip!(client.get_orders().await, "get_orders");
    println!("✔ Order book: {} orders today", orders.len());
}

#[tokio::test]
async fn test_trade_book() {
    let client = require_client!();
    let trades = sandbox_ok_or_skip!(client.get_trades().await, "get_trades");
    println!("✔ Trade book: {} trades today", trades.len());
}

// ===================================================================
// Portfolio
// ===================================================================

#[tokio::test]
async fn test_holdings() {
    let client = require_client!();
    let holdings = sandbox_ok_or_skip!(client.get_holdings().await, "get_holdings");
    println!("✔ Holdings: {} items", holdings.len());
}

#[tokio::test]
async fn test_positions() {
    let client = require_client!();
    let positions = sandbox_ok_or_skip!(client.get_positions().await, "get_positions");
    println!("✔ Positions: {} open", positions.len());
}

// ===================================================================
// Funds
// ===================================================================

#[tokio::test]
async fn test_fund_limit() {
    let client = require_client!();
    let funds = sandbox_ok_or_skip!(client.get_fund_limit().await, "get_fund_limit");
    println!("✔ Fund limit: {funds:?}");
}

#[tokio::test]
async fn test_margin_calculator() {
    let client = require_client!();
    let req = MarginCalculatorRequest {
        dhan_client_id: client.client_id().to_owned(),
        exchange_segment: ExchangeSegment::NSE_EQ,
        transaction_type: TransactionType::BUY,
        quantity: 1,
        product_type: ProductType::INTRADAY,
        security_id: TCS_SECURITY_ID.into(),
        price: 3500.0,
        trigger_price: None,
    };
    let margin = sandbox_ok_or_skip!(client.calculate_margin(&req).await, "calculate_margin");
    println!(
        "✔ Margin: total={:?}, available={:?}",
        margin.total_margin, margin.available_balance
    );
}

// ===================================================================
// Market Data (REST)
// ===================================================================

/// NOTE: Market quote endpoints (`/marketfeed/*`) are not available in the
/// sandbox. These tests will be skipped when running against sandbox.
#[tokio::test]
async fn test_ltp() {
    let client = require_client!();
    let mut instruments = HashMap::new();
    instruments.insert("NSE_EQ".to_string(), vec![11536u64]); // TCS
    let resp = sandbox_ok_or_skip!(client.get_ltp(&instruments).await, "get_ltp");
    assert_eq!(resp.status, "success");
    println!("✔ LTP response: {resp:?}");
}

#[tokio::test]
async fn test_ohlc() {
    let client = require_client!();
    let mut instruments = HashMap::new();
    instruments.insert("NSE_EQ".to_string(), vec![11536u64]);
    let resp = sandbox_ok_or_skip!(client.get_ohlc(&instruments).await, "get_ohlc");
    assert_eq!(resp.status, "success");
    println!("✔ OHLC response received");
}

#[tokio::test]
async fn test_full_quote() {
    let client = require_client!();
    let mut instruments = HashMap::new();
    instruments.insert("NSE_EQ".to_string(), vec![11536u64]);
    let resp = sandbox_ok_or_skip!(client.get_quote(&instruments).await, "get_quote");
    assert_eq!(resp.status, "success");
    println!("✔ Full quote response received");
}

// ===================================================================
// Historical Data
// ===================================================================

#[tokio::test]
async fn test_daily_historical() {
    let client = require_client!();
    let req = HistoricalDataRequest {
        security_id: TCS_SECURITY_ID.into(),
        exchange_segment: ExchangeSegment::NSE_EQ,
        instrument: Instrument::EQUITY,
        expiry_code: None,
        oi: None,
        from_date: "2025-01-01".into(),
        to_date: "2025-01-31".into(),
    };
    let candles = sandbox_ok_or_skip!(
        client.get_daily_historical(&req).await,
        "get_daily_historical"
    );
    assert!(
        !candles.close.is_empty(),
        "should have at least one daily candle"
    );
    println!("✔ Daily historical: {} candles", candles.close.len());
}

#[tokio::test]
async fn test_intraday_historical() {
    let client = require_client!();
    let req = IntradayDataRequest {
        security_id: TCS_SECURITY_ID.into(),
        exchange_segment: ExchangeSegment::NSE_EQ,
        instrument: Instrument::EQUITY,
        interval: "5".into(),
        oi: None,
        from_date: "2025-01-15 09:15:00".into(),
        to_date: "2025-01-15 15:30:00".into(),
    };
    let candles = client
        .get_intraday_historical(&req)
        .await
        .expect("get_intraday_historical failed");
    assert!(!candles.close.is_empty(), "should have intraday candles");
    println!("✔ Intraday historical: {} candles", candles.close.len());
}

// ===================================================================
// Forever Orders
// ===================================================================

/// NOTE: The sandbox uses `GET /forever/orders` for listing, while the
/// production API uses `GET /forever/all`. This test will be skipped on sandbox.
#[tokio::test]
async fn test_forever_orders_list() {
    let client = require_client!();
    let orders = sandbox_ok_or_skip!(
        client.get_all_forever_orders().await,
        "get_all_forever_orders"
    );
    println!("✔ Forever orders: {} active", orders.len());
}

// ===================================================================
// Super Orders
// ===================================================================

/// NOTE: Super Orders are not available in the sandbox (404).
#[tokio::test]
async fn test_super_orders_list() {
    let client = require_client!();
    let orders = sandbox_ok_or_skip!(client.get_super_orders().await, "get_super_orders");
    println!("✔ Super orders: {} today", orders.len());
}

// ===================================================================
// Kill Switch
// ===================================================================

/// NOTE: The sandbox only supports POST (activate/deactivate) for kill switch,
/// not GET status. This test will be skipped on sandbox.
#[tokio::test]
async fn test_kill_switch_status() {
    let client = require_client!();
    let status = sandbox_ok_or_skip!(
        client.get_kill_switch_status().await,
        "get_kill_switch_status"
    );
    println!("✔ Kill switch: {status:?}");
}

// ===================================================================
// Statements
// ===================================================================

#[tokio::test]
async fn test_ledger() {
    let client = require_client!();
    let entries = client
        .get_ledger("2025-01-01", "2025-01-31")
        .await
        .expect("get_ledger failed");
    println!("✔ Ledger: {} entries", entries.len());
}

#[tokio::test]
async fn test_trade_history() {
    let client = require_client!();
    let entries = client
        .get_trade_history("2025-01-01", "2025-01-31", 0)
        .await
        .expect("get_trade_history failed");
    println!("✔ Trade history: {} entries", entries.len());
}

// ===================================================================
// Error Handling
// ===================================================================

#[tokio::test]
async fn test_invalid_token_returns_api_error() {
    // Use an intentionally invalid token — should get DH-901 (or a raw 401/403
    // if a WAF/CDN intercepts the request before the API layer).
    let client = DhanClient::with_base_url("invalid", "invalid-token", SANDBOX_BASE_URL);
    let err = client.get_profile().await.unwrap_err();
    match &err {
        DhanError::Api(body) => {
            assert_eq!(body.error_code.as_deref(), Some("DH-901"));
            println!("✔ Auth error correctly parsed: {body}");
        }
        DhanError::HttpStatus { status, .. } => {
            assert!(
                status.as_u16() == 401 || status.as_u16() == 403,
                "Expected 401 or 403, got {status}"
            );
            println!("✔ Auth rejected with HTTP {status} (WAF/CDN response)");
        }
        other => panic!("Expected DhanError::Api or HttpStatus, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_invalid_order_returns_api_error() {
    let client = require_client!();
    // Place an order with nonsense security ID — should get an API error.
    let req = PlaceOrderRequest {
        dhan_client_id: client.client_id().to_owned(),
        correlation_id: None,
        transaction_type: TransactionType::BUY,
        exchange_segment: ExchangeSegment::NSE_EQ,
        product_type: ProductType::INTRADAY,
        order_type: OrderType::MARKET,
        validity: Validity::DAY,
        security_id: "0".into(), // invalid
        quantity: 1,
        disclosed_quantity: None,
        price: None,
        trigger_price: None,
        after_market_order: None,
        amo_time: None,
        bo_profit_value: None,
        bo_stop_loss_value: None,
    };
    let err = client.place_order(&req).await.unwrap_err();
    match &err {
        DhanError::Api(body) => {
            println!("✔ Invalid order correctly rejected: {body}");
        }
        other => panic!("Expected DhanError::Api, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_nonexistent_order_returns_error() {
    let client = require_client!();
    let err = client.get_order("000000000000").await.unwrap_err();
    match &err {
        DhanError::Api(_) | DhanError::HttpStatus { .. } => {
            println!("✔ Nonexistent order correctly returned error: {err}");
        }
        other => panic!("Expected API or HTTP error, got: {other:?}"),
    }
}

// ===================================================================
// Client construction
// ===================================================================

#[tokio::test]
async fn test_with_base_url_trailing_slash() {
    // Verify trailing slash is stripped so URLs don't get doubled.
    let client = DhanClient::with_base_url("test", "test", "https://sandbox.dhan.co/");
    assert_eq!(client.base_url(), "https://sandbox.dhan.co");
}

#[tokio::test]
async fn test_set_access_token() {
    let mut client = DhanClient::with_base_url("test", "old-token", SANDBOX_BASE_URL);
    assert_eq!(client.access_token(), "old-token");
    client.set_access_token("new-token");
    assert_eq!(client.access_token(), "new-token");
}
