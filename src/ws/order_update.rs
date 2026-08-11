#![allow(missing_docs)]
//! Live Order Update WebSocket client.
//!
//! Connects to `wss://api-order-update.dhan.co` and streams real-time order
//! status changes as JSON messages.
//!
//! # Example
//!
//! ```no_run
//! use dhan_rs::ws::order_update::OrderUpdateStream;
//! use futures_util::StreamExt;
//!
//! # #[tokio::main]
//! # async fn main() -> dhan_rs::error::Result<()> {
//! let mut stream = OrderUpdateStream::connect("1000000001", "your-jwt-token").await?;
//!
//! while let Some(msg) = stream.next().await {
//!     match msg {
//!         Ok(update) => println!("Order {:?} → {:?}", update.Data.OrderNo, update.Data.Status),
//!         Err(e) => eprintln!("Error: {e}"),
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::stream::{self, SplitSink, SplitStream};
use futures_util::{SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::constants::WS_ORDER_UPDATE_URL;
use crate::error::{DhanError, Result};

// ---------------------------------------------------------------------------
// Auth messages
// ---------------------------------------------------------------------------

/// Login request payload for individual users.
#[derive(Serialize)]
#[allow(non_snake_case)]
struct LoginRequest {
    MsgCode: u8,
    ClientId: String,
    Token: String,
}

/// Auth message envelope for individual users.
#[derive(Serialize)]
#[allow(non_snake_case)]
struct IndividualAuthMessage {
    LoginReq: LoginRequest,
    UserType: String,
}

/// Login request payload for partner users (no Token field).
#[derive(Serialize)]
#[allow(non_snake_case)]
struct PartnerLoginRequest {
    MsgCode: u8,
    ClientId: String,
}

/// Auth message envelope for partner users.
#[derive(Serialize)]
#[allow(non_snake_case)]
struct PartnerAuthMessage {
    LoginReq: PartnerLoginRequest,
    UserType: String,
    Secret: String,
}

// ---------------------------------------------------------------------------
// Order Update data types
// ---------------------------------------------------------------------------

/// An incoming order-update message from the WebSocket.
///
/// The top-level envelope has a `Type` field (always `"order_alert"`) and a
/// `Data` field with the actual order details.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct OrderUpdateMessage {
    /// Message type — typically `"order_alert"`.
    pub Type: String,
    /// The order update payload.
    pub Data: OrderUpdateData,
}

fn deserialize_optional_f64<'de, D>(deserializer: D) -> std::result::Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => number
            .as_f64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("invalid numeric order-update field")),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::String(text)) => text
            .trim()
            .parse::<f64>()
            .map(Some)
            .map_err(|_| serde::de::Error::custom("invalid numeric order-update field")),
        Some(_) => Err(serde::de::Error::custom(
            "invalid numeric order-update field",
        )),
    }
}

fn deserialize_optional_i64<'de, D>(deserializer: D) -> std::result::Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("invalid integer order-update field")),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| serde::de::Error::custom("invalid integer order-update field")),
        Some(_) => Err(serde::de::Error::custom(
            "invalid integer order-update field",
        )),
    }
}

/// Detailed order update data received via WebSocket.
///
/// Field names are PascalCase matching the wire format. Abbreviated product /
/// transaction / order-type codes are used (e.g. `"C"` for CNC, `"B"` for Buy,
/// `"LMT"` for Limit).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct OrderUpdateData {
    /// Exchange (e.g. `"NSE"`, `"BSE"`, `"MCX"`).
    #[serde(default)]
    pub Exchange: Option<String>,
    /// Segment (e.g. `"E"` for Equity, `"D"` for Derivatives).
    #[serde(default)]
    pub Segment: Option<String>,
    /// Source platform (`"P"` for API orders).
    #[serde(default)]
    pub Source: Option<String>,
    /// Exchange standard security ID.
    #[serde(default)]
    pub SecurityId: Option<String>,
    /// Dhan client ID.
    #[serde(default)]
    pub ClientId: Option<String>,
    /// Exchange-generated order number.
    #[serde(default)]
    pub ExchOrderNo: Option<String>,
    /// Dhan-generated order number.
    #[serde(default)]
    pub OrderNo: Option<String>,
    /// Product type code (`"C"` = CNC, `"I"` = Intraday, `"M"` = Margin, `"F"` = MTF, `"V"` = CO, `"B"` = BO).
    #[serde(default)]
    pub Product: Option<String>,
    /// Transaction type (`"B"` = Buy, `"S"` = Sell).
    #[serde(default)]
    pub TxnType: Option<String>,
    /// Order type (`"LMT"`, `"MKT"`, `"SL"`, `"SLM"`).
    #[serde(default)]
    pub OrderType: Option<String>,
    /// Order validity (`"DAY"`, `"IOC"`).
    #[serde(default)]
    pub Validity: Option<String>,
    /// Number of shares disclosed/visible.
    #[serde(default)]
    pub DiscQuantity: Option<i64>,
    /// Disclosed quantity remaining.
    #[serde(default)]
    pub DiscQtyRem: Option<i64>,
    /// Quantity pending for execution.
    #[serde(default)]
    pub RemainingQuantity: Option<i64>,
    /// Total order quantity placed.
    #[serde(default)]
    pub Quantity: Option<i64>,
    /// Actual quantity executed on exchange.
    #[serde(default)]
    pub TradedQty: Option<i64>,
    /// Price at which the order was placed.
    #[serde(default)]
    pub Price: Option<f64>,
    /// Trigger price for SL/SL-M/CO/BO.
    #[serde(default)]
    pub TriggerPrice: Option<f64>,
    /// Price at which the trade was executed.
    #[serde(default)]
    pub TradedPrice: Option<f64>,
    /// Average traded price (differs from `TradedPrice` for partials).
    #[serde(default)]
    pub AvgTradedPrice: Option<f64>,
    /// Entry leg order number for BO/CO tracking.
    #[serde(default, deserialize_with = "deserialize_optional_f64")]
    pub AlgoOrdNo: Option<f64>,
    /// `"1"` for AMO orders, `"0"` otherwise.
    #[serde(default)]
    pub OffMktFlag: Option<String>,
    /// Time at which the order was received by Dhan.
    #[serde(default)]
    pub OrderDateTime: Option<String>,
    /// Time at which the order was placed on exchange.
    #[serde(default)]
    pub ExchOrderTime: Option<String>,
    /// Last update time of modification or trade.
    #[serde(default)]
    pub LastUpdatedTime: Option<String>,
    /// Additional remarks (e.g. `"Super Order"`).
    #[serde(default)]
    pub Remarks: Option<String>,
    /// Market type (`"NL"` = Normal, `"AU"` / `"A1"` / `"A2"` = Auction).
    #[serde(default)]
    pub MktType: Option<String>,
    /// Rejection/status reason description.
    #[serde(default)]
    pub ReasonDescription: Option<String>,
    /// Leg number (1 = Entry, 2 = Stop Loss, 3 = Target).
    #[serde(default)]
    pub LegNo: Option<i32>,
    /// Instrument type (e.g. `"EQUITY"`, `"FUTIDX"`).
    #[serde(default)]
    pub Instrument: Option<String>,
    /// Trading symbol.
    #[serde(default)]
    pub Symbol: Option<String>,
    /// Product type name (e.g. `"CNC"`, `"INTRADAY"`).
    #[serde(default)]
    pub ProductName: Option<String>,
    /// Order status (`"Transit"`, `"Pending"`, `"Rejected"`, `"Cancelled"`, `"Traded"`, `"Expired"`).
    #[serde(default)]
    pub Status: Option<String>,
    /// Lot size for derivatives.
    #[serde(default)]
    pub LotSize: Option<i64>,
    /// Strike price for option contracts.
    #[serde(default, deserialize_with = "deserialize_optional_f64")]
    pub StrikePrice: Option<f64>,
    /// Expiry date of the contract.
    #[serde(default)]
    pub ExpiryDate: Option<String>,
    /// Option type (`"CE"` or `"PE"`, `"XX"` for non-options).
    #[serde(default)]
    pub OptType: Option<String>,
    /// Display name of the instrument.
    #[serde(default)]
    pub DisplayName: Option<String>,
    /// ISIN of the instrument.
    #[serde(default)]
    pub Isin: Option<String>,
    /// Exchange series (e.g. `"EQ"`).
    #[serde(default)]
    pub Series: Option<String>,
    /// Good-till date for forever orders.
    #[serde(default)]
    pub GoodTillDaysDate: Option<String>,
    /// LTP at time of order update.
    #[serde(default)]
    pub RefLtp: Option<f64>,
    /// Tick size of the instrument.
    #[serde(default)]
    pub TickSize: Option<f64>,
    /// Exchange ID for special order types.
    #[serde(default)]
    pub AlgoId: Option<String>,
    /// Multiplier for commodity/currency contracts.
    #[serde(default)]
    pub Multiplier: Option<i64>,
    /// User/partner generated tracking ID.
    #[serde(default)]
    pub CorrelationId: Option<String>,

    // Fields with lowercase names from the wire format
    /// Exchange series (duplicate, lowercase variant).
    #[serde(default)]
    pub series: Option<String>,
    /// Good-till date (duplicate, lowercase variant).
    #[serde(default, alias = "goodTillDaysDate")]
    pub good_till_days_date: Option<String>,
    /// Instrument type (lowercase variant).
    #[serde(default, alias = "instrumentType")]
    pub instrument_type: Option<String>,
    /// Reference LTP (lowercase variant).
    #[serde(default, alias = "refLtp")]
    pub ref_ltp: Option<f64>,
    /// Tick size (lowercase variant).
    #[serde(default, alias = "tickSize")]
    pub tick_size: Option<f64>,
    /// Algo ID (lowercase variant).
    #[serde(default, alias = "algoId")]
    pub algo_id: Option<String>,
    /// Multiplier (lowercase variant).
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub multiplier: Option<i64>,
}

// ---------------------------------------------------------------------------
// Stream wrapper
// ---------------------------------------------------------------------------

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

const DEFAULT_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// The transport and readiness state of an order-update connection.
///
/// Dhan does not document a positive authorization acknowledgement. Sending
/// the authorization envelope therefore reaches [`Self::ReadinessPending`],
/// not `Live`; only a valid order update proves that the connection is
/// delivering application data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderUpdateConnectionState {
    /// The WebSocket upgrade completed, but no authorization request was sent.
    TransportConnected,
    /// The authorization envelope is being written.
    Authorizing,
    /// The authorization envelope was written; no positive ACK is claimed.
    ReadinessPending,
    /// At least one valid order update arrived on this transport.
    Live,
    /// A graceful close has begun.
    Closing,
    /// The peer closed the socket or the transport reached EOF.
    Closed,
}

/// A WebSocket close frame retained for caller diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderUpdateClose {
    /// RFC 6455 close code as transmitted by the peer.
    pub code: Option<u16>,
    /// UTF-8 close reason as transmitted by the peer.
    pub reason: String,
}

/// Classification of non-order JSON received on the order-update socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderUpdateControlKind {
    /// A login/authorization response or status message.
    Authorization,
    /// A broker error response.
    Error,
    /// Another valid JSON control message.
    Control,
}

/// Sanitized control-plane data. The complete JSON is deliberately not kept
/// so order details or broker credentials cannot accidentally enter logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderUpdateControlMessage {
    /// Control-plane classification.
    pub kind: OrderUpdateControlKind,
    /// Broker-provided numeric code, when one is present.
    pub code: Option<i64>,
    /// Broker-provided type name, when one is present.
    pub message_type: Option<String>,
}

/// A fully classified frame from the order-update WebSocket.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum OrderUpdateProtocolEvent {
    /// A validated `Type = "order_alert"` application message.
    Update(OrderUpdateMessage),
    /// A login/authorization control message.
    Authorization(OrderUpdateControlMessage),
    /// Another JSON control message.
    Control(OrderUpdateControlMessage),
    /// A broker error message.
    Error(OrderUpdateControlMessage),
    /// The peer's exact close code and reason.
    Close(OrderUpdateClose),
}

fn control_code(value: &Value) -> Option<i64> {
    [
        "code",
        "Code",
        "ErrorCode",
        "errorCode",
        "MsgCode",
        "msgCode",
    ]
    .iter()
    .find_map(|key| value.get(*key).and_then(Value::as_i64))
}

fn is_valid_application_update(update: &OrderUpdateMessage) -> bool {
    let populated = |value: Option<&str>| value.is_some_and(|text| !text.trim().is_empty());
    let has_order_identity = populated(update.Data.OrderNo.as_deref())
        || populated(update.Data.ExchOrderNo.as_deref())
        || populated(update.Data.CorrelationId.as_deref());
    update.Type == "order_alert"
        && has_order_identity
        && populated(update.Data.ClientId.as_deref())
        && populated(update.Data.Status.as_deref())
}

fn classify_json(bytes: &[u8]) -> Result<OrderUpdateProtocolEvent> {
    let value: Value = serde_json::from_slice(bytes)?;
    let object = value.as_object().ok_or_else(|| {
        DhanError::InvalidArgument("order-update JSON must be an object".to_owned())
    })?;
    let message_type = object
        .get("Type")
        .or_else(|| object.get("type"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    if message_type.as_deref() == Some("order_alert") {
        let update =
            serde_json::from_value::<OrderUpdateMessage>(value).map_err(DhanError::Json)?;
        if !is_valid_application_update(&update) {
            return Err(DhanError::InvalidArgument(
                "malformed order-update application payload".to_owned(),
            ));
        }
        return Ok(OrderUpdateProtocolEvent::Update(update));
    }

    if object.contains_key("Data") || object.contains_key("data") {
        return Err(DhanError::InvalidArgument(
            "unsupported order-update Type discriminator".to_owned(),
        ));
    }

    let lower_type = message_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_authorization = lower_type.contains("auth")
        || lower_type.contains("login")
        || object.contains_key("LoginResp")
        || object.contains_key("LoginResponse");
    let is_error = lower_type.contains("error")
        || object.contains_key("ErrorCode")
        || object.contains_key("errorCode")
        || object.contains_key("error")
        || object.get("success").and_then(Value::as_bool) == Some(false);
    let kind = if is_error {
        OrderUpdateControlKind::Error
    } else if is_authorization {
        OrderUpdateControlKind::Authorization
    } else {
        OrderUpdateControlKind::Control
    };
    let control = OrderUpdateControlMessage {
        kind,
        code: control_code(&value),
        message_type,
    };
    Ok(match kind {
        OrderUpdateControlKind::Authorization => OrderUpdateProtocolEvent::Authorization(control),
        OrderUpdateControlKind::Error => OrderUpdateProtocolEvent::Error(control),
        OrderUpdateControlKind::Control => OrderUpdateProtocolEvent::Control(control),
    })
}

fn close_details(
    frame: Option<tokio_tungstenite::tungstenite::protocol::CloseFrame>,
) -> OrderUpdateClose {
    match frame {
        Some(frame) => OrderUpdateClose {
            code: Some(u16::from(frame.code)),
            reason: frame.reason.to_string(),
        },
        None => OrderUpdateClose {
            code: None,
            reason: String::new(),
        },
    }
}

/// Classify a raw WebSocket message.
///
/// Text frames contain JSON. Binary frames are accepted only when their entire
/// payload is valid UTF-8 JSON; other binary data is a protocol error. Ping and
/// Pong are consumed by the transport and return `None`.
fn classify_frame(message: Message) -> Result<Option<OrderUpdateProtocolEvent>> {
    match message {
        Message::Text(text) => classify_json(text.as_bytes()).map(Some),
        Message::Binary(bytes) => {
            std::str::from_utf8(&bytes).map_err(|_| {
                DhanError::InvalidArgument("binary order-update frame is not UTF-8 JSON".to_owned())
            })?;
            classify_json(&bytes).map(Some)
        }
        Message::Close(frame) => Ok(Some(OrderUpdateProtocolEvent::Close(close_details(frame)))),
        Message::Ping(_) | Message::Pong(_) => Ok(None),
        _ => Ok(None),
    }
}

fn frame_claims_order_alert(message: &Message) -> bool {
    let bytes = match message {
        Message::Text(text) => text.as_bytes(),
        Message::Binary(bytes) => bytes.as_ref(),
        _ => return false,
    };
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .get("Type")
                .or_else(|| value.get("type"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .as_deref()
        == Some("order_alert")
}

fn self_auth_json(client_id: &str, access_token: &str) -> Result<String> {
    Ok(serde_json::to_string(&IndividualAuthMessage {
        LoginReq: LoginRequest {
            MsgCode: 42,
            ClientId: client_id.to_owned(),
            Token: access_token.to_owned(),
        },
        UserType: "SELF".to_owned(),
    })?)
}

fn partner_auth_json(partner_id: &str, partner_secret: &str) -> Result<String> {
    Ok(serde_json::to_string(&PartnerAuthMessage {
        LoginReq: PartnerLoginRequest {
            MsgCode: 42,
            ClientId: partner_id.to_owned(),
        },
        UserType: "PARTNER".to_owned(),
        Secret: partner_secret.to_owned(),
    })?)
}

/// A streaming connection for receiving live order updates.
///
/// Implements [`Stream<Item = Result<OrderUpdateMessage>>`] so you can use it
/// with `StreamExt::next()` and other stream combinators.
pub struct OrderUpdateStream {
    read: SplitStream<WsStream>,
    write: SplitSink<WsStream, Message>,
    state: OrderUpdateConnectionState,
    last_close: Option<OrderUpdateClose>,
}

impl OrderUpdateStream {
    /// Connect to the order-update WebSocket as an individual user.
    ///
    /// Sends the authentication message immediately after connection is
    /// established. This does not claim that Dhan positively acknowledged it.
    pub async fn connect(client_id: &str, access_token: &str) -> Result<Self> {
        Self::connect_to(WS_ORDER_UPDATE_URL, client_id, access_token).await
    }

    /// Connect to an explicit endpoint as an individual user.
    ///
    /// This is primarily useful for deterministic local protocol tests and
    /// private compatible gateways.
    pub async fn connect_to(endpoint: &str, client_id: &str, access_token: &str) -> Result<Self> {
        let (ws, _resp) = connect_async(endpoint).await?;

        let (mut write, read) = ws.split();
        let auth_json = self_auth_json(client_id, access_token)?;
        write.send(Message::Text(auth_json.into())).await?;

        tracing::info!("Order-update transport connected; authorization request sent");

        Ok(Self {
            read,
            write,
            state: OrderUpdateConnectionState::ReadinessPending,
            last_close: None,
        })
    }

    /// Connect to the order-update WebSocket as a partner.
    ///
    /// Partner platforms receive order updates for all connected users.
    pub async fn connect_partner(partner_id: &str, partner_secret: &str) -> Result<Self> {
        Self::connect_partner_to(WS_ORDER_UPDATE_URL, partner_id, partner_secret).await
    }

    /// Connect to an explicit endpoint in partner mode.
    pub async fn connect_partner_to(
        endpoint: &str,
        partner_id: &str,
        partner_secret: &str,
    ) -> Result<Self> {
        let (ws, _resp) = connect_async(endpoint).await?;

        let (mut write, read) = ws.split();
        let auth_json = partner_auth_json(partner_id, partner_secret)?;
        write.send(Message::Text(auth_json.into())).await?;

        tracing::info!("Order-update partner transport connected; authorization request sent");

        Ok(Self {
            read,
            write,
            state: OrderUpdateConnectionState::ReadinessPending,
            last_close: None,
        })
    }

    /// Return the current transport/readiness state.
    pub fn state(&self) -> &OrderUpdateConnectionState {
        &self.state
    }

    /// Return the most recently observed peer close frame.
    pub fn last_close(&self) -> Option<&OrderUpdateClose> {
        self.last_close.as_ref()
    }

    /// Receive one classified protocol event.
    pub async fn next_protocol_event(&mut self) -> Result<Option<OrderUpdateProtocolEvent>> {
        loop {
            let Some(message) = self.read.next().await else {
                self.state = OrderUpdateConnectionState::Closed;
                return Ok(None);
            };
            match classify_frame(message?) {
                Ok(Some(event)) => {
                    match &event {
                        OrderUpdateProtocolEvent::Update(_) => {
                            self.state = OrderUpdateConnectionState::Live;
                        }
                        OrderUpdateProtocolEvent::Close(close) => {
                            self.last_close = Some(close.clone());
                            self.state = OrderUpdateConnectionState::Closed;
                        }
                        _ => {}
                    }
                    return Ok(Some(event));
                }
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "Failed to classify order-update frame; payload omitted"
                    );
                    return Err(error);
                }
            }
        }
    }

    /// Close gracefully, waiting up to five seconds for the peer Close frame.
    pub async fn close(self) -> Result<()> {
        self.close_with_timeout(DEFAULT_CLOSE_TIMEOUT).await
    }

    /// Close gracefully with a caller-selected peer handshake timeout.
    pub async fn close_with_timeout(mut self, wait: Duration) -> Result<()> {
        self.state = OrderUpdateConnectionState::Closing;
        self.write.send(Message::Close(None)).await?;
        let peer_close = async {
            while let Some(message) = self.read.next().await {
                match message? {
                    Message::Close(_) => return Ok::<(), DhanError>(()),
                    _ => continue,
                }
            }
            Ok(())
        };
        timeout(wait, peer_close).await.map_err(|_| {
            DhanError::InvalidArgument("order-update close handshake timed out".to_owned())
        })??;
        Ok(())
    }
}

impl Stream for OrderUpdateStream {
    type Item = Result<OrderUpdateMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.read.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(msg))) => match classify_frame(msg) {
                    Ok(Some(OrderUpdateProtocolEvent::Update(update))) => {
                        self.state = OrderUpdateConnectionState::Live;
                        return Poll::Ready(Some(Ok(update)));
                    }
                    Ok(Some(OrderUpdateProtocolEvent::Close(close))) => {
                        self.last_close = Some(close);
                        self.state = OrderUpdateConnectionState::Closed;
                        return Poll::Ready(None);
                    }
                    Ok(Some(_)) | Ok(None) => continue,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "Failed to classify order-update frame; payload omitted"
                        );
                        return Poll::Ready(Some(Err(error)));
                    }
                },
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(DhanError::WebSocket(Box::new(e)))));
                }
                Poll::Ready(None) => {
                    self.state = OrderUpdateConnectionState::Closed;
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Managed order-update supervisor
// ---------------------------------------------------------------------------

/// Authentication mode used by a managed order-update connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderUpdateAuthMode {
    /// Individual Dhan account (`UserType = SELF`).
    SelfUser,
    /// Partner platform (`UserType = PARTNER`).
    Partner,
}

#[derive(Clone)]
enum CredentialSecret {
    SelfUser {
        client_id: String,
        access_token: String,
    },
    Partner {
        partner_id: String,
        partner_secret: String,
    },
}

/// A versioned credential snapshot used for one connection attempt.
///
/// Its `Debug` implementation always redacts the access token or partner
/// secret. A running connection is not interrupted by an update, but every
/// future attempt reads the newest strictly higher version.
#[derive(Clone)]
pub struct OrderUpdateCredentialSnapshot {
    version: u64,
    secret: CredentialSecret,
}

impl OrderUpdateCredentialSnapshot {
    /// Construct individual-user credentials.
    pub fn self_user(
        version: u64,
        client_id: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self {
            version,
            secret: CredentialSecret::SelfUser {
                client_id: client_id.into(),
                access_token: access_token.into(),
            },
        }
    }

    /// Construct partner credentials.
    pub fn partner(
        version: u64,
        partner_id: impl Into<String>,
        partner_secret: impl Into<String>,
    ) -> Self {
        Self {
            version,
            secret: CredentialSecret::Partner {
                partner_id: partner_id.into(),
                partner_secret: partner_secret.into(),
            },
        }
    }

    /// Monotonically increasing credential version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Authentication mode represented by this snapshot.
    pub fn mode(&self) -> OrderUpdateAuthMode {
        match self.secret {
            CredentialSecret::SelfUser { .. } => OrderUpdateAuthMode::SelfUser,
            CredentialSecret::Partner { .. } => OrderUpdateAuthMode::Partner,
        }
    }

    fn principal_id(&self) -> &str {
        match &self.secret {
            CredentialSecret::SelfUser { client_id, .. } => client_id,
            CredentialSecret::Partner { partner_id, .. } => partner_id,
        }
    }

    fn auth_json(&self) -> Result<String> {
        match &self.secret {
            CredentialSecret::SelfUser {
                client_id,
                access_token,
            } => self_auth_json(client_id, access_token),
            CredentialSecret::Partner {
                partner_id,
                partner_secret,
            } => partner_auth_json(partner_id, partner_secret),
        }
    }
}

impl fmt::Debug for OrderUpdateCredentialSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrderUpdateCredentialSnapshot")
            .field("version", &self.version)
            .field("mode", &self.mode())
            .field("principal_id", &"[REDACTED]")
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// Sender for replacing managed credentials with a higher-version snapshot.
#[derive(Clone)]
pub struct OrderUpdateCredentialUpdater {
    tx: watch::Sender<OrderUpdateCredentialSnapshot>,
}

impl fmt::Debug for OrderUpdateCredentialUpdater {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrderUpdateCredentialUpdater")
            .field("current_version", &self.tx.borrow().version())
            .finish()
    }
}

impl OrderUpdateCredentialUpdater {
    /// Replace credentials if `snapshot.version()` is strictly newer.
    pub fn replace(&self, snapshot: OrderUpdateCredentialSnapshot) -> bool {
        self.tx.send_if_modified(|current| {
            if snapshot.version() > current.version() {
                *current = snapshot;
                true
            } else {
                false
            }
        })
    }

    /// Return the currently published credential version.
    pub fn version(&self) -> u64 {
        self.tx.borrow().version()
    }
}

/// Receiving half of the refreshable credential channel.
pub struct OrderUpdateCredentialReceiver {
    rx: watch::Receiver<OrderUpdateCredentialSnapshot>,
}

/// Create a refreshable, versioned credential channel.
pub fn order_update_credential_channel(
    initial: OrderUpdateCredentialSnapshot,
) -> (OrderUpdateCredentialUpdater, OrderUpdateCredentialReceiver) {
    let (tx, rx) = watch::channel(initial);
    (
        OrderUpdateCredentialUpdater { tx },
        OrderUpdateCredentialReceiver { rx },
    )
}

/// Managed reconnect and buffering settings.
#[derive(Debug, Clone)]
pub struct ManagedOrderUpdateConfig {
    /// WebSocket endpoint; defaults to Dhan's production order-update URL.
    pub endpoint: String,
    /// Bounded application event channel capacity.
    pub event_capacity: usize,
    /// Maximum live updates buffered while a snapshot is in flight.
    pub reconciliation_buffer_capacity: usize,
    /// Maximum time allowed for one client's reconciliation provider call.
    pub reconciliation_client_timeout: Duration,
    /// Maximum time allowed for the complete multi-client reconciliation pass.
    pub reconciliation_overall_timeout: Duration,
    /// Maximum number of client snapshots fetched concurrently.
    pub reconciliation_concurrency: usize,
    /// WebSocket upgrade deadline.
    pub connect_timeout: Duration,
    /// Authorization and Close-frame write deadline.
    pub write_timeout: Duration,
    /// Time without any WebSocket frame before the transport is reconnected.
    pub inactivity_timeout: Duration,
    /// Time to wait for the first valid order update before emitting an honest
    /// readiness diagnostic. A quiet socket remains open after this deadline.
    pub readiness_timeout: Duration,
    /// Peer close-handshake deadline.
    pub close_timeout: Duration,
    /// Initial exponential retry ceiling.
    pub initial_backoff: Duration,
    /// Maximum exponential retry ceiling.
    pub max_backoff: Duration,
    /// Healthy-traffic duration after which the reconnect attempt counter resets.
    pub stable_connection_period: Duration,
}

impl Default for ManagedOrderUpdateConfig {
    fn default() -> Self {
        Self {
            endpoint: WS_ORDER_UPDATE_URL.to_owned(),
            event_capacity: 256,
            reconciliation_buffer_capacity: 1024,
            reconciliation_client_timeout: Duration::from_secs(10),
            reconciliation_overall_timeout: Duration::from_secs(30),
            reconciliation_concurrency: 8,
            connect_timeout: Duration::from_secs(10),
            write_timeout: Duration::from_secs(5),
            inactivity_timeout: Duration::from_secs(45),
            readiness_timeout: Duration::from_secs(30),
            close_timeout: DEFAULT_CLOSE_TIMEOUT,
            initial_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(30),
            stable_connection_period: Duration::from_secs(60),
        }
    }
}

impl ManagedOrderUpdateConfig {
    fn validate(&self) -> Result<()> {
        if self.event_capacity == 0 {
            return Err(DhanError::InvalidArgument(
                "managed order-update event capacity must be non-zero".to_owned(),
            ));
        }
        if self.reconciliation_buffer_capacity == 0 {
            return Err(DhanError::InvalidArgument(
                "managed order-update reconciliation capacity must be non-zero".to_owned(),
            ));
        }
        if self.reconciliation_concurrency == 0 {
            return Err(DhanError::InvalidArgument(
                "managed order-update reconciliation concurrency must be non-zero".to_owned(),
            ));
        }
        if self.reconciliation_client_timeout.is_zero()
            || self.reconciliation_overall_timeout.is_zero()
        {
            return Err(DhanError::InvalidArgument(
                "managed order-update reconciliation timeouts must be non-zero".to_owned(),
            ));
        }
        if self.connect_timeout.is_zero()
            || self.write_timeout.is_zero()
            || self.inactivity_timeout.is_zero()
            || self.readiness_timeout.is_zero()
            || self.close_timeout.is_zero()
        {
            return Err(DhanError::InvalidArgument(
                "managed order-update transport and readiness timeouts must be non-zero".to_owned(),
            ));
        }
        if self.initial_backoff.is_zero() || self.max_backoff.is_zero() {
            return Err(DhanError::InvalidArgument(
                "managed order-update retry delays must be non-zero".to_owned(),
            ));
        }
        if self.initial_backoff > self.max_backoff {
            return Err(DhanError::InvalidArgument(
                "initial order-update backoff exceeds maximum".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Why managed order state may be incomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderUpdateGapCause {
    /// The transport ended without a locally completed close handshake.
    UncertainDisconnect,
    /// A bounded downstream consumer skipped messages.
    ConsumerLag { dropped: u64 },
    /// A frame claimed to be an order alert but its application payload was invalid.
    MalformedApplicationFrame,
    /// Live updates exceeded the bounded snapshot reconciliation buffer.
    ReconciliationBufferOverflow,
}

/// Truthful managed lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedOrderUpdateState {
    /// No connection attempt is active.
    Stopped,
    /// A WebSocket upgrade is in progress.
    Connecting { attempt: u64 },
    /// The transport upgrade completed.
    TransportConnected,
    /// The authorization envelope is being sent.
    Authorizing,
    /// Authorization was sent; no positive ACK has been inferred.
    ReadinessPending,
    /// No valid order event arrived by the diagnostic deadline. The socket is
    /// still continuously polled because a legitimately quiet account cannot
    /// be distinguished from an unacknowledged authorization request.
    ReadinessUnconfirmed { waited: Duration },
    /// A valid order update was observed on this transport.
    Live,
    /// Current order state may be incomplete.
    GapDetected { cause: OrderUpdateGapCause },
    /// A snapshot is being fetched while live updates are buffered.
    Reconciling { clients_pending: usize },
    /// Current state was recovered only to the stated bounded extent.
    Degraded,
    /// A transient failure is waiting for its full-jitter retry delay.
    Backoff { attempt: u64, delay: Duration },
    /// The broker rejected the current credential version.
    AuthBlocked { credential_version: u64 },
    /// A graceful close is in progress.
    Closing,
}

/// Events delivered by the managed supervisor.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ManagedOrderUpdateEvent {
    /// Current order state update. Delivery is at-least-observed and may be
    /// deduplicated; it is not described as replayed or exactly once.
    Update(OrderUpdateMessage),
    /// Lifecycle state transition.
    StateChanged(ManagedOrderUpdateState),
    /// Sanitized authorization response.
    Authorization(OrderUpdateControlMessage),
    /// Sanitized non-order control response.
    Control(OrderUpdateControlMessage),
    /// Sanitized broker error response.
    Error(OrderUpdateControlMessage),
    /// No valid order event arrived by the configured diagnostic deadline.
    ReadinessTimeout { waited: Duration },
    /// Peer close or transport termination diagnostics.
    Disconnect {
        /// Exact peer close, when the transport supplied one.
        close: Option<OrderUpdateClose>,
        /// Redacted transport classification.
        reason: String,
    },
    /// Explicit order-state uncertainty notification.
    GapDetected { cause: OrderUpdateGapCause },
    /// Snapshot-plus-buffer merge completed for these authorized clients.
    Reconciled { client_ids: Vec<String> },
    /// Reconciliation could not be authorized or completed for one client.
    GapUnresolved {
        /// `None` means a partner gap could include an unseen client ID.
        client_id: Option<String>,
        /// Sanitized reason.
        reason: String,
    },
    /// A deterministic merge found an ambiguity and retained the safer state.
    ReconciliationWarning { order_id: Option<String> },
    /// The supervisor terminated after shutdown.
    Stopped,
}

/// Snapshot provider error without any credential-bearing context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrderUpdateReconciliationError {
    /// No authorized REST context is available for this client ID.
    #[error("authorized reconciliation context unavailable")]
    AuthorizationUnavailable,
    /// The authorized snapshot request failed.
    #[error("snapshot reconciliation failed: {0}")]
    SnapshotFailed(String),
}

/// Boxed future returned by [`OrderUpdateReconciler`].
pub type OrderUpdateReconciliationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = std::result::Result<
                    Vec<OrderUpdateMessage>,
                    OrderUpdateReconciliationError,
                >,
            > + Send
            + 'a,
    >,
>;

/// Application-supplied authorized snapshot abstraction.
///
/// SELF callers normally use the same client's authorized REST context.
/// PARTNER callers must resolve every client ID independently; the WebSocket
/// partner secret is never assumed to authorize REST order snapshots.
pub trait OrderUpdateReconciler: Send + Sync + 'static {
    /// Fetch current order state for exactly one authorized client.
    fn reconcile<'a>(&'a self, client_id: &'a str) -> OrderUpdateReconciliationFuture<'a>;
}

#[derive(Debug)]
enum SupervisorCommand {
    Shutdown,
}

#[derive(Debug, Clone, Copy, Default)]
struct ConsumerLagSignal {
    total_dropped: u64,
}

/// Receiver that converts broadcast overflow into an explicit gap event and
/// informs the supervisor so reconciliation can begin.
pub struct ManagedOrderUpdateReceiver {
    rx: broadcast::Receiver<ManagedOrderUpdateEvent>,
    lag_tx: watch::Sender<ConsumerLagSignal>,
}

impl ManagedOrderUpdateReceiver {
    /// Receive the next event. `None` means the supervisor stopped and all
    /// senders were dropped.
    pub async fn recv(&mut self) -> Option<ManagedOrderUpdateEvent> {
        match self.rx.recv().await {
            Ok(event) => Some(event),
            Err(broadcast::error::RecvError::Lagged(dropped)) => {
                self.lag_tx.send_modify(|signal| {
                    signal.total_dropped = signal.total_dropped.saturating_add(dropped);
                });
                Some(ManagedOrderUpdateEvent::GapDetected {
                    cause: OrderUpdateGapCause::ConsumerLag { dropped },
                })
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

/// Handle for a continuously polled, reconnecting order-update supervisor.
pub struct ManagedOrderUpdate {
    event_tx: broadcast::Sender<ManagedOrderUpdateEvent>,
    command_tx: mpsc::Sender<SupervisorCommand>,
    lag_tx: watch::Sender<ConsumerLagSignal>,
    state_rx: watch::Receiver<ManagedOrderUpdateState>,
    task: Option<JoinHandle<()>>,
    shutdown_timeout: Duration,
}

impl ManagedOrderUpdate {
    /// Start a supervisor and return its first event receiver.
    pub fn start(
        config: ManagedOrderUpdateConfig,
        credentials: OrderUpdateCredentialReceiver,
        reconciler: Option<Arc<dyn OrderUpdateReconciler>>,
    ) -> Result<(Self, ManagedOrderUpdateReceiver)> {
        config.validate()?;
        let (event_tx, event_rx) = broadcast::channel(config.event_capacity);
        let (command_tx, command_rx) = mpsc::channel(32);
        let (lag_tx, lag_rx) = watch::channel(ConsumerLagSignal::default());
        let (state_tx, state_rx) = watch::channel(ManagedOrderUpdateState::Stopped);
        let task_event_tx = event_tx.clone();
        let receiver_lag_tx = lag_tx.clone();
        let shutdown_timeout = config.close_timeout + config.write_timeout + Duration::from_secs(1);
        let task = tokio::spawn(run_supervisor(
            config,
            credentials.rx,
            reconciler,
            task_event_tx,
            state_tx,
            command_rx,
            lag_rx,
        ));
        Ok((
            Self {
                event_tx,
                command_tx: command_tx.clone(),
                lag_tx,
                state_rx,
                task: Some(task),
                shutdown_timeout,
            },
            ManagedOrderUpdateReceiver {
                rx: event_rx,
                lag_tx: receiver_lag_tx,
            },
        ))
    }

    /// Subscribe an additional bounded consumer.
    pub fn subscribe(&self) -> ManagedOrderUpdateReceiver {
        ManagedOrderUpdateReceiver {
            rx: self.event_tx.subscribe(),
            lag_tx: self.lag_tx.clone(),
        }
    }

    /// Borrow the latest lifecycle state snapshot.
    pub fn state(&self) -> ManagedOrderUpdateState {
        self.state_rx.borrow().clone()
    }

    /// Observe future state changes without depending on order-event traffic.
    pub fn state_receiver(&self) -> watch::Receiver<ManagedOrderUpdateState> {
        self.state_rx.clone()
    }

    /// Request a graceful Close handshake and join the owner task.
    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.command_tx.send(SupervisorCommand::Shutdown).await;
        let Some(mut task) = self.task.take() else {
            return Ok(());
        };
        if timeout(self.shutdown_timeout, &mut task).await.is_err() {
            task.abort();
            let _ = task.await;
            return Err(DhanError::InvalidArgument(
                "managed order-update shutdown timed out".to_owned(),
            ));
        }
        Ok(())
    }
}

impl Drop for ManagedOrderUpdate {
    fn drop(&mut self) {
        let _ = self.command_tx.try_send(SupervisorCommand::Shutdown);
        // Dropping a JoinHandle detaches its task. Abort as a final fallback so
        // a full command channel or stalled close handshake cannot orphan a
        // broker connection when callers omit the async shutdown path.
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn transition(
    state_tx: &watch::Sender<ManagedOrderUpdateState>,
    event_tx: &broadcast::Sender<ManagedOrderUpdateEvent>,
    state: ManagedOrderUpdateState,
) {
    state_tx.send_replace(state.clone());
    let _ = event_tx.send(ManagedOrderUpdateEvent::StateChanged(state));
}

fn emit_gap(
    state_tx: &watch::Sender<ManagedOrderUpdateState>,
    event_tx: &broadcast::Sender<ManagedOrderUpdateEvent>,
    cause: OrderUpdateGapCause,
) {
    transition(
        state_tx,
        event_tx,
        ManagedOrderUpdateState::GapDetected {
            cause: cause.clone(),
        },
    );
    let _ = event_tx.send(ManagedOrderUpdateEvent::GapDetected { cause });
}

fn consume_lag_signal(
    lag_rx: &mut watch::Receiver<ConsumerLagSignal>,
    observed_total: &mut u64,
) -> u64 {
    let total = lag_rx.borrow_and_update().total_dropped;
    let dropped = total.saturating_sub(*observed_total);
    *observed_total = total;
    dropped
}

static JITTER_STATE: AtomicU64 = AtomicU64::new(0);

fn full_jitter(ceiling: Duration) -> Duration {
    let nanos = ceiling.as_nanos().min(u128::from(u64::MAX)) as u64;
    if nanos == 0 {
        return Duration::ZERO;
    }
    let seed = JITTER_STATE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        let mut next = if current == 0 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
        } else {
            current
        };
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        Some(next)
    });
    let value = seed.unwrap_or_else(|value| value);
    Duration::from_nanos(value % nanos.saturating_add(1))
}

fn retry_ceiling(config: &ManagedOrderUpdateConfig, attempt: u64) -> Duration {
    let factor = 1_u32 << attempt.saturating_sub(1).min(31);
    config
        .initial_backoff
        .checked_mul(factor)
        .unwrap_or(config.max_backoff)
        .min(config.max_backoff)
}

#[derive(Debug)]
struct ReconciliationOutcome {
    client_id: String,
    result: std::result::Result<Vec<OrderUpdateMessage>, OrderUpdateReconciliationError>,
}

async fn reconcile_clients(
    clients: Vec<String>,
    reconciler: Option<Arc<dyn OrderUpdateReconciler>>,
    client_timeout: Duration,
    overall_timeout: Duration,
    concurrency: usize,
) -> Vec<ReconciliationOutcome> {
    let mut pending: HashSet<String> = clients.iter().cloned().collect();
    let calls = stream::iter(clients).map(|client_id| {
        let provider = reconciler.clone();
        async move {
            let result = match provider {
                Some(provider) => {
                    match timeout(client_timeout, provider.reconcile(&client_id)).await {
                        Ok(result) => result,
                        Err(_) => Err(OrderUpdateReconciliationError::SnapshotFailed(
                            "per-client reconciliation deadline exceeded".to_owned(),
                        )),
                    }
                }
                None => Err(OrderUpdateReconciliationError::AuthorizationUnavailable),
            };
            ReconciliationOutcome { client_id, result }
        }
    });
    let calls = calls.buffer_unordered(concurrency);
    tokio::pin!(calls);
    let overall_deadline = tokio::time::sleep(overall_timeout);
    tokio::pin!(overall_deadline);
    let mut outcomes = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        tokio::select! {
            outcome = calls.next() => {
                let Some(outcome) = outcome else {
                    break;
                };
                pending.remove(&outcome.client_id);
                outcomes.push(outcome);
            }
            _ = &mut overall_deadline => {
                let mut timed_out: Vec<_> = pending.drain().collect();
                timed_out.sort();
                outcomes.extend(timed_out.into_iter().map(|client_id| ReconciliationOutcome {
                    client_id,
                    result: Err(OrderUpdateReconciliationError::SnapshotFailed(
                        "overall reconciliation deadline exceeded".to_owned(),
                    )),
                }));
            }
        }
    }
    outcomes
}

#[allow(clippy::too_many_arguments)]
fn start_reconciliation(
    credential: &OrderUpdateCredentialSnapshot,
    config: &ManagedOrderUpdateConfig,
    reconciler: Option<Arc<dyn OrderUpdateReconciler>>,
    known_clients: &HashSet<String>,
    event_tx: &broadcast::Sender<ManagedOrderUpdateEvent>,
    state_tx: &watch::Sender<ManagedOrderUpdateState>,
    task: &mut Option<JoinHandle<Vec<ReconciliationOutcome>>>,
    buffer: &mut Vec<OrderUpdateMessage>,
    partner_unknown_unresolved: &mut bool,
) {
    if let Some(previous) = task.take() {
        previous.abort();
    }
    buffer.clear();
    let clients = match credential.mode() {
        OrderUpdateAuthMode::SelfUser => vec![credential.principal_id().to_owned()],
        OrderUpdateAuthMode::Partner => {
            *partner_unknown_unresolved = true;
            known_clients.iter().cloned().collect()
        }
    };
    if *partner_unknown_unresolved {
        let _ = event_tx.send(ManagedOrderUpdateEvent::GapUnresolved {
            client_id: None,
            reason: "partner gap may include a client ID not observed before detection".to_owned(),
        });
    }
    transition(
        state_tx,
        event_tx,
        ManagedOrderUpdateState::Reconciling {
            clients_pending: clients.len(),
        },
    );
    let client_timeout = config.reconciliation_client_timeout;
    let overall_timeout = config.reconciliation_overall_timeout;
    let concurrency = config.reconciliation_concurrency;
    *task = Some(tokio::spawn(reconcile_clients(
        clients,
        reconciler,
        client_timeout,
        overall_timeout,
        concurrency,
    )));
}

#[derive(Debug, Clone, PartialEq)]
struct UpdateFingerprint {
    status: Option<String>,
    traded_quantity: Option<i64>,
    price_bits: Option<u64>,
    update_time: Option<String>,
}

fn update_order_id(update: &OrderUpdateMessage) -> Option<String> {
    update
        .Data
        .OrderNo
        .clone()
        .or_else(|| update.Data.ExchOrderNo.clone())
        .or_else(|| update.Data.CorrelationId.clone())
}

fn update_fingerprint(update: &OrderUpdateMessage) -> UpdateFingerprint {
    UpdateFingerprint {
        status: update.Data.Status.clone(),
        traded_quantity: update.Data.TradedQty,
        price_bits: update
            .Data
            .AvgTradedPrice
            .or(update.Data.Price)
            .map(f64::to_bits),
        update_time: update
            .Data
            .LastUpdatedTime
            .clone()
            .or_else(|| update.Data.ExchOrderTime.clone()),
    }
}

fn is_terminal(update: &OrderUpdateMessage) -> bool {
    matches!(
        update.Data.Status.as_deref(),
        Some("Rejected" | "Cancelled" | "Traded" | "Expired")
    )
}

fn prefer_incoming(current: &OrderUpdateMessage, incoming: &OrderUpdateMessage) -> Option<bool> {
    match (is_terminal(current), is_terminal(incoming)) {
        (true, false) => return Some(false),
        (false, true) => return Some(true),
        _ => {}
    }
    match (current.Data.TradedQty, incoming.Data.TradedQty) {
        (Some(left), Some(right)) if left != right => return Some(right > left),
        _ => {}
    }
    let current_time = current
        .Data
        .LastUpdatedTime
        .as_ref()
        .or(current.Data.ExchOrderTime.as_ref());
    let incoming_time = incoming
        .Data
        .LastUpdatedTime
        .as_ref()
        .or(incoming.Data.ExchOrderTime.as_ref());
    match (current_time, incoming_time) {
        (Some(left), Some(right)) if left != right => Some(right > left),
        _ if update_fingerprint(current) == update_fingerprint(incoming) => Some(false),
        _ => None,
    }
}

fn merged_updates(
    snapshots: Vec<OrderUpdateMessage>,
    buffered: Vec<OrderUpdateMessage>,
) -> (Vec<OrderUpdateMessage>, Vec<Option<String>>) {
    let mut keyed: HashMap<String, OrderUpdateMessage> = HashMap::new();
    let mut unkeyed = Vec::new();
    let mut warnings = Vec::new();
    for update in snapshots.into_iter().chain(buffered) {
        if !is_valid_application_update(&update) {
            warnings.push(update_order_id(&update));
            continue;
        }
        let Some(order_id) = update_order_id(&update) else {
            unkeyed.push(update);
            continue;
        };
        match keyed.get(&order_id) {
            None => {
                keyed.insert(order_id, update);
            }
            Some(current) => match prefer_incoming(current, &update) {
                Some(true) => {
                    keyed.insert(order_id, update);
                }
                Some(false) => {}
                None => warnings.push(Some(order_id)),
            },
        }
    }
    let mut result: Vec<_> = keyed.into_values().collect();
    result.extend(unkeyed);
    (result, warnings)
}

fn emit_update_if_new(
    update: OrderUpdateMessage,
    last_delivered: &mut HashMap<String, UpdateFingerprint>,
    event_tx: &broadcast::Sender<ManagedOrderUpdateEvent>,
) {
    if let Some(order_id) = update_order_id(&update) {
        let fingerprint = update_fingerprint(&update);
        if last_delivered.get(&order_id) == Some(&fingerprint) {
            return;
        }
        last_delivered.insert(order_id, fingerprint);
    }
    let _ = event_tx.send(ManagedOrderUpdateEvent::Update(update));
}

enum AttemptEnd {
    Shutdown,
    Disconnected {
        close: Option<OrderUpdateClose>,
        reason: String,
        auth_blocked: bool,
        stable_traffic: bool,
    },
}

fn had_stable_traffic(healthy_since: Option<Instant>, required: Duration) -> bool {
    healthy_since.is_some_and(|started| started.elapsed() >= required)
}

#[allow(clippy::too_many_arguments)]
async fn run_connection(
    mut ws: WsStream,
    credential: &OrderUpdateCredentialSnapshot,
    config: &ManagedOrderUpdateConfig,
    reconciler: Option<Arc<dyn OrderUpdateReconciler>>,
    pending_gap: bool,
    known_clients: &mut HashSet<String>,
    last_delivered: &mut HashMap<String, UpdateFingerprint>,
    event_tx: &broadcast::Sender<ManagedOrderUpdateEvent>,
    state_tx: &watch::Sender<ManagedOrderUpdateState>,
    command_rx: &mut mpsc::Receiver<SupervisorCommand>,
    credential_rx: &mut watch::Receiver<OrderUpdateCredentialSnapshot>,
    lag_rx: &mut watch::Receiver<ConsumerLagSignal>,
    observed_lag_total: &mut u64,
) -> AttemptEnd {
    let mut healthy_since = None;
    transition(
        state_tx,
        event_tx,
        ManagedOrderUpdateState::TransportConnected,
    );
    transition(state_tx, event_tx, ManagedOrderUpdateState::Authorizing);
    let auth_json = match credential.auth_json() {
        Ok(json) => json,
        Err(error) => {
            return AttemptEnd::Disconnected {
                close: None,
                reason: error.to_string(),
                auth_blocked: true,
                stable_traffic: false,
            };
        }
    };
    match timeout(
        config.write_timeout,
        ws.send(Message::Text(auth_json.into())),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return AttemptEnd::Disconnected {
                close: None,
                reason: format!("authorization write failed: {error}"),
                auth_blocked: false,
                stable_traffic: false,
            };
        }
        Err(_) => {
            return AttemptEnd::Disconnected {
                close: None,
                reason: "authorization write timed out".to_owned(),
                auth_blocked: false,
                stable_traffic: false,
            };
        }
    }
    transition(
        state_tx,
        event_tx,
        ManagedOrderUpdateState::ReadinessPending,
    );

    let mut reconcile_task: Option<JoinHandle<Vec<ReconciliationOutcome>>> = None;
    let mut reconcile_buffer = Vec::new();
    let mut partner_unknown_unresolved = false;
    let mut has_unresolved_gap = pending_gap;
    let mut readiness_confirmed = false;
    let mut readiness_reported = false;
    let readiness = tokio::time::sleep(config.readiness_timeout);
    tokio::pin!(readiness);
    let inactivity = tokio::time::sleep(config.inactivity_timeout);
    tokio::pin!(inactivity);
    if pending_gap {
        start_reconciliation(
            credential,
            config,
            reconciler.clone(),
            known_clients,
            event_tx,
            state_tx,
            &mut reconcile_task,
            &mut reconcile_buffer,
            &mut partner_unknown_unresolved,
        );
    }

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(SupervisorCommand::Shutdown) | None => {
                        transition(state_tx, event_tx, ManagedOrderUpdateState::Closing);
                        let _ = timeout(config.write_timeout, ws.send(Message::Close(None))).await;
                        let close_wait = async {
                            while let Some(message) = ws.next().await {
                                if matches!(message, Ok(Message::Close(_))) {
                                    break;
                                }
                            }
                        };
                        let _ = timeout(config.close_timeout, close_wait).await;
                        if let Some(task) = reconcile_task.take() {
                            task.abort();
                        }
                        return AttemptEnd::Shutdown;
                    }
                }
            }
            changed = lag_rx.changed(), if lag_rx.has_changed().is_ok() => {
                if changed.is_ok() {
                    let dropped = consume_lag_signal(lag_rx, observed_lag_total);
                    if dropped > 0 {
                        has_unresolved_gap = true;
                        emit_gap(state_tx, event_tx, OrderUpdateGapCause::ConsumerLag { dropped });
                        start_reconciliation(
                            credential,
                            config,
                            reconciler.clone(),
                            known_clients,
                            event_tx,
                            state_tx,
                            &mut reconcile_task,
                            &mut reconcile_buffer,
                            &mut partner_unknown_unresolved,
                        );
                    }
                }
            }
            changed = credential_rx.changed(), if credential_rx.has_changed().is_ok() => {
                if changed.is_ok() {
                    credential_rx.borrow_and_update();
                }
            }
            _ = &mut readiness, if !readiness_confirmed && !readiness_reported => {
                readiness_reported = true;
                if !has_unresolved_gap && reconcile_task.is_none() {
                    transition(
                        state_tx,
                        event_tx,
                        ManagedOrderUpdateState::ReadinessUnconfirmed {
                            waited: config.readiness_timeout,
                        },
                    );
                }
                let _ = event_tx.send(ManagedOrderUpdateEvent::ReadinessTimeout {
                    waited: config.readiness_timeout,
                });
            }
            _ = &mut inactivity => {
                return AttemptEnd::Disconnected {
                    close: None,
                    reason: "order-update inactivity deadline exceeded".to_owned(),
                    auth_blocked: false,
                    stable_traffic: had_stable_traffic(
                        healthy_since,
                        config.stable_connection_period,
                    ),
                };
            }
            outcome = async {
                match reconcile_task.as_mut() {
                    Some(task) => task.await.ok(),
                    None => None,
                }
            }, if reconcile_task.is_some() => {
                reconcile_task = None;
                let mut snapshots = Vec::new();
                let mut reconciled_clients = Vec::new();
                let mut unresolved = partner_unknown_unresolved;
                if let Some(outcomes) = outcome {
                    for outcome in outcomes {
                        match outcome.result {
                            Ok(updates) => {
                                snapshots.extend(updates);
                                reconciled_clients.push(outcome.client_id);
                            }
                            Err(error) => {
                                unresolved = true;
                                let _ = event_tx.send(ManagedOrderUpdateEvent::GapUnresolved {
                                    client_id: Some(outcome.client_id),
                                    reason: error.to_string(),
                                });
                            }
                        }
                    }
                } else {
                    unresolved = true;
                    let _ = event_tx.send(ManagedOrderUpdateEvent::GapUnresolved {
                        client_id: None,
                        reason: "reconciliation task stopped before producing a snapshot".to_owned(),
                    });
                }
                let (updates, warnings) = merged_updates(snapshots, std::mem::take(&mut reconcile_buffer));
                for warning in warnings {
                    let _ = event_tx.send(ManagedOrderUpdateEvent::ReconciliationWarning { order_id: warning });
                }
                for update in updates {
                    emit_update_if_new(update, last_delivered, event_tx);
                }
                if !reconciled_clients.is_empty() {
                    reconciled_clients.sort();
                    let _ = event_tx.send(ManagedOrderUpdateEvent::Reconciled { client_ids: reconciled_clients });
                }
                has_unresolved_gap = unresolved;
                if unresolved {
                    transition(state_tx, event_tx, ManagedOrderUpdateState::Degraded);
                } else if readiness_confirmed {
                    transition(state_tx, event_tx, ManagedOrderUpdateState::Live);
                } else if readiness_reported {
                    transition(
                        state_tx,
                        event_tx,
                        ManagedOrderUpdateState::ReadinessUnconfirmed {
                            waited: config.readiness_timeout,
                        },
                    );
                } else {
                    transition(state_tx, event_tx, ManagedOrderUpdateState::ReadinessPending);
                }
            }
            incoming = ws.next() => {
                let Some(incoming) = incoming else {
                    return AttemptEnd::Disconnected {
                        close: None,
                        reason: "order-update transport reached EOF".to_owned(),
                        auth_blocked: false,
                        stable_traffic: had_stable_traffic(
                            healthy_since,
                            config.stable_connection_period,
                        ),
                    };
                };
                let message = match incoming {
                    Ok(message) => {
                        inactivity.as_mut().reset(Instant::now() + config.inactivity_timeout);
                        message
                    },
                    Err(error) => {
                        return AttemptEnd::Disconnected {
                            close: None,
                            reason: format!("order-update transport failed: {error}"),
                            auth_blocked: false,
                            stable_traffic: had_stable_traffic(
                                healthy_since,
                                config.stable_connection_period,
                            ),
                        };
                    }
                };
                let claimed_order_alert = frame_claims_order_alert(&message);
                let event = match classify_frame(message) {
                    Ok(Some(event)) => event,
                    Ok(None) => continue,
                    Err(error) => {
                        tracing::warn!(error = %error, "Rejected malformed order-update frame; payload omitted");
                        let _ = event_tx.send(ManagedOrderUpdateEvent::Error(OrderUpdateControlMessage {
                            kind: OrderUpdateControlKind::Error,
                            code: None,
                            message_type: Some("protocol_error".to_owned()),
                        }));
                        if claimed_order_alert {
                            has_unresolved_gap = true;
                            emit_gap(
                                state_tx,
                                event_tx,
                                OrderUpdateGapCause::MalformedApplicationFrame,
                            );
                            start_reconciliation(
                                credential,
                                config,
                                reconciler.clone(),
                                known_clients,
                                event_tx,
                                state_tx,
                                &mut reconcile_task,
                                &mut reconcile_buffer,
                                &mut partner_unknown_unresolved,
                            );
                        }
                        continue;
                    }
                };
                match event {
                    OrderUpdateProtocolEvent::Update(update) => {
                        readiness_confirmed = true;
                        healthy_since.get_or_insert_with(Instant::now);
                        if let Some(client_id) = update.Data.ClientId.clone() {
                            known_clients.insert(client_id);
                        }
                        if reconcile_task.is_some() {
                            if reconcile_buffer.len() == config.reconciliation_buffer_capacity {
                                has_unresolved_gap = true;
                                emit_gap(state_tx, event_tx, OrderUpdateGapCause::ReconciliationBufferOverflow);
                                start_reconciliation(
                                    credential,
                                    config,
                                    reconciler.clone(),
                                    known_clients,
                                    event_tx,
                                    state_tx,
                                    &mut reconcile_task,
                                    &mut reconcile_buffer,
                                    &mut partner_unknown_unresolved,
                                );
                            }
                            reconcile_buffer.push(update);
                        } else {
                            emit_update_if_new(update, last_delivered, event_tx);
                            if has_unresolved_gap {
                                transition(state_tx, event_tx, ManagedOrderUpdateState::Degraded);
                            } else {
                                transition(state_tx, event_tx, ManagedOrderUpdateState::Live);
                            }
                        }
                    }
                    OrderUpdateProtocolEvent::Authorization(control) => {
                        let _ = event_tx.send(ManagedOrderUpdateEvent::Authorization(control));
                    }
                    OrderUpdateProtocolEvent::Control(control) => {
                        let _ = event_tx.send(ManagedOrderUpdateEvent::Control(control));
                    }
                    OrderUpdateProtocolEvent::Error(control) => {
                        let auth_blocked = matches!(control.code, Some(401 | 403 | 807 | 808 | 809));
                        let _ = event_tx.send(ManagedOrderUpdateEvent::Error(control));
                        if auth_blocked {
                            return AttemptEnd::Disconnected {
                                close: None,
                                reason: "broker rejected the current credential version".to_owned(),
                                auth_blocked: true,
                                stable_traffic: had_stable_traffic(
                                    healthy_since,
                                    config.stable_connection_period,
                                ),
                            };
                        }
                    }
                    OrderUpdateProtocolEvent::Close(close) => {
                        return AttemptEnd::Disconnected {
                            close: Some(close),
                            reason: "peer closed order-update transport".to_owned(),
                            auth_blocked: false,
                            stable_traffic: had_stable_traffic(
                                healthy_since,
                                config.stable_connection_period,
                            ),
                        };
                    }
                }
            }
        }
    }
}

async fn run_supervisor(
    config: ManagedOrderUpdateConfig,
    mut credential_rx: watch::Receiver<OrderUpdateCredentialSnapshot>,
    reconciler: Option<Arc<dyn OrderUpdateReconciler>>,
    event_tx: broadcast::Sender<ManagedOrderUpdateEvent>,
    state_tx: watch::Sender<ManagedOrderUpdateState>,
    mut command_rx: mpsc::Receiver<SupervisorCommand>,
    mut lag_rx: watch::Receiver<ConsumerLagSignal>,
) {
    let mut attempt = 0_u64;
    let mut pending_gap = false;
    let mut known_clients = HashSet::new();
    let mut last_delivered = HashMap::new();
    let mut blocked_version = None;
    let mut observed_lag_total = 0_u64;

    'supervisor: loop {
        if let Some(version) = blocked_version {
            transition(
                &state_tx,
                &event_tx,
                ManagedOrderUpdateState::AuthBlocked {
                    credential_version: version,
                },
            );
            loop {
                if credential_rx.borrow().version() > version {
                    blocked_version = None;
                    attempt = 0;
                    break;
                }
                tokio::select! {
                    command = command_rx.recv() => match command {
                        Some(SupervisorCommand::Shutdown) | None => break 'supervisor,
                    },
                    changed = lag_rx.changed(), if lag_rx.has_changed().is_ok() => {
                        if changed.is_ok() {
                            let dropped = consume_lag_signal(&mut lag_rx, &mut observed_lag_total);
                            if dropped == 0 {
                                continue;
                            }
                            pending_gap = true;
                            emit_gap(&state_tx, &event_tx, OrderUpdateGapCause::ConsumerLag { dropped });
                        }
                    }
                    changed = credential_rx.changed(), if credential_rx.has_changed().is_ok() => {
                        if changed.is_err() {
                            break 'supervisor;
                        }
                        credential_rx.borrow_and_update();
                    }
                }
            }
        }

        attempt = attempt.saturating_add(1);
        transition(
            &state_tx,
            &event_tx,
            ManagedOrderUpdateState::Connecting { attempt },
        );
        let credential = credential_rx.borrow().clone();
        let connected = tokio::select! {
            command = command_rx.recv() => match command {
                Some(SupervisorCommand::Shutdown) | None => break 'supervisor,
            },
            changed = lag_rx.changed(), if lag_rx.has_changed().is_ok() => {
                if changed.is_ok() {
                    let dropped = consume_lag_signal(&mut lag_rx, &mut observed_lag_total);
                    if dropped == 0 {
                        continue;
                    }
                    pending_gap = true;
                    emit_gap(&state_tx, &event_tx, OrderUpdateGapCause::ConsumerLag { dropped });
                    continue;
                }
                continue;
            }
            result = timeout(config.connect_timeout, connect_async(&config.endpoint)) => result,
        };

        let ws = match connected {
            Ok(Ok((ws, _response))) => ws,
            Ok(Err(error)) => {
                tracing::warn!(attempt, error = %error, "Order-update connection attempt failed");
                let _ = event_tx.send(ManagedOrderUpdateEvent::Disconnect {
                    close: None,
                    reason: format!("connection attempt failed: {error}"),
                });
                let ceiling = retry_ceiling(&config, attempt);
                let delay = full_jitter(ceiling);
                transition(
                    &state_tx,
                    &event_tx,
                    ManagedOrderUpdateState::Backoff { attempt, delay },
                );
                if wait_backoff(
                    delay,
                    &mut command_rx,
                    &mut credential_rx,
                    &state_tx,
                    &event_tx,
                    &mut pending_gap,
                    &mut lag_rx,
                    &mut observed_lag_total,
                )
                .await
                {
                    break;
                }
                continue;
            }
            Err(_) => {
                let _ = event_tx.send(ManagedOrderUpdateEvent::Disconnect {
                    close: None,
                    reason: "connection attempt timed out".to_owned(),
                });
                let ceiling = retry_ceiling(&config, attempt);
                let delay = full_jitter(ceiling);
                transition(
                    &state_tx,
                    &event_tx,
                    ManagedOrderUpdateState::Backoff { attempt, delay },
                );
                if wait_backoff(
                    delay,
                    &mut command_rx,
                    &mut credential_rx,
                    &state_tx,
                    &event_tx,
                    &mut pending_gap,
                    &mut lag_rx,
                    &mut observed_lag_total,
                )
                .await
                {
                    break;
                }
                continue;
            }
        };

        match run_connection(
            ws,
            &credential,
            &config,
            reconciler.clone(),
            pending_gap,
            &mut known_clients,
            &mut last_delivered,
            &event_tx,
            &state_tx,
            &mut command_rx,
            &mut credential_rx,
            &mut lag_rx,
            &mut observed_lag_total,
        )
        .await
        {
            AttemptEnd::Shutdown => break,
            AttemptEnd::Disconnected {
                close,
                reason,
                auth_blocked,
                stable_traffic,
            } => {
                let _ = event_tx.send(ManagedOrderUpdateEvent::Disconnect { close, reason });
                pending_gap = true;
                emit_gap(
                    &state_tx,
                    &event_tx,
                    OrderUpdateGapCause::UncertainDisconnect,
                );
                if auth_blocked {
                    blocked_version = Some(credential.version());
                    continue;
                }
                let retry_attempt = if stable_traffic { 1 } else { attempt };
                let ceiling = retry_ceiling(&config, retry_attempt);
                let delay = full_jitter(ceiling);
                transition(
                    &state_tx,
                    &event_tx,
                    ManagedOrderUpdateState::Backoff {
                        attempt: retry_attempt,
                        delay,
                    },
                );
                if stable_traffic {
                    attempt = 0;
                }
                if wait_backoff(
                    delay,
                    &mut command_rx,
                    &mut credential_rx,
                    &state_tx,
                    &event_tx,
                    &mut pending_gap,
                    &mut lag_rx,
                    &mut observed_lag_total,
                )
                .await
                {
                    break;
                }
            }
        }
    }

    transition(&state_tx, &event_tx, ManagedOrderUpdateState::Stopped);
    let _ = event_tx.send(ManagedOrderUpdateEvent::Stopped);
}

#[allow(clippy::too_many_arguments)]
async fn wait_backoff(
    delay: Duration,
    command_rx: &mut mpsc::Receiver<SupervisorCommand>,
    credential_rx: &mut watch::Receiver<OrderUpdateCredentialSnapshot>,
    state_tx: &watch::Sender<ManagedOrderUpdateState>,
    event_tx: &broadcast::Sender<ManagedOrderUpdateEvent>,
    pending_gap: &mut bool,
    lag_rx: &mut watch::Receiver<ConsumerLagSignal>,
    observed_lag_total: &mut u64,
) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = credential_rx.changed(), if credential_rx.has_changed().is_ok() => {
            if changed.is_ok() {
                credential_rx.borrow_and_update();
            }
            false
        }
        command = command_rx.recv() => match command {
            Some(SupervisorCommand::Shutdown) | None => true,
        },
        changed = lag_rx.changed(), if lag_rx.has_changed().is_ok() => {
            if changed.is_ok() {
                let dropped = consume_lag_signal(lag_rx, observed_lag_total);
                if dropped == 0 {
                    return false;
                }
                *pending_gap = true;
                emit_gap(state_tx, event_tx, OrderUpdateGapCause::ConsumerLag { dropped });
            }
            false
        }
    }
}
