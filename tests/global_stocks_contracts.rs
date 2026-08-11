//! Deterministic loopback contracts for the Global Stocks OpenAPI operations.

use dhan_rs::client::DhanClient;
use dhan_rs::error::DhanError;
use dhan_rs::types::global_stocks::{
    GlobalStockEstimatorRequest, GlobalStockHolding, GlobalStockLegName,
    GlobalStockModifyOrderRequest, GlobalStockOrder, GlobalStockOrderRequest,
    GlobalStockOrderStatus, GlobalStockOrderType, GlobalStockTrade, GlobalStockTransactionType,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn serve_sequence(responses: Vec<String>) -> (String, oneshot::Receiver<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (requests_tx, requests_rx) = oneshot::channel();

    tokio::spawn(async move {
        let mut requests = Vec::with_capacity(responses.len());
        for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            let header_end = loop {
                let count = stream.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0, "client closed before sending a request");
                request.extend_from_slice(&buffer[..count]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = std::str::from_utf8(&request[..header_end]).unwrap();
            let content_length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            while request.len() < header_end + content_length {
                let count = stream.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0, "client closed before sending its request body");
                request.extend_from_slice(&buffer[..count]);
            }
            requests.push(request);
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        }
        requests_tx.send(requests).unwrap();
    });

    (format!("http://{address}"), requests_rx)
}

fn response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn split_request(request: &[u8]) -> (&str, Value) {
    let request = std::str::from_utf8(request).unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    (
        headers.lines().next().unwrap(),
        serde_json::from_str(body).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn all_twelve_global_stocks_operations_use_documented_routes_and_payloads() {
    let (base_url, requests_rx) = serve_sequence(vec![
        response("[]"),
        response(r#"{"orderId":"GS-1","orderStatus":"PENDING"}"#),
        response(r#"{"orderId":"GS-1","securityId":"US-ABC"}"#),
        response(r#"{"orderId":"GS-1","orderStatus":"MODIFIED"}"#),
        response(r#"{"orderId":"GS-1","orderStatus":"CANCELLED"}"#),
        response(r#"{"brokerage":0.25}"#),
        response(r#"{"totalMargin":15.0}"#),
        response("[]"),
        response("[]"),
        response(r#"{"status":"open"}"#),
        response("[]"),
        response(r#"{"availableCash":100.0}"#),
    ])
    .await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);
    let order = GlobalStockOrderRequest {
        dhan_client_id: None,
        correlation_id: Some("local-correlation".into()),
        transaction_type: GlobalStockTransactionType::BUY,
        order_type: GlobalStockOrderType::LIMIT,
        security_id: "US-ABC".into(),
        quantity: Some(1.5),
        price: Some(12.5),
        trigger_price: None,
        stop_loss_price: None,
        target_price: None,
        amount: None,
        after_market_order: Some(false),
    };
    let modify = GlobalStockModifyOrderRequest {
        dhan_client_id: None,
        order_type: GlobalStockOrderType::LIMIT,
        transaction_type: GlobalStockTransactionType::BUY,
        security_id: "US-ABC".into(),
        quantity: Some(2.0),
        price: Some(12.75),
        leg_name: Some(GlobalStockLegName::ENTRY_LEG),
    };
    let estimator = GlobalStockEstimatorRequest {
        security_id: "US-ABC".into(),
        price: "12.50".into(),
        quantity: "1.5".into(),
        transaction_type: GlobalStockTransactionType::SELL,
    };

    client.get_global_stock_orders().await.unwrap();
    client.place_global_stock_order(&order).await.unwrap();
    client.get_global_stock_order("GS-1").await.unwrap();
    client
        .modify_global_stock_order("GS-1", &modify)
        .await
        .unwrap();
    client.cancel_global_stock_order("GS-1").await.unwrap();
    client
        .estimate_global_stock_order(&estimator)
        .await
        .unwrap();
    client
        .calculate_global_stock_margin(&estimator)
        .await
        .unwrap();
    client.get_global_stock_trades().await.unwrap();
    client
        .get_global_stock_trades_for_security("US-ABC")
        .await
        .unwrap();
    client.get_global_stock_market_status().await.unwrap();
    client.get_global_stock_holdings().await.unwrap();
    client.get_global_stock_fund_limit().await.unwrap();

    let requests = requests_rx.await.unwrap();
    let parsed = requests
        .iter()
        .map(|request| split_request(request))
        .collect::<Vec<_>>();
    assert_eq!(
        parsed.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
        vec![
            "GET /v2/globalstocks/orders HTTP/1.1",
            "POST /v2/globalstocks/orders HTTP/1.1",
            "GET /v2/globalstocks/orders/GS-1 HTTP/1.1",
            "PUT /v2/globalstocks/orders/GS-1 HTTP/1.1",
            "DELETE /v2/globalstocks/orders/GS-1 HTTP/1.1",
            "POST /v2/globalstocks/transEstimate HTTP/1.1",
            "POST /v2/globalstocks/margincalculator HTTP/1.1",
            "GET /v2/globalstocks/trades HTTP/1.1",
            "GET /v2/globalstocks/trades/US-ABC HTTP/1.1",
            "GET /v2/globalstocks/marketstatus HTTP/1.1",
            "GET /v2/globalstocks/holdings HTTP/1.1",
            "GET /v2/globalstocks/fundlimit HTTP/1.1",
        ]
    );
    assert_eq!(
        parsed[1].1,
        json!({
            "correlationId": "local-correlation",
            "transactionType": "BUY",
            "orderType": "LIMIT",
            "securityId": "US-ABC",
            "quantity": 1.5,
            "price": 12.5,
            "afterMarketOrder": false,
        })
    );
    assert_eq!(
        parsed[3].1,
        json!({
            "orderType": "LIMIT",
            "transactionType": "BUY",
            "securityId": "US-ABC",
            "quantity": 2.0,
            "price": 12.75,
            "legName": "ENTRY_LEG",
        })
    );
    assert_eq!(
        parsed[5].1,
        json!({
            "securityId": "US-ABC",
            "price": "12.50",
            "quantity": "1.5",
            "transactionType": "SELL",
        })
    );
    assert_eq!(parsed[6].1, parsed[5].1);
}

#[test]
fn global_stocks_response_fixtures_are_tolerant_of_partial_and_new_status_values() {
    let order: GlobalStockOrder = serde_json::from_value(json!({
        "orderId": "GS-1",
        "quantity": 0.25,
        "orderStatus": "AWAITING_SETTLEMENT",
        "childOrders": [{ "orderId": "GS-1-A", "orderStatus": "PENDING" }]
    }))
    .unwrap();
    assert_eq!(order.quantity, Some(0.25));
    assert!(matches!(
        order.order_status,
        Some(GlobalStockOrderStatus::Unknown(ref value)) if value == "AWAITING_SETTLEMENT"
    ));
    assert_eq!(order.child_orders.unwrap().len(), 1);

    let trade: GlobalStockTrade = serde_json::from_value(json!({
        "securityId": "US-ABC",
        "tradedQuantity": 0.5,
        "tradedPrice": 12.5,
        "orderStatus": "TRADED"
    }))
    .unwrap();
    assert_eq!(trade.traded_quantity, Some(0.5));
    assert!(matches!(
        trade.order_status,
        Some(GlobalStockOrderStatus::TRADED)
    ));

    let holding: GlobalStockHolding = serde_json::from_value(json!({
        "securityId": "US-ABC",
        "quantity": 0.75,
        "ltp": 13.0,
        "unexpectedProviderField": true
    }))
    .unwrap();
    assert_eq!(holding.quantity, Some(0.75));
    assert_eq!(holding.ltp, Some(13.0));
}

#[tokio::test]
async fn global_stocks_rejects_empty_ids_and_invalid_documented_numeric_inputs_before_io() {
    let client = DhanClient::with_base_url("local-client", "local-token", "http://127.0.0.1:9");
    let invalid_order = GlobalStockOrderRequest {
        dhan_client_id: None,
        correlation_id: Some("x".repeat(31)),
        transaction_type: GlobalStockTransactionType::BUY,
        order_type: GlobalStockOrderType::LIMIT,
        security_id: " ".into(),
        quantity: Some(0.0),
        price: None,
        trigger_price: None,
        stop_loss_price: None,
        target_price: None,
        amount: None,
        after_market_order: None,
    };
    let invalid_estimator = GlobalStockEstimatorRequest {
        security_id: "US-ABC".into(),
        price: "NaN".into(),
        quantity: "0".into(),
        transaction_type: GlobalStockTransactionType::BUY,
    };

    assert!(matches!(
        client.get_global_stock_order(" ").await,
        Err(DhanError::InvalidArgument(_))
    ));
    assert!(matches!(
        client.get_global_stock_trades_for_security("").await,
        Err(DhanError::InvalidArgument(_))
    ));
    assert!(matches!(
        client.place_global_stock_order(&invalid_order).await,
        Err(DhanError::InvalidArgument(_))
    ));
    assert!(matches!(
        client.estimate_global_stock_order(&invalid_estimator).await,
        Err(DhanError::InvalidArgument(_))
    ));
}
