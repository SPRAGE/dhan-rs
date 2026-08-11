//! Deterministic tests for the remaining REST/model contract remediation.

use dhan_rs::client::DhanClient;
use dhan_rs::error::DhanError;
use dhan_rs::types::conditional::{
    MultiOrderExchangeSegment, MultiOrderItemRequest, MultiOrderProductType, MultiOrderRequest,
};
use dhan_rs::types::edis::{EdisBulkFormRequest, EdisExchange, EdisInquiry, EdisSegment};
use dhan_rs::types::enums::{AmoTime, OrderType, TransactionType, Validity};
use dhan_rs::types::orders::{OrderDetail, TradeDetail};
use dhan_rs::types::portfolio::Holding;
use dhan_rs::types::postback::{PostbackPayload, PostbackValidationError};
use dhan_rs::types::super_order::SuperOrderDetail;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn serve_once(response: String) -> (String, oneshot::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request_tx, request_rx) = oneshot::channel();

    tokio::spawn(async move {
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
        request_tx.send(request).unwrap();
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    (format!("http://{address}"), request_rx)
}

fn http_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn multi_order_request() -> MultiOrderRequest {
    MultiOrderRequest {
        dhan_client_id: "local-client".into(),
        orders: vec![MultiOrderItemRequest {
            sequence: "first".into(),
            correlation_id: Some("batch-1".into()),
            transaction_type: TransactionType::BUY,
            exchange_segment: MultiOrderExchangeSegment::NSE_COMM,
            product_type: Some(MultiOrderProductType::MARGIN),
            order_type: Some(OrderType::LIMIT),
            validity: Some(Validity::DAY),
            security_id: Some("12345".into()),
            quantity: Some(2),
            after_market_order: Some(true),
            amo_time: Some(AmoTime::OPEN),
            price: Some(12.5),
            trigger_price: Some(11.0),
            disclosed_quantity: Some(1),
        }],
    }
}

#[test]
fn multi_order_serialization_and_validation_match_openapi() {
    assert_eq!(
        serde_json::to_value(multi_order_request()).unwrap(),
        json!({
            "dhanClientId": "local-client",
            "orders": [{
                "sequence": "first",
                "correlationId": "batch-1",
                "transactionType": "BUY",
                "exchangeSegment": "NSE_COMM",
                "productType": "MARGIN",
                "orderType": "LIMIT",
                "validity": "DAY",
                "securityId": "12345",
                "quantity": 2,
                "afterMarketOrder": true,
                "amoTime": "OPEN",
                "price": 12.5,
                "triggerPrice": 11.0,
                "disclosedQuantity": 1
            }]
        })
    );

    let empty = MultiOrderRequest {
        dhan_client_id: " ".into(),
        orders: Vec::new(),
    };
    assert!(empty.validate().is_err());

    let mut invalid_correlation = multi_order_request();
    invalid_correlation.orders[0].correlation_id = Some("x".repeat(31));
    assert!(invalid_correlation.validate().is_err());

    let mut oversized_quantity = multi_order_request();
    oversized_quantity.orders[0].quantity = Some(i32::MAX as u32 + 1);
    assert!(oversized_quantity.validate().is_err());
}

#[tokio::test]
async fn multi_order_uses_exact_post_path_and_body() {
    let (base_url, request_rx) = serve_once(http_response(
        r#"{"orders":[{"orderId":"order-1","sequence":"first","orderStatus":"PENDING"}]}"#,
    ))
    .await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);
    let response = client
        .place_multi_order(&multi_order_request())
        .await
        .unwrap();
    assert_eq!(response.orders[0].order_id.as_deref(), Some("order-1"));

    let received = request_rx.await.unwrap();
    let request_text = std::str::from_utf8(&received).unwrap();
    let (headers, body) = request_text.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /v2/alerts/multi/orders HTTP/1.1\r\n"));
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap(),
        serde_json::to_value(multi_order_request()).unwrap()
    );
}

#[test]
fn bulk_edis_serialization_and_validation_match_openapi() {
    let request = EdisBulkFormRequest {
        isin: vec!["INE123A01016".into(), "INE456B01011".into()],
        exchange: EdisExchange::NSE,
        segment: EdisSegment::EQ,
    };
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({"isin": ["INE123A01016", "INE456B01011"], "exchange": "NSE", "segment": "EQ"})
    );
    assert!(
        EdisBulkFormRequest {
            isin: vec![" ".into()],
            exchange: EdisExchange::BSE,
            segment: EdisSegment::FNO,
        }
        .validate()
        .is_err()
    );
}

#[tokio::test]
async fn bulk_edis_uses_exact_post_path_and_body() {
    let (base_url, request_rx) = serve_once(http_response(
        r#"{"dhanClientId":"local-client","edisFormHtml":"&lt;form&gt;"}"#,
    ))
    .await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);
    let request = EdisBulkFormRequest {
        isin: vec!["INE123A01016".into()],
        exchange: EdisExchange::MCX,
        segment: EdisSegment::COMM,
    };
    let response = client.generate_bulk_edis_form(&request).await.unwrap();
    assert_eq!(response.dhan_client_id, "local-client");

    let received = request_rx.await.unwrap();
    let request_text = std::str::from_utf8(&received).unwrap();
    let (headers, body) = request_text.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /v2/edis/bulkform HTTP/1.1\r\n"));
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap(),
        serde_json::to_value(request).unwrap()
    );
}

#[test]
fn response_drift_and_edis_quantities_decode_tolerantly() {
    let inquiry: EdisInquiry = serde_json::from_value(json!({
        "totalQty": "25", "aprvdQty": 7
    }))
    .unwrap();
    assert_eq!(inquiry.total_qty, Some(25));
    assert_eq!(inquiry.aprvd_qty, Some(7));

    let holding: Holding = serde_json::from_value(json!({
        "mtf_t1_qty": 2, "mtf_qty": 3, "lastTradedPrice": 99.5
    }))
    .unwrap();
    assert_eq!(holding.mtf_t1_qty, Some(2));
    assert_eq!(holding.mtf_qty, Some(3));
    assert_eq!(holding.last_traded_price, Some(99.5));

    let order: OrderDetail =
        serde_json::from_value(json!({"exchangeOrderId": "exchange-1"})).unwrap();
    assert_eq!(order.exchange_order_id.as_deref(), Some("exchange-1"));
    let trade: TradeDetail = serde_json::from_value(json!({"customSymbol": "CUSTOM"})).unwrap();
    assert_eq!(trade.custom_symbol.as_deref(), Some("CUSTOM"));
    let super_order: SuperOrderDetail =
        serde_json::from_value(json!({"algoId": "algo-1"})).unwrap();
    assert_eq!(super_order.algo_id.as_deref(), Some("algo-1"));
}

#[test]
fn postback_deserialization_stays_permissive_but_validation_requires_identity_order_and_status() {
    let empty: PostbackPayload = serde_json::from_value(json!({})).unwrap();
    assert_eq!(
        empty.validate(),
        Err(PostbackValidationError::MissingClientId)
    );

    let missing_order: PostbackPayload = serde_json::from_value(json!({
        "dhanClientId": "local-client"
    }))
    .unwrap();
    assert_eq!(
        missing_order.validate(),
        Err(PostbackValidationError::MissingOrderId)
    );

    let valid: PostbackPayload = serde_json::from_value(json!({
        "dhanClientId": "local-client", "orderId": "order-1", "orderStatus": "PENDING"
    }))
    .unwrap();
    assert!(valid.validate().is_ok());
}

#[tokio::test]
async fn invalid_multi_and_bulk_requests_are_rejected_before_transport() {
    let client = DhanClient::with_base_url("local-client", "local-token", "http://127.0.0.1:9");
    let error = client
        .place_multi_order(&MultiOrderRequest {
            dhan_client_id: "local-client".into(),
            orders: Vec::new(),
        })
        .await
        .unwrap_err();
    assert!(matches!(error, DhanError::InvalidArgument(_)));

    let error = client
        .generate_bulk_edis_form(&EdisBulkFormRequest {
            isin: Vec::new(),
            exchange: EdisExchange::ALL,
            segment: EdisSegment::EQ,
        })
        .await
        .unwrap_err();
    assert!(matches!(error, DhanError::InvalidArgument(_)));
}
