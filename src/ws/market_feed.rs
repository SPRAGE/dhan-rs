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
//!     Instrument::new("NSE_EQ", "1333"),
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

use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, Stream, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use url::Url;

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
// Binary packet parser
// ---------------------------------------------------------------------------

const MAX_INSTRUMENTS_PER_REQUEST: usize = 100;
const MAX_UNIQUE_INSTRUMENTS: usize = 5_000;
const DISCONNECT_WAIT: Duration = Duration::from_secs(2);

fn invalid_packet(message: impl Into<String>) -> DhanError {
    DhanError::InvalidArgument(message.into())
}

fn exact_packet_length(response_code: FeedResponseCode) -> Option<usize> {
    match response_code {
        FeedResponseCode::Ticker | FeedResponseCode::PrevClose => Some(16),
        FeedResponseCode::Quote => Some(50),
        FeedResponseCode::OI => Some(12),
        FeedResponseCode::Full => Some(162),
        FeedResponseCode::Index => Some(32),
        FeedResponseCode::Disconnect => Some(10),
        // The published documentation names this packet but does not define
        // a payload schema or fixed wire length.
        FeedResponseCode::MarketStatus => None,
    }
}

// These helpers are only called after `parse_packet` has checked an exact
// packet length for every packet carrying structured fields.
fn read_u8(data: &[u8], offset: &mut usize) -> u8 {
    let value = data[*offset];
    *offset += 1;
    value
}

fn read_u16_le(data: &[u8], offset: &mut usize) -> u16 {
    let value = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    value
}

fn read_i16_le(data: &[u8], offset: &mut usize) -> i16 {
    let value = i16::from_le_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    value
}

fn read_i32_le(data: &[u8], offset: &mut usize) -> i32 {
    let value = i32::from_le_bytes(
        data[*offset..*offset + 4]
            .try_into()
            .expect("exact packet length"),
    );
    *offset += 4;
    value
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> u32 {
    let value = u32::from_le_bytes(
        data[*offset..*offset + 4]
            .try_into()
            .expect("header length checked"),
    );
    *offset += 4;
    value
}

fn read_f32_le(data: &[u8], offset: &mut usize) -> f32 {
    let value = f32::from_le_bytes(
        data[*offset..*offset + 4]
            .try_into()
            .expect("exact packet length"),
    );
    *offset += 4;
    value
}

/// Parse the 8-byte packet header from a raw binary market feed packet.
///
/// The header layout (little-endian):
///
/// | Offset | Size | Field |
/// |--------|------|------------------|
/// | 0      | 1    | Response code    |
/// | 1      | 2    | Message length   |
/// | 3      | 1    | Exchange segment |
/// | 4      | 4    | Security ID      |
pub fn parse_header(data: &[u8]) -> Result<PacketHeader> {
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
///
/// The input `data` should be a full binary WebSocket message as received
/// from DhanHQ, starting with the 8-byte packet header.
///
/// This is also useful for parsing raw frames obtained from
/// [`DhanFeedManager::get_raw_channel`](super::manager::DhanFeedManager::get_raw_channel).
pub fn parse_packet(data: &[u8]) -> Result<MarketFeedEvent> {
    let header = parse_header(data)?;
    if usize::from(header.message_length) != data.len() {
        return Err(invalid_packet(format!(
            "declared packet length {} does not match WebSocket binary message length {}",
            header.message_length,
            data.len()
        )));
    }
    if let Some(expected) = exact_packet_length(header.response_code) {
        if data.len() != expected {
            return Err(invalid_packet(format!(
                "response code {:?} requires an exact {expected}-byte packet, received {} bytes",
                header.response_code,
                data.len()
            )));
        }
    }
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
    subscriptions: HashMap<(String, String), u8>,
}

impl MarketFeedStream {
    /// Connect to the market feed WebSocket.
    ///
    /// Authentication is done via query parameters on the WebSocket URL.
    pub async fn connect(client_id: &str, access_token: &str) -> Result<Self> {
        Self::connect_to(WS_MARKET_FEED_URL, client_id, access_token).await
    }

    /// Connect to a compatible market-feed endpoint.
    ///
    /// This is primarily useful for loopback integration tests. Credentials
    /// are added with URL query encoding and never interpolated into a URL.
    pub async fn connect_to(endpoint: &str, client_id: &str, access_token: &str) -> Result<Self> {
        let url = market_feed_url(endpoint, client_id, access_token)?;

        let (ws, _resp) = connect_async(url.as_str()).await?;
        let (write, read) = ws.split();

        tracing::info!("Connected to market-feed WebSocket");

        Ok(Self {
            read,
            write,
            subscriptions: HashMap::new(),
        })
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
        let mode_bit = validate_mode(mode, true)?;
        validate_instruments(instruments)?;
        let new_instruments = instruments
            .iter()
            .filter(|instrument| !self.subscriptions.contains_key(&instrument_key(instrument)))
            .count();
        if self.subscriptions.len() + new_instruments > MAX_UNIQUE_INSTRUMENTS {
            return Err(DhanError::InvalidArgument(format!(
                "subscription would exceed the {MAX_UNIQUE_INSTRUMENTS} unique-instrument market-feed limit"
            )));
        }
        let req = FeedSubscribeRequest {
            RequestCode: mode as u8,
            InstrumentCount: instruments.len(),
            InstrumentList: instruments.to_vec(),
        };
        let json = serde_json::to_string(&req)?;
        self.write.send(Message::Text(json.into())).await?;
        for instrument in instruments {
            *self
                .subscriptions
                .entry(instrument_key(instrument))
                .or_default() |= mode_bit;
        }

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
        let mode_bit = validate_mode(mode, false)?;
        validate_instruments(instruments)?;
        let req = FeedSubscribeRequest {
            RequestCode: mode as u8,
            InstrumentCount: instruments.len(),
            InstrumentList: instruments.to_vec(),
        };
        let json = serde_json::to_string(&req)?;
        self.write.send(Message::Text(json.into())).await?;
        for instrument in instruments {
            let key = instrument_key(instrument);
            let remove = if let Some(modes) = self.subscriptions.get_mut(&key) {
                *modes &= !mode_bit;
                *modes == 0
            } else {
                false
            };
            if remove {
                self.subscriptions.remove(&key);
            }
        }

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

        let wait_for_close = async {
            while let Some(message) = self.read.next().await {
                match message {
                    Ok(Message::Close(_)) => return Ok(()),
                    Ok(_) => continue,
                    Err(error) => return Err(DhanError::WebSocket(Box::new(error))),
                }
            }
            Ok(())
        };
        tokio::time::timeout(DISCONNECT_WAIT, wait_for_close)
            .await
            .map_err(|_| {
                DhanError::InvalidArgument("market-feed close handshake timed out".into())
            })??;

        tracing::info!("Disconnected from market-feed WebSocket");
        Ok(())
    }
}

fn market_feed_url(endpoint: &str, client_id: &str, access_token: &str) -> Result<Url> {
    let mut url = Url::parse(endpoint)?;
    url.query_pairs_mut()
        .append_pair("version", "2")
        .append_pair("token", access_token)
        .append_pair("clientId", client_id)
        .append_pair("authType", "2");
    Ok(url)
}

fn instrument_key(instrument: &Instrument) -> (String, String) {
    (
        instrument.ExchangeSegment.clone(),
        instrument.SecurityId.clone(),
    )
}

fn validate_instruments(instruments: &[Instrument]) -> Result<()> {
    if instruments.is_empty() {
        return Err(DhanError::InvalidArgument(
            "market-feed request must contain at least one instrument".into(),
        ));
    }
    if instruments.len() > MAX_INSTRUMENTS_PER_REQUEST {
        return Err(DhanError::InvalidArgument(format!(
            "market-feed request may contain at most {MAX_INSTRUMENTS_PER_REQUEST} instruments"
        )));
    }
    let mut seen = std::collections::HashSet::with_capacity(instruments.len());
    for instrument in instruments {
        if instrument.ExchangeSegment.trim().is_empty() || instrument.SecurityId.trim().is_empty() {
            return Err(DhanError::InvalidArgument(
                "market-feed instruments require non-empty exchange segment and security ID".into(),
            ));
        }
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
                "unsupported standard-feed exchange segment: {}",
                instrument.ExchangeSegment
            )));
        }
        if instrument.SecurityId.parse::<u32>().is_err() {
            return Err(DhanError::InvalidArgument(
                "market-feed security ID must be an unsigned 32-bit integer".into(),
            ));
        }
        if !seen.insert(instrument_key(instrument)) {
            return Err(DhanError::InvalidArgument(
                "market-feed request contains a duplicate instrument".into(),
            ));
        }
    }
    Ok(())
}

fn validate_mode(mode: FeedRequestCode, subscribe: bool) -> Result<u8> {
    match (subscribe, mode) {
        (true, FeedRequestCode::SubscribeTicker) | (false, FeedRequestCode::UnsubscribeTicker) => Ok(1),
        (true, FeedRequestCode::SubscribeQuote) | (false, FeedRequestCode::UnsubscribeQuote) => Ok(2),
        (true, FeedRequestCode::SubscribeFull) | (false, FeedRequestCode::UnsubscribeFull) => Ok(4),
        (true, _) => Err(DhanError::InvalidArgument(
            "standard market feed subscribe accepts only ticker, quote, or full modes; full market depth uses its dedicated protocol".into(),
        )),
        (false, _) => Err(DhanError::InvalidArgument(
            "standard market feed unsubscribe accepts only ticker, quote, or full modes; full market depth uses its dedicated protocol".into(),
        )),
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

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    use super::*;

    fn packet(code: u8, payload: Vec<u8>) -> Vec<u8> {
        let length = u16::try_from(8 + payload.len()).expect("test packet fits u16");
        let mut data = vec![code];
        data.extend_from_slice(&length.to_le_bytes());
        data.push(1);
        data.extend_from_slice(&1333_u32.to_le_bytes());
        data.extend(payload);
        data
    }

    #[test]
    fn parses_golden_fixed_packets() {
        let ticker = packet(2, [123.5_f32.to_le_bytes(), 17_i32.to_le_bytes()].concat());
        assert!(
            matches!(parse_packet(&ticker), Ok(MarketFeedEvent::Ticker { ltp, ltt, .. }) if ltp == 123.5 && ltt == 17)
        );

        let prev_close = packet(6, [99.5_f32.to_le_bytes(), 8_i32.to_le_bytes()].concat());
        assert!(
            matches!(parse_packet(&prev_close), Ok(MarketFeedEvent::PrevClose { prev_close, prev_oi, .. }) if prev_close == 99.5 && prev_oi == 8)
        );

        let mut quote_payload = Vec::new();
        quote_payload.extend_from_slice(&1.0_f32.to_le_bytes());
        quote_payload.extend_from_slice(&2_i16.to_le_bytes());
        quote_payload.extend_from_slice(&3_i32.to_le_bytes());
        quote_payload.extend_from_slice(&4.0_f32.to_le_bytes());
        for value in [5_i32, 6, 7] {
            quote_payload.extend_from_slice(&value.to_le_bytes());
        }
        for value in [8.0_f32, 9.0, 10.0, 11.0] {
            quote_payload.extend_from_slice(&value.to_le_bytes());
        }
        assert!(
            matches!(parse_packet(&packet(4, quote_payload)), Ok(MarketFeedEvent::Quote { ltp, last_qty, low, .. }) if ltp == 1.0 && last_qty == 2 && low == 11.0)
        );

        assert!(
            matches!(parse_packet(&packet(5, 12_i32.to_le_bytes().to_vec())), Ok(MarketFeedEvent::OI { oi, .. }) if oi == 12)
        );

        let mut full_payload = Vec::new();
        full_payload.extend_from_slice(&1.0_f32.to_le_bytes());
        full_payload.extend_from_slice(&2_i16.to_le_bytes());
        full_payload.extend_from_slice(&3_i32.to_le_bytes());
        full_payload.extend_from_slice(&4.0_f32.to_le_bytes());
        for value in [5_i32, 6, 7, 8, 9, 10] {
            full_payload.extend_from_slice(&value.to_le_bytes());
        }
        for value in [11.0_f32, 12.0, 13.0, 14.0] {
            full_payload.extend_from_slice(&value.to_le_bytes());
        }
        for value in 0..5_i32 {
            full_payload.extend_from_slice(&(20 + value).to_le_bytes());
            full_payload.extend_from_slice(&(30 + value).to_le_bytes());
            full_payload.extend_from_slice(&(40_i16 + value as i16).to_le_bytes());
            full_payload.extend_from_slice(&(50_i16 + value as i16).to_le_bytes());
            full_payload.extend_from_slice(&(60.0_f32 + value as f32).to_le_bytes());
            full_payload.extend_from_slice(&(70.0_f32 + value as f32).to_le_bytes());
        }
        assert!(
            matches!(parse_packet(&packet(8, full_payload)), Ok(MarketFeedEvent::Full { depth, .. }) if depth[4].ask_price == 74.0)
        );

        assert!(
            matches!(parse_packet(&packet(1, vec![0; 24])), Ok(MarketFeedEvent::Index { raw, .. }) if raw.len() == 24)
        );
        assert!(
            matches!(parse_packet(&packet(50, 805_i16.to_le_bytes().to_vec())), Ok(MarketFeedEvent::Disconnect { reason_code, .. }) if reason_code == 805)
        );
    }

    #[test]
    fn rejects_length_mismatches_and_all_fixed_packet_boundaries() {
        for (code, length) in [
            (1, 32),
            (2, 16),
            (4, 50),
            (5, 12),
            (6, 16),
            (8, 162),
            (50, 10),
        ] {
            let valid = packet(code, vec![0; length - 8]);
            for boundary in 0..valid.len() {
                let parsed = std::panic::catch_unwind(|| parse_packet(&valid[..boundary]));
                assert!(
                    parsed.is_ok(),
                    "parser panicked for code {code} at {boundary}"
                );
                assert!(
                    parsed.unwrap().is_err(),
                    "truncation for code {code} at {boundary}"
                );
            }

            let mut wrong_declared = valid;
            wrong_declared[1..3].copy_from_slice(&u16::try_from(length - 1).unwrap().to_le_bytes());
            assert!(
                parse_packet(&wrong_declared).is_err(),
                "declared length for code {code}"
            );
        }
        assert!(parse_packet(&packet(2, vec![0; 9])).is_err());
        assert!(parse_packet(&[2, 8, 0, 1, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn validates_standard_modes_and_requests() {
        assert!(validate_mode(FeedRequestCode::SubscribeTicker, true).is_ok());
        assert!(validate_mode(FeedRequestCode::UnsubscribeFull, false).is_ok());
        assert!(validate_mode(FeedRequestCode::UnsubscribeTicker, true).is_err());
        assert!(validate_mode(FeedRequestCode::SubscribeFullMarketDepth, true).is_err());
        assert!(validate_mode(FeedRequestCode::UnsubscribeFullMarketDepth, false).is_err());
        assert!(validate_mode(FeedRequestCode::Connect, true).is_err());

        assert!(validate_instruments(&[]).is_err());
        assert!(validate_instruments(&[Instrument::new("NSE_EQ", "")]).is_err());
        assert!(validate_instruments(&[Instrument::new("NSE_COMM", "1")]).is_err());
        assert!(validate_instruments(&[Instrument::new("NSE_EQ", "not-a-number")]).is_err());
        assert!(
            validate_instruments(&[
                Instrument::new("NSE_EQ", "1"),
                Instrument::new("NSE_EQ", "1")
            ])
            .is_err()
        );
        let instruments = (0..101)
            .map(|i| Instrument::new("NSE_EQ", i.to_string()))
            .collect::<Vec<_>>();
        assert!(validate_instruments(&instruments).is_err());
    }

    #[test]
    fn safely_encodes_auth_query_parameters() {
        let url = market_feed_url("ws://127.0.0.1:9000/feed", "client & id", "token?&=").unwrap();
        let pairs = url.query_pairs().collect::<HashMap<_, _>>();
        assert_eq!(
            pairs.get("clientId").map(|value| value.as_ref()),
            Some("client & id")
        );
        assert_eq!(
            pairs.get("token").map(|value| value.as_ref()),
            Some("token?&=")
        );
    }

    #[tokio::test]
    async fn disconnect_waits_past_ping_for_peer_close() {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(error) if error.kind() == ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind loopback listener: {error}"),
        };
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(tcp).await.unwrap();
            assert!(matches!(socket.next().await, Some(Ok(Message::Text(_)))));
            socket
                .send(Message::Ping(vec![1, 2, 3].into()))
                .await
                .unwrap();
            loop {
                match tokio::time::timeout(DISCONNECT_WAIT, socket.next()).await {
                    Ok(Some(Ok(Message::Close(_)))) => {
                        socket.flush().await.unwrap();
                        break;
                    }
                    Ok(Some(Ok(_))) => continue,
                    other => panic!("expected client Close frame, got {other:?}"),
                }
            }
        });

        let endpoint = format!("ws://{address}/feed");
        let stream = MarketFeedStream::connect_to(&endpoint, "client", "token")
            .await
            .unwrap();
        stream.disconnect().await.unwrap();
        server.await.unwrap();
    }
}
