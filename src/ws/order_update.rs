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
//!         Ok(update) => println!("Order {} → {}", update.data.order_no, update.data.status),
//!         Err(e) => eprintln!("Error: {e}"),
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::constants::WS_ORDER_UPDATE_URL;
use crate::error::{DhanError, Result};

// ---------------------------------------------------------------------------
// Auth messages
// ---------------------------------------------------------------------------

/// Login request payload for individual users.
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct LoginRequest {
    MsgCode: u8,
    ClientId: String,
    Token: String,
}

/// Auth message envelope for individual users.
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct IndividualAuthMessage {
    LoginReq: LoginRequest,
    UserType: String,
}

/// Login request payload for partner users (no Token field).
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct PartnerLoginRequest {
    MsgCode: u8,
    ClientId: String,
}

/// Auth message envelope for partner users.
#[derive(Debug, Serialize)]
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
#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case)]
pub struct OrderUpdateMessage {
    /// Message type — typically `"order_alert"`.
    pub Type: String,
    /// The order update payload.
    pub Data: OrderUpdateData,
}

/// Detailed order update data received via WebSocket.
///
/// Field names are PascalCase matching the wire format. Abbreviated product /
/// transaction / order-type codes are used (e.g. `"C"` for CNC, `"B"` for Buy,
/// `"LMT"` for Limit).
#[derive(Debug, Clone, Deserialize)]
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
    #[serde(default)]
    pub AlgoOrdNo: Option<serde_json::Value>,
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
    #[serde(default)]
    pub StrikePrice: Option<serde_json::Value>,
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
    #[serde(default)]
    pub multiplier: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Stream wrapper
// ---------------------------------------------------------------------------

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A streaming connection for receiving live order updates.
///
/// Implements [`Stream<Item = Result<OrderUpdateMessage>>`] so you can use it
/// with `StreamExt::next()` and other stream combinators.
pub struct OrderUpdateStream {
    read: SplitStream<WsStream>,
    _write: SplitSink<WsStream, Message>,
}

impl OrderUpdateStream {
    /// Connect to the order-update WebSocket as an individual user.
    ///
    /// Sends the authentication message immediately after connection is
    /// established.
    pub async fn connect(client_id: &str, access_token: &str) -> Result<Self> {
        let (ws, _resp) = connect_async(WS_ORDER_UPDATE_URL).await?;

        let (mut write, read) = ws.split();

        // Send individual auth message
        let auth = IndividualAuthMessage {
            LoginReq: LoginRequest {
                MsgCode: 42,
                ClientId: client_id.to_owned(),
                Token: access_token.to_owned(),
            },
            UserType: "SELF".to_owned(),
        };
        let auth_json = serde_json::to_string(&auth)?;
        write.send(Message::Text(auth_json.into())).await?;

        tracing::info!("Connected to order-update WebSocket");

        Ok(Self {
            read,
            _write: write,
        })
    }

    /// Connect to the order-update WebSocket as a partner.
    ///
    /// Partner platforms receive order updates for all connected users.
    pub async fn connect_partner(partner_id: &str, partner_secret: &str) -> Result<Self> {
        let (ws, _resp) = connect_async(WS_ORDER_UPDATE_URL).await?;

        let (mut write, read) = ws.split();

        let auth = PartnerAuthMessage {
            LoginReq: PartnerLoginRequest {
                MsgCode: 42,
                ClientId: partner_id.to_owned(),
            },
            UserType: "PARTNER".to_owned(),
            Secret: partner_secret.to_owned(),
        };
        let auth_json = serde_json::to_string(&auth)?;
        write.send(Message::Text(auth_json.into())).await?;

        tracing::info!("Connected to order-update WebSocket (partner mode)");

        Ok(Self {
            read,
            _write: write,
        })
    }

    /// Close the WebSocket connection gracefully.
    pub async fn close(mut self) -> Result<()> {
        self._write.send(Message::Close(None)).await?;
        Ok(())
    }
}

impl Stream for OrderUpdateStream {
    type Item = Result<OrderUpdateMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.read.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(msg))) => {
                    match msg {
                        Message::Text(text) => {
                            match serde_json::from_str::<OrderUpdateMessage>(&text) {
                                Ok(update) => return Poll::Ready(Some(Ok(update))),
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to parse order update: {e}, raw: {text}"
                                    );
                                    return Poll::Ready(Some(Err(DhanError::Json(e))));
                                }
                            }
                        }
                        Message::Ping(_) | Message::Pong(_) => {
                            // Ping/pong handled automatically by tungstenite
                            continue;
                        }
                        Message::Close(_) => {
                            tracing::info!("Order-update WebSocket closed by server");
                            return Poll::Ready(None);
                        }
                        _ => continue,
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(DhanError::WebSocket(e))));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
