#![allow(missing_docs)]
//! Supervised, multi-connection DhanHQ standard market-feed manager.
//!
//! Connections are opened lazily on first subscription.  Each live socket is
//! owned by exactly one supervisor task; callers change desired state through
//! commands, so no mutex is held across network I/O.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use url::Url;

use crate::constants::WS_MARKET_FEED_URL;
use crate::error::{DhanError, Result};
use crate::types::enums::FeedRequestCode;
use crate::ws::market_feed::{Instrument, MAX_PACKETS_PER_MESSAGE, MarketFeedEvent, parse_packets};

const DHAN_MAX_CONNECTIONS: u8 = 5;
const DHAN_MAX_INSTRUMENTS: usize = 5_000;
const DHAN_CONTROL_FRAME_LIMIT: usize = 100;
const COMMAND_CAPACITY: usize = 64;
const LIFECYCLE_CAPACITY: usize = 256;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const JOIN_TIMEOUT: Duration = Duration::from_secs(1);
const NO_FRAME_TIMEOUT: Duration = Duration::from_secs(30);
const STABLE_RETRY_RESET: Duration = Duration::from_secs(60);
const MAX_RETRY_BASE: Duration = Duration::from_secs(30);

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type DesiredSubscriptions = HashMap<InstrumentKey, (Instrument, FeedRequestCode)>;

/// Identifies one of the managed WebSocket connection slots (0-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u8);

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Connection({})", self.0)
    }
}

/// Truthful lifecycle state of a connection supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionLifecycle {
    Stopped,
    Idle,
    Connecting,
    Resubscribing,
    ReadinessPending,
    Live,
    Degraded,
    Backoff,
    Blocked,
    Closing,
}

/// Data-quality state is independent of transport connectivity. In
/// particular, a reconnect cannot repair ticks already missed during a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDataQuality {
    Unavailable,
    ReadinessPending,
    Current,
    GapDetected,
}

/// Typed cause of a market-data gap that requires caller reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GapCause {
    Disconnect {
        close_code: Option<u16>,
        reason: String,
    },
    ParseError {
        error: String,
    },
    ReceiverLag {
        dropped: u64,
    },
    NoReceiver {
        event: &'static str,
    },
}

/// Typed control-plane events emitted separately from market ticks.
#[derive(Debug, Clone)]
pub enum ManagerLifecycleEvent {
    StateChanged {
        id: ConnectionId,
        state: ConnectionLifecycle,
    },
    Connected {
        id: ConnectionId,
        credential_version: u64,
    },
    Disconnected {
        id: ConnectionId,
        close_code: Option<u16>,
        reason: String,
    },
    DhanDisconnected {
        id: ConnectionId,
        reason_code: i16,
    },
    ParseError {
        id: ConnectionId,
        error: String,
    },
    ReceiverLag {
        id: ConnectionId,
        dropped: u64,
    },
    GapDetected {
        id: ConnectionId,
        cause: GapCause,
    },
    GapAcknowledged {
        id: ConnectionId,
    },
    ReadinessFailure {
        id: ConnectionId,
        error: String,
    },
    NoReceiver {
        id: ConnectionId,
        event: &'static str,
    },
    RetryScheduled {
        id: ConnectionId,
        attempt: u64,
        delay: Duration,
    },
    Error {
        id: ConnectionId,
        error: String,
    },
    Stopped {
        id: ConnectionId,
        error: Option<String>,
    },
}

/// Health status of a single managed connection.
#[derive(Debug, Clone)]
pub struct ConnectionHealth {
    /// Whether the slot's background supervisor task is alive.
    pub is_alive: bool,
    pub id: ConnectionId,
    /// Latest desired subscription count (not a server acknowledgement).
    pub instrument_count: usize,
    /// Successful connections after the first transport.
    pub reconnect_count: u64,
    pub lifecycle: ConnectionLifecycle,
    pub transport_connected: bool,
    /// True only after a valid binary data frame on the current transport.
    pub data_live: bool,
    /// Durable data quality. `GapDetected` remains until acknowledge_gap().
    pub data_quality: MarketDataQuality,
    pub gap_cause: Option<GapCause>,
    pub gap_detected_at: Option<SystemTime>,
    pub gap_count: u64,
    pub desired_generation: u64,
    /// Latest desired generation completely written to the current transport.
    pub applied_generation: u64,
    /// Current consecutive retry attempt.
    pub retry_count: u64,
    pub last_error: Option<String>,
    pub last_frame_at: Option<SystemTime>,
    pub last_data_at: Option<SystemTime>,
    pub last_state_change_at: SystemTime,
    pub retry_at: Option<SystemTime>,
    pub parser_error_count: u64,
    pub lagged_event_count: u64,
    /// Latest credential version observed by the supervisor. While a socket is
    /// live this may be newer than the version that authenticated that socket.
    pub credential_version: u64,
}

impl ConnectionHealth {
    fn new(id: ConnectionId) -> Self {
        Self {
            is_alive: false,
            id,
            instrument_count: 0,
            reconnect_count: 0,
            lifecycle: ConnectionLifecycle::Stopped,
            transport_connected: false,
            data_live: false,
            data_quality: MarketDataQuality::Unavailable,
            gap_cause: None,
            gap_detected_at: None,
            gap_count: 0,
            desired_generation: 0,
            applied_generation: 0,
            retry_count: 0,
            last_error: None,
            last_frame_at: None,
            last_data_at: None,
            last_state_change_at: SystemTime::now(),
            retry_at: None,
            parser_error_count: 0,
            lagged_event_count: 0,
            credential_version: 0,
        }
    }
}

/// Aggregate health summary across all configured slots.
#[derive(Debug, Clone)]
pub struct HealthSummary {
    pub connections: Vec<ConnectionHealth>,
    pub total_instruments: usize,
    pub alive_connections: usize,
}

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct FeedSubscribeRequest {
    RequestCode: u8,
    InstrumentCount: usize,
    InstrumentList: Vec<Instrument>,
}

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct FeedDisconnectRequest {
    RequestCode: u8,
}

/// Configuration for [`DhanFeedManager`].
#[derive(Debug, Clone)]
pub struct DhanFeedConfig {
    pub max_connections: u8,
    pub max_instruments_per_connection: usize,
    pub enable_raw_frames: bool,
    /// Initial exponential retry base in milliseconds. Full jitter is applied.
    pub reconnect_delay_ms: u64,
    pub parsed_channel_capacity: usize,
    pub raw_channel_capacity: usize,
    pub auto_reconnect: bool,
}

impl Default for DhanFeedConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
            max_instruments_per_connection: DHAN_MAX_INSTRUMENTS,
            enable_raw_frames: false,
            reconnect_delay_ms: 250,
            parsed_channel_capacity: MAX_PACKETS_PER_MESSAGE,
            raw_channel_capacity: 4_096,
            auto_reconnect: true,
        }
    }
}

/// Builder for [`DhanFeedManager`].
pub struct DhanFeedManagerBuilder {
    client_id: String,
    access_token: String,
    config: DhanFeedConfig,
    endpoint: String,
}

impl DhanFeedManagerBuilder {
    pub fn new(client_id: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            access_token: access_token.into(),
            config: DhanFeedConfig::default(),
            endpoint: WS_MARKET_FEED_URL.to_owned(),
        }
    }

    pub fn max_connections(mut self, n: u8) -> Self {
        self.config.max_connections = n.clamp(1, DHAN_MAX_CONNECTIONS);
        self
    }

    pub fn max_instruments_per_connection(mut self, n: usize) -> Self {
        self.config.max_instruments_per_connection = n.min(DHAN_MAX_INSTRUMENTS);
        self
    }

    pub fn enable_raw_frames(mut self, enable: bool) -> Self {
        self.config.enable_raw_frames = enable;
        self
    }

    pub fn reconnect_delay_ms(mut self, ms: u64) -> Self {
        self.config.reconnect_delay_ms = ms;
        self
    }

    pub fn parsed_channel_capacity(mut self, cap: usize) -> Self {
        self.config.parsed_channel_capacity = cap;
        self
    }

    pub fn raw_channel_capacity(mut self, cap: usize) -> Self {
        self.config.raw_channel_capacity = cap;
        self
    }

    pub fn auto_reconnect(mut self, enable: bool) -> Self {
        self.config.auto_reconnect = enable;
        self
    }

    /// Override the WebSocket endpoint, primarily for local deterministic tests.
    pub fn market_feed_url(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn build(self) -> DhanFeedManager {
        DhanFeedManager::new_with_endpoint(
            self.client_id,
            self.access_token,
            self.config,
            self.endpoint,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct InstrumentKey {
    exchange_segment: String,
    security_id: String,
}

impl From<&Instrument> for InstrumentKey {
    fn from(inst: &Instrument) -> Self {
        Self {
            exchange_segment: inst.ExchangeSegment.clone(),
            security_id: inst.SecurityId.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct Credentials {
    token: String,
    version: u64,
}

enum SupervisorCommand {
    ReplaceDesired {
        generation: u64,
        desired: DesiredSubscriptions,
        acknowledged: oneshot::Sender<()>,
    },
    AcknowledgeGap {
        acknowledged: oneshot::Sender<std::result::Result<(), String>>,
    },
    Shutdown {
        acknowledged: oneshot::Sender<()>,
    },
}

struct ManagedConnection {
    id: ConnectionId,
    parsed_tx: broadcast::Sender<MarketFeedEvent>,
    raw_tx: Option<broadcast::Sender<Bytes>>,
    lifecycle_tx: broadcast::Sender<ManagerLifecycleEvent>,
    health_rx: watch::Receiver<ConnectionHealth>,
    health_tx: watch::Sender<ConnectionHealth>,
    command_tx: mpsc::Sender<SupervisorCommand>,
    command_rx: Option<mpsc::Receiver<SupervisorCommand>>,
    task: Option<JoinHandle<()>>,
    instruments: DesiredSubscriptions,
    desired_generation: u64,
    previous_close: Arc<StdMutex<HashMap<(u8, u32), MarketFeedEvent>>>,
}

/// Multi-connection manager for the standard DhanHQ market feed.
pub struct DhanFeedManager {
    client_id: String,
    config: DhanFeedConfig,
    endpoint: String,
    credentials_tx: watch::Sender<Credentials>,
    connections: Vec<ManagedConnection>,
    started: bool,
}

impl DhanFeedManager {
    pub fn new(
        client_id: impl Into<String>,
        access_token: impl Into<String>,
        config: DhanFeedConfig,
    ) -> Self {
        Self::new_with_endpoint(
            client_id,
            access_token,
            config,
            WS_MARKET_FEED_URL.to_owned(),
        )
    }

    fn new_with_endpoint(
        client_id: impl Into<String>,
        access_token: impl Into<String>,
        config: DhanFeedConfig,
        endpoint: String,
    ) -> Self {
        let (credentials_tx, _) = watch::channel(Credentials {
            token: access_token.into(),
            version: 0,
        });
        let slot_count = config.max_connections.min(DHAN_MAX_CONNECTIONS) as usize;
        let connections = (0..slot_count)
            .map(|index| Self::new_connection(ConnectionId(index as u8), &config))
            .collect();
        Self {
            client_id: client_id.into(),
            config,
            endpoint,
            credentials_tx,
            connections,
            started: false,
        }
    }

    fn new_connection(id: ConnectionId, config: &DhanFeedConfig) -> ManagedConnection {
        // Use one here to keep the infallible constructor from panicking. start()
        // rejects the caller's zero capacity before any task or socket exists.
        let (parsed_tx, _) = broadcast::channel(config.parsed_channel_capacity.max(1));
        let raw_tx = config.enable_raw_frames.then(|| {
            let (tx, _) = broadcast::channel(config.raw_channel_capacity.max(1));
            tx
        });
        let (lifecycle_tx, _) = broadcast::channel(LIFECYCLE_CAPACITY);
        let (health_tx, health_rx) = watch::channel(ConnectionHealth::new(id));
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        ManagedConnection {
            id,
            parsed_tx,
            raw_tx,
            lifecycle_tx,
            health_rx,
            health_tx,
            command_tx,
            command_rx: Some(command_rx),
            task: None,
            instruments: HashMap::new(),
            desired_generation: 0,
            previous_close: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    /// Validate configuration and enable subscriptions. No socket is consumed
    /// until the first instrument is requested.
    pub async fn start(&mut self) -> Result<()> {
        if self.started {
            return Err(DhanError::InvalidArgument("manager already started".into()));
        }
        self.validate_config()?;
        Url::parse(&self.endpoint)?;
        self.started = true;
        tracing::info!(
            slots = self.connections.len(),
            "DhanFeedManager started lazily"
        );
        Ok(())
    }

    fn validate_config(&self) -> Result<()> {
        if !(1..=DHAN_MAX_CONNECTIONS).contains(&self.config.max_connections) {
            return Err(DhanError::InvalidArgument(
                "max_connections must be between 1 and 5".into(),
            ));
        }
        if !(1..=DHAN_MAX_INSTRUMENTS).contains(&self.config.max_instruments_per_connection) {
            return Err(DhanError::InvalidArgument(
                "max_instruments_per_connection must be between 1 and 5000".into(),
            ));
        }
        if self.config.parsed_channel_capacity == 0 || self.config.raw_channel_capacity == 0 {
            return Err(DhanError::InvalidArgument(
                "channel capacities must be nonzero".into(),
            ));
        }
        if self.config.reconnect_delay_ms == 0 {
            return Err(DhanError::InvalidArgument(
                "reconnect_delay_ms must be nonzero to prevent a hot retry loop".into(),
            ));
        }
        Ok(())
    }

    /// Replace the credential used by all future connection attempts.
    pub fn update_access_token(&mut self, access_token: impl Into<String>) -> Result<()> {
        let token = access_token.into();
        if token.is_empty() {
            return Err(DhanError::InvalidArgument(
                "access token must not be empty".into(),
            ));
        }
        let version = self.credentials_tx.borrow().version.saturating_add(1);
        self.credentials_tx
            .send_replace(Credentials { token, version });
        Ok(())
    }

    /// Clear a durable market-data gap after the caller has reconciled current
    /// state from an authoritative snapshot. A reconnect alone never calls
    /// this method or clears the quality latch.
    pub async fn acknowledge_gap(&mut self, id: ConnectionId) -> Result<()> {
        self.ensure_started()?;
        let connection = self.connections.get(id.0 as usize).ok_or_else(|| {
            DhanError::InvalidArgument(format!("unknown connection slot {}", id.0))
        })?;
        if connection.health_rx.borrow().data_quality != MarketDataQuality::GapDetected {
            return Ok(());
        }
        if !connection.health_rx.borrow().data_live {
            return Err(DhanError::InvalidArgument(format!(
                "{id} gap can be acknowledged only after the replacement transport is data-live"
            )));
        }
        if !connection
            .task
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            return Err(DhanError::InvalidArgument(format!(
                "{id} has no live supervisor to acknowledge"
            )));
        }
        let command_tx = connection.command_tx.clone();
        let (acknowledged, receiver) = oneshot::channel();
        command_tx
            .send(SupervisorCommand::AcknowledgeGap { acknowledged })
            .await
            .map_err(|_| DhanError::InvalidArgument(format!("{id} supervisor unavailable")))?;
        receiver
            .await
            .map_err(|_| DhanError::InvalidArgument(format!("{id} supervisor unavailable")))?
            .map_err(DhanError::InvalidArgument)
    }

    pub async fn subscribe(
        &mut self,
        instruments: &[Instrument],
        mode: FeedRequestCode,
    ) -> Result<()> {
        self.ensure_started()?;
        validate_subscribe_mode(mode)?;
        validate_instruments(instruments)?;
        if instruments.is_empty() {
            return Ok(());
        }

        let old: Vec<_> = self
            .connections
            .iter()
            .map(|connection| connection.instruments.clone())
            .collect();
        let mut candidate = old.clone();
        let mut loads: Vec<_> = candidate.iter().map(HashMap::len).collect();
        let mut locations = HashMap::new();
        for (index, desired) in candidate.iter().enumerate() {
            for key in desired.keys() {
                locations.insert(key.clone(), index);
            }
        }

        for instrument in instruments {
            let key = InstrumentKey::from(instrument);
            if let Some(index) = locations.get(&key).copied() {
                candidate[index].insert(key, (instrument.clone(), mode));
                continue;
            }
            let index = loads
                .iter()
                .enumerate()
                .filter(|(_, load)| **load < self.config.max_instruments_per_connection)
                .min_by_key(|(_, load)| **load)
                .map(|(index, _)| index)
                .ok_or_else(|| {
                    DhanError::InvalidArgument(format!(
                        "all connections at capacity ({} instruments each)",
                        self.config.max_instruments_per_connection
                    ))
                })?;
            candidate[index].insert(key.clone(), (instrument.clone(), mode));
            loads[index] += 1;
            locations.insert(key, index);
        }

        self.commit_desired(candidate, old).await
    }

    /// Remove instruments according to their tracked subscription mode.  The
    /// supplied request code is validated as an unsubscribe operation, but the
    /// wire code is derived from the manager's recorded mode.
    pub async fn unsubscribe(
        &mut self,
        instruments: &[Instrument],
        mode: FeedRequestCode,
    ) -> Result<()> {
        self.ensure_started()?;
        validate_unsubscribe_mode(mode)?;
        validate_instruments(instruments)?;
        let old: Vec<_> = self
            .connections
            .iter()
            .map(|connection| connection.instruments.clone())
            .collect();
        let mut candidate = old.clone();
        for instrument in instruments {
            let key = InstrumentKey::from(instrument);
            for desired in &mut candidate {
                if desired.remove(&key).is_some() {
                    break;
                }
            }
        }
        self.commit_desired(candidate, old).await
    }

    async fn commit_desired(
        &mut self,
        candidate: Vec<DesiredSubscriptions>,
        old: Vec<DesiredSubscriptions>,
    ) -> Result<()> {
        let affected: Vec<_> = candidate
            .iter()
            .zip(&old)
            .enumerate()
            .filter_map(|(index, (new, old))| (!desired_equal(new, old)).then_some(index))
            .collect();
        if affected.is_empty() {
            return Ok(());
        }

        // Desired state is committed before network work. Every supervisor
        // acknowledges its local replacement; if a task unexpectedly vanished,
        // all manager-side state is rolled back and previously updated tasks are
        // restored before the error is returned.
        let mut newly_spawned = HashSet::new();
        for &index in &affected {
            self.connections[index].instruments = candidate[index].clone();
            self.connections[index].desired_generation =
                self.connections[index].desired_generation.saturating_add(1);
            if self.ensure_supervisor(index)? {
                newly_spawned.insert(index);
            }
        }

        let mut applied = Vec::new();
        for &index in &affected {
            if newly_spawned.contains(&index) {
                // The initial desired generation is moved into the newly
                // spawned supervisor, so no duplicate command is needed.
                applied.push(index);
                continue;
            }
            let (acknowledged, receiver) = oneshot::channel();
            let command = SupervisorCommand::ReplaceDesired {
                generation: self.connections[index].desired_generation,
                desired: self.connections[index].instruments.clone(),
                acknowledged,
            };
            let result = self.connections[index].command_tx.send(command).await;
            if result.is_ok() && receiver.await.is_ok() {
                applied.push(index);
                continue;
            }

            for &rollback_index in &affected {
                self.connections[rollback_index].instruments = old[rollback_index].clone();
                self.connections[rollback_index].desired_generation = self.connections
                    [rollback_index]
                    .desired_generation
                    .saturating_add(1);
            }
            for rollback_index in applied {
                let (acknowledged, _) = oneshot::channel();
                let _ = self.connections[rollback_index]
                    .command_tx
                    .send(SupervisorCommand::ReplaceDesired {
                        generation: self.connections[rollback_index].desired_generation,
                        desired: old[rollback_index].clone(),
                        acknowledged,
                    })
                    .await;
            }
            return Err(DhanError::InvalidArgument(format!(
                "{} supervisor unavailable",
                self.connections[index].id
            )));
        }
        Ok(())
    }

    fn ensure_started(&self) -> Result<()> {
        if self.started {
            Ok(())
        } else {
            Err(DhanError::InvalidArgument(
                "manager not started - call start() first".into(),
            ))
        }
    }

    fn ensure_supervisor(&mut self, index: usize) -> Result<bool> {
        if self.connections[index]
            .task
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            return Ok(false);
        }
        if self.connections[index].task.is_some() {
            return Err(DhanError::InvalidArgument(format!(
                "{} supervisor terminated; inspect lifecycle health",
                self.connections[index].id
            )));
        }
        let receiver = self.connections[index].command_rx.take().ok_or_else(|| {
            DhanError::InvalidArgument("supervisor command receiver unavailable".into())
        })?;
        let arguments = SupervisorArguments {
            id: self.connections[index].id,
            client_id: self.client_id.clone(),
            endpoint: self.endpoint.clone(),
            auto_reconnect: self.config.auto_reconnect,
            retry_base: Duration::from_millis(self.config.reconnect_delay_ms),
            enable_raw: self.config.enable_raw_frames,
            parsed_capacity: self.config.parsed_channel_capacity,
            raw_capacity: self.config.raw_channel_capacity,
            parsed_tx: self.connections[index].parsed_tx.clone(),
            raw_tx: self.connections[index].raw_tx.clone(),
            lifecycle_tx: self.connections[index].lifecycle_tx.clone(),
            health_tx: self.connections[index].health_tx.clone(),
            credentials_rx: self.credentials_tx.subscribe(),
            command_rx: receiver,
            previous_close: self.connections[index].previous_close.clone(),
            desired: self.connections[index].instruments.clone(),
            generation: self.connections[index].desired_generation,
        };
        self.connections[index].task = Some(tokio::spawn(supervisor(arguments)));
        Ok(true)
    }

    pub fn get_parsed_channel(
        &self,
        id: ConnectionId,
    ) -> Option<broadcast::Receiver<MarketFeedEvent>> {
        self.connections
            .get(id.0 as usize)
            .map(|connection| connection.parsed_tx.subscribe())
    }

    pub fn get_all_parsed_channels(
        &self,
    ) -> Vec<(ConnectionId, broadcast::Receiver<MarketFeedEvent>)> {
        self.connections
            .iter()
            .map(|connection| (connection.id, connection.parsed_tx.subscribe()))
            .collect()
    }

    pub fn get_raw_channel(&self, id: ConnectionId) -> Option<broadcast::Receiver<Bytes>> {
        self.connections
            .get(id.0 as usize)
            .and_then(|connection| connection.raw_tx.as_ref().map(broadcast::Sender::subscribe))
    }

    pub fn get_all_raw_channels(&self) -> Vec<(ConnectionId, broadcast::Receiver<Bytes>)> {
        self.connections
            .iter()
            .filter_map(|connection| {
                connection
                    .raw_tx
                    .as_ref()
                    .map(|sender| (connection.id, sender.subscribe()))
            })
            .collect()
    }

    /// Subscribe to durable lifecycle diagnostics for one connection slot.
    pub fn get_lifecycle_channel(
        &self,
        id: ConnectionId,
    ) -> Option<broadcast::Receiver<ManagerLifecycleEvent>> {
        self.connections
            .get(id.0 as usize)
            .map(|connection| connection.lifecycle_tx.subscribe())
    }

    /// Watch the latest durable health snapshot for one connection slot.
    pub fn get_health_channel(
        &self,
        id: ConnectionId,
    ) -> Option<watch::Receiver<ConnectionHealth>> {
        self.connections
            .get(id.0 as usize)
            .map(|connection| connection.health_rx.clone())
    }

    /// Retrieve cached one-shot Previous Close events, including values that
    /// arrived before a broadcast receiver was created.
    pub fn previous_close_snapshot(&self, id: ConnectionId) -> Vec<MarketFeedEvent> {
        self.connections
            .get(id.0 as usize)
            .map(|connection| {
                connection
                    .previous_close
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .values()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn health(&self) -> HealthSummary {
        let connections: Vec<_> = self
            .connections
            .iter()
            .map(|connection| {
                let mut health = connection.health_rx.borrow().clone();
                health.is_alive = connection
                    .task
                    .as_ref()
                    .is_some_and(|task| !task.is_finished());
                health.instrument_count = connection.instruments.len();
                health.desired_generation = connection.desired_generation;
                health
            })
            .collect();
        HealthSummary {
            total_instruments: connections
                .iter()
                .map(|health| health.instrument_count)
                .sum(),
            alive_connections: connections.iter().filter(|health| health.is_alive).count(),
            connections,
        }
    }

    /// Gracefully stop reconnects, send Dhan RequestCode 12, complete a bounded
    /// WebSocket close handshake, and join every supervisor. Abort is fallback.
    pub async fn shutdown(&mut self) -> Result<()> {
        if !self.started {
            return Ok(());
        }
        let mut first_error = None;
        for connection in &mut self.connections {
            if connection.task.is_none() {
                connection.instruments.clear();
                continue;
            }
            let (acknowledged, receiver) = oneshot::channel();
            if connection
                .command_tx
                .send(SupervisorCommand::Shutdown { acknowledged })
                .await
                .is_ok()
                && timeout(CLOSE_TIMEOUT + JOIN_TIMEOUT, receiver)
                    .await
                    .is_err()
            {
                first_error.get_or_insert_with(|| {
                    format!(
                        "{} graceful shutdown acknowledgement timed out",
                        connection.id
                    )
                });
            }
            if let Some(mut task) = connection.task.take() {
                if timeout(CLOSE_TIMEOUT + JOIN_TIMEOUT, &mut task)
                    .await
                    .is_err()
                {
                    task.abort();
                    let _ = task.await;
                    first_error.get_or_insert_with(|| {
                        format!(
                            "{} supervisor join timed out and was aborted",
                            connection.id
                        )
                    });
                }
            }
            connection.instruments.clear();
            connection.desired_generation = 0;
            connection
                .previous_close
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clear();
            let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
            connection.command_tx = command_tx;
            connection.command_rx = Some(command_rx);
        }
        self.started = false;
        if let Some(error) = first_error {
            Err(DhanError::InvalidArgument(error))
        } else {
            Ok(())
        }
    }

    pub fn total_instruments(&self) -> usize {
        self.connections
            .iter()
            .map(|connection| connection.instruments.len())
            .sum()
    }

    pub fn config(&self) -> &DhanFeedConfig {
        &self.config
    }
}

impl Drop for DhanFeedManager {
    fn drop(&mut self) {
        for connection in &mut self.connections {
            if let Some(task) = connection.task.take() {
                task.abort();
            }
        }
    }
}

struct SupervisorArguments {
    id: ConnectionId,
    client_id: String,
    endpoint: String,
    auto_reconnect: bool,
    retry_base: Duration,
    enable_raw: bool,
    parsed_capacity: usize,
    raw_capacity: usize,
    parsed_tx: broadcast::Sender<MarketFeedEvent>,
    raw_tx: Option<broadcast::Sender<Bytes>>,
    lifecycle_tx: broadcast::Sender<ManagerLifecycleEvent>,
    health_tx: watch::Sender<ConnectionHealth>,
    credentials_rx: watch::Receiver<Credentials>,
    command_rx: mpsc::Receiver<SupervisorCommand>,
    previous_close: Arc<StdMutex<HashMap<(u8, u32), MarketFeedEvent>>>,
    desired: DesiredSubscriptions,
    generation: u64,
}

struct Reporter {
    snapshot: ConnectionHealth,
    health_tx: watch::Sender<ConnectionHealth>,
    lifecycle_tx: broadcast::Sender<ManagerLifecycleEvent>,
}

impl Reporter {
    fn publish(&self) {
        let _ = self.health_tx.send(self.snapshot.clone());
    }

    fn state(&mut self, state: ConnectionLifecycle) {
        if self.snapshot.lifecycle != state {
            self.snapshot.lifecycle = state;
            self.snapshot.last_state_change_at = SystemTime::now();
            let _ = self.lifecycle_tx.send(ManagerLifecycleEvent::StateChanged {
                id: self.snapshot.id,
                state,
            });
        }
        self.publish();
    }

    fn error(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.snapshot.last_error = Some(error.clone());
        let _ = self.lifecycle_tx.send(ManagerLifecycleEvent::Error {
            id: self.snapshot.id,
            error,
        });
        self.publish();
    }

    fn readiness_pending(&mut self) {
        if self.snapshot.data_quality != MarketDataQuality::GapDetected {
            self.snapshot.data_quality = if self.snapshot.instrument_count == 0 {
                MarketDataQuality::Unavailable
            } else {
                MarketDataQuality::ReadinessPending
            };
        }
        self.publish();
    }

    fn valid_data(&mut self) {
        self.snapshot.data_live = true;
        if self.snapshot.data_quality != MarketDataQuality::GapDetected {
            self.snapshot.data_quality = MarketDataQuality::Current;
            self.state(ConnectionLifecycle::Live);
        } else {
            self.state(ConnectionLifecycle::Degraded);
        }
    }

    fn gap(&mut self, cause: GapCause) {
        self.snapshot.data_quality = MarketDataQuality::GapDetected;
        self.snapshot.gap_cause = Some(cause.clone());
        self.snapshot.gap_detected_at = Some(SystemTime::now());
        self.snapshot.gap_count = self.snapshot.gap_count.saturating_add(1);
        let _ = self.lifecycle_tx.send(ManagerLifecycleEvent::GapDetected {
            id: self.snapshot.id,
            cause,
        });
        self.state(ConnectionLifecycle::Degraded);
    }

    fn readiness_failure(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.snapshot.last_error = Some(error.clone());
        let _ = self
            .lifecycle_tx
            .send(ManagerLifecycleEvent::ReadinessFailure {
                id: self.snapshot.id,
                error,
            });
        self.readiness_pending();
        self.state(ConnectionLifecycle::Degraded);
    }

    fn acknowledge_gap(&mut self) -> std::result::Result<(), String> {
        if self.snapshot.data_quality != MarketDataQuality::GapDetected {
            return Ok(());
        }
        if !self.snapshot.data_live {
            return Err(format!(
                "{} gap can be acknowledged only after the replacement transport is data-live",
                self.snapshot.id
            ));
        }
        self.snapshot.gap_cause = None;
        self.snapshot.gap_detected_at = None;
        self.snapshot.data_quality = if self.snapshot.data_live {
            MarketDataQuality::Current
        } else if self.snapshot.instrument_count > 0 {
            MarketDataQuality::ReadinessPending
        } else {
            MarketDataQuality::Unavailable
        };
        let _ = self
            .lifecycle_tx
            .send(ManagerLifecycleEvent::GapAcknowledged {
                id: self.snapshot.id,
            });
        let state = if self.snapshot.data_live {
            ConnectionLifecycle::Live
        } else if self.snapshot.instrument_count > 0 {
            self.snapshot.lifecycle
        } else {
            ConnectionLifecycle::Idle
        };
        self.state(state);
        Ok(())
    }
}

enum ConnectedOutcome {
    Retry,
    WaitForCredential(u64),
    Blocked,
    Idle,
    Shutdown,
}

async fn supervisor(mut args: SupervisorArguments) {
    let mut reporter = Reporter {
        snapshot: ConnectionHealth::new(args.id),
        health_tx: args.health_tx.clone(),
        lifecycle_tx: args.lifecycle_tx.clone(),
    };
    reporter.snapshot.is_alive = true;
    reporter.snapshot.instrument_count = args.desired.len();
    reporter.snapshot.desired_generation = args.generation;
    reporter.readiness_pending();
    reporter.state(ConnectionLifecycle::Idle);

    let mut retry_attempt = 0_u64;
    let mut jitter_state = 0x9E37_79B9_7F4A_7C15_u64 ^ u64::from(args.id.0);
    let mut connected_once = false;
    let mut ever_live = false;
    let mut shutdown_error = None;

    'owner: loop {
        if args.desired.is_empty() {
            reporter.snapshot.transport_connected = false;
            reporter.snapshot.data_live = false;
            reporter.snapshot.retry_at = None;
            reporter.readiness_pending();
            reporter.state(ConnectionLifecycle::Idle);
            match wait_for_change(&mut args, &mut reporter, WaitCondition::Forever).await {
                WaitOutcome::Changed => continue,
                WaitOutcome::Shutdown => break 'owner,
            }
        }

        let connection_credential = args.credentials_rx.borrow().clone();
        let connection_credential_version = connection_credential.version;
        reporter.snapshot.credential_version = connection_credential_version;
        reporter.snapshot.retry_count = retry_attempt;
        reporter.readiness_pending();
        reporter.state(ConnectionLifecycle::Connecting);
        let url = match connection_url(
            &args.endpoint,
            &args.client_id,
            &connection_credential.token,
        ) {
            Ok(url) => url,
            Err(error) => {
                reporter.error(format!("invalid market-feed endpoint: {error}"));
                reporter.state(ConnectionLifecycle::Blocked);
                if matches!(
                    wait_for_change(&mut args, &mut reporter, WaitCondition::Forever).await,
                    WaitOutcome::Shutdown
                ) {
                    break;
                }
                continue;
            }
        };
        // The URL now owns the encoded query value; do not retain an extra
        // access-token String for the entire lifetime of the live transport.
        drop(connection_credential);

        let connection = connect_with_commands(&url, &mut args, &mut reporter).await;
        let mut socket = match connection {
            ConnectOutcome::Connected(socket) => socket,
            ConnectOutcome::Changed => continue,
            ConnectOutcome::Shutdown => break,
            ConnectOutcome::Failed(error) => {
                reporter.error(error.clone());
                if !ever_live {
                    reporter.readiness_failure(error);
                }
                retry_attempt = retry_attempt.saturating_add(1);
                if !args.auto_reconnect {
                    reporter.state(ConnectionLifecycle::Blocked);
                    if matches!(
                        wait_for_change(&mut args, &mut reporter, WaitCondition::Forever).await,
                        WaitOutcome::Shutdown
                    ) {
                        break;
                    }
                    continue;
                }
                let delay = jitter_delay(args.retry_base, retry_attempt, &mut jitter_state);
                schedule_retry(&mut reporter, retry_attempt, delay);
                match wait_for_change(&mut args, &mut reporter, WaitCondition::Delay(delay)).await {
                    WaitOutcome::Changed => continue,
                    WaitOutcome::Shutdown => break,
                }
            }
        };

        if connected_once {
            reporter.snapshot.reconnect_count = reporter.snapshot.reconnect_count.saturating_add(1);
        }
        connected_once = true;
        reporter.snapshot.transport_connected = true;
        reporter.snapshot.data_live = false;
        reporter.snapshot.applied_generation = 0;
        reporter.snapshot.retry_at = None;
        reporter.readiness_pending();
        let _ = args.lifecycle_tx.send(ManagerLifecycleEvent::Connected {
            id: args.id,
            credential_version: connection_credential_version,
        });
        reporter.publish();

        match run_connected(
            &mut socket,
            &mut args,
            &mut reporter,
            &mut retry_attempt,
            &mut ever_live,
            connection_credential_version,
        )
        .await
        {
            ConnectedOutcome::Shutdown => {
                if let Err(error) = graceful_disconnect(&mut socket).await {
                    reporter.error(error.clone());
                    shutdown_error = Some(error);
                }
                break 'owner;
            }
            ConnectedOutcome::Idle => {
                if let Err(error) = graceful_disconnect(&mut socket).await {
                    reporter.error(error);
                }
                retry_attempt = 0;
                ever_live = false;
                reporter.readiness_pending();
                continue;
            }
            ConnectedOutcome::WaitForCredential(version) => {
                reporter.snapshot.transport_connected = false;
                reporter.snapshot.data_live = false;
                reporter.readiness_pending();
                reporter.state(ConnectionLifecycle::Blocked);
                if matches!(
                    wait_for_change(
                        &mut args,
                        &mut reporter,
                        WaitCondition::CredentialAfter(version),
                    )
                    .await,
                    WaitOutcome::Shutdown
                ) {
                    break;
                }
                retry_attempt = 0;
                continue;
            }
            ConnectedOutcome::Blocked => {
                reporter.snapshot.transport_connected = false;
                reporter.snapshot.data_live = false;
                reporter.readiness_pending();
                reporter.state(ConnectionLifecycle::Blocked);
                if matches!(
                    wait_for_change(&mut args, &mut reporter, WaitCondition::Forever).await,
                    WaitOutcome::Shutdown
                ) {
                    break;
                }
                continue;
            }
            ConnectedOutcome::Retry => {
                reporter.snapshot.transport_connected = false;
                reporter.snapshot.data_live = false;
                reporter.snapshot.applied_generation = 0;
                reporter.readiness_pending();
                retry_attempt = retry_attempt.saturating_add(1);
                if !args.auto_reconnect {
                    reporter.state(ConnectionLifecycle::Blocked);
                    if matches!(
                        wait_for_change(&mut args, &mut reporter, WaitCondition::Forever).await,
                        WaitOutcome::Shutdown
                    ) {
                        break;
                    }
                    continue;
                }
                let delay = jitter_delay(args.retry_base, retry_attempt, &mut jitter_state);
                schedule_retry(&mut reporter, retry_attempt, delay);
                if matches!(
                    wait_for_change(&mut args, &mut reporter, WaitCondition::Delay(delay)).await,
                    WaitOutcome::Shutdown
                ) {
                    break;
                }
            }
        }
    }

    reporter.snapshot.is_alive = false;
    reporter.snapshot.transport_connected = false;
    reporter.snapshot.data_live = false;
    reporter.readiness_pending();
    reporter.state(ConnectionLifecycle::Stopped);
    let _ = args.lifecycle_tx.send(ManagerLifecycleEvent::Stopped {
        id: args.id,
        error: shutdown_error,
    });
}

enum WaitOutcome {
    Changed,
    Shutdown,
}

enum WaitCondition {
    Forever,
    Delay(Duration),
    CredentialAfter(u64),
}

async fn wait_for_change(
    args: &mut SupervisorArguments,
    reporter: &mut Reporter,
    condition: WaitCondition,
) -> WaitOutcome {
    match condition {
        WaitCondition::CredentialAfter(version) => loop {
            // A replacement may have been published (and observed by the
            // connected loop) before Dhan's authentication-disconnect packet
            // arrived. Do not wait for yet another watch notification when the
            // credential needed for the next attempt is already available.
            if args.credentials_rx.borrow().version > version {
                return WaitOutcome::Changed;
            }
            tokio::select! {
                command = args.command_rx.recv() => {
                    match handle_command(command, args, reporter) {
                        CommandResult::Changed => return WaitOutcome::Changed,
                        CommandResult::Shutdown => return WaitOutcome::Shutdown,
                    }
                }
                changed = args.credentials_rx.changed() => {
                    if changed.is_err() || args.credentials_rx.borrow().version > version {
                        return WaitOutcome::Changed;
                    }
                }
            }
        },
        WaitCondition::Delay(delay) => {
            tokio::select! {
                _ = tokio::time::sleep(delay) => WaitOutcome::Changed,
                command = args.command_rx.recv() => match handle_command(command, args, reporter) {
                    CommandResult::Changed => WaitOutcome::Changed,
                    CommandResult::Shutdown => WaitOutcome::Shutdown,
                },
                _ = args.credentials_rx.changed() => WaitOutcome::Changed,
            }
        }
        WaitCondition::Forever => {
            tokio::select! {
                command = args.command_rx.recv() => match handle_command(command, args, reporter) {
                    CommandResult::Changed => WaitOutcome::Changed,
                    CommandResult::Shutdown => WaitOutcome::Shutdown,
                },
                _ = args.credentials_rx.changed() => WaitOutcome::Changed,
            }
        }
    }
}

enum CommandResult {
    Changed,
    Shutdown,
}

fn handle_command(
    command: Option<SupervisorCommand>,
    args: &mut SupervisorArguments,
    reporter: &mut Reporter,
) -> CommandResult {
    match command {
        Some(SupervisorCommand::ReplaceDesired {
            generation,
            desired,
            acknowledged,
        }) => {
            args.generation = generation;
            args.desired = desired;
            reporter.snapshot.instrument_count = args.desired.len();
            reporter.snapshot.desired_generation = generation;
            reporter.publish();
            let _ = acknowledged.send(());
            CommandResult::Changed
        }
        Some(SupervisorCommand::AcknowledgeGap { acknowledged }) => {
            let result = reporter.acknowledge_gap();
            let _ = acknowledged.send(result);
            CommandResult::Changed
        }
        Some(SupervisorCommand::Shutdown { acknowledged }) => {
            let _ = acknowledged.send(());
            CommandResult::Shutdown
        }
        None => CommandResult::Shutdown,
    }
}

enum ConnectOutcome {
    Connected(Box<WsStream>),
    Changed,
    Shutdown,
    Failed(String),
}

async fn connect_with_commands(
    url: &Url,
    args: &mut SupervisorArguments,
    reporter: &mut Reporter,
) -> ConnectOutcome {
    tokio::select! {
        result = timeout(CONNECT_TIMEOUT, connect_async(url.as_str())) => {
            match result {
                Ok(Ok((socket, _))) => ConnectOutcome::Connected(Box::new(socket)),
                Ok(Err(error)) => ConnectOutcome::Failed(format!("WebSocket connect failed: {error}")),
                Err(_) => ConnectOutcome::Failed("WebSocket connect timed out".into()),
            }
        }
        command = args.command_rx.recv() => match handle_command(command, args, reporter) {
            CommandResult::Changed => ConnectOutcome::Changed,
            CommandResult::Shutdown => ConnectOutcome::Shutdown,
        },
        _ = args.credentials_rx.changed() => ConnectOutcome::Changed,
    }
}

fn connection_url(endpoint: &str, client_id: &str, token: &str) -> Result<Url> {
    let mut url = Url::parse(endpoint)?;
    url.query_pairs_mut()
        .append_pair("version", "2")
        .append_pair("token", token)
        .append_pair("clientId", client_id)
        .append_pair("authType", "2");
    Ok(url)
}

async fn run_connected(
    socket: &mut WsStream,
    args: &mut SupervisorArguments,
    reporter: &mut Reporter,
    retry_attempt: &mut u64,
    ever_live: &mut bool,
    connection_credential_version: u64,
) -> ConnectedOutcome {
    let mut applied = DesiredSubscriptions::new();
    let connected_at = Instant::now();

    loop {
        reporter.state(ConnectionLifecycle::Resubscribing);
        if let Err(error) = reconcile(socket, &applied, &args.desired).await {
            let error = format!("subscription reconciliation failed: {error}");
            reporter.error(error.clone());
            record_transport_loss(reporter, *ever_live, None, error);
            return ConnectedOutcome::Retry;
        }
        applied = args.desired.clone();
        reporter.snapshot.applied_generation = args.generation;
        reporter.readiness_pending();
        reporter.state(ConnectionLifecycle::ReadinessPending);

        let inactivity = tokio::time::sleep(NO_FRAME_TIMEOUT);
        tokio::pin!(inactivity);
        let readiness = tokio::time::sleep(NO_FRAME_TIMEOUT);
        tokio::pin!(readiness);
        loop {
            tokio::select! {
                command = args.command_rx.recv() => {
                    match command {
                        Some(SupervisorCommand::ReplaceDesired { generation, desired, acknowledged }) => {
                            args.generation = generation;
                            args.desired = desired;
                            reporter.snapshot.instrument_count = args.desired.len();
                            reporter.snapshot.desired_generation = generation;
                            reporter.publish();
                            let _ = acknowledged.send(());
                            if args.desired.is_empty() {
                                return ConnectedOutcome::Idle;
                            }
                            break;
                        }
                        Some(SupervisorCommand::AcknowledgeGap { acknowledged }) => {
                            let result = reporter.acknowledge_gap();
                            let _ = acknowledged.send(result);
                        }
                        Some(SupervisorCommand::Shutdown { acknowledged }) => {
                            // The manager waits for this acknowledgement only as a
                            // signal that cancellation reached the owner. The task
                            // join proves graceful close completion.
                            let _ = acknowledged.send(());
                            reporter.state(ConnectionLifecycle::Closing);
                            return ConnectedOutcome::Shutdown;
                        }
                        None => return ConnectedOutcome::Shutdown,
                    }
                }
                changed = args.credentials_rx.changed() => {
                    if changed.is_err() {
                        return ConnectedOutcome::Shutdown;
                    }
                    reporter.snapshot.credential_version = args.credentials_rx.borrow().version;
                    reporter.publish();
                }
                _ = &mut inactivity => {
                    let error = "market-feed inactivity deadline exceeded".to_owned();
                    reporter.error(error.clone());
                    record_transport_loss(reporter, *ever_live, None, error);
                    return ConnectedOutcome::Retry;
                }
                _ = &mut readiness, if !reporter.snapshot.data_live => {
                    let error = "market-feed readiness deadline exceeded without valid data".to_owned();
                    reporter.error(error.clone());
                    record_transport_loss(reporter, *ever_live, None, error);
                    return ConnectedOutcome::Retry;
                }
                message = socket.next() => {
                    let now = SystemTime::now();
                    reporter.snapshot.last_frame_at = Some(now);
                    inactivity.as_mut().reset(Instant::now() + NO_FRAME_TIMEOUT);
                    match message {
                        Some(Ok(Message::Binary(data))) => {
                            reporter.snapshot.last_data_at = Some(now);
                            if connected_at.elapsed() >= STABLE_RETRY_RESET {
                                *retry_attempt = 0;
                                reporter.snapshot.retry_count = 0;
                            }
                            if args.enable_raw {
                                if let Some(sender) = &args.raw_tx {
                                    detect_lag(
                                        sender,
                                        args.raw_capacity,
                                        args.id,
                                        &args.lifecycle_tx,
                                        reporter,
                                    );
                                    let _ = sender.send(Bytes::copy_from_slice(&data));
                                }
                            }
                            match parse_packets(&data) {
                                Ok(events) => {
                                    let has_receiver = args.parsed_tx.receiver_count() > 0;
                                    for event in events {
                                        if let MarketFeedEvent::PrevClose { header, .. } = &event {
                                            args.previous_close
                                                .lock()
                                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                                .insert((header.exchange_segment_raw, header.security_id), event.clone());
                                            if !has_receiver {
                                                let _ = args.lifecycle_tx.send(ManagerLifecycleEvent::NoReceiver {
                                                    id: args.id,
                                                    event: "previous-close-cached",
                                                });
                                            }
                                        } else if !has_receiver {
                                            let event_name = market_event_name(&event);
                                            let _ = args.lifecycle_tx.send(ManagerLifecycleEvent::NoReceiver {
                                                id: args.id,
                                                event: event_name,
                                            });
                                            reporter.gap(GapCause::NoReceiver { event: event_name });
                                        }
                                        let dhan_disconnect = match &event {
                                            MarketFeedEvent::Disconnect { reason_code, .. } => Some(*reason_code),
                                            _ => None,
                                        };
                                        detect_lag(
                                            &args.parsed_tx,
                                            args.parsed_capacity,
                                            args.id,
                                            &args.lifecycle_tx,
                                            reporter,
                                        );
                                        let _ = args.parsed_tx.send(event);
                                        if let Some(reason_code) = dhan_disconnect {
                                            let _ = args.lifecycle_tx.send(ManagerLifecycleEvent::DhanDisconnected {
                                                id: args.id,
                                                reason_code,
                                            });
                                            let error = format!("Dhan disconnected feed with reason {reason_code}");
                                            reporter.error(error.clone());
                                            record_transport_loss(reporter, *ever_live, None, error);
                                            return match reason_code {
                                                807..=809 => ConnectedOutcome::WaitForCredential(connection_credential_version),
                                                804 | 806 | 810..=814 => ConnectedOutcome::Blocked,
                                                _ => ConnectedOutcome::Retry,
                                            };
                                        }
                                        *ever_live = true;
                                        reporter.valid_data();
                                    }
                                }
                                Err(error) => {
                                    let error = error.to_string();
                                    reporter.snapshot.parser_error_count = reporter.snapshot.parser_error_count.saturating_add(1);
                                    reporter.snapshot.last_error = Some(error.clone());
                                    let _ = args.lifecycle_tx.send(ManagerLifecycleEvent::ParseError {
                                        id: args.id,
                                        error: error.clone(),
                                    });
                                    if *ever_live {
                                        reporter.gap(GapCause::ParseError { error });
                                    } else {
                                        reporter.readiness_failure(error);
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            let (close_code, reason) = frame
                                .map(|frame| (Some(u16::from(frame.code)), frame.reason.to_string()))
                                .unwrap_or((None, String::new()));
                            let _ = args.lifecycle_tx.send(ManagerLifecycleEvent::Disconnected {
                                id: args.id,
                                close_code,
                                reason: reason.clone(),
                            });
                            record_transport_loss(
                                reporter,
                                *ever_live,
                                close_code,
                                if reason.is_empty() {
                                    "WebSocket peer closed the connection".to_owned()
                                } else {
                                    reason
                                },
                            );
                            return ConnectedOutcome::Retry;
                        }
                        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                        Some(Ok(Message::Text(_))) | Some(Ok(Message::Frame(_))) => {}
                        Some(Err(error)) => {
                            let error = format!("WebSocket read failed: {error}");
                            reporter.error(error.clone());
                            record_transport_loss(reporter, *ever_live, None, error);
                            return ConnectedOutcome::Retry;
                        }
                        None => {
                            let error = "WebSocket stream ended".to_owned();
                            reporter.error(error.clone());
                            record_transport_loss(reporter, *ever_live, None, error);
                            return ConnectedOutcome::Retry;
                        }
                    }
                    reporter.publish();
                }
            }
        }
    }
}

fn market_event_name(event: &MarketFeedEvent) -> &'static str {
    match event {
        MarketFeedEvent::Ticker { .. } => "ticker",
        MarketFeedEvent::PrevClose { .. } => "previous-close",
        MarketFeedEvent::Quote { .. } => "quote",
        MarketFeedEvent::OI { .. } => "open-interest",
        MarketFeedEvent::Full { .. } => "full",
        MarketFeedEvent::MarketStatus { .. } => "market-status",
        MarketFeedEvent::Index { .. } => "index",
        MarketFeedEvent::Disconnect { .. } => "disconnect",
    }
}

fn detect_lag<T: Clone>(
    sender: &broadcast::Sender<T>,
    capacity: usize,
    id: ConnectionId,
    lifecycle_tx: &broadcast::Sender<ManagerLifecycleEvent>,
    reporter: &mut Reporter,
) {
    if sender.receiver_count() > 0 && sender.len() >= capacity {
        reporter.snapshot.lagged_event_count =
            reporter.snapshot.lagged_event_count.saturating_add(1);
        let _ = lifecycle_tx.send(ManagerLifecycleEvent::ReceiverLag { id, dropped: 1 });
        reporter.gap(GapCause::ReceiverLag { dropped: 1 });
    }
}

fn record_transport_loss(
    reporter: &mut Reporter,
    ever_live: bool,
    close_code: Option<u16>,
    reason: String,
) {
    if ever_live {
        reporter.gap(GapCause::Disconnect { close_code, reason });
    } else {
        reporter.readiness_failure(reason);
    }
}

fn desired_equal(left: &DesiredSubscriptions, right: &DesiredSubscriptions) -> bool {
    left.len() == right.len()
        && left.iter().all(|(key, (left_instrument, left_mode))| {
            right
                .get(key)
                .is_some_and(|(right_instrument, right_mode)| {
                    left_mode == right_mode
                        && left_instrument.ExchangeSegment == right_instrument.ExchangeSegment
                        && left_instrument.SecurityId == right_instrument.SecurityId
                })
        })
}

async fn reconcile(
    socket: &mut WsStream,
    applied: &DesiredSubscriptions,
    desired: &DesiredSubscriptions,
) -> std::result::Result<(), String> {
    let mut unsubscribe: HashMap<u8, Vec<Instrument>> = HashMap::new();
    let mut subscribe: HashMap<u8, Vec<Instrument>> = HashMap::new();
    let keys: HashSet<_> = applied.keys().chain(desired.keys()).cloned().collect();

    for key in keys {
        match (applied.get(&key), desired.get(&key)) {
            (Some((old_instrument, old_mode)), Some((new_instrument, new_mode)))
                if old_mode != new_mode =>
            {
                unsubscribe
                    .entry(unsubscribe_code(*old_mode))
                    .or_default()
                    .push(old_instrument.clone());
                subscribe
                    .entry(*new_mode as u8)
                    .or_default()
                    .push(new_instrument.clone());
            }
            (Some((old_instrument, old_mode)), None) => {
                unsubscribe
                    .entry(unsubscribe_code(*old_mode))
                    .or_default()
                    .push(old_instrument.clone());
            }
            (None, Some((instrument, mode))) => {
                subscribe
                    .entry(*mode as u8)
                    .or_default()
                    .push(instrument.clone());
            }
            _ => {}
        }
    }

    for (request_code, instruments) in unsubscribe.into_iter().chain(subscribe) {
        for chunk in instruments.chunks(DHAN_CONTROL_FRAME_LIMIT) {
            let request = FeedSubscribeRequest {
                RequestCode: request_code,
                InstrumentCount: chunk.len(),
                InstrumentList: chunk.to_vec(),
            };
            let json = serde_json::to_string(&request).map_err(|error| error.to_string())?;
            write_message(socket, Message::Text(json.into())).await?;
        }
    }
    timeout(WRITE_TIMEOUT, socket.flush())
        .await
        .map_err(|_| "WebSocket flush timed out".to_owned())?
        .map_err(|error| format!("WebSocket flush failed: {error}"))
}

async fn write_message(socket: &mut WsStream, message: Message) -> std::result::Result<(), String> {
    timeout(WRITE_TIMEOUT, socket.send(message))
        .await
        .map_err(|_| "WebSocket write timed out".to_owned())?
        .map_err(|error| format!("WebSocket write failed: {error}"))
}

async fn graceful_disconnect(socket: &mut WsStream) -> std::result::Result<(), String> {
    let request = serde_json::to_string(&FeedDisconnectRequest { RequestCode: 12 })
        .map_err(|error| error.to_string())?;
    write_message(socket, Message::Text(request.into())).await?;
    timeout(WRITE_TIMEOUT, socket.flush())
        .await
        .map_err(|_| "disconnect flush timed out".to_owned())?
        .map_err(|error| format!("disconnect flush failed: {error}"))?;
    write_message(socket, Message::Close(None)).await?;

    let close = async {
        while let Some(message) = socket.next().await {
            match message {
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    };
    timeout(CLOSE_TIMEOUT, close)
        .await
        .map_err(|_| "WebSocket close handshake timed out".to_owned())?;
    Ok(())
}

fn schedule_retry(reporter: &mut Reporter, attempt: u64, delay: Duration) {
    reporter.snapshot.retry_count = attempt;
    reporter.snapshot.retry_at = SystemTime::now().checked_add(delay);
    reporter.state(ConnectionLifecycle::Backoff);
    let _ = reporter
        .lifecycle_tx
        .send(ManagerLifecycleEvent::RetryScheduled {
            id: reporter.snapshot.id,
            attempt,
            delay,
        });
}

fn jitter_delay(base: Duration, attempt: u64, state: &mut u64) -> Duration {
    let exponent = attempt.saturating_sub(1).min(31) as u32;
    let cap_millis = base
        .as_millis()
        .saturating_mul(1_u128 << exponent)
        .min(MAX_RETRY_BASE.as_millis()) as u64;
    if cap_millis == 0 {
        return Duration::ZERO;
    }
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    Duration::from_millis(*state % (cap_millis + 1))
}

fn validate_subscribe_mode(mode: FeedRequestCode) -> Result<()> {
    match mode {
        FeedRequestCode::SubscribeTicker
        | FeedRequestCode::SubscribeQuote
        | FeedRequestCode::SubscribeFull => Ok(()),
        _ => Err(DhanError::InvalidArgument(
            "standard market feed accepts only ticker, quote, or full subscription modes".into(),
        )),
    }
}

fn validate_unsubscribe_mode(mode: FeedRequestCode) -> Result<()> {
    match mode {
        FeedRequestCode::UnsubscribeTicker
        | FeedRequestCode::UnsubscribeQuote
        | FeedRequestCode::UnsubscribeFull => Ok(()),
        _ => Err(DhanError::InvalidArgument(
            "unsubscribe requires a standard ticker, quote, or full unsubscribe code".into(),
        )),
    }
}

fn validate_instruments(instruments: &[Instrument]) -> Result<()> {
    let mut unique = HashSet::with_capacity(instruments.len());
    for (index, instrument) in instruments.iter().enumerate() {
        if !matches!(
            instrument.ExchangeSegment.as_str(),
            "IDX_I"
                | "NSE_EQ"
                | "NSE_FNO"
                | "NSE_CURRENCY"
                | "BSE_EQ"
                | "MCX_COMM"
                | "BSE_CURRENCY"
                | "BSE_FNO"
        ) {
            return Err(DhanError::InvalidArgument(format!(
                "instrument {index} has an unsupported standard-feed exchange segment"
            )));
        }
        if instrument.SecurityId.trim().is_empty() {
            return Err(DhanError::InvalidArgument(format!(
                "instrument {index} requires a nonblank security ID"
            )));
        }
        if instrument.SecurityId.parse::<u32>().is_err() {
            return Err(DhanError::InvalidArgument(format!(
                "instrument {index} security ID must be an unsigned 32-bit integer"
            )));
        }
        // Exact duplicates are intentionally idempotent. The desired map has
        // one mode per key, so a batch can never create competing modes.
        unique.insert(InstrumentKey::from(instrument));
    }
    Ok(())
}

fn unsubscribe_code(mode: FeedRequestCode) -> u8 {
    match mode {
        FeedRequestCode::SubscribeTicker => FeedRequestCode::UnsubscribeTicker as u8,
        FeedRequestCode::SubscribeQuote => FeedRequestCode::UnsubscribeQuote as u8,
        FeedRequestCode::SubscribeFull => FeedRequestCode::UnsubscribeFull as u8,
        _ => unreachable!("desired state contains only validated subscription modes"),
    }
}
