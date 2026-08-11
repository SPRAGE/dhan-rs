//! Deterministic local REST contract and credential-safety tests.
//!
//! These tests use loopback TCP listeners only; they never contact DhanHQ or
//! require account credentials.

use dhan_rs::client::DhanClient;
use dhan_rs::error::DhanError;
use dhan_rs::types::auth::{IpInfo, IpMatchStatus, TokenResponse};
use dhan_rs::types::enums::{ExchangeSegment, ProductType, TransactionType};
use dhan_rs::types::funds::{MarginScript, MultiMarginRequest, MultiMarginResponse};
use dhan_rs::types::statements::LedgerResponse;
use dhan_rs::types::traders_control::{PnlExitConfig, PnlExitRequest, PnlProductType};
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

fn http_response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n", body.len());
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("Connection: close\r\n\r\n");
    response.push_str(body);
    response
}

#[tokio::test]
async fn pnl_exit_uses_documented_post_path_headers_and_body() {
    let response = http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"pnlExitStatus":"ACTIVE","message":"configured"}"#,
    );
    let (base_url, request_rx) = serve_once(response).await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);
    let request = PnlExitRequest {
        profit_value: "2500".into(),
        loss_value: "1000".into(),
        product_type: vec![PnlProductType::Intraday, PnlProductType::Delivery],
        enable_kill_switch: true,
    };

    let response = client.set_pnl_exit(&request).await.unwrap();
    assert_eq!(response.pnl_exit_status, "ACTIVE");

    let received = request_rx.await.unwrap();
    let request_text = std::str::from_utf8(&received).unwrap();
    let (headers, body) = request_text.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /v2/pnlExit HTTP/1.1\r\n"));
    assert!(headers.contains("content-type: application/json\r\n"));
    assert!(headers.contains("access-token: local-token\r\n"));
    assert!(headers.contains("client-id: local-client\r\n"));
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap(),
        json!({
            "profitValue": "2500",
            "lossValue": "1000",
            "productType": ["INTRADAY", "DELIVERY"],
            "enableKillSwitch": true,
        })
    );
}

#[test]
fn ledger_object_response_decodes_and_legacy_array_remains_tolerated() {
    let object: LedgerResponse = serde_json::from_value(json!({
        "dhanClientId": "local-client",
        "narration": "Opening balance",
        "debit": "0.00",
        "credit": "100.00"
    }))
    .unwrap();
    let object = object.into_entries();
    assert_eq!(object.len(), 1);
    assert_eq!(object[0].dhan_client_id.as_deref(), Some("local-client"));
    assert_eq!(object[0].narration.as_deref(), Some("Opening balance"));

    let legacy: LedgerResponse =
        serde_json::from_value(json!([{ "narration": "legacy" }])).unwrap();
    assert_eq!(legacy.into_entries().len(), 1);
}

#[tokio::test]
async fn ledger_method_accepts_the_exact_documented_object() {
    let response = http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"dhanClientId":"local-client","narration":"FUNDS WITHDRAWAL","voucherdate":"Jun 22, 2022","exchange":"NSE-CAPITAL","voucherdesc":"PAYBNK","vouchernumber":"202200036701","debit":"20000.00","credit":"0.00","runbal":"957.29"}"#,
    );
    let (base_url, request_rx) = serve_once(response).await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);

    let entries = client.get_ledger("2026-01-01", "2026-01-31").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].narration.as_deref(), Some("FUNDS WITHDRAWAL"));

    let received = request_rx.await.unwrap();
    let request = std::str::from_utf8(&received).unwrap();
    assert!(
        request.starts_with("GET /v2/ledger?from-date=2026-01-01&to-date=2026-01-31 HTTP/1.1\r\n")
    );
}

#[test]
fn multi_margin_uses_official_request_names_and_tolerates_response_variants() {
    let request = MultiMarginRequest {
        include_position: Some(true),
        include_orders: Some(false),
        dhan_client_id: Some("local-client".into()),
        scripts: vec![MarginScript {
            exchange_segment: ExchangeSegment::NSE_EQ,
            transaction_type: TransactionType::BUY,
            quantity: 2,
            product_type: ProductType::INTRADAY,
            security_id: "11536".into(),
            price: 123.45,
            trigger_price: None,
        }],
    };

    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({
            "includePosition": true,
            "includeOrder": false,
            "dhanClientId": "local-client",
            "scripList": [{
                "exchangeSegment": "NSE_EQ",
                "transactionType": "BUY",
                "quantity": 2,
                "productType": "INTRADAY",
                "securityId": "11536",
                "price": 123.45
            }]
        })
    );

    let camel: MultiMarginResponse = serde_json::from_value(json!({
        "totalMargin": 101.25,
        "spanMargin": "90.00",
        "hedgeBenefit": 4
    }))
    .unwrap();
    assert_eq!(camel.total_margin.as_deref(), Some("101.25"));
    assert_eq!(camel.span_margin.as_deref(), Some("90.00"));
    assert_eq!(camel.hedge_benefit.as_deref(), Some("4"));

    let snake: MultiMarginResponse = serde_json::from_value(json!({
        "total_margin": "101.25",
        "commodity_margin": 12.5
    }))
    .unwrap();
    assert_eq!(snake.total_margin.as_deref(), Some("101.25"));
    assert_eq!(snake.commodity_margin.as_deref(), Some("12.5"));
}

#[test]
fn current_ip_pnl_and_rest_only_segment_response_drift_is_supported() {
    let ip: IpInfo = serde_json::from_value(json!({
        "primaryIP": "203.0.113.1",
        "detectedIP": "203.0.113.2",
        "ipMatchStatus": "MISMATCH",
        "ordersAllowed": false
    }))
    .unwrap();
    assert_eq!(ip.detected_ip.as_deref(), Some("203.0.113.2"));
    assert_eq!(ip.ip_match_status, Some(IpMatchStatus::MISMATCH));
    assert_eq!(ip.orders_allowed, Some(false));

    let pnl: PnlExitConfig = serde_json::from_value(json!({
        "profit": 1250.5,
        "loss": "500",
        "enable_kill_switch": true
    }))
    .unwrap();
    assert_eq!(pnl.profit.as_deref(), Some("1250.5"));
    assert_eq!(pnl.loss.as_deref(), Some("500"));
    assert_eq!(pnl.enable_kill_switch, Some(true));

    let numeric_request = PnlExitRequest {
        profit_value: 1250.5.into(),
        loss_value: 500.0.into(),
        product_type: vec![PnlProductType::Intraday],
        enable_kill_switch: false,
    };
    let numeric_request = serde_json::to_value(numeric_request).unwrap();
    assert_eq!(numeric_request["profitValue"], 1250.5);
    assert_eq!(numeric_request["lossValue"], 500.0);

    assert_eq!(
        serde_json::to_value(ExchangeSegment::NSE_COMM).unwrap(),
        json!("NSE_COMM")
    );
    assert_eq!(ExchangeSegment::NSE_COMM.segment_code(), None);
}

#[tokio::test]
async fn kill_switch_post_has_no_undocumented_json_body() {
    let response = http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"dhanClientId":"local-client","killSwitchStatus":"ACTIVATE"}"#,
    );
    let (base_url, request_rx) = serve_once(response).await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);
    client.manage_kill_switch("ACTIVATE").await.unwrap();

    let received = request_rx.await.unwrap();
    let request = std::str::from_utf8(&received).unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /v2/killswitch?killSwitchStatus=ACTIVATE HTTP/1.1\r\n"));
    assert!(body.is_empty());
}

#[tokio::test]
async fn invalid_credentials_are_typed_errors_without_constructor_panics() {
    assert!(matches!(
        DhanClient::try_new("local-client", "bad\ntoken"),
        Err(DhanError::InvalidHeaderValue(_))
    ));
    assert!(matches!(
        DhanClient::try_new("bad\nclient", "valid-token"),
        Err(DhanError::InvalidHeaderValue(_))
    ));

    let client = DhanClient::with_base_url("local-client", "bad\ntoken", "http://127.0.0.1:9");
    assert!(matches!(
        client.get::<Value>("/never-sent").await,
        Err(DhanError::InvalidHeaderValue(_))
    ));

    let mut replaced =
        DhanClient::with_base_url("local-client", "valid-token", "http://127.0.0.1:9");
    replaced.set_access_token("bad\ntoken");
    assert!(matches!(
        replaced.get::<Value>("/never-sent").await,
        Err(DhanError::InvalidHeaderValue(_))
    ));
}

#[test]
fn debug_output_redacts_client_and_token_response_tokens() {
    let client = DhanClient::new("debug-client-id", "debug-secret-token");
    let client_debug = format!("{client:?}");
    assert!(!client_debug.contains("debug-client-id"));
    assert!(!client_debug.contains("debug-secret-token"));

    let token = TokenResponse {
        dhan_client_id: "local-client".into(),
        dhan_client_name: None,
        dhan_client_ucc: None,
        given_power_of_attorney: None,
        access_token: "debug-secret-token".into(),
        expiry_time: None,
    };
    assert!(!format!("{token:?}").contains("debug-secret-token"));
}

#[tokio::test]
async fn authenticated_client_rejects_redirects() {
    let response = http_response(
        "302 Found",
        &[("Location", "http://127.0.0.1:1/redirected")],
        "",
    );
    let (base_url, request_rx) = serve_once(response).await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);

    let error = client.get::<Value>("/redirect").await.unwrap_err();
    assert!(matches!(
        error,
        DhanError::HttpStatus {
            status,
            ..
        } if status == reqwest::StatusCode::FOUND
    ));
    request_rx.await.unwrap();
}

#[tokio::test]
async fn body_read_failures_preserve_status_and_reqwest_source() {
    let malformed_response =
        "HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{}".to_owned();
    let (base_url, request_rx) = serve_once(malformed_response).await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);

    let error = client.get::<Value>("/truncated").await.unwrap_err();
    assert!(matches!(
        error,
        DhanError::ResponseBody {
            status,
            source: _,
        } if status == reqwest::StatusCode::OK
    ));
    request_rx.await.unwrap();
}

#[tokio::test]
async fn official_forever_list_routes_are_both_explicit() {
    for (openapi, expected_path) in [(false, "/v2/forever/all"), (true, "/v2/forever/orders")] {
        let response = http_response("200 OK", &[("Content-Type", "application/json")], "[]");
        let (base_url, request_rx) = serve_once(response).await;
        let client = DhanClient::with_base_url("local-client", "local-token", base_url);

        if openapi {
            client.get_all_forever_orders_openapi().await.unwrap();
        } else {
            client.get_all_forever_orders().await.unwrap();
        }

        let received = request_rx.await.unwrap();
        let request = std::str::from_utf8(&received).unwrap();
        assert!(request.starts_with(&format!("GET {expected_path} HTTP/1.1\r\n")));
    }
}

#[tokio::test]
async fn super_order_cancel_supports_json_and_empty_success_contracts() {
    let json_response = http_response(
        "200 OK",
        &[("Content-Type", "application/json")],
        r#"{"orderId":"order-1","orderStatus":"CANCELLED"}"#,
    );
    let (base_url, request_rx) = serve_once(json_response).await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);
    let response = client
        .cancel_super_order("order-1", "TARGET_LEG")
        .await
        .unwrap();
    assert_eq!(response.order_id, "order-1");
    let received = request_rx.await.unwrap();
    assert!(
        std::str::from_utf8(&received)
            .unwrap()
            .starts_with("DELETE /v2/super/orders/order-1/TARGET_LEG HTTP/1.1\r\n")
    );

    let empty_response = http_response("202 Accepted", &[], "");
    let (base_url, request_rx) = serve_once(empty_response).await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);
    client
        .cancel_super_order_no_content("order-1", "STOP_LOSS_LEG")
        .await
        .unwrap();
    let received = request_rx.await.unwrap();
    assert!(
        std::str::from_utf8(&received)
            .unwrap()
            .starts_with("DELETE /v2/super/orders/order-1/STOP_LOSS_LEG HTTP/1.1\r\n")
    );
}
