//! Local-only feed delivery, overflow, cache, and framing regressions.
use dhan_rs::types::enums::FeedRequestCode;
use dhan_rs::ws::manager::{
    ConnectionId, DhanFeedManager, DhanFeedManagerBuilder, MarketDataQuality, MarketFeedDelivery,
};
use dhan_rs::ws::market_feed::{Instrument, MarketFeedEvent};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

async fn server() -> (
    String,
    mpsc::Sender<Message>,
    oneshot::Receiver<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (tx, mut rx) = mpsc::channel::<Message>(8);
    let (ready, ready_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let mut ready = Some(ready);
        loop {
            tokio::select! {
                message = rx.recv() => {
                    if let Some(message) = message { socket.send(message).await.unwrap(); }
                    else { break; }
                }
                message = socket.next() => match message {
                    Some(Ok(Message::Text(_))) => { if let Some(ready) = ready.take() { let _ = ready.send(()); } }
                    Some(Ok(Message::Close(_))) => { let _ = socket.close(None).await; break; }
                    None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    });
    (endpoint, tx, ready_rx, task)
}

fn builder(endpoint: String) -> DhanFeedManagerBuilder {
    DhanFeedManagerBuilder::new("client", "token")
        .max_connections(1)
        .market_feed_url(endpoint)
}

async fn subscribe(manager: &mut DhanFeedManager, ready: oneshot::Receiver<()>) {
    manager
        .subscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(3), ready)
        .await
        .unwrap()
        .unwrap();
}

fn ticker(sequence: i32) -> Vec<u8> {
    let mut packet = vec![2, 16, 0, 1];
    packet.extend(101u32.to_le_bytes());
    packet.extend((100.0 + sequence as f32).to_le_bytes());
    packet.extend(sequence.to_le_bytes());
    packet
}

async fn wait_quality(manager: &DhanFeedManager, quality: MarketDataQuality) {
    timeout(Duration::from_secs(3), async {
        while manager.health().connections[0].data_quality != quality {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn requested_non_power_of_two_capacities_do_not_invent_loss() {
    for capacity in [3, 15_016] {
        let (endpoint, tx, ready, task) = server().await;
        let mut manager = builder(endpoint).parsed_channel_capacity(capacity).build();
        manager.start().await.unwrap();
        let mut receiver = manager.get_parsed_channel(ConnectionId(0)).unwrap();
        subscribe(&mut manager, ready).await;
        let count = capacity + 1;
        let mut frame = Vec::new();
        for sequence in 0..count.min(15_016) {
            frame.extend(ticker(sequence as i32));
        }
        tx.send(Message::Binary(frame.into())).await.unwrap();
        if count > 15_016 {
            tx.send(Message::Binary(ticker(15_016).into()))
                .await
                .unwrap();
        }
        wait_quality(&manager, MarketDataQuality::Current).await;
        for _ in 0..count {
            timeout(Duration::from_secs(3), receiver.recv())
                .await
                .unwrap()
                .unwrap();
        }
        assert_eq!(manager.health().connections[0].gap_count, 0);
        manager.shutdown().await.unwrap();
        task.await.unwrap();
    }
}

#[tokio::test]
async fn actual_parsed_overflow_reports_exact_receiver_loss() {
    let (endpoint, tx, ready, task) = server().await;
    let mut manager = builder(endpoint).parsed_channel_capacity(3).build();
    manager.start().await.unwrap();
    let mut receiver = manager.get_parsed_channel(ConnectionId(0)).unwrap();
    subscribe(&mut manager, ready).await;
    tx.send(Message::Binary(ticker(1).repeat(5).into()))
        .await
        .unwrap();
    wait_quality(&manager, MarketDataQuality::Current).await;
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(1))
    ));
    wait_quality(&manager, MarketDataQuality::GapDetected).await;
    assert_eq!(manager.health().connections[0].lagged_event_count, 1);
    for _ in 0..4 {
        receiver.recv().await.unwrap();
    }
    manager.shutdown().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn batches_preserve_order_recycle_storage_and_ignore_lagging_raw_observers() {
    let (endpoint, tx, ready, task) = server().await;
    let mut manager = builder(endpoint)
        .delivery(MarketFeedDelivery::Batches)
        .enable_raw_frames(true)
        .raw_channel_capacity(1)
        .build();
    manager.start().await.unwrap();
    assert!(manager.get_all_parsed_channels().is_empty());
    let mut receiver = manager.get_batch_channel(ConnectionId(0)).unwrap();
    assert!(manager.get_batch_channel(ConnectionId(0)).is_none());
    let mut raw = manager.get_raw_channel(ConnectionId(0)).unwrap();
    subscribe(&mut manager, ready).await;
    let frame = [ticker(1), ticker(2), ticker(3)].concat();
    let mut pointer = None;
    for _ in 0..3 {
        tx.send(Message::Binary(frame.clone().into()))
            .await
            .unwrap();
        let batch = timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        let sequences: Vec<_> = batch
            .events()
            .iter()
            .map(|e| match e {
                MarketFeedEvent::Ticker { ltt, .. } => *ltt,
                _ => panic!(),
            })
            .collect();
        assert_eq!(sequences, [1, 2, 3]);
        if let Some(pointer) = pointer {
            assert_eq!(batch.events().as_ptr(), pointer);
        }
        pointer = Some(batch.events().as_ptr());
        assert!(receiver.reserved_bytes() > 0);
        drop(batch);
        assert_eq!(receiver.reserved_bytes(), 0);
    }
    assert!(matches!(
        raw.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(2))
    ));
    assert_eq!(manager.health().connections[0].gap_count, 0);
    manager.shutdown().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn batch_byte_budget_includes_held_batches_and_recovers_after_release() {
    let (endpoint, tx, ready, task) = server().await;
    let mut manager = builder(endpoint)
        .delivery(MarketFeedDelivery::Batches)
        .batch_byte_capacity(1_024)
        .build();
    manager.start().await.unwrap();
    let mut receiver = manager.get_batch_channel(ConnectionId(0)).unwrap();
    subscribe(&mut manager, ready).await;
    tx.send(Message::Binary(ticker(1).into())).await.unwrap();
    let held = timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    tx.send(Message::Binary(ticker(2).into())).await.unwrap();
    assert!(matches!(
        timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap(),
        Err(tokio::sync::broadcast::error::RecvError::Lagged(1))
    ));
    wait_quality(&manager, MarketDataQuality::GapDetected).await;
    assert!(receiver.reserved_bytes() <= 1_024);
    drop(held);
    assert_eq!(receiver.reserved_bytes(), 0);
    tx.send(Message::Binary(ticker(3).into())).await.unwrap();
    let batch = timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        batch.events()[0],
        MarketFeedEvent::Ticker { ltt: 3, .. }
    ));
    assert_eq!(
        manager.health().connections[0].data_quality,
        MarketDataQuality::GapDetected
    );
    drop(batch);
    manager.acknowledge_gap(ConnectionId(0)).await.unwrap();
    assert_eq!(
        manager.health().connections[0].data_quality,
        MarketDataQuality::Current
    );
    manager.shutdown().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn malformed_batch_tail_delivers_nothing_and_cache_is_pruned_on_unsubscribe() {
    let (endpoint, tx, ready, task) = server().await;
    let mut manager = builder(endpoint)
        .delivery(MarketFeedDelivery::Batches)
        .build();
    manager.start().await.unwrap();
    let mut receiver = manager.get_batch_channel(ConnectionId(0)).unwrap();
    subscribe(&mut manager, ready).await;
    let mut reference = ticker(1);
    reference[0] = 6;
    tx.send(Message::Binary(reference.into())).await.unwrap();
    drop(
        timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    assert_eq!(manager.previous_close_records(ConnectionId(0)).len(), 1);
    let mut malformed = ticker(1);
    malformed.push(2);
    tx.send(Message::Binary(malformed.into())).await.unwrap();
    wait_quality(&manager, MarketDataQuality::GapDetected).await;
    assert!(
        timeout(Duration::from_millis(50), receiver.recv())
            .await
            .is_err()
    );
    manager
        .unsubscribe(
            &[Instrument::new("NSE_EQ", "101")],
            FeedRequestCode::UnsubscribeTicker,
        )
        .await
        .unwrap();
    assert!(manager.previous_close_snapshot(ConnectionId(0)).is_empty());
    manager.shutdown().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn missing_batch_consumer_is_a_durable_gap() {
    let (endpoint, tx, ready, task) = server().await;
    let mut manager = builder(endpoint)
        .delivery(MarketFeedDelivery::Batches)
        .build();
    manager.start().await.unwrap();
    subscribe(&mut manager, ready).await;
    tx.send(Message::Binary(ticker(1).into())).await.unwrap();
    wait_quality(&manager, MarketDataQuality::GapDetected).await;
    manager.shutdown().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn full_batch_queue_does_not_block_shutdown() {
    let (endpoint, tx, ready, task) = server().await;
    let mut manager = builder(endpoint)
        .delivery(MarketFeedDelivery::Batches)
        .batch_channel_capacity(1)
        .build();
    manager.start().await.unwrap();
    let mut receiver = manager.get_batch_channel(ConnectionId(0)).unwrap();
    subscribe(&mut manager, ready).await;
    tx.send(Message::Binary(ticker(1).into())).await.unwrap();
    tx.send(Message::Binary(ticker(2).into())).await.unwrap();
    wait_quality(&manager, MarketDataQuality::GapDetected).await;
    assert!(matches!(
        receiver.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Lagged(1))
    ));
    timeout(Duration::from_secs(3), manager.shutdown())
        .await
        .unwrap()
        .unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn an_oversized_rejected_buffer_does_not_poison_later_small_batches() {
    let (endpoint, tx, ready, task) = server().await;
    let mut manager = builder(endpoint)
        .delivery(MarketFeedDelivery::Batches)
        .batch_byte_capacity(1_024)
        .build();
    manager.start().await.unwrap();
    let mut receiver = manager.get_batch_channel(ConnectionId(0)).unwrap();
    subscribe(&mut manager, ready).await;
    tx.send(Message::Binary(ticker(1).repeat(10).into()))
        .await
        .unwrap();
    assert!(matches!(
        timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap(),
        Err(tokio::sync::broadcast::error::RecvError::Lagged(10))
    ));
    tx.send(Message::Binary(ticker(11).into())).await.unwrap();
    let batch = timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        batch.events()[0],
        MarketFeedEvent::Ticker { ltt: 11, .. }
    ));
    assert!(receiver.reserved_bytes() <= 1_024);
    drop(batch);
    manager.shutdown().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
#[allow(clippy::result_large_err)] // Tungstenite fixes the handshake callback error type.
async fn batches_survive_rotated_credentials_reconnect_gap_ack_and_idle_resubscribe() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (query_tx, mut queries) = mpsc::unbounded_channel();
    let (close, mut close_rx) = mpsc::channel(1);
    let task = tokio::spawn(async move {
        for attempt in 0..3 {
            let (stream, _) = listener.accept().await.unwrap();
            let sender = query_tx.clone();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                stream,
                move |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                      response| {
                    let _ = sender.send(request.uri().query().unwrap_or_default().to_owned());
                    Ok(response)
                },
            )
            .await
            .unwrap();
            loop {
                tokio::select! {
                    _ = close_rx.recv(), if attempt == 0 => {
                        socket.send(Message::Close(None)).await.unwrap();
                        break;
                    }
                    message = socket.next() => match message {
                        Some(Ok(Message::Text(text))) => {
                            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                            if matches!(value["RequestCode"].as_u64(), Some(15) | Some(17)) {
                                socket.send(Message::Binary(ticker(attempt + 1).into())).await.unwrap();
                            }
                        }
                        Some(Ok(Message::Close(_))) => { let _ = socket.close(None).await; break; }
                        None | Some(Err(_)) => break,
                        _ => {}
                    }
                }
            }
        }
    });
    let mut manager = DhanFeedManagerBuilder::new("client", "old-token")
        .max_connections(1)
        .delivery(MarketFeedDelivery::Batches)
        .reconnect_delay_ms(10)
        .market_feed_url(endpoint)
        .build();
    manager.start().await.unwrap();
    let mut receiver = manager.get_batch_channel(ConnectionId(0)).unwrap();
    let instrument = Instrument::new("NSE_EQ", "101");
    manager
        .subscribe(
            std::slice::from_ref(&instrument),
            FeedRequestCode::SubscribeTicker,
        )
        .await
        .unwrap();
    assert!(
        timeout(Duration::from_secs(3), queries.recv())
            .await
            .unwrap()
            .unwrap()
            .contains("token=old-token")
    );
    drop(
        timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    manager.update_access_token("new-token").unwrap();
    close.send(()).await.unwrap();
    assert!(
        timeout(Duration::from_secs(3), queries.recv())
            .await
            .unwrap()
            .unwrap()
            .contains("token=new-token")
    );
    let batch = timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        batch.events()[0],
        MarketFeedEvent::Ticker { ltt: 2, .. }
    ));
    drop(batch);
    wait_quality(&manager, MarketDataQuality::GapDetected).await;
    manager.acknowledge_gap(ConnectionId(0)).await.unwrap();
    wait_quality(&manager, MarketDataQuality::Current).await;
    manager
        .unsubscribe(
            std::slice::from_ref(&instrument),
            FeedRequestCode::UnsubscribeTicker,
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(3), async {
        while manager.health().connections[0].lifecycle
            != dhan_rs::ws::manager::ConnectionLifecycle::Idle
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    manager
        .subscribe(&[instrument], FeedRequestCode::SubscribeQuote)
        .await
        .unwrap();
    assert!(
        timeout(Duration::from_secs(3), queries.recv())
            .await
            .unwrap()
            .unwrap()
            .contains("token=new-token")
    );
    let batch = timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        batch.events()[0],
        MarketFeedEvent::Ticker { ltt: 3, .. }
    ));
    drop(batch);
    wait_quality(&manager, MarketDataQuality::Current).await;
    manager.shutdown().await.unwrap();
    task.await.unwrap();
}
