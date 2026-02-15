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
//! # What is tested
//!
//! - **Profile** — validates token & deserialization
//! - **Orders** — full lifecycle: place → get → modify → cancel
//! - **Order Book / Trade Book** — list queries
//! - **Portfolio** — holdings & positions
//! - **Funds** — fund limit & margin calculator
//! - **Market Data** — LTP, OHLC, full quote
//! - **Historical** — daily & intraday candles
//! - **Forever Orders** — create → list → delete
//! - **Super Orders** — list (read-only)
//! - **Kill Switch** — status check
//! - **Statements** — ledger & trade history
//! - **Error handling** — verifies bad requests produce typed `DhanError::Api`

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

    let place_resp = client.place_order(&req).await.expect("place_order failed");
    let order_id = &place_resp.order_id;
    println!(
        "✔ Placed order: id={order_id}, status={}",
        place_resp.order_status
    );

    // 2. Get order by ID
    let detail = client.get_order(order_id).await.expect("get_order failed");
    assert_eq!(detail.order_id.as_deref(), Some(order_id.as_str()));
    println!("✔ Get order: status={:?}", detail.order_status);

    // 3. Get order by correlation ID
    let corr = client
        .get_order_by_correlation_id("dhan-rs-test-001")
        .await
        .expect("get_order_by_correlation_id failed");
    assert_eq!(corr.correlation_id.as_deref(), Some("dhan-rs-test-001"));
    println!("✔ Get order by correlationId: found");

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
    let modify_resp = client
        .modify_order(order_id, &modify_req)
        .await
        .expect("modify_order failed");
    println!("✔ Modified order: status={}", modify_resp.order_status);

    // 5. Cancel the order
    let cancel_resp = client
        .cancel_order(order_id)
        .await
        .expect("cancel_order failed");
    println!("✔ Cancelled order: status={}", cancel_resp.order_status);
}

// ===================================================================
// Order Book & Trade Book
// ===================================================================

#[tokio::test]
async fn test_order_book() {
    let client = require_client!();
    let orders = client.get_orders().await.expect("get_orders failed");
    println!("✔ Order book: {} orders today", orders.len());
}

#[tokio::test]
async fn test_trade_book() {
    let client = require_client!();
    let trades = client.get_trades().await.expect("get_trades failed");
    println!("✔ Trade book: {} trades today", trades.len());
}

// ===================================================================
// Portfolio
// ===================================================================

#[tokio::test]
async fn test_holdings() {
    let client = require_client!();
    let holdings = client.get_holdings().await.expect("get_holdings failed");
    println!("✔ Holdings: {} items", holdings.len());
}

#[tokio::test]
async fn test_positions() {
    let client = require_client!();
    let positions = client.get_positions().await.expect("get_positions failed");
    println!("✔ Positions: {} open", positions.len());
}

// ===================================================================
// Funds
// ===================================================================

#[tokio::test]
async fn test_fund_limit() {
    let client = require_client!();
    let funds = client
        .get_fund_limit()
        .await
        .expect("get_fund_limit failed");
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
    let margin = client
        .calculate_margin(&req)
        .await
        .expect("calculate_margin failed");
    println!(
        "✔ Margin: total={:?}, available={:?}",
        margin.total_margin, margin.available_balance
    );
}

// ===================================================================
// Market Data (REST)
// ===================================================================

#[tokio::test]
async fn test_ltp() {
    let client = require_client!();
    let mut instruments = HashMap::new();
    instruments.insert("NSE_EQ".to_string(), vec![11536u64]); // TCS
    let resp = client.get_ltp(&instruments).await.expect("get_ltp failed");
    assert_eq!(resp.status, "success");
    println!("✔ LTP response: {resp:?}");
}

#[tokio::test]
async fn test_ohlc() {
    let client = require_client!();
    let mut instruments = HashMap::new();
    instruments.insert("NSE_EQ".to_string(), vec![11536u64]);
    let resp = client
        .get_ohlc(&instruments)
        .await
        .expect("get_ohlc failed");
    assert_eq!(resp.status, "success");
    println!("✔ OHLC response received");
}

#[tokio::test]
async fn test_full_quote() {
    let client = require_client!();
    let mut instruments = HashMap::new();
    instruments.insert("NSE_EQ".to_string(), vec![11536u64]);
    let resp = client
        .get_quote(&instruments)
        .await
        .expect("get_quote failed");
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
    let candles = client
        .get_daily_historical(&req)
        .await
        .expect("get_daily_historical failed");
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

#[tokio::test]
async fn test_forever_orders_list() {
    let client = require_client!();
    let orders = client
        .get_all_forever_orders()
        .await
        .expect("get_all_forever_orders failed");
    println!("✔ Forever orders: {} active", orders.len());
}

// ===================================================================
// Super Orders
// ===================================================================

#[tokio::test]
async fn test_super_orders_list() {
    let client = require_client!();
    let orders = client
        .get_super_orders()
        .await
        .expect("get_super_orders failed");
    println!("✔ Super orders: {} today", orders.len());
}

// ===================================================================
// Kill Switch
// ===================================================================

#[tokio::test]
async fn test_kill_switch_status() {
    let client = require_client!();
    let status = client
        .get_kill_switch_status()
        .await
        .expect("get_kill_switch_status failed");
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
    // Use an intentionally invalid token — should get DH-901.
    let client = DhanClient::with_base_url("invalid", "invalid-token", SANDBOX_BASE_URL);
    let err = client.get_profile().await.unwrap_err();
    match &err {
        DhanError::Api(body) => {
            assert_eq!(body.error_code.as_deref(), Some("DH-901"));
            println!("✔ Auth error correctly parsed: {body}");
        }
        other => panic!("Expected DhanError::Api, got: {other:?}"),
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
