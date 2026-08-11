//! Route construction tests using loopback HTTP only.

use dhan_rs::client::DhanClient;
use dhan_rs::error::DhanError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn serve_json_once(body: &'static str) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (path_tx, path_rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            assert_ne!(count, 0, "client closed before completing request headers");
            request.extend_from_slice(&buffer[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8(request).unwrap();
        let path = request
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .to_owned();
        path_tx.send(path).unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    (format!("http://{address}"), path_rx)
}

#[tokio::test]
async fn path_ids_are_encoded_as_one_segment() {
    let (base_url, path_rx) = serve_json_once("{}").await;
    let client = DhanClient::with_base_url("client", "token", base_url);
    client.get_order("order/../? #1").await.unwrap();
    assert_eq!(path_rx.await.unwrap(), "/v2/orders/order%2F..%2F%3F%20%231");

    let (base_url, path_rx) = serve_json_once("{}").await;
    let client = DhanClient::with_base_url("client", "token", base_url);
    client
        .get_order_by_correlation_id("source/../../other?x=1#fragment")
        .await
        .unwrap();
    assert_eq!(
        path_rx.await.unwrap(),
        "/v2/orders/external/source%2F..%2F..%2Fother%3Fx%3D1%23fragment"
    );

    let (base_url, path_rx) = serve_json_once("{}").await;
    let client = DhanClient::with_base_url("client", "token", base_url);
    client.inquire_edis("INE/123?holding#all").await.unwrap();
    assert_eq!(
        path_rx.await.unwrap(),
        "/v2/edis/inquire/INE%2F123%3Fholding%23all"
    );
}

#[tokio::test]
async fn dates_are_encoded_in_query_and_path_components() {
    let (base_url, path_rx) = serve_json_once("{}").await;
    let client = DhanClient::with_base_url("client", "token", base_url);
    client
        .get_ledger_response("2026-01-01&admin=true", "2026-01-31#truncated")
        .await
        .unwrap();
    assert_eq!(
        path_rx.await.unwrap(),
        "/v2/ledger?from-date=2026-01-01%26admin%3Dtrue&to-date=2026-01-31%23truncated"
    );

    let (base_url, path_rx) = serve_json_once("[]").await;
    let client = DhanClient::with_base_url("client", "token", base_url);
    client
        .get_trade_history("2026/01/01", "2026?01#31", 0)
        .await
        .unwrap();
    assert_eq!(
        path_rx.await.unwrap(),
        "/v2/trades/2026%2F01%2F01/2026%3F01%2331/0"
    );
}

#[tokio::test]
async fn empty_required_route_values_fail_before_io() {
    let client = DhanClient::with_base_url("client", "token", "http://127.0.0.1:9");

    for error in [
        client.get_order("").await.unwrap_err(),
        client.get_order("..").await.unwrap_err(),
        client.get_order_by_correlation_id("   ").await.unwrap_err(),
        client.inquire_edis("").await.unwrap_err(),
        client
            .get_ledger_response("", "2026-01-31")
            .await
            .unwrap_err(),
        client
            .get_trade_history("2026-01-01", "", 0)
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(error, DhanError::InvalidArgument(_)));
    }
}
