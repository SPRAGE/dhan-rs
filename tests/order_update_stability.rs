use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dhan_rs::ws::order_update::{
    ManagedOrderUpdate, ManagedOrderUpdateConfig, ManagedOrderUpdateEvent, ManagedOrderUpdateState,
    OrderUpdateClose, OrderUpdateConnectionState, OrderUpdateControlKind,
    OrderUpdateCredentialSnapshot, OrderUpdateGapCause, OrderUpdateMessage,
    OrderUpdateProtocolEvent, OrderUpdateReconciler, OrderUpdateReconciliationError,
    OrderUpdateReconciliationFuture, OrderUpdateStream, order_update_credential_channel,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

fn self_auth(client: &str, token: &str) -> String {
    format!(
        r#"{{"LoginReq":{{"MsgCode":42,"ClientId":"{client}","Token":"{token}"}},"UserType":"SELF"}}"#
    )
}

fn partner_auth(partner: &str, secret: &str) -> String {
    format!(
        r#"{{"LoginReq":{{"MsgCode":42,"ClientId":"{partner}"}},"UserType":"PARTNER","Secret":"{secret}"}}"#
    )
}

fn update_json(client: &str, order: &str, status: &str, traded: i64, time: &str) -> String {
    format!(
        r#"{{"Type":"order_alert","Data":{{"ClientId":"{client}","OrderNo":"{order}","Status":"{status}","TradedQty":{traded},"LastUpdatedTime":"{time}"}}}}"#
    )
}

fn update(client: &str, order: &str, status: &str, traded: i64, time: &str) -> OrderUpdateMessage {
    serde_json::from_str(&update_json(client, order, status, traded, time)).unwrap()
}

async fn endpoint(listener: &TcpListener) -> String {
    format!("ws://{}", listener.local_addr().unwrap())
}

fn managed_config(endpoint: String) -> ManagedOrderUpdateConfig {
    ManagedOrderUpdateConfig {
        endpoint,
        event_capacity: 64,
        reconciliation_buffer_capacity: 16,
        reconciliation_client_timeout: Duration::from_millis(100),
        reconciliation_overall_timeout: Duration::from_millis(250),
        reconciliation_concurrency: 4,
        connect_timeout: Duration::from_secs(1),
        write_timeout: Duration::from_secs(1),
        inactivity_timeout: Duration::from_secs(1),
        readiness_timeout: Duration::from_millis(250),
        close_timeout: Duration::from_secs(1),
        initial_backoff: Duration::from_millis(2),
        max_backoff: Duration::from_millis(10),
        stable_connection_period: Duration::from_millis(100),
    }
}

#[test]
fn documented_numeric_variants_deserialize_to_typed_scalars() {
    let string_and_number: OrderUpdateMessage = serde_json::from_str(
        r#"{"Type":"order_alert","Data":{"ClientId":"client","OrderNo":"order","Status":"Pending","AlgoOrdNo":"17.5","StrikePrice":100.25,"multiplier":"25"}}"#,
    )
    .unwrap();
    assert_eq!(string_and_number.Data.AlgoOrdNo, Some(17.5));
    assert_eq!(string_and_number.Data.StrikePrice, Some(100.25));
    assert_eq!(string_and_number.Data.multiplier, Some(25));

    let number_string_and_blank: OrderUpdateMessage = serde_json::from_str(
        r#"{"Type":"order_alert","Data":{"ClientId":"client","OrderNo":"order","Status":"Pending","AlgoOrdNo":18,"StrikePrice":"101.5","multiplier":" "}}"#,
    )
    .unwrap();
    assert_eq!(number_string_and_blank.Data.AlgoOrdNo, Some(18.0));
    assert_eq!(number_string_and_blank.Data.StrikePrice, Some(101.5));
    assert_eq!(number_string_and_blank.Data.multiplier, None);

    let nulls: OrderUpdateMessage = serde_json::from_str(
        r#"{"Type":"order_alert","Data":{"ClientId":"client","OrderNo":"order","Status":"Pending","AlgoOrdNo":null,"StrikePrice":"","multiplier":null}}"#,
    )
    .unwrap();
    assert_eq!(nulls.Data.AlgoOrdNo, None);
    assert_eq!(nulls.Data.StrikePrice, None);
    assert_eq!(nulls.Data.multiplier, None);
}

#[tokio::test]
async fn low_level_self_frame_control_binary_policy_and_close_are_exact() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        assert_eq!(
            ws.next().await.unwrap().unwrap(),
            Message::Text(self_auth("client-1", "token-1").into())
        );
        ws.send(Message::Text(
            r#"{"Type":"login_response","success":true}"#.into(),
        ))
        .await
        .unwrap();
        ws.send(Message::Binary(
            update_json("client-1", "order-1", "Pending", 0, "2026-08-11 10:00:00")
                .into_bytes()
                .into(),
        ))
        .await
        .unwrap();
        ws.send(Message::Close(Some(CloseFrame {
            code: CloseCode::Policy,
            reason: Utf8Bytes::from_static("credential rejected"),
        })))
        .await
        .unwrap();
    });

    let mut stream = OrderUpdateStream::connect_to(&endpoint, "client-1", "token-1")
        .await
        .unwrap();
    assert_eq!(
        stream.state(),
        &OrderUpdateConnectionState::ReadinessPending
    );
    let authorization = stream.next_protocol_event().await.unwrap().unwrap();
    assert!(matches!(
        authorization,
        OrderUpdateProtocolEvent::Authorization(ref control)
            if control.kind == OrderUpdateControlKind::Authorization
    ));
    let event = stream.next_protocol_event().await.unwrap().unwrap();
    assert!(matches!(event, OrderUpdateProtocolEvent::Update(_)));
    assert_eq!(stream.state(), &OrderUpdateConnectionState::Live);
    let close = stream.next_protocol_event().await.unwrap().unwrap();
    assert_eq!(
        close,
        OrderUpdateProtocolEvent::Close(OrderUpdateClose {
            code: Some(1008),
            reason: "credential rejected".to_owned(),
        })
    );
    assert_eq!(
        stream.last_close(),
        Some(&OrderUpdateClose {
            code: Some(1008),
            reason: "credential rejected".to_owned(),
        })
    );
    server.await.unwrap();
}

#[tokio::test]
async fn low_level_partner_frame_is_exact_and_type_discriminator_is_enforced() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        assert_eq!(
            ws.next().await.unwrap().unwrap(),
            Message::Text(partner_auth("partner-1", "partner-secret").into())
        );
        ws.send(Message::Text(r#"{"Type":"heartbeat"}"#.into()))
            .await
            .unwrap();
        ws.send(Message::Text(r#"{"Type":"error","ErrorCode":809}"#.into()))
            .await
            .unwrap();
        ws.send(Message::Text(
            r#"{"Type":"not_an_order_alert","Data":{}}"#.into(),
        ))
        .await
        .unwrap();
    });

    let mut stream =
        OrderUpdateStream::connect_partner_to(&endpoint, "partner-1", "partner-secret")
            .await
            .unwrap();
    assert!(matches!(
        stream.next_protocol_event().await.unwrap(),
        Some(OrderUpdateProtocolEvent::Control(_))
    ));
    assert!(matches!(
        stream.next_protocol_event().await.unwrap(),
        Some(OrderUpdateProtocolEvent::Error(ref control))
            if control.kind == OrderUpdateControlKind::Error && control.code == Some(809)
    ));
    let error = stream.next_protocol_event().await.unwrap_err().to_string();
    assert_eq!(
        error,
        "Invalid argument: unsupported order-update Type discriminator"
    );
    server.await.unwrap();
}

#[derive(Clone)]
struct MapReconciler {
    snapshots:
        Arc<HashMap<String, Result<Vec<OrderUpdateMessage>, OrderUpdateReconciliationError>>>,
    delay: Duration,
}

impl OrderUpdateReconciler for MapReconciler {
    fn reconcile<'a>(&'a self, client_id: &'a str) -> OrderUpdateReconciliationFuture<'a> {
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            self.snapshots.get(client_id).cloned().unwrap_or(Err(
                OrderUpdateReconciliationError::AuthorizationUnavailable,
            ))
        })
    }
}

#[derive(Clone)]
struct CountingReconciler {
    calls: Arc<AtomicUsize>,
    delay: Duration,
    result: Result<Vec<OrderUpdateMessage>, OrderUpdateReconciliationError>,
}

impl OrderUpdateReconciler for CountingReconciler {
    fn reconcile<'a>(&'a self, _client_id: &'a str) -> OrderUpdateReconciliationFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.result.clone()
        })
    }
}

async fn wait_for_calls(calls: &AtomicUsize, expected: usize) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while calls.load(Ordering::SeqCst) < expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

struct HungPartnerReconciler;

impl OrderUpdateReconciler for HungPartnerReconciler {
    fn reconcile<'a>(&'a self, client_id: &'a str) -> OrderUpdateReconciliationFuture<'a> {
        Box::pin(async move {
            match client_id {
                "A" => Ok(vec![update("A", "a-1", "Traded", 1, "02")]),
                "B" => std::future::pending().await,
                "C" => Err(OrderUpdateReconciliationError::AuthorizationUnavailable),
                _ => Err(OrderUpdateReconciliationError::AuthorizationUnavailable),
            }
        })
    }
}

#[tokio::test]
async fn managed_retries_repeated_failures_and_uses_rotated_token() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel();
    let (send_recovered_tx, send_recovered_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut send_recovered_rx = Some(send_recovered_rx);
        for connection in 0..3 {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            let auth = ws
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap()
                .to_string();
            auth_tx.send(auth).unwrap();
            if connection < 2 {
                drop(ws);
            } else {
                send_recovered_rx.take().unwrap().await.unwrap();
                ws.send(Message::Text(
                    update_json("client-1", "order-recovered", "Pending", 0, "03").into(),
                ))
                .await
                .unwrap();
                while let Some(message) = ws.next().await {
                    if matches!(message, Ok(Message::Close(_))) {
                        break;
                    }
                }
            }
        }
    });

    let (updater, credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client-1", "old-token"),
    );
    let (manager, mut events) =
        ManagedOrderUpdate::start(managed_config(endpoint), credentials, None).unwrap();
    assert_eq!(
        auth_rx.recv().await.unwrap(),
        self_auth("client-1", "old-token")
    );
    assert!(updater.replace(OrderUpdateCredentialSnapshot::self_user(
        2,
        "client-1",
        "new-token"
    )));
    let second = auth_rx.recv().await.unwrap();
    let third = auth_rx.recv().await.unwrap();
    assert_eq!(second, self_auth("client-1", "new-token"));
    assert_eq!(third, self_auth("client-1", "new-token"));

    tokio::time::timeout(Duration::from_secs(1), async {
        while manager.state() != ManagedOrderUpdateState::Degraded {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    send_recovered_tx.send(()).unwrap();

    let recovered = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(ManagedOrderUpdateEvent::Update(update)) = events.recv().await
                && update.Data.OrderNo.as_deref() == Some("order-recovered")
            {
                break;
            }
        }
    })
    .await;
    assert!(recovered.is_ok());
    assert_eq!(manager.state(), ManagedOrderUpdateState::Degraded);
    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_order_alert_triggers_reconciliation_but_control_does_not() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let (malformed_tx, malformed_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        let _ = ws.next().await;
        ws.send(Message::Text(r#"{"Type":"heartbeat"}"#.into()))
            .await
            .unwrap();
        malformed_rx.await.unwrap();
        ws.send(Message::Text(r#"{"Type":"order_alert","Data":{}}"#.into()))
            .await
            .unwrap();
        while let Some(message) = ws.next().await {
            if matches!(message, Ok(Message::Close(_))) {
                break;
            }
        }
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let reconciler = Arc::new(CountingReconciler {
        calls: calls.clone(),
        delay: Duration::from_millis(150),
        result: Ok(Vec::new()),
    });
    let (_, credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client", "token"),
    );
    let mut config = managed_config(endpoint);
    config.readiness_timeout = Duration::from_millis(50);
    config.reconciliation_client_timeout = Duration::from_millis(300);
    config.reconciliation_overall_timeout = Duration::from_millis(400);
    let (manager, mut events) =
        ManagedOrderUpdate::start(config, credentials, Some(reconciler)).unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                events.recv().await,
                Some(ManagedOrderUpdateEvent::Control(_))
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    malformed_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                events.recv().await,
                Some(ManagedOrderUpdateEvent::GapDetected {
                    cause: OrderUpdateGapCause::MalformedApplicationFrame
                })
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    wait_for_calls(&calls, 1).await;
    tokio::time::sleep(Duration::from_millis(70)).await;
    assert!(matches!(
        manager.state(),
        ManagedOrderUpdateState::Reconciling { .. }
    ));
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                events.recv().await,
                Some(ManagedOrderUpdateEvent::Reconciled { .. })
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        manager.state(),
        ManagedOrderUpdateState::ReadinessPending
            | ManagedOrderUpdateState::ReadinessUnconfirmed { .. }
    ));
    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn silent_transport_hits_inactivity_gap_and_reconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let server = tokio::spawn(async move {
        let (first_tcp, _) = listener.accept().await.unwrap();
        let mut first = accept_async(first_tcp).await.unwrap();
        let _ = first.next().await;
        while first.next().await.is_some() {}

        let (second_tcp, _) = listener.accept().await.unwrap();
        let mut second = accept_async(second_tcp).await.unwrap();
        let _ = second.next().await;
        second
            .send(Message::Text(
                update_json("client", "after-silence", "Pending", 0, "01").into(),
            ))
            .await
            .unwrap();
        while let Some(message) = second.next().await {
            if matches!(message, Ok(Message::Close(_))) {
                break;
            }
        }
    });
    let (_, credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client", "token"),
    );
    let mut config = managed_config(endpoint);
    config.readiness_timeout = Duration::from_millis(10);
    config.inactivity_timeout = Duration::from_millis(30);
    let (manager, mut events) = ManagedOrderUpdate::start(config, credentials, None).unwrap();

    let (saw_inactivity, saw_gap, saw_reconnect, saw_update) =
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut saw_inactivity = false;
            let mut saw_gap = false;
            let mut saw_reconnect = false;
            let mut saw_update = false;
            while !(saw_inactivity && saw_gap && saw_reconnect && saw_update) {
                match events.recv().await {
                    Some(ManagedOrderUpdateEvent::Disconnect { reason, .. }) => {
                        saw_inactivity |= reason.contains("inactivity deadline");
                    }
                    Some(ManagedOrderUpdateEvent::GapDetected {
                        cause: OrderUpdateGapCause::UncertainDisconnect,
                    }) => saw_gap = true,
                    Some(ManagedOrderUpdateEvent::StateChanged(
                        ManagedOrderUpdateState::Connecting { attempt },
                    )) if attempt >= 2 => saw_reconnect = true,
                    Some(ManagedOrderUpdateEvent::Update(update))
                        if update.Data.OrderNo.as_deref() == Some("after-silence") =>
                    {
                        saw_update = true;
                    }
                    _ => {}
                }
            }
            (saw_inactivity, saw_gap, saw_reconnect, saw_update)
        })
        .await
        .expect("silent transport did not reconnect with an explicit gap");
    assert!(saw_inactivity && saw_gap && saw_reconnect && saw_update);
    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn quiet_connection_reports_unconfirmed_readiness_without_false_live_state() {
    let (_, invalid_credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client", "token"),
    );
    let invalid_config = ManagedOrderUpdateConfig {
        initial_backoff: Duration::ZERO,
        ..ManagedOrderUpdateConfig::default()
    };
    assert!(ManagedOrderUpdate::start(invalid_config, invalid_credentials, None).is_err());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        let _ = ws.next().await;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if ws.send(Message::Ping(vec![1].into())).await.is_err() {
                        break;
                    }
                }
                message = ws.next() => {
                    match message {
                        Some(Ok(Message::Close(_))) => {
                            let _ = closed_tx.send(());
                            break;
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                    }
                }
            }
        }
    });
    let (_, credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client", "token"),
    );
    let mut config = managed_config(endpoint);
    config.readiness_timeout = Duration::from_millis(30);
    config.inactivity_timeout = Duration::from_millis(80);
    let (manager, mut events) = ManagedOrderUpdate::start(config, credentials, None).unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                events.recv().await,
                Some(ManagedOrderUpdateEvent::ReadinessTimeout { waited })
                    if waited == Duration::from_millis(30)
            ) {
                break;
            }
        }
    })
    .await
    .expect("quiet connection did not report readiness timeout");
    assert_eq!(
        manager.state(),
        ManagedOrderUpdateState::ReadinessUnconfirmed {
            waited: Duration::from_millis(30)
        }
    );
    manager.shutdown().await.unwrap();
    closed_rx.await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn stable_healthy_traffic_resets_the_reconnect_attempt_counter() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let (fourth_ready_tx, fourth_ready_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut fourth_ready_tx = Some(fourth_ready_tx);
        for connection in 0..4 {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(tcp).await.unwrap();
            let _ = ws.next().await;
            match connection {
                0 | 1 => drop(ws),
                2 => {
                    ws.send(Message::Text(
                        update_json("client", "stable-order", "Pending", 0, "01").into(),
                    ))
                    .await
                    .unwrap();
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    drop(ws);
                }
                _ => {
                    fourth_ready_tx.take().unwrap().send(()).unwrap();
                    while let Some(message) = ws.next().await {
                        if matches!(message, Ok(Message::Close(_))) {
                            break;
                        }
                    }
                }
            }
        }
    });
    let (_, credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client", "token"),
    );
    let mut config = managed_config(endpoint);
    config.stable_connection_period = Duration::from_millis(25);
    let (manager, mut events) = ManagedOrderUpdate::start(config, credentials, None).unwrap();

    let attempts = tokio::time::timeout(Duration::from_secs(2), async {
        let mut attempts = Vec::new();
        while attempts.len() < 4 {
            if let Some(ManagedOrderUpdateEvent::StateChanged(
                ManagedOrderUpdateState::Connecting { attempt },
            )) = events.recv().await
            {
                attempts.push(attempt);
            }
        }
        attempts
    })
    .await
    .unwrap();
    assert_eq!(attempts, vec![1, 2, 3, 1]);
    fourth_ready_rx.await.unwrap();
    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reconnect_buffers_live_data_and_snapshot_terminal_state_wins() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut first = accept_async(tcp).await.unwrap();
        let _ = first.next().await;
        first
            .send(Message::Text(
                update_json("client-1", "order-1", "Pending", 0, "01").into(),
            ))
            .await
            .unwrap();
        drop(first);

        let (tcp, _) = listener.accept().await.unwrap();
        let mut second = accept_async(tcp).await.unwrap();
        let _ = second.next().await;
        second
            .send(Message::Text(
                update_json("client-1", "order-1", "Pending", 0, "01").into(),
            ))
            .await
            .unwrap();
        while let Some(message) = second.next().await {
            if matches!(message, Ok(Message::Close(_))) {
                break;
            }
        }
    });
    let snapshots = HashMap::from([(
        "client-1".to_owned(),
        Ok(vec![update("client-1", "order-1", "Traded", 10, "02")]),
    )]);
    let reconciler = Arc::new(MapReconciler {
        snapshots: Arc::new(snapshots),
        delay: Duration::from_millis(30),
    });
    let (_, credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client-1", "token"),
    );
    let (manager, mut events) =
        ManagedOrderUpdate::start(managed_config(endpoint), credentials, Some(reconciler)).unwrap();

    let mut saw_gap = false;
    let reconciled_status = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Some(ManagedOrderUpdateEvent::GapDetected {
                    cause: OrderUpdateGapCause::UncertainDisconnect,
                }) => saw_gap = true,
                Some(ManagedOrderUpdateEvent::Update(update))
                    if saw_gap && update.Data.OrderNo.as_deref() == Some("order-1") =>
                {
                    break update.Data.Status;
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(reconciled_status.as_deref(), Some("Traded"));
    tokio::time::timeout(Duration::from_secs(1), async {
        while manager.state() != ManagedOrderUpdateState::Live {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn partner_reconciliation_isolates_hung_client_and_keeps_gap_unresolved() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let (disconnect_tx, disconnect_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut first = accept_async(tcp).await.unwrap();
        assert_eq!(
            first.next().await.unwrap().unwrap(),
            Message::Text(partner_auth("partner", "secret").into())
        );
        first
            .send(Message::Text(
                update_json("A", "a-1", "Pending", 0, "01").into(),
            ))
            .await
            .unwrap();
        first
            .send(Message::Text(
                update_json("B", "b-1", "Pending", 0, "01").into(),
            ))
            .await
            .unwrap();
        first
            .send(Message::Text(
                update_json("C", "c-1", "Pending", 0, "01").into(),
            ))
            .await
            .unwrap();
        disconnect_rx.await.unwrap();
        drop(first);
        let (tcp, _) = listener.accept().await.unwrap();
        let mut second = accept_async(tcp).await.unwrap();
        let _ = second.next().await;
        while let Some(message) = second.next().await {
            if matches!(message, Ok(Message::Close(_))) {
                break;
            }
        }
    });
    let reconciler = Arc::new(HungPartnerReconciler);
    let (_, credentials) = order_update_credential_channel(OrderUpdateCredentialSnapshot::partner(
        1, "partner", "secret",
    ));
    let mut config = managed_config(endpoint);
    config.reconciliation_client_timeout = Duration::from_millis(20);
    config.reconciliation_overall_timeout = Duration::from_millis(60);
    config.reconciliation_concurrency = 2;
    config.readiness_timeout = Duration::from_millis(30);
    let (manager, mut events) =
        ManagedOrderUpdate::start(config, credentials, Some(reconciler)).unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        let mut seen = [false; 3];
        while !seen.into_iter().all(|value| value) {
            if let Some(ManagedOrderUpdateEvent::Update(update)) = events.recv().await {
                match update.Data.ClientId.as_deref() {
                    Some("A") => seen[0] = true,
                    Some("B") => seen[1] = true,
                    Some("C") => seen[2] = true,
                    _ => {}
                }
            }
        }
    })
    .await
    .unwrap();
    disconnect_tx.send(()).unwrap();

    let (saw_a, saw_b, saw_c) = tokio::time::timeout(Duration::from_secs(2), async {
        let mut saw_a = false;
        let mut saw_b = false;
        let mut saw_c = false;
        while !(saw_a && saw_b && saw_c) {
            match events.recv().await {
                Some(ManagedOrderUpdateEvent::Reconciled { client_ids }) => {
                    saw_a |= client_ids == vec!["A".to_owned()];
                }
                Some(ManagedOrderUpdateEvent::GapUnresolved { client_id, reason }) => {
                    saw_b |=
                        client_id.as_deref() == Some("B") && reason.contains("deadline exceeded");
                    saw_c |= client_id.as_deref() == Some("C")
                        && reason.contains("authorized reconciliation context unavailable");
                }
                _ => {}
            }
        }
        (saw_a, saw_b, saw_c)
    })
    .await
    .unwrap();
    assert!(saw_a && saw_b && saw_c);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(manager.state(), ManagedOrderUpdateState::Degraded);
    manager.shutdown().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn consumer_overflow_reports_gap_and_shutdown_awaits_peer_close() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        let _ = ws.next().await;
        for index in 0..20 {
            ws.send(Message::Text(
                update_json("client", &format!("order-{index}"), "Pending", 0, "01").into(),
            ))
            .await
            .unwrap();
        }
        while let Some(message) = ws.next().await {
            if matches!(message, Ok(Message::Close(_))) {
                let _ = closed_tx.send(());
                break;
            }
        }
    });
    let (_, credentials) = order_update_credential_channel(
        OrderUpdateCredentialSnapshot::self_user(1, "client", "token"),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let reconciler = Arc::new(CountingReconciler {
        calls: calls.clone(),
        delay: Duration::from_millis(100),
        result: Ok(Vec::new()),
    });
    let mut config = managed_config(endpoint);
    config.event_capacity = 2;
    config.reconciliation_client_timeout = Duration::from_millis(500);
    config.reconciliation_overall_timeout = Duration::from_millis(700);
    let (manager, mut events) =
        ManagedOrderUpdate::start(config, credentials, Some(reconciler)).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let event = events.recv().await.unwrap();
    assert!(matches!(
        event,
        ManagedOrderUpdateEvent::GapDetected {
            cause: OrderUpdateGapCause::ConsumerLag { dropped }
        } if dropped > 0
    ));
    wait_for_calls(&calls, 1).await;
    assert!(matches!(
        manager.state(),
        ManagedOrderUpdateState::Reconciling { .. }
    ));
    tokio::time::timeout(Duration::from_secs(1), async {
        while manager.state() != ManagedOrderUpdateState::Live {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    manager.shutdown().await.unwrap();
    closed_rx.await.unwrap();
    server.await.unwrap();
}

#[derive(Clone)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn credentials_and_malformed_payloads_never_enter_tracing() {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let writer_logs = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || SharedWriter(writer_logs.clone()))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = endpoint(&listener).await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(tcp).await.unwrap();
        let _ = ws.next().await;
        ws.send(Message::Text(
            r#"{"Type":"payload-marker-7D2B","Data":{}}"#.into(),
        ))
        .await
        .unwrap();
    });
    let credential = OrderUpdateCredentialSnapshot::self_user(
        7,
        "principal-marker-8C31",
        "credential-marker-4A91",
    );
    let credential_debug = format!("{credential:?}");
    assert!(!credential_debug.contains("principal-marker-8C31"));
    assert!(!credential_debug.contains("credential-marker-4A91"));
    let mut stream = OrderUpdateStream::connect_to(&endpoint, "client", "credential-marker-4A91")
        .await
        .unwrap();
    assert!(stream.next_protocol_event().await.is_err());
    server.await.unwrap();
    let output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(!output.contains("credential-marker-4A91"));
    assert!(!output.contains("payload-marker-7D2B"));
}
