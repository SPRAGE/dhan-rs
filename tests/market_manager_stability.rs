use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use dhan_rs::types::enums::FeedRequestCode;
use dhan_rs::ws::manager::{
    ConnectionId, ConnectionLifecycle, DhanFeedConfig, DhanFeedManager, DhanFeedManagerBuilder,
    GapCause, ManagerLifecycleEvent, MarketDataQuality,
};
use dhan_rs::ws::market_feed::{Instrument, MarketFeedEvent};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

#[derive(Debug)]
enum ServerEvent {
    Query(usize, String),
    Control(usize, Value),
    Closed(usize),
    PreviousCloseSent,
}

async fn next_event(receiver: &mut mpsc::UnboundedReceiver<ServerEvent>) -> ServerEvent {
    timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("server event timed out")
        .expect("server event channel closed")
}

#[allow(clippy::result_large_err)]
async fn accept_with_query(
    stream: TcpStream,
    attempt: usize,
    sender: mpsc::UnboundedSender<ServerEvent>,
) -> Result<tokio_tungstenite::WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Error> {
    accept_hdr_async(stream, move |request: &Request, response: Response| {
        let _ = sender.send(ServerEvent::Query(
            attempt,
            request.uri().query().unwrap_or_default().to_owned(),
        ));
        Ok(response)
    })
    .await
}

#[tokio::test]
async fn reconnects_repeatedly_with_latest_state_and_rotated_token_then_closes_cleanly() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let attempts = Arc::new(AtomicUsize::new(0));
    let allow_final = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let server_attempts = attempts.clone();
    let server_allow_final = allow_final.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let attempt = server_attempts.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt != 1 && !server_allow_final.load(Ordering::SeqCst) {
                drop(stream);
                continue;
            }

            let Ok(mut socket) = accept_with_query(stream, attempt, event_tx.clone()).await else {
                continue;
            };
            while let Some(message) = socket.next().await {
                let Ok(message) = message else {
                    break;
                };
                match message {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(&text).unwrap();
                        let request_code = value["RequestCode"].as_u64();
                        let _ = event_tx.send(ServerEvent::Control(attempt, value));
                        if attempt == 1 && request_code == Some(15) {
                            socket
                                .send(Message::Binary(ticker_packet(1).into()))
                                .await
                                .unwrap();
                            socket.send(Message::Close(None)).await.unwrap();
                            break;
                        } else if attempt > 1 && matches!(request_code, Some(17) | Some(21)) {
                            socket
                                .send(Message::Binary(ticker_packet(2).into()))
                                .await
                                .unwrap();
                        }
                    }
                    Message::Close(_) => {
                        let _ = event_tx.send(ServerEvent::Closed(attempt));
                        let _ = socket.close(None).await;
                        return;
                    }
                    _ => {}
                }
            }
        }
    });

    let mut manager = DhanFeedManagerBuilder::new("client", "old-token")
        .max_connections(1)
        .reconnect_delay_ms(20)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    assert_eq!(manager.health().alive_connections, 0, "start must be lazy");
    let mut lifecycle = manager.get_lifecycle_channel(ConnectionId(0)).unwrap();
    let _parsed = manager.get_parsed_channel(ConnectionId(0)).unwrap();

    let a = Instrument::new("NSE_EQ", "101");
    let b = Instrument::new("NSE_EQ", "102");
    let c = Instrument::new("NSE_EQ", "103");
    manager
        .subscribe(&[a.clone(), b.clone()], FeedRequestCode::SubscribeTicker)
        .await
        .unwrap();

    let mut saw_initial = false;
    while !saw_initial {
        if let ServerEvent::Control(1, value) = next_event(&mut event_rx).await {
            saw_initial = value["RequestCode"] == 15 && value["InstrumentCount"] == 2;
        }
    }

    timeout(Duration::from_secs(5), async {
        while attempts.load(Ordering::SeqCst) < 4 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("supervisor did not make several redial attempts");
    assert_eq!(
        manager.health().connections[0].data_quality,
        MarketDataQuality::GapDetected
    );
    assert!(
        manager.acknowledge_gap(ConnectionId(0)).await.is_err(),
        "a gap must not clear before the replacement transport is data-live"
    );

    // These changes occur while every reconnect is failing. The successful
    // transport must receive only this latest desired generation.
    manager
        .subscribe(std::slice::from_ref(&a), FeedRequestCode::SubscribeQuote)
        .await
        .unwrap();
    manager
        .subscribe(std::slice::from_ref(&c), FeedRequestCode::SubscribeFull)
        .await
        .unwrap();
    manager
        .unsubscribe(&[b], FeedRequestCode::UnsubscribeFull)
        .await
        .unwrap();
    manager.update_access_token("new-token").unwrap();
    allow_final.store(true, Ordering::SeqCst);

    let mut final_attempt = None;
    let mut controls = Vec::new();
    while controls.len() < 2 {
        match next_event(&mut event_rx).await {
            ServerEvent::Query(attempt, query)
                if attempt > 1 && query.contains("token=new-token") =>
            {
                final_attempt = Some(attempt);
            }
            ServerEvent::Control(attempt, value) if Some(attempt) == final_attempt => {
                controls.push(value);
            }
            _ => {}
        }
    }

    let codes: Vec<_> = controls
        .iter()
        .map(|value| value["RequestCode"].as_u64().unwrap())
        .collect();
    assert!(codes.contains(&17));
    assert!(codes.contains(&21));
    let security_ids: Vec<_> = controls
        .iter()
        .flat_map(|value| value["InstrumentList"].as_array().unwrap())
        .map(|instrument| instrument["SecurityId"].as_str().unwrap())
        .collect();
    assert_eq!(security_ids.len(), 2);
    assert!(security_ids.contains(&"101"));
    assert!(security_ids.contains(&"103"));
    assert!(!security_ids.contains(&"102"));

    timeout(Duration::from_secs(5), async {
        loop {
            let health = manager.health();
            let connection = &health.connections[0];
            if connection.reconnect_count > 0 && connection.data_live {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("reconnected transport never became data-live");
    let health = manager.health();
    let connection = &health.connections[0];
    assert_eq!(connection.data_quality, MarketDataQuality::GapDetected);
    assert!(matches!(
        connection.gap_cause,
        Some(GapCause::Disconnect { .. })
    ));
    assert!(connection.gap_count > 0);
    timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                lifecycle.recv().await,
                Ok(ManagerLifecycleEvent::GapDetected {
                    cause: GapCause::Disconnect { .. },
                    ..
                })
            ) {
                break;
            }
        }
    })
    .await
    .expect("disconnect gap lifecycle event was not surfaced");

    manager.acknowledge_gap(ConnectionId(0)).await.unwrap();
    let health = manager.health();
    let connection = &health.connections[0];
    assert_eq!(connection.data_quality, MarketDataQuality::Current);
    assert!(connection.gap_cause.is_none());

    manager.shutdown().await.unwrap();
    let final_attempt = final_attempt.unwrap();
    let mut saw_disconnect = false;
    let mut saw_close = false;
    while !saw_close {
        match next_event(&mut event_rx).await {
            ServerEvent::Control(attempt, value) if attempt == final_attempt => {
                saw_disconnect |= value["RequestCode"] == 12;
            }
            ServerEvent::Closed(attempt) if attempt == final_attempt => saw_close = true,
            _ => {}
        }
    }
    assert!(saw_disconnect, "shutdown must send Dhan RequestCode 12");
    server.await.unwrap();
}

#[tokio::test]
async fn already_published_token_is_used_after_live_socket_reports_expiry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (reject_old_tx, mut reject_old_rx) = mpsc::channel(1);

    let server = tokio::spawn(async move {
        for attempt in 1..=2 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_with_query(stream, attempt, event_tx.clone())
                .await
                .expect("local WebSocket upgrade");
            while let Some(message) = socket.next().await {
                match message.unwrap() {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(&text).unwrap();
                        let request_code = value["RequestCode"].as_u64();
                        let _ = event_tx.send(ServerEvent::Control(attempt, value));
                        if request_code == Some(15) {
                            socket
                                .send(Message::Binary(ticker_packet(attempt as i32).into()))
                                .await
                                .unwrap();
                            if attempt == 1 {
                                reject_old_rx
                                    .recv()
                                    .await
                                    .expect("test releases old credential rejection");
                                socket
                                    .send(Message::Binary(auth_disconnect_packet(807).into()))
                                    .await
                                    .unwrap();
                                break;
                            }
                        }
                    }
                    Message::Close(_) => {
                        let _ = event_tx.send(ServerEvent::Closed(attempt));
                        let _ = socket.close(None).await;
                        break;
                    }
                    _ => {}
                }
            }
        }
    });

    let mut manager = DhanFeedManagerBuilder::new("client", "old-token")
        .max_connections(1)
        .reconnect_delay_ms(20)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    let _parsed = manager.get_parsed_channel(ConnectionId(0)).unwrap();
    manager
        .subscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();

    let first_query = loop {
        if let ServerEvent::Query(1, query) = next_event(&mut event_rx).await {
            break query;
        }
    };
    assert!(first_query.contains("token=old-token"));
    timeout(Duration::from_secs(2), async {
        while !manager.health().connections[0].data_live {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("old-token transport never became live");

    manager.update_access_token("new-token").unwrap();
    timeout(Duration::from_secs(2), async {
        while manager.health().connections[0].credential_version != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("live supervisor did not observe the published token");
    reject_old_tx.send(()).await.unwrap();

    let second_query = timeout(Duration::from_secs(2), async {
        loop {
            if let ServerEvent::Query(2, query) = next_event(&mut event_rx).await {
                break query;
            }
        }
    })
    .await
    .expect("manager waited for a third token instead of reusing the published token");
    assert!(second_query.contains("token=new-token"));
    timeout(Duration::from_secs(2), async {
        loop {
            let health = manager.health();
            if health.connections[0].reconnect_count == 1 && health.connections[0].data_live {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("new-token transport never became live");

    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn caches_previous_close_before_receiver_and_surfaces_broadcast_lag() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (release_tx, mut release_rx) = mpsc::channel(1);

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_with_query(stream, 1, event_tx.clone())
            .await
            .expect("local WebSocket upgrade");
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value["RequestCode"] == 15 {
                        socket
                            .send(Message::Binary(previous_close_packet().into()))
                            .await
                            .unwrap();
                        let _ = event_tx.send(ServerEvent::PreviousCloseSent);
                        release_rx.recv().await.unwrap();
                        for sequence in 0..3 {
                            socket
                                .send(Message::Binary(ticker_packet(sequence).into()))
                                .await
                                .unwrap();
                        }
                    } else if value["RequestCode"] == 12 {
                        let _ = event_tx.send(ServerEvent::Control(1, value));
                    }
                }
                Message::Close(_) => {
                    let _ = event_tx.send(ServerEvent::Closed(1));
                    let _ = socket.close(None).await;
                    break;
                }
                _ => {}
            }
        }
    });

    let mut manager = DhanFeedManagerBuilder::new("client", "token")
        .max_connections(1)
        .parsed_channel_capacity(1)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    let mut lifecycle = manager
        .get_lifecycle_channel(ConnectionId(0))
        .expect("lifecycle channel");
    manager
        .subscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();

    while !matches!(
        next_event(&mut event_rx).await,
        ServerEvent::PreviousCloseSent
    ) {}
    timeout(Duration::from_secs(2), async {
        while manager.previous_close_snapshot(ConnectionId(0)).is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Previous Close was not cached");
    assert!(matches!(
        &manager.previous_close_snapshot(ConnectionId(0))[0],
        MarketFeedEvent::PrevClose { prev_close, .. } if (*prev_close - 99.5).abs() < f32::EPSILON
    ));

    let _slow_receiver = manager.get_parsed_channel(ConnectionId(0)).unwrap();
    release_tx.send(()).await.unwrap();
    timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                lifecycle.recv().await,
                Ok(ManagerLifecycleEvent::ReceiverLag { dropped: 1, .. })
            ) {
                break;
            }
        }
    })
    .await
    .expect("receiver lag was not surfaced");
    let health = &manager.health().connections[0];
    assert!(health.lagged_event_count > 0);
    assert_eq!(health.lifecycle, ConnectionLifecycle::Degraded);
    assert_eq!(health.data_quality, MarketDataQuality::GapDetected);
    assert!(matches!(
        health.gap_cause,
        Some(GapCause::ReceiverLag { .. })
    ));

    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn live_data_without_a_receiver_latches_an_explicit_gap() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (event_tx, _) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_with_query(stream, 1, event_tx)
            .await
            .expect("local WebSocket upgrade");
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value["RequestCode"] == 15 {
                        socket
                            .send(Message::Binary(ticker_packet(1).into()))
                            .await
                            .unwrap();
                    }
                }
                Message::Close(_) => {
                    let _ = socket.close(None).await;
                    break;
                }
                _ => {}
            }
        }
    });

    let mut manager = DhanFeedManagerBuilder::new("client", "token")
        .max_connections(1)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    let mut lifecycle = manager.get_lifecycle_channel(ConnectionId(0)).unwrap();
    manager
        .subscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();

    timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                lifecycle.recv().await,
                Ok(ManagerLifecycleEvent::NoReceiver {
                    event: "ticker",
                    ..
                })
            ) {
                break;
            }
        }
    })
    .await
    .expect("missing receiver was not surfaced");
    let health = manager.health();
    assert_eq!(
        health.connections[0].data_quality,
        MarketDataQuality::GapDetected
    );
    assert!(matches!(
        health.connections[0].gap_cause,
        Some(GapCause::NoReceiver { event: "ticker" })
    ));

    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn coalesced_binary_message_delivers_every_packet_without_a_gap() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (event_tx, _) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_with_query(stream, 1, event_tx)
            .await
            .expect("local WebSocket upgrade");
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value["RequestCode"] == 15 {
                        let message =
                            [ticker_packet(1), ticker_packet(2), ticker_packet(3)].concat();
                        socket.send(Message::Binary(message.into())).await.unwrap();
                    }
                }
                Message::Close(_) => {
                    let _ = socket.close(None).await;
                    break;
                }
                _ => {}
            }
        }
    });

    let mut manager = DhanFeedManagerBuilder::new("client", "token")
        .max_connections(1)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    let mut parsed = manager.get_parsed_channel(ConnectionId(0)).unwrap();
    manager
        .subscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();

    let sequences = timeout(Duration::from_secs(2), async {
        let mut sequences = Vec::new();
        while sequences.len() < 3 {
            if let MarketFeedEvent::Ticker { ltt, .. } = parsed.recv().await.unwrap() {
                sequences.push(ltt);
            }
        }
        sequences
    })
    .await
    .expect("coalesced packets were not all delivered");
    assert_eq!(sequences, [1, 2, 3]);

    let health = manager.health();
    let connection = &health.connections[0];
    assert_eq!(connection.data_quality, MarketDataQuality::Current);
    assert_eq!(connection.parser_error_count, 0);
    assert_eq!(connection.gap_count, 0);
    assert!(connection.gap_cause.is_none());

    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn pre_live_parse_failure_is_readiness_failure_not_tick_gap() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (event_tx, _) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_with_query(stream, 1, event_tx)
            .await
            .expect("local WebSocket upgrade");
        while let Some(message) = socket.next().await {
            if matches!(message.unwrap(), Message::Text(_)) {
                socket
                    .send(Message::Binary(vec![2, 0].into()))
                    .await
                    .unwrap();
                socket.send(Message::Close(None)).await.unwrap();
                break;
            }
        }
    });

    let mut manager = DhanFeedManagerBuilder::new("client", "token")
        .max_connections(1)
        .auto_reconnect(false)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    let mut lifecycle = manager.get_lifecycle_channel(ConnectionId(0)).unwrap();
    manager
        .subscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();

    timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                lifecycle.recv().await,
                Ok(ManagerLifecycleEvent::ReadinessFailure { .. })
            ) {
                break;
            }
        }
    })
    .await
    .expect("pre-live parse failure was not surfaced as readiness failure");
    let health = manager.health();
    let connection = &health.connections[0];
    assert_eq!(connection.data_quality, MarketDataQuality::ReadinessPending);
    assert_eq!(connection.gap_count, 0);
    assert!(connection.gap_cause.is_none());

    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn ping_only_transport_cannot_satisfy_first_data_readiness() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (event_tx, _) = mpsc::unbounded_channel();
    let (subscribed_tx, subscribed_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_with_query(stream, 1, event_tx)
            .await
            .expect("local WebSocket upgrade");
        while let Some(message) = socket.next().await {
            if let Message::Text(text) = message.unwrap() {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["RequestCode"] == 15 {
                    subscribed_tx.send(()).unwrap();
                    for _ in 0..8 {
                        if socket.send(Message::Ping(vec![1].into())).await.is_err() {
                            return;
                        }
                        tokio::time::sleep(Duration::from_secs(4)).await;
                    }
                    return;
                }
            }
        }
    });

    let mut manager = DhanFeedManagerBuilder::new("client", "token")
        .max_connections(1)
        .auto_reconnect(false)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    let mut lifecycle = manager.get_lifecycle_channel(ConnectionId(0)).unwrap();
    manager
        .subscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();

    subscribed_rx.await.unwrap();
    for _ in 0..8 {
        tokio::time::advance(Duration::from_secs(4)).await;
        tokio::task::yield_now().await;
    }

    timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                lifecycle.recv().await,
                Ok(ManagerLifecycleEvent::ReadinessFailure { error, .. })
                    if error.contains("readiness deadline")
            ) {
                break;
            }
        }
    })
    .await
    .expect("ping-only transport incorrectly remained readiness-pending forever");
    let health = manager.health();
    assert_eq!(
        health.connections[0].data_quality,
        MarketDataQuality::ReadinessPending
    );
    assert_eq!(health.connections[0].gap_count, 0);

    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn rejects_invalid_configuration_and_non_standard_modes_without_sockets() {
    let invalid = DhanFeedConfig {
        max_connections: 0,
        ..DhanFeedConfig::default()
    };
    let mut manager = DhanFeedManager::new("client", "token", invalid);
    assert!(manager.start().await.is_err());
    assert_eq!(manager.health().alive_connections, 0);

    let zero_retry = DhanFeedConfig {
        reconnect_delay_ms: 0,
        ..DhanFeedConfig::default()
    };
    let mut manager = DhanFeedManager::new("client", "token", zero_retry);
    assert!(manager.start().await.is_err());
    assert_eq!(manager.health().alive_connections, 0);

    let mut manager = DhanFeedManagerBuilder::new("client", "token")
        .max_connections(1)
        .market_feed_url("ws://127.0.0.1:9")
        .build();
    manager.start().await.unwrap();
    assert!(
        manager
            .subscribe(
                &[Instrument::new("NSE_EQ", "101")],
                FeedRequestCode::SubscribeFullMarketDepth,
            )
            .await
            .is_err()
    );
    assert_eq!(manager.health().alive_connections, 0);

    let valid = Instrument::new("NSE_EQ", "101");
    manager
        .subscribe(
            &[valid.clone(), valid.clone()],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();
    assert_eq!(
        manager.total_instruments(),
        1,
        "exact duplicates are idempotent"
    );

    let mixed = [
        Instrument::new("BSE_EQ", "202"),
        Instrument::new("NSE_COMM", "303"),
    ];
    assert!(
        manager
            .subscribe(&mixed, FeedRequestCode::SubscribeQuote)
            .await
            .is_err()
    );
    assert_eq!(
        manager.total_instruments(),
        1,
        "an invalid colocated instrument must not commit the valid peer"
    );
    assert!(
        manager
            .subscribe(
                &[Instrument::new("NSE_EQ", "   ")],
                FeedRequestCode::SubscribeTicker,
            )
            .await
            .is_err()
    );
    assert!(
        manager
            .subscribe(
                &[Instrument::new("NSE_EQ", "not-a-number")],
                FeedRequestCode::SubscribeTicker,
            )
            .await
            .is_err()
    );
    assert!(
        manager
            .unsubscribe(
                &[valid, Instrument::new("DEPTH_ONLY", "404")],
                FeedRequestCode::UnsubscribeTicker,
            )
            .await
            .is_err()
    );
    assert_eq!(manager.total_instruments(), 1);
    manager.shutdown().await.unwrap();
}

fn previous_close_packet() -> Vec<u8> {
    let mut packet = vec![6];
    packet.extend_from_slice(&16_u16.to_le_bytes());
    packet.push(1);
    packet.extend_from_slice(&101_u32.to_le_bytes());
    packet.extend_from_slice(&99.5_f32.to_le_bytes());
    packet.extend_from_slice(&500_i32.to_le_bytes());
    packet
}

fn ticker_packet(sequence: i32) -> Vec<u8> {
    let mut packet = vec![2];
    packet.extend_from_slice(&16_u16.to_le_bytes());
    packet.push(1);
    packet.extend_from_slice(&101_u32.to_le_bytes());
    packet.extend_from_slice(&(100.0 + sequence as f32).to_le_bytes());
    packet.extend_from_slice(&sequence.to_le_bytes());
    packet
}

fn auth_disconnect_packet(reason_code: i16) -> Vec<u8> {
    let mut packet = vec![50];
    packet.extend_from_slice(&10_u16.to_le_bytes());
    packet.push(1);
    packet.extend_from_slice(&101_u32.to_le_bytes());
    packet.extend_from_slice(&reason_code.to_le_bytes());
    packet
}
