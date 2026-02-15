#![allow(missing_docs)]
//! Live Market Feed WebSocket client.
//!
//! Connects to `wss://api-feed.dhan.co` and streams real-time market data as
//! binary packets. Supports Ticker, Quote, and Full data modes.
//!
//! # Example
//!
//! ```no_run
//! use dhan_rs::ws::market_feed::{MarketFeedStream, Instrument};
//! use dhan_rs::types::enums::FeedRequestCode;
//! use futures_util::StreamExt;
//!
//! # #[tokio::main]
//! # async fn main() -> dhan_rs::error::Result<()> {
//! let mut stream = MarketFeedStream::connect("1000000001", "your-jwt-token").await?;
//!
//! // Subscribe to ticker data for HDFC Bank on NSE
//! let instruments = vec![
//!     Instrument { exchange_segment: "NSE_EQ".into(), security_id: "1333".into() },
//! ];
//! stream.subscribe(FeedRequestCode::SubscribeTicker, &instruments).await?;
//!
//! while let Some(event) = stream.next().await {
//!     match event {
//!         Ok(e) => println!("{e:?}"),
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
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::constants::WS_MARKET_FEED_URL;
use crate::error::{DhanError, Result};
use crate::types::enums::{ExchangeSegment, FeedRequestCode, FeedResponseCode};

// ---------------------------------------------------------------------------
// Subscribe / Unsubscribe request types
// ---------------------------------------------------------------------------

/// An instrument to subscribe to in the market feed.
#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct Instrument {
    /// Exchange segment (e.g. `"NSE_EQ"`, `"NSE_FNO"`).
    pub ExchangeSegment: String,
    /// Exchange standard security ID.
    pub SecurityId: String,
}

/// Convenience constructor for [`Instrument`].
impl Instrument {
    /// Create a new instrument subscription entry.
    pub fn new(exchange_segment: impl Into<String>, security_id: impl Into<String>) -> Self {
        Self {
            ExchangeSegment: exchange_segment.into(),
            SecurityId: security_id.into(),
        }
    }
}

/// JSON subscribe/unsubscribe request sent over the WebSocket.
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct FeedSubscribeRequest {
    RequestCode: u8,
    InstrumentCount: usize,
    InstrumentList: Vec<Instrument>,
}

/// JSON disconnect request.
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct FeedDisconnectRequest {
    RequestCode: u8,
}

// ---------------------------------------------------------------------------
// Parsed binary response header
// ---------------------------------------------------------------------------

/// Header parsed from the first 8 bytes of every binary market feed packet.
#[derive(Debug, Clone, Copy)]
pub struct PacketHeader {
    /// The response code identifying the packet type.
    pub response_code: FeedResponseCode,
    /// Total message length in bytes (including header).
    pub message_length: u16,
    /// Exchange segment the data belongs to.
    pub exchange_segment: Option<ExchangeSegment>,
    /// Raw exchange segment byte (always available even if enum variant unknown).
    pub exchange_segment_raw: u8,
    /// Security ID of the instrument.
    pub security_id: u32,
}

// ---------------------------------------------------------------------------
// Parsed market data events
// ---------------------------------------------------------------------------

/// A parsed market feed event.
#[derive(Debug, Clone)]
pub enum MarketFeedEvent {
    /// Ticker data (LTP + LTT). Response code 2.
    Ticker {
        header: PacketHeader,
        /// Last traded price.
        ltp: f32,
        /// Last trade time (epoch seconds).
        ltt: i32,
    },

    /// Previous close data. Response code 6.
    /// Sent once when an instrument is first subscribed.
    PrevClose {
        header: PacketHeader,
        /// Previous day closing price.
        prev_close: f32,
        /// Previous day open interest.
        prev_oi: i32,
    },

    /// Quote data with OHLC, volume, etc. Response code 4.
    Quote {
        header: PacketHeader,
        /// Last traded price.
        ltp: f32,
        /// Last traded quantity.
        last_qty: i16,
        /// Last trade time (epoch seconds).
        ltt: i32,
        /// Average trade price.
        atp: f32,
        /// Total traded volume for the day.
        volume: i32,
        /// Total sell quantity pending.
        total_sell_qty: i32,
        /// Total buy quantity pending.
        total_buy_qty: i32,
        /// Day open price.
        open: f32,
        /// Day close price (only after market close).
        close: f32,
        /// Day high price.
        high: f32,
        /// Day low price.
        low: f32,
    },

    /// Open Interest data. Response code 5.
    /// Sent alongside Quote subscriptions for derivatives.
    OI {
        header: PacketHeader,
        /// Current open interest.
        oi: i32,
    },

    /// Full data packet including quote + OI + market depth. Response code 8.
    Full {
        header: PacketHeader,
        /// Last traded price.
        ltp: f32,
        /// Last traded quantity.
        last_qty: i16,
        /// Last trade time (epoch seconds).
        ltt: i32,
        /// Average trade price.
        atp: f32,
        /// Total traded volume.
        volume: i32,
        /// Total sell quantity pending.
        total_sell_qty: i32,
        /// Total buy quantity pending.
        total_buy_qty: i32,
        /// Open interest.
        oi: i32,
        /// Day high OI (NSE_FNO only).
        oi_day_high: i32,
        /// Day low OI (NSE_FNO only).
        oi_day_low: i32,
        /// Day open price.
        open: f32,
        /// Day close price.
        close: f32,
        /// Day high price.
        high: f32,
        /// Day low price.
        low: f32,
        /// 5 levels of market depth (stack-allocated, no heap alloc).
        depth: [DepthLevel; 5],
    },

    /// Market status packet. Response code 7.
    MarketStatus {
        header: PacketHeader,
        /// Raw payload bytes (structure not documented in detail).
        raw: Vec<u8>,
    },

    /// Index packet. Response code 1.
    Index {
        header: PacketHeader,
        /// Raw payload bytes.
        raw: Vec<u8>,
    },

    /// Server-initiated disconnect. Response code 50.
    Disconnect {
        header: PacketHeader,
        /// Disconnect reason code (e.g. 805 = too many connections).
        reason_code: i16,
    },
}

/// A single level of market depth (bid or ask side) from a Full packet.
#[derive(Debug, Clone, Copy)]
pub struct DepthLevel {
    /// Bid (buy) quantity.
    pub bid_qty: i32,
    /// Ask (sell) quantity.
    pub ask_qty: i32,
    /// Number of bid orders.
    pub bid_orders: i16,
    /// Number of ask orders.
    pub ask_orders: i16,
    /// Bid price.
    pub bid_price: f32,
    /// Ask price.
    pub ask_price: f32,
}

// ---------------------------------------------------------------------------
// Binary packet parser — zero-copy with native `from_le_bytes()`
// ---------------------------------------------------------------------------

/// Read a `u8` from `data` at `offset`. Advances `offset` by 1.
#[inline(always)]
fn read_u8(data: &[u8], offset: &mut usize) -> u8 {
    let v = data[*offset];
    *offset += 1;
    v
}

/// Read a little-endian `u16` from `data` at `offset`. Advances `offset` by 2.
#[inline(always)]
fn read_u16_le(data: &[u8], offset: &mut usize) -> u16 {
    let v = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    v
}

/// Read a little-endian `i16` from `data` at `offset`. Advances `offset` by 2.
#[inline(always)]
fn read_i16_le(data: &[u8], offset: &mut usize) -> i16 {
    let v = i16::from_le_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    v
}

/// Read a little-endian `i32` from `data` at `offset`. Advances `offset` by 4.
#[inline(always)]
fn read_i32_le(data: &[u8], offset: &mut usize) -> i32 {
    let v = i32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    v
}

/// Read a little-endian `u32` from `data` at `offset`. Advances `offset` by 4.
#[inline(always)]
fn read_u32_le(data: &[u8], offset: &mut usize) -> u32 {
    let v = u32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    v
}

/// Read a little-endian `f32` from `data` at `offset`. Advances `offset` by 4.
#[inline(always)]
fn read_f32_le(data: &[u8], offset: &mut usize) -> f32 {
    let v = f32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    v
}

/// Parse the 8-byte packet header.
fn parse_header(data: &[u8]) -> Result<PacketHeader> {
    if data.len() < 8 {
        return Err(DhanError::InvalidArgument(format!(
            "packet too short for header: {} bytes",
            data.len()
        )));
    }
    let mut off = 0usize;

    let response_code_byte = read_u8(data, &mut off);
    let response_code = FeedResponseCode::from_byte(response_code_byte).ok_or_else(|| {
        DhanError::InvalidArgument(format!("unknown feed response code: {response_code_byte}"))
    })?;

    let message_length = read_u16_le(data, &mut off);
    let exchange_segment_raw = read_u8(data, &mut off);
    let exchange_segment = ExchangeSegment::from_segment_code(exchange_segment_raw);
    let security_id = read_u32_le(data, &mut off);

    Ok(PacketHeader {
        response_code,
        message_length,
        exchange_segment,
        exchange_segment_raw,
        security_id,
    })
}

/// Parse a complete binary packet into a [`MarketFeedEvent`].
fn parse_packet(data: &[u8]) -> Result<MarketFeedEvent> {
    let header = parse_header(data)?;
    let payload = &data[8..];

    match header.response_code {
        FeedResponseCode::Ticker => {
            if payload.len() < 8 {
                return Err(DhanError::InvalidArgument(
                    "ticker packet payload too short".into(),
                ));
            }
            let mut off = 0;
            let ltp = read_f32_le(payload, &mut off);
            let ltt = read_i32_le(payload, &mut off);
            Ok(MarketFeedEvent::Ticker { header, ltp, ltt })
        }

        FeedResponseCode::PrevClose => {
            if payload.len() < 8 {
                return Err(DhanError::InvalidArgument(
                    "prev close packet payload too short".into(),
                ));
            }
            let mut off = 0;
            let prev_close = read_f32_le(payload, &mut off);
            let prev_oi = read_i32_le(payload, &mut off);
            Ok(MarketFeedEvent::PrevClose {
                header,
                prev_close,
                prev_oi,
            })
        }

        FeedResponseCode::Quote => {
            if payload.len() < 42 {
                return Err(DhanError::InvalidArgument(
                    "quote packet payload too short".into(),
                ));
            }
            let mut off = 0;
            let ltp = read_f32_le(payload, &mut off);
            let last_qty = read_i16_le(payload, &mut off);
            let ltt = read_i32_le(payload, &mut off);
            let atp = read_f32_le(payload, &mut off);
            let volume = read_i32_le(payload, &mut off);
            let total_sell_qty = read_i32_le(payload, &mut off);
            let total_buy_qty = read_i32_le(payload, &mut off);
            let open = read_f32_le(payload, &mut off);
            let close = read_f32_le(payload, &mut off);
            let high = read_f32_le(payload, &mut off);
            let low = read_f32_le(payload, &mut off);
            Ok(MarketFeedEvent::Quote {
                header,
                ltp,
                last_qty,
                ltt,
                atp,
                volume,
                total_sell_qty,
                total_buy_qty,
                open,
                close,
                high,
                low,
            })
        }

        FeedResponseCode::OI => {
            if payload.len() < 4 {
                return Err(DhanError::InvalidArgument(
                    "OI packet payload too short".into(),
                ));
            }
            let mut off = 0;
            let oi = read_i32_le(payload, &mut off);
            Ok(MarketFeedEvent::OI { header, oi })
        }

        FeedResponseCode::Full => {
            if payload.len() < 154 {
                return Err(DhanError::InvalidArgument(format!(
                    "full packet payload too short: {} bytes (need ≥ 154)",
                    payload.len()
                )));
            }
            let mut off = 0;

            let ltp = read_f32_le(payload, &mut off);
            let last_qty = read_i16_le(payload, &mut off);
            let ltt = read_i32_le(payload, &mut off);
            let atp = read_f32_le(payload, &mut off);
            let volume = read_i32_le(payload, &mut off);
            let total_sell_qty = read_i32_le(payload, &mut off);
            let total_buy_qty = read_i32_le(payload, &mut off);

            let oi = read_i32_le(payload, &mut off);
            let oi_day_high = read_i32_le(payload, &mut off);
            let oi_day_low = read_i32_le(payload, &mut off);

            let open = read_f32_le(payload, &mut off);
            let close = read_f32_le(payload, &mut off);
            let high = read_f32_le(payload, &mut off);
            let low = read_f32_le(payload, &mut off);

            // 5 depth levels × 20 bytes each — stack-allocated array
            let mut depth = [DepthLevel {
                bid_qty: 0,
                ask_qty: 0,
                bid_orders: 0,
                ask_orders: 0,
                bid_price: 0.0,
                ask_price: 0.0,
            }; 5];
            for level in &mut depth {
                level.bid_qty = read_i32_le(payload, &mut off);
                level.ask_qty = read_i32_le(payload, &mut off);
                level.bid_orders = read_i16_le(payload, &mut off);
                level.ask_orders = read_i16_le(payload, &mut off);
                level.bid_price = read_f32_le(payload, &mut off);
                level.ask_price = read_f32_le(payload, &mut off);
            }

            Ok(MarketFeedEvent::Full {
                header,
                ltp,
                last_qty,
                ltt,
                atp,
                volume,
                total_sell_qty,
                total_buy_qty,
                oi,
                oi_day_high,
                oi_day_low,
                open,
                close,
                high,
                low,
                depth,
            })
        }

        FeedResponseCode::Disconnect => {
            if payload.len() < 2 {
                return Err(DhanError::InvalidArgument(
                    "disconnect packet payload too short".into(),
                ));
            }
            let mut off = 0;
            let reason_code = read_i16_le(payload, &mut off);
            Ok(MarketFeedEvent::Disconnect {
                header,
                reason_code,
            })
        }

        FeedResponseCode::MarketStatus => Ok(MarketFeedEvent::MarketStatus {
            header,
            raw: payload.to_vec(),
        }),

        FeedResponseCode::Index => Ok(MarketFeedEvent::Index {
            header,
            raw: payload.to_vec(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Stream wrapper
// ---------------------------------------------------------------------------

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A streaming connection for receiving live market data.
///
/// Implements [`Stream<Item = Result<MarketFeedEvent>>`] so you can use it
/// with `StreamExt::next()` and other stream combinators.
///
/// Subscribe to instruments using [`subscribe()`](Self::subscribe) with the
/// desired [`FeedRequestCode`] mode after connecting.
pub struct MarketFeedStream {
    read: SplitStream<WsStream>,
    write: SplitSink<WsStream, Message>,
}

impl MarketFeedStream {
    /// Connect to the market feed WebSocket.
    ///
    /// Authentication is done via query parameters on the WebSocket URL.
    pub async fn connect(client_id: &str, access_token: &str) -> Result<Self> {
        let url = format!(
            "{WS_MARKET_FEED_URL}?version=2&token={access_token}&clientId={client_id}&authType=2"
        );

        let (ws, _resp) = connect_async(&url).await?;
        let (write, read) = ws.split();

        tracing::info!("Connected to market-feed WebSocket");

        Ok(Self { read, write })
    }

    /// Subscribe to instruments in the given data mode.
    ///
    /// Use [`FeedRequestCode::SubscribeTicker`], [`FeedRequestCode::SubscribeQuote`],
    /// or [`FeedRequestCode::SubscribeFull`] as the `mode`.
    ///
    /// A maximum of 100 instruments can be sent per message. For more, call
    /// this method multiple times.
    pub async fn subscribe(
        &mut self,
        mode: FeedRequestCode,
        instruments: &[Instrument],
    ) -> Result<()> {
        let req = FeedSubscribeRequest {
            RequestCode: mode as u8,
            InstrumentCount: instruments.len(),
            InstrumentList: instruments.to_vec(),
        };
        let json = serde_json::to_string(&req)?;
        self.write.send(Message::Text(json.into())).await?;

        tracing::debug!(
            mode = ?mode,
            count = instruments.len(),
            "Subscribed to instruments"
        );
        Ok(())
    }

    /// Unsubscribe from instruments in the given data mode.
    ///
    /// Use [`FeedRequestCode::UnsubscribeTicker`], [`FeedRequestCode::UnsubscribeQuote`],
    /// or [`FeedRequestCode::UnsubscribeFull`] as the `mode`.
    pub async fn unsubscribe(
        &mut self,
        mode: FeedRequestCode,
        instruments: &[Instrument],
    ) -> Result<()> {
        let req = FeedSubscribeRequest {
            RequestCode: mode as u8,
            InstrumentCount: instruments.len(),
            InstrumentList: instruments.to_vec(),
        };
        let json = serde_json::to_string(&req)?;
        self.write.send(Message::Text(json.into())).await?;

        tracing::debug!(
            mode = ?mode,
            count = instruments.len(),
            "Unsubscribed from instruments"
        );
        Ok(())
    }

    /// Send a disconnect request and close the WebSocket.
    pub async fn disconnect(mut self) -> Result<()> {
        let req = FeedDisconnectRequest { RequestCode: 12 };
        let json = serde_json::to_string(&req)?;
        self.write.send(Message::Text(json.into())).await?;
        self.write.send(Message::Close(None)).await?;

        tracing::info!("Disconnected from market-feed WebSocket");
        Ok(())
    }
}

impl Stream for MarketFeedStream {
    type Item = Result<MarketFeedEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.read.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(msg))) => {
                    match msg {
                        Message::Binary(data) => match parse_packet(&data) {
                            Ok(event) => return Poll::Ready(Some(Ok(event))),
                            Err(e) => {
                                tracing::warn!("Failed to parse market feed packet: {e}");
                                return Poll::Ready(Some(Err(e)));
                            }
                        },
                        Message::Ping(_) | Message::Pong(_) => {
                            // Ping/pong handled automatically by tungstenite
                            continue;
                        }
                        Message::Close(_) => {
                            tracing::info!("Market-feed WebSocket closed by server");
                            return Poll::Ready(None);
                        }
                        Message::Text(text) => {
                            tracing::debug!("Received text message on market feed: {text}");
                            continue;
                        }
                        _ => continue,
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(DhanError::WebSocket(Box::new(e)))));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
