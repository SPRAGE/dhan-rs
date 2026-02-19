#![allow(missing_docs)]
//! Multi-connection market feed manager for DhanHQ WebSocket.
//!
//! Manages up to 5 concurrent WebSocket connections, each handling up to 5,000
//! instruments, for a total capacity of **25,000 instruments**. Provides
//! automatic load-balancing, health monitoring, auto-reconnect, and both
//! parsed and raw-frame delivery channels.
//!
//! # Architecture
//!
//! ```text
//!            ┌─────────────────────────────────┐
//!            │        DhanFeedManager           │
//!            │  (builder, subscription routing) │
//!            └──┬────────┬────────┬─────────┬───┘
//!               │        │        │         │
//!          Connection 0  ...  Connection N  │
//!          (ws task)          (ws task)      │
//!               │        │        │         │
//!          broadcast::Sender  ──────────────┘
//!               │        │        │
//!          Receivers (user spawns per-channel consumers)
//! ```
//!
//! Each connection runs in its own Tokio task. Binary frames are either:
//! - **Parsed** into [`MarketFeedEvent`] and sent on a `broadcast` channel, or
//! - **Raw** `bytes::Bytes` forwarded on a separate `broadcast` channel for
//!   zero-copy / low-latency consumers.
//!
//! # Quick Start
//!
//! ```no_run
//! use dhan_rs::ws::manager::{DhanFeedManagerBuilder, DhanFeedManager};
//! use dhan_rs::ws::market_feed::Instrument;
//! use dhan_rs::types::enums::FeedRequestCode;
//!
//! # #[tokio::main]
//! # async fn main() -> dhan_rs::error::Result<()> {
//! let mut manager = DhanFeedManagerBuilder::new("client-id", "access-token")
//!     .max_connections(3)
//!     .max_instruments_per_connection(5000)
//!     .enable_raw_frames(true)
//!     .reconnect_delay_ms(2000)
//!     .build();
//!
//! manager.start().await?;
//!
//! // Subscribe — instruments are distributed automatically
//! let instruments = vec![
//!     Instrument::new("NSE_EQ", "1333"),
//!     Instrument::new("NSE_EQ", "11536"),
//! ];
//! manager
//!     .subscribe(&instruments, FeedRequestCode::SubscribeTicker)
//!     .await?;
//!
//! // Consume parsed events from a specific connection
//! if let Some(mut rx) = manager.get_parsed_channel(ConnectionId(0)) {
//!     tokio::spawn(async move {
//!         while let Ok(event) = rx.recv().await {
//!             println!("{event:?}");
//!         }
//!     });
//! }
//!
//! // Or consume raw binary frames for advanced zero-copy processing
//! if let Some(mut rx) = manager.get_raw_channel(ConnectionId(0)) {
//!     tokio::spawn(async move {
//!         while let Ok(frame) = rx.recv().await {
//!             println!("raw frame: {} bytes", frame.len());
//!         }
//!     });
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Limits
//!
//! - Maximum **5** WebSocket connections per user
//! - Up to **5,000** instruments per connection
//! - Up to **100** instruments per subscribe/unsubscribe message
//! - Total capacity: **25,000** instruments

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::constants::WS_MARKET_FEED_URL;
use crate::error::{DhanError, Result};
use crate::types::enums::FeedRequestCode;
use crate::ws::market_feed::{Instrument, MarketFeedEvent, parse_packet};

// ---------------------------------------------------------------------------
// Connection ID
// ---------------------------------------------------------------------------

/// Identifies one of the managed WebSocket connections (0–4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u8);

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Connection({})", self.0)
    }
}

// ---------------------------------------------------------------------------
// Connection health
// ---------------------------------------------------------------------------

/// Health status of a single managed connection.
#[derive(Debug, Clone)]
pub struct ConnectionHealth {
    /// Whether the connection's background task is alive.
    pub is_alive: bool,
    /// The connection's identifier.
    pub id: ConnectionId,
    /// Number of instruments currently subscribed on this connection.
    pub instrument_count: usize,
    /// Number of reconnections that have occurred.
    pub reconnect_count: u64,
}

/// Aggregate health summary across all managed connections.
#[derive(Debug, Clone)]
pub struct HealthSummary {
    /// Per-connection health snapshots.
    pub connections: Vec<ConnectionHealth>,
    /// Total instruments subscribed across all connections.
    pub total_instruments: usize,
    /// Number of connections that are alive.
    pub alive_connections: usize,
}

// ---------------------------------------------------------------------------
// Internal subscribe request (reuses market_feed structure)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct FeedSubscribeRequest {
    RequestCode: u8,
    InstrumentCount: usize,
    InstrumentList: Vec<Instrument>,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the [`DhanFeedManager`].
#[derive(Debug, Clone)]
pub struct DhanFeedConfig {
    /// Maximum number of concurrent WebSocket connections (1–5).
    pub max_connections: u8,
    /// Maximum instruments per connection (up to 5,000).
    pub max_instruments_per_connection: usize,
    /// Whether raw binary frames should also be broadcast.
    pub enable_raw_frames: bool,
    /// Delay before attempting reconnection (milliseconds).
    pub reconnect_delay_ms: u64,
    /// Broadcast channel capacity for parsed events per connection.
    pub parsed_channel_capacity: usize,
    /// Broadcast channel capacity for raw frames per connection.
    pub raw_channel_capacity: usize,
    /// Whether to automatically reconnect on disconnect.
    pub auto_reconnect: bool,
}

impl Default for DhanFeedConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
            max_instruments_per_connection: 5_000,
            enable_raw_frames: false,
            reconnect_delay_ms: 2_000,
            parsed_channel_capacity: 4096,
            raw_channel_capacity: 4096,
            auto_reconnect: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for constructing a [`DhanFeedManager`] with custom configuration.
///
/// # Example
///
/// ```no_run
/// use dhan_rs::ws::manager::DhanFeedManagerBuilder;
///
/// let manager = DhanFeedManagerBuilder::new("client_id", "access_token")
///     .max_connections(3)
///     .enable_raw_frames(true)
///     .build();
/// ```
pub struct DhanFeedManagerBuilder {
    client_id: String,
    access_token: String,
    config: DhanFeedConfig,
}

impl DhanFeedManagerBuilder {
    /// Create a new builder with the given credentials.
    pub fn new(
        client_id: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            access_token: access_token.into(),
            config: DhanFeedConfig::default(),
        }
    }

    /// Set maximum number of connections (1–5). Default: 5.
    pub fn max_connections(mut self, n: u8) -> Self {
        self.config.max_connections = n.min(5).max(1);
        self
    }

    /// Set maximum instruments per connection (up to 5,000). Default: 5,000.
    pub fn max_instruments_per_connection(mut self, n: usize) -> Self {
        self.config.max_instruments_per_connection = n.min(5_000);
        self
    }

    /// Enable or disable raw binary frame broadcasting. Default: false.
    pub fn enable_raw_frames(mut self, enable: bool) -> Self {
        self.config.enable_raw_frames = enable;
        self
    }

    /// Set the reconnect delay in milliseconds. Default: 2,000.
    pub fn reconnect_delay_ms(mut self, ms: u64) -> Self {
        self.config.reconnect_delay_ms = ms;
        self
    }

    /// Set the broadcast channel capacity for parsed events. Default: 4,096.
    pub fn parsed_channel_capacity(mut self, cap: usize) -> Self {
        self.config.parsed_channel_capacity = cap;
        self
    }

    /// Set the broadcast channel capacity for raw frames. Default: 4,096.
    pub fn raw_channel_capacity(mut self, cap: usize) -> Self {
        self.config.raw_channel_capacity = cap;
        self
    }

    /// Enable or disable auto-reconnect on disconnect. Default: true.
    pub fn auto_reconnect(mut self, enable: bool) -> Self {
        self.config.auto_reconnect = enable;
        self
    }

    /// Build the [`DhanFeedManager`].
    pub fn build(self) -> DhanFeedManager {
        DhanFeedManager::new(self.client_id, self.access_token, self.config)
    }
}

// ---------------------------------------------------------------------------
// Instrument key for tracking subscriptions
// ---------------------------------------------------------------------------

/// A unique key for an instrument (exchange_segment + security_id).
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

// ---------------------------------------------------------------------------
// Per-connection state
// ---------------------------------------------------------------------------

struct ManagedConnection {
    id: ConnectionId,
    /// Channel sender for parsed events.
    parsed_tx: broadcast::Sender<MarketFeedEvent>,
    /// Channel sender for raw frames (if enabled).
    raw_tx: Option<broadcast::Sender<Bytes>>,
    /// The background task handle.
    task: Option<JoinHandle<()>>,
    /// The write half of the WebSocket, shared with the background task.
    writer: Arc<Mutex<Option<WriterHalf>>>,
    /// Instruments subscribed on this connection.
    instruments: HashMap<InstrumentKey, (Instrument, FeedRequestCode)>,
    /// Reconnect count.
    reconnect_count: u64,
}

type WriterHalf = futures_util::stream::SplitSink<
    WebSocketStream<MaybeTlsStream<TcpStream>>,
    Message,
>;

// ---------------------------------------------------------------------------
// DhanFeedManager
// ---------------------------------------------------------------------------

/// Multi-connection manager for DhanHQ market feed WebSocket streams.
///
/// Manages up to 5 connections, each supporting up to 5,000 instruments,
/// with automatic load-balancing, auto-reconnect, and both parsed and
/// raw-frame delivery channels.
///
/// Use [`DhanFeedManagerBuilder`] for ergonomic construction.
///
/// # Example
///
/// ```no_run
/// use dhan_rs::ws::manager::{DhanFeedManager, DhanFeedConfig, ConnectionId};
/// use dhan_rs::ws::market_feed::Instrument;
/// use dhan_rs::types::enums::FeedRequestCode;
///
/// # #[tokio::main]
/// # async fn main() -> dhan_rs::error::Result<()> {
/// let mut manager = DhanFeedManager::new(
///     "client-id",
///     "access-token",
///     DhanFeedConfig::default(),
/// );
/// manager.start().await?;
///
/// let instruments = vec![Instrument::new("NSE_EQ", "1333")];
/// manager.subscribe(&instruments, FeedRequestCode::SubscribeTicker).await?;
///
/// // Get a broadcast receiver for parsed events
/// let mut rx = manager.get_parsed_channel(ConnectionId(0)).unwrap();
/// tokio::spawn(async move {
///     while let Ok(event) = rx.recv().await {
///         println!("{event:?}");
///     }
/// });
/// # Ok(())
/// # }
/// ```
pub struct DhanFeedManager {
    client_id: String,
    access_token: String,
    config: DhanFeedConfig,
    connections: Vec<ManagedConnection>,
    started: bool,
}

impl DhanFeedManager {
    /// Create a new manager with explicit configuration.
    ///
    /// Prefer [`DhanFeedManagerBuilder`] for a more ergonomic API.
    pub fn new(
        client_id: impl Into<String>,
        access_token: impl Into<String>,
        config: DhanFeedConfig,
    ) -> Self {
        let client_id = client_id.into();
        let access_token = access_token.into();
        let n = config.max_connections as usize;

        let connections = (0..n)
            .map(|i| {
                let (parsed_tx, _) = broadcast::channel(config.parsed_channel_capacity);
                let raw_tx = if config.enable_raw_frames {
                    let (tx, _) = broadcast::channel(config.raw_channel_capacity);
                    Some(tx)
                } else {
                    None
                };
                ManagedConnection {
                    id: ConnectionId(i as u8),
                    parsed_tx,
                    raw_tx,
                    task: None,
                    writer: Arc::new(Mutex::new(None)),
                    instruments: HashMap::new(),
                    reconnect_count: 0,
                }
            })
            .collect();

        Self {
            client_id,
            access_token,
            config,
            connections,
            started: false,
        }
    }

    /// Start all configured WebSocket connections.
    ///
    /// Each connection is run in a dedicated Tokio task that reads binary
    /// frames, parses them, and distributes events on broadcast channels.
    pub async fn start(&mut self) -> Result<()> {
        if self.started {
            return Err(DhanError::InvalidArgument(
                "manager already started".into(),
            ));
        }

        for conn in &mut self.connections {
            Self::spawn_connection(
                &self.client_id,
                &self.access_token,
                conn,
                self.config.auto_reconnect,
                self.config.reconnect_delay_ms,
                self.config.enable_raw_frames,
            )
            .await?;
        }
        self.started = true;

        tracing::info!(
            connections = self.connections.len(),
            "DhanFeedManager started"
        );
        Ok(())
    }

    /// Subscribe instruments on the manager.
    ///
    /// Instruments are distributed across connections using round-robin
    /// load-balancing, respecting the per-connection instrument limit.
    /// Subscriptions are sent in batches of 100 per the Dhan API limit.
    pub async fn subscribe(
        &mut self,
        instruments: &[Instrument],
        mode: FeedRequestCode,
    ) -> Result<()> {
        if !self.started {
            return Err(DhanError::InvalidArgument(
                "manager not started — call start() first".into(),
            ));
        }

        // Distribute instruments across connections
        let assignments = self.assign_instruments(instruments, mode)?;

        for (conn_idx, batch) in assignments {
            let conn = &mut self.connections[conn_idx];
            let writer = conn.writer.clone();

            // Send in chunks of 100
            for chunk in batch.chunks(100) {
                let req = FeedSubscribeRequest {
                    RequestCode: mode as u8,
                    InstrumentCount: chunk.len(),
                    InstrumentList: chunk.to_vec(),
                };
                let json = serde_json::to_string(&req)?;

                let mut guard = writer.lock().await;
                if let Some(ref mut w) = *guard {
                    w.send(Message::Text(json.into())).await?;
                } else {
                    return Err(DhanError::InvalidArgument(format!(
                        "{} writer not available",
                        conn.id
                    )));
                }
            }

            // Track subscriptions
            for inst in &batch {
                let key = InstrumentKey::from(inst);
                conn.instruments.insert(key, (inst.clone(), mode));
            }

            tracing::debug!(
                connection = %conn.id,
                count = batch.len(),
                mode = ?mode,
                "Subscribed instruments"
            );
        }

        Ok(())
    }

    /// Unsubscribe instruments.
    ///
    /// Finds which connection each instrument lives on and sends the
    /// appropriate unsubscribe request.
    pub async fn unsubscribe(
        &mut self,
        instruments: &[Instrument],
        mode: FeedRequestCode,
    ) -> Result<()> {
        if !self.started {
            return Err(DhanError::InvalidArgument(
                "manager not started".into(),
            ));
        }

        // Group instruments by which connection they're on
        let mut per_conn: HashMap<usize, Vec<Instrument>> = HashMap::new();
        for inst in instruments {
            let key = InstrumentKey::from(inst);
            for (idx, conn) in self.connections.iter().enumerate() {
                if conn.instruments.contains_key(&key) {
                    per_conn.entry(idx).or_default().push(inst.clone());
                    break;
                }
            }
        }

        for (conn_idx, batch) in per_conn {
            let conn = &mut self.connections[conn_idx];
            let writer = conn.writer.clone();

            for chunk in batch.chunks(100) {
                let req = FeedSubscribeRequest {
                    RequestCode: mode as u8,
                    InstrumentCount: chunk.len(),
                    InstrumentList: chunk.to_vec(),
                };
                let json = serde_json::to_string(&req)?;

                let mut guard = writer.lock().await;
                if let Some(ref mut w) = *guard {
                    w.send(Message::Text(json.into())).await?;
                }
            }

            // Remove from tracking
            for inst in &batch {
                let key = InstrumentKey::from(inst);
                conn.instruments.remove(&key);
            }

            tracing::debug!(
                connection = %conn.id,
                count = batch.len(),
                mode = ?mode,
                "Unsubscribed instruments"
            );
        }

        Ok(())
    }

    /// Get a broadcast receiver for parsed [`MarketFeedEvent`]s from a
    /// specific connection.
    ///
    /// Returns `None` if the connection ID is out of range.
    pub fn get_parsed_channel(
        &self,
        id: ConnectionId,
    ) -> Option<broadcast::Receiver<MarketFeedEvent>> {
        self.connections
            .get(id.0 as usize)
            .map(|c| c.parsed_tx.subscribe())
    }

    /// Get broadcast receivers for parsed events from **all** connections.
    ///
    /// Returns a vec of `(ConnectionId, Receiver)` tuples.
    pub fn get_all_parsed_channels(
        &self,
    ) -> Vec<(ConnectionId, broadcast::Receiver<MarketFeedEvent>)> {
        self.connections
            .iter()
            .map(|c| (c.id, c.parsed_tx.subscribe()))
            .collect()
    }

    /// Get a broadcast receiver for raw binary frames from a specific
    /// connection.
    ///
    /// Returns `None` if the connection ID is out of range or raw frames
    /// were not enabled in the config.
    pub fn get_raw_channel(
        &self,
        id: ConnectionId,
    ) -> Option<broadcast::Receiver<Bytes>> {
        self.connections
            .get(id.0 as usize)
            .and_then(|c| c.raw_tx.as_ref().map(|tx| tx.subscribe()))
    }

    /// Get raw-frame receivers from **all** connections.
    ///
    /// Returns an empty vec if raw frames are not enabled.
    pub fn get_all_raw_channels(
        &self,
    ) -> Vec<(ConnectionId, broadcast::Receiver<Bytes>)> {
        self.connections
            .iter()
            .filter_map(|c| {
                c.raw_tx
                    .as_ref()
                    .map(|tx| (c.id, tx.subscribe()))
            })
            .collect()
    }

    /// Get health information for all managed connections.
    pub fn health(&self) -> HealthSummary {
        let connections: Vec<_> = self
            .connections
            .iter()
            .map(|c| ConnectionHealth {
                id: c.id,
                is_alive: c
                    .task
                    .as_ref()
                    .is_some_and(|t| !t.is_finished()),
                instrument_count: c.instruments.len(),
                reconnect_count: c.reconnect_count,
            })
            .collect();

        let total_instruments = connections.iter().map(|c| c.instrument_count).sum();
        let alive_connections = connections.iter().filter(|c| c.is_alive).count();

        HealthSummary {
            connections,
            total_instruments,
            alive_connections,
        }
    }

    /// Shut down all managed connections gracefully.
    pub async fn shutdown(&mut self) -> Result<()> {
        for conn in &mut self.connections {
            // Send close frame
            let mut guard = conn.writer.lock().await;
            if let Some(ref mut w) = *guard {
                let _ = w.send(Message::Close(None)).await;
            }
            *guard = None;

            // Abort the background task
            if let Some(task) = conn.task.take() {
                task.abort();
            }
            conn.instruments.clear();
        }
        self.started = false;

        tracing::info!("DhanFeedManager shut down");
        Ok(())
    }

    /// Total number of instruments subscribed across all connections.
    pub fn total_instruments(&self) -> usize {
        self.connections.iter().map(|c| c.instruments.len()).sum()
    }

    /// Get the configuration.
    pub fn config(&self) -> &DhanFeedConfig {
        &self.config
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Assign instruments to connections using round-robin load balancing.
    fn assign_instruments(
        &self,
        instruments: &[Instrument],
        mode: FeedRequestCode,
    ) -> Result<Vec<(usize, Vec<Instrument>)>> {
        let mut assignments: HashMap<usize, Vec<Instrument>> = HashMap::new();
        let _n = self.connections.len();
        let max_per = self.config.max_instruments_per_connection;

        // Build a set of already-subscribed keys so we skip duplicates
        let mut all_keys: HashMap<InstrumentKey, usize> = HashMap::new();
        for (idx, conn) in self.connections.iter().enumerate() {
            for key in conn.instruments.keys() {
                all_keys.insert(key.clone(), idx);
            }
        }

        // Find the connection with fewest instruments for round-robin start
        let mut conn_loads: Vec<usize> = self
            .connections
            .iter()
            .map(|c| c.instruments.len())
            .collect();

        for inst in instruments {
            let key = InstrumentKey::from(inst);

            // Skip if already subscribed somewhere
            if all_keys.contains_key(&key) {
                continue;
            }

            // Pick the connection with the fewest instruments
            let best_idx = conn_loads
                .iter()
                .enumerate()
                .min_by_key(|&(_, load)| *load)
                .map(|(idx, _)| idx)
                .ok_or_else(|| {
                    DhanError::InvalidArgument("no connections available".into())
                })?;

            if conn_loads[best_idx] >= max_per {
                return Err(DhanError::InvalidArgument(format!(
                    "all connections at capacity ({max_per} instruments each)"
                )));
            }

            assignments.entry(best_idx).or_default().push(inst.clone());
            conn_loads[best_idx] += 1;
            all_keys.insert(key, best_idx);
        }

        let _ = mode; // mode used by caller for the subscribe message
        Ok(assignments.into_iter().collect())
    }

    /// Spawn (or re-spawn) a WebSocket connection task for the given
    /// connection slot.
    async fn spawn_connection(
        client_id: &str,
        access_token: &str,
        conn: &mut ManagedConnection,
        auto_reconnect: bool,
        reconnect_delay_ms: u64,
        enable_raw: bool,
    ) -> Result<()> {
        let url = format!(
            "{WS_MARKET_FEED_URL}?version=2&token={access_token}&clientId={client_id}&authType=2"
        );

        let (ws, _resp) = connect_async(&url).await?;
        let (write, read) = ws.split();
        *conn.writer.lock().await = Some(write);

        let parsed_tx = conn.parsed_tx.clone();
        let raw_tx = conn.raw_tx.clone();
        let conn_id = conn.id;
        let writer_arc = conn.writer.clone();

        // Collect instruments to re-subscribe on reconnect
        let existing_subs: Vec<(Instrument, FeedRequestCode)> =
            conn.instruments.values().cloned().collect();

        let client_id_owned = client_id.to_owned();
        let access_token_owned = access_token.to_owned();

        let task = tokio::spawn(async move {
            Self::connection_loop(
                conn_id,
                read,
                writer_arc,
                parsed_tx,
                raw_tx,
                auto_reconnect,
                reconnect_delay_ms,
                enable_raw,
                &client_id_owned,
                &access_token_owned,
                existing_subs,
            )
            .await;
        });

        conn.task = Some(task);

        tracing::info!(connection = %conn.id, "WebSocket connection spawned");
        Ok(())
    }

    /// The main loop for a single WebSocket connection.
    ///
    /// Reads frames, parses binary packets, broadcasts events, and handles
    /// reconnection on failure.
    async fn connection_loop(
        conn_id: ConnectionId,
        mut read: futures_util::stream::SplitStream<
            WebSocketStream<MaybeTlsStream<TcpStream>>,
        >,
        writer: Arc<Mutex<Option<WriterHalf>>>,
        parsed_tx: broadcast::Sender<MarketFeedEvent>,
        raw_tx: Option<broadcast::Sender<Bytes>>,
        auto_reconnect: bool,
        reconnect_delay_ms: u64,
        enable_raw: bool,
        client_id: &str,
        access_token: &str,
        existing_subs: Vec<(Instrument, FeedRequestCode)>,
    ) {
        // Re-subscribe existing instruments after initial connect or reconnect
        if !existing_subs.is_empty() {
            if let Err(e) = Self::resubscribe(
                &writer,
                &existing_subs,
            )
            .await
            {
                tracing::error!(
                    connection = %conn_id,
                    error = %e,
                    "Failed to resubscribe after connect"
                );
            }
        }

        loop {
            match read.next().await {
                Some(Ok(msg)) => match msg {
                    Message::Binary(data) => {
                        // Broadcast raw frame first (if enabled)
                        if enable_raw {
                            if let Some(ref tx) = raw_tx {
                                let _ = tx.send(Bytes::from(data.to_vec()));
                            }
                        }

                        // Parse and broadcast
                        match parse_packet(&data) {
                            Ok(event) => {
                                let _ = parsed_tx.send(event);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    connection = %conn_id,
                                    error = %e,
                                    "Failed to parse packet"
                                );
                            }
                        }
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Close(_) => {
                        tracing::info!(
                            connection = %conn_id,
                            "WebSocket closed by server"
                        );
                        break;
                    }
                    Message::Text(text) => {
                        tracing::debug!(
                            connection = %conn_id,
                            "Received text: {text}"
                        );
                    }
                    _ => {}
                },
                Some(Err(e)) => {
                    tracing::error!(
                        connection = %conn_id,
                        error = %e,
                        "WebSocket error"
                    );
                    break;
                }
                None => {
                    tracing::info!(
                        connection = %conn_id,
                        "WebSocket stream ended"
                    );
                    break;
                }
            }
        }

        // Reconnect if enabled
        if auto_reconnect {
            tracing::info!(
                connection = %conn_id,
                delay_ms = reconnect_delay_ms,
                "Attempting reconnect..."
            );
            tokio::time::sleep(Duration::from_millis(reconnect_delay_ms)).await;

            let url = format!(
                "{WS_MARKET_FEED_URL}?version=2&token={access_token}&clientId={client_id}&authType=2"
            );

            match connect_async(&url).await {
                Ok((ws, _)) => {
                    let (write, new_read) = ws.split();
                    *writer.lock().await = Some(write);

                    tracing::info!(
                        connection = %conn_id,
                        "Reconnected successfully"
                    );

                    // Recurse into connection_loop for the new read half
                    Box::pin(Self::connection_loop(
                        conn_id,
                        new_read,
                        writer,
                        parsed_tx,
                        raw_tx,
                        auto_reconnect,
                        reconnect_delay_ms,
                        enable_raw,
                        client_id,
                        access_token,
                        existing_subs,
                    ))
                    .await;
                }
                Err(e) => {
                    tracing::error!(
                        connection = %conn_id,
                        error = %e,
                        "Reconnection failed"
                    );
                }
            }
        }
    }

    /// Re-subscribe a set of instruments on a connection writer.
    async fn resubscribe(
        writer: &Arc<Mutex<Option<WriterHalf>>>,
        subs: &[(Instrument, FeedRequestCode)],
    ) -> Result<()> {
        // Group by mode
        let mut by_mode: HashMap<u8, Vec<Instrument>> = HashMap::new();
        for (inst, mode) in subs {
            by_mode
                .entry(*mode as u8)
                .or_default()
                .push(inst.clone());
        }

        let mut guard = writer.lock().await;
        let w = guard.as_mut().ok_or_else(|| {
            DhanError::InvalidArgument("writer not available for resubscribe".into())
        })?;

        for (mode_code, instruments) in by_mode {
            for chunk in instruments.chunks(100) {
                let req = FeedSubscribeRequest {
                    RequestCode: mode_code,
                    InstrumentCount: chunk.len(),
                    InstrumentList: chunk.to_vec(),
                };
                let json = serde_json::to_string(&req)?;
                w.send(Message::Text(json.into())).await?;
            }
        }

        Ok(())
    }
}

impl Drop for DhanFeedManager {
    fn drop(&mut self) {
        for conn in &mut self.connections {
            if let Some(task) = conn.task.take() {
                task.abort();
            }
        }
    }
}
