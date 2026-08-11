//! Deterministic contracts for the Data APIs and instrument-master routes.

use dhan_rs::client::DhanClient;
use dhan_rs::types::data::{
    CompanyInfoRequest, CompanyInstrument, FundamentalExchangeSegment, FundamentalMetricSection,
    MarketMoverCategory, MarketMoverExchangeSegment, MarketMoverInstrument, MarketMoverUniverse,
    MarketMoversRequest, RollingDataField, RollingExchangeSegment, RollingExpiryCode,
    RollingExpiryFlag, RollingInstrument, RollingInterval, RollingOptionRequest, RollingOptionType,
    TechnicalExchangeSegment, TechnicalIndicator, TechnicalInstrument, TechnicalMetricsRequest,
    TechnicalTimeframe,
};
use dhan_rs::types::instruments::InstrumentSegment;
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

fn response(content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn split_request(request: &[u8]) -> (&str, &str, Value) {
    let request = std::str::from_utf8(request).unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    (
        headers.lines().next().unwrap(),
        headers,
        serde_json::from_str(body).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn data_and_segment_instrument_methods_use_exact_contracts() {
    let (base_url, requests_rx) = serve_sequence(vec![
        response("application/json", r#"{"data":{"ce":null,"pe":null}}"#),
        response(
            "application/json",
            r#"{"securityId":"1333","timeframe":"D","data":{}}"#,
        ),
        response(
            "application/json",
            r#"{"exchangeSegment":"NSE_EQ","category":"PRICE_GAINERS","data":[]}"#,
        ),
        response("application/json", r#"{"securityId":"1333","data":{}}"#),
        response(
            "text/csv",
            "SEM_EXM_EXCH_ID,SEM_SMST_SECURITY_ID\nNSE,1333\n",
        ),
    ])
    .await;
    let client = DhanClient::with_base_url("local-client", "local-token", base_url);

    let rolling = RollingOptionRequest {
        exchange_segment: RollingExchangeSegment::NseFno,
        interval: RollingInterval::OneMinute,
        security_id: 13,
        instrument: RollingInstrument::Optidx,
        expiry_flag: RollingExpiryFlag::Month,
        expiry_code: RollingExpiryCode::First,
        strike: "ATM".into(),
        drv_option_type: RollingOptionType::Call,
        required_data: vec![RollingDataField::Open, RollingDataField::Iv],
        from_date: "2026-07-01".into(),
        to_date: "2026-07-31".into(),
    };
    client.get_rolling_option_data(&rolling).await.unwrap();

    let technical = TechnicalMetricsRequest {
        security_id: "1333".into(),
        exchange_segment: TechnicalExchangeSegment::NseEq,
        instrument: TechnicalInstrument::Equity,
        timeframe: TechnicalTimeframe::Daily,
        indicators: vec![TechnicalIndicator::Sma20, TechnicalIndicator::Rsi14],
    };
    client.get_technical_metrics(&technical).await.unwrap();

    let movers = MarketMoversRequest {
        exchange_segment: MarketMoverExchangeSegment::NseEq,
        instrument: vec![MarketMoverInstrument::Equity],
        category: MarketMoverCategory::PriceGainers,
        expiry: None,
        universe: Some(MarketMoverUniverse::Nifty50),
        limit: 20,
    };
    client.get_market_movers(&movers).await.unwrap();

    let company = CompanyInfoRequest {
        security_id: "1333".into(),
        exchange_segment: FundamentalExchangeSegment::NseEq,
        instrument: CompanyInstrument::Equity,
        metrics: vec![
            FundamentalMetricSection::Co,
            FundamentalMetricSection::Ratios,
        ],
    };
    client.get_company_info(&company).await.unwrap();

    let csv = client
        .download_segment_instruments_csv(InstrumentSegment::NseEq)
        .await
        .unwrap();
    assert!(csv.starts_with(b"SEM_EXM_EXCH_ID"));

    let requests = requests_rx.await.unwrap();
    let parsed = requests
        .iter()
        .map(|request| split_request(request))
        .collect::<Vec<_>>();
    assert_eq!(
        parsed.iter().map(|(line, _, _)| *line).collect::<Vec<_>>(),
        vec![
            "POST /v2/charts/rollingoption HTTP/1.1",
            "POST /v2/data/technical HTTP/1.1",
            "POST /v2/data/marketmovers HTTP/1.1",
            "POST /v2/data/companyinfo HTTP/1.1",
            "GET /v2/instrument/NSE_EQ HTTP/1.1",
        ]
    );
    assert_eq!(parsed[0].2["expiryCode"], 1);
    assert_eq!(parsed[1].2["indicators"], json!(["SMA_20", "RSI_14"]));
    assert_eq!(parsed[2].2["universe"], "NIFTY_50");
    assert_eq!(parsed[3].2["instrument"], "EQUITY");
    for (_, headers, _) in &parsed[..4] {
        assert!(headers.contains("access-token: local-token\r\n"));
        assert!(headers.contains("client-id: local-client\r\n"));
    }
    assert!(!parsed[4].1.contains("access-token:"));
    assert!(!parsed[4].1.contains("client-id:"));
}

#[tokio::test]
async fn data_validation_rejects_invalid_cross_field_combinations_before_io() {
    let client = DhanClient::with_base_url("local-client", "local-token", "http://127.0.0.1:9");
    let mixed = MarketMoversRequest {
        exchange_segment: MarketMoverExchangeSegment::NseEq,
        instrument: vec![MarketMoverInstrument::Equity, MarketMoverInstrument::Futstk],
        category: MarketMoverCategory::TopVolume,
        expiry: Some("2026-08-27".into()),
        universe: Some(MarketMoverUniverse::All),
        limit: 20,
    };
    assert!(client.get_market_movers(&mixed).await.is_err());

    let missing_expiry = MarketMoversRequest {
        exchange_segment: MarketMoverExchangeSegment::NseFno,
        instrument: vec![MarketMoverInstrument::Optidx],
        category: MarketMoverCategory::HighestOi,
        expiry: None,
        universe: None,
        limit: 20,
    };
    assert!(client.get_market_movers(&missing_expiry).await.is_err());
}
