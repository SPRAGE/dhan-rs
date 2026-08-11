//! Full Market Depth WebSocket protocols.
//!
//! This module intentionally does not share the standard market-feed parser or
//! request types. Dhan documents separate endpoints, authentication URLs,
//! request envelopes, and 12-byte binary framing for the 20-level and
//! 200-level depth feeds.
//!
//! Official protocol reference (accessed 2026-08-11):
//! <https://dhanhq.co/docs/v2/full-market-depth/>.

use std::collections::{HashSet, VecDeque};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use url::Url;

use crate::constants::{WS_DEPTH_20_URL, WS_DEPTH_200_URL};
use crate::error::{DhanError, Result};
use crate::types::enums::ExchangeSegment;

const DEPTH_HEADER_LEN: usize = 12;
const DEPTH_LEVEL_LEN: usize = 16;
const TWENTY_DEPTH_LEVELS: usize = 20;
const TWO_HUNDRED_DEPTH_MAX_LEVELS: usize = 200;
const TWENTY_DEPTH_MAX_INSTRUMENTS: usize = 50;
const STANDARD_DISCONNECT_HEADER_LEN: usize = 8;
const DISCONNECT_REASON_LEN: usize = 2;
const DEPTH_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Build the documented, authenticated 20-level Full Market Depth URL.
pub fn twenty_depth_url(client_id: &str, access_token: &str) -> Result<Url> {
    depth_url(WS_DEPTH_20_URL, client_id, access_token)
}

/// Build the documented, authenticated 200-level Full Market Depth URL.
pub fn two_hundred_depth_url(client_id: &str, access_token: &str) -> Result<Url> {
    depth_url(WS_DEPTH_200_URL, client_id, access_token)
}

fn depth_url(endpoint: &str, client_id: &str, access_token: &str) -> Result<Url> {
    let mut url = Url::parse(endpoint)?;
    url.query_pairs_mut()
        .append_pair("token", access_token)
        .append_pair("clientId", client_id)
        .append_pair("authType", "2");
    Ok(url)
}

/// Exchange segments enabled by Dhan for Full Market Depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum DepthExchangeSegment {
    /// NSE Equity Cash.
    #[serde(rename = "NSE_EQ")]
    NseEq,
    /// NSE Futures and Options.
    #[serde(rename = "NSE_FNO")]
    NseFno,
}

/// A Dhan instrument that can be used by a Full Market Depth protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[allow(non_snake_case)]
pub struct DepthInstrument {
    ExchangeSegment: DepthExchangeSegment,
    SecurityId: String,
}

impl DepthInstrument {
    /// Create a depth subscription instrument.
    pub fn new(exchange_segment: DepthExchangeSegment, security_id: impl Into<String>) -> Self {
        Self {
            ExchangeSegment: exchange_segment,
            SecurityId: security_id.into(),
        }
    }
}

/// The exact 20-level instrument-list request envelope.
#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct TwentyDepthSubscriptionRequest {
    RequestCode: u8,
    InstrumentCount: usize,
    InstrumentList: Vec<DepthInstrument>,
}

impl TwentyDepthSubscriptionRequest {
    /// Build a 20-level subscription request for at most 50 instruments.
    pub fn subscribe(instruments: Vec<DepthInstrument>) -> Result<Self> {
        Self::new(23, instruments)
    }

    /// Build a 20-level unsubscription request for at most 50 instruments.
    pub fn unsubscribe(instruments: Vec<DepthInstrument>) -> Result<Self> {
        Self::new(24, instruments)
    }

    fn new(request_code: u8, instruments: Vec<DepthInstrument>) -> Result<Self> {
        validate_twenty_request(&instruments)?;
        Ok(Self {
            RequestCode: request_code,
            InstrumentCount: instruments.len(),
            InstrumentList: instruments,
        })
    }
}

/// The exact flat, single-instrument 200-level request envelope.
#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct TwoHundredDepthSubscriptionRequest {
    RequestCode: u8,
    ExchangeSegment: DepthExchangeSegment,
    SecurityId: String,
}

impl TwoHundredDepthSubscriptionRequest {
    /// Build the sole 200-level subscription request allowed on a connection.
    pub fn subscribe(instrument: DepthInstrument) -> Result<Self> {
        Self::new(23, instrument)
    }

    /// Build the flat 200-level unsubscription request.
    pub fn unsubscribe(instrument: DepthInstrument) -> Result<Self> {
        Self::new(24, instrument)
    }

    fn new(request_code: u8, instrument: DepthInstrument) -> Result<Self> {
        validate_depth_instrument(&instrument)?;
        Ok(Self {
            RequestCode: request_code,
            ExchangeSegment: instrument.ExchangeSegment,
            SecurityId: instrument.SecurityId,
        })
    }
}

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct DisconnectRequest {
    RequestCode: u8,
}

fn disconnect_json() -> Result<String> {
    Ok(serde_json::to_string(&DisconnectRequest {
        RequestCode: 12,
    })?)
}

fn validate_twenty_request(instruments: &[DepthInstrument]) -> Result<()> {
    if instruments.is_empty() {
        return Err(DhanError::InvalidArgument(
            "20-level depth requests must contain at least one instrument".into(),
        ));
    }
    if instruments.len() > TWENTY_DEPTH_MAX_INSTRUMENTS {
        return Err(DhanError::InvalidArgument(format!(
            "20-level depth supports at most {TWENTY_DEPTH_MAX_INSTRUMENTS} instruments per connection"
        )));
    }
    let unique: HashSet<_> = instruments.iter().collect();
    if unique.len() != instruments.len() {
        return Err(DhanError::InvalidArgument(
            "20-level depth request contains duplicate instruments".into(),
        ));
    }
    for instrument in instruments {
        validate_depth_instrument(instrument)?;
    }
    Ok(())
}

fn validate_depth_instrument(instrument: &DepthInstrument) -> Result<()> {
    if instrument.SecurityId.parse::<u32>().is_err() {
        return Err(DhanError::InvalidArgument(
            "Full Market Depth security ID must be an unsigned 32-bit integer".into(),
        ));
    }
    Ok(())
}

/// One bid or ask level from a Full Market Depth packet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthLevel {
    /// Price, decoded as little-endian IEEE 754 `float64`.
    pub price: f64,
    /// Quantity at this price.
    pub quantity: u32,
    /// Number of orders at this price.
    pub order_count: u32,
}

/// Header for a 20-level depth packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TwentyDepthHeader {
    /// Total packet length, including this 12-byte header.
    pub message_length: u16,
    /// Known exchange segment, if the received byte is supported by this crate.
    pub exchange_segment: Option<ExchangeSegment>,
    /// Raw exchange segment byte.
    pub exchange_segment_raw: u8,
    /// Exchange security ID.
    pub security_id: u32,
    /// Message sequence; Dhan documents this field as ignorable.
    pub message_sequence: u32,
}

/// Header for a 200-level depth packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TwoHundredDepthHeader {
    /// Total packet length, including this 12-byte header.
    pub message_length: u16,
    /// Known exchange segment, if the received byte is supported by this crate.
    pub exchange_segment: Option<ExchangeSegment>,
    /// Raw exchange segment byte.
    pub exchange_segment_raw: u8,
    /// Exchange security ID.
    pub security_id: u32,
    /// Number of 16-byte depth rows in this packet.
    pub row_count: u32,
}

/// Header available when Dhan sends a code-50 disconnect with the standard
/// market-feed's 8-byte header instead of the documented 12-byte depth header.
///
/// Dhan's Full Market Depth page is internally inconsistent here: its depth
/// header section says 12 bytes, while its disconnect table says 8 bytes. The
/// parser accepts both tightly framed forms and exposes the actual form used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandardDepthDisconnectHeader {
    /// Total packet length, including the 8-byte standard header.
    pub message_length: u16,
    /// Known exchange segment, if the received byte is supported by this crate.
    pub exchange_segment: Option<ExchangeSegment>,
    /// Raw exchange segment byte.
    pub exchange_segment_raw: u8,
    /// Exchange security ID.
    pub security_id: u32,
}

/// Header form present on a 20-level server disconnect event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwentyDepthDisconnectHeader {
    /// The 8-byte standard market-feed header described by Dhan's disconnect table.
    Standard(StandardDepthDisconnectHeader),
    /// The 12-byte depth header described by Dhan's response-header table.
    Depth(TwentyDepthHeader),
}

/// Header form present on a 200-level server disconnect event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoHundredDepthDisconnectHeader {
    /// The 8-byte standard market-feed header described by Dhan's disconnect table.
    Standard(StandardDepthDisconnectHeader),
    /// The 12-byte depth header described by Dhan's response-header table.
    Depth(TwoHundredDepthHeader),
}

/// Peer-close information observed during a graceful client disconnect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthClose {
    /// WebSocket close code; 1005 means the peer omitted a close frame payload.
    pub code: u16,
    /// Peer-provided close reason, if any.
    pub reason: String,
}

/// A parsed 20-level Full Market Depth event.
#[derive(Debug, Clone, PartialEq)]
pub enum TwentyDepthEvent {
    /// Bid packet (response code 41).
    Bid {
        /// Packet header.
        header: TwentyDepthHeader,
        /// Exactly 20 documented depth levels.
        levels: [DepthLevel; TWENTY_DEPTH_LEVELS],
    },
    /// Ask packet (response code 51).
    ///
    /// Dhan's page labels the parenthetical buy/sell descriptions for bid and
    /// ask inconsistently. This variant preserves only the protocol code.
    Ask {
        /// Packet header.
        header: TwentyDepthHeader,
        /// Exactly 20 documented depth levels.
        levels: [DepthLevel; TWENTY_DEPTH_LEVELS],
    },
    /// Server-initiated disconnect (response code 50).
    Disconnect {
        /// The header form sent by this server packet.
        header: TwentyDepthDisconnectHeader,
        /// Dhan disconnect reason, such as 805 for a connection limit.
        reason_code: i16,
    },
}

/// A parsed 200-level Full Market Depth event.
#[derive(Debug, Clone, PartialEq)]
pub enum TwoHundredDepthEvent {
    /// Bid packet (response code 41).
    Bid {
        /// Packet header.
        header: TwoHundredDepthHeader,
        /// The documented number of rows, at most 200.
        levels: Vec<DepthLevel>,
    },
    /// Ask packet (response code 51).
    ///
    /// Dhan's page labels the parenthetical buy/sell descriptions for bid and
    /// ask inconsistently. This variant preserves only the protocol code.
    Ask {
        /// Packet header.
        header: TwoHundredDepthHeader,
        /// The documented number of rows, at most 200.
        levels: Vec<DepthLevel>,
    },
    /// Server-initiated disconnect (response code 50).
    Disconnect {
        /// The header form sent by this server packet.
        header: TwoHundredDepthDisconnectHeader,
        /// Dhan disconnect reason, such as 805 for a connection limit.
        reason_code: i16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepthResponseCode {
    Bid = 41,
    Ask = 51,
    Disconnect = 50,
}

impl DepthResponseCode {
    fn from_byte(value: u8) -> Result<Self> {
        match value {
            41 => Ok(Self::Bid),
            50 => Ok(Self::Disconnect),
            51 => Ok(Self::Ask),
            _ => Err(DhanError::InvalidArgument(format!(
                "unknown full market depth response code: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RawDepthHeader {
    message_length: u16,
    response_code: DepthResponseCode,
    exchange_segment_raw: u8,
    security_id: u32,
    tail: u32,
}

fn parse_raw_header(data: &[u8]) -> Result<RawDepthHeader> {
    if data.len() < DEPTH_HEADER_LEN {
        return Err(DhanError::InvalidArgument(format!(
            "depth packet too short for 12-byte header: {} bytes",
            data.len()
        )));
    }
    Ok(RawDepthHeader {
        message_length: u16::from_le_bytes([data[0], data[1]]),
        response_code: DepthResponseCode::from_byte(data[2])?,
        exchange_segment_raw: data[3],
        security_id: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
        tail: u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
    })
}

fn checked_packet_len(data: &[u8], offset: usize) -> Result<usize> {
    let remaining = data.len().saturating_sub(offset);
    if remaining < DEPTH_HEADER_LEN {
        return Err(DhanError::InvalidArgument(format!(
            "incomplete stacked depth header: {remaining} bytes remain"
        )));
    }
    let length = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
    if length < DEPTH_HEADER_LEN {
        return Err(DhanError::InvalidArgument(format!(
            "invalid depth packet length {length}; it is shorter than the 12-byte header"
        )));
    }
    if length > remaining {
        return Err(DhanError::InvalidArgument(format!(
            "truncated depth packet: declared {length} bytes but only {remaining} remain"
        )));
    }
    Ok(length)
}

fn checked_standard_disconnect_packet_len(data: &[u8], offset: usize) -> Result<usize> {
    let remaining = data.len().saturating_sub(offset);
    if remaining < STANDARD_DISCONNECT_HEADER_LEN {
        return Err(DhanError::InvalidArgument(format!(
            "incomplete standard disconnect header: {remaining} bytes remain"
        )));
    }
    let length = u16::from_le_bytes([data[offset + 1], data[offset + 2]]) as usize;
    let expected = STANDARD_DISCONNECT_HEADER_LEN + DISCONNECT_REASON_LEN;
    if length != expected {
        return Err(DhanError::InvalidArgument(format!(
            "invalid standard disconnect length {length}; expected {expected}"
        )));
    }
    if length > remaining {
        return Err(DhanError::InvalidArgument(format!(
            "truncated standard disconnect packet: declared {length} bytes but only {remaining} remain"
        )));
    }
    Ok(length)
}

fn parse_standard_disconnect_header(data: &[u8]) -> Result<StandardDepthDisconnectHeader> {
    let expected = STANDARD_DISCONNECT_HEADER_LEN + DISCONNECT_REASON_LEN;
    if data.len() != expected {
        return Err(DhanError::InvalidArgument(format!(
            "standard disconnect packet has length {}, expected {expected}",
            data.len()
        )));
    }
    if data[0] != DepthResponseCode::Disconnect as u8 {
        return Err(DhanError::InvalidArgument(
            "standard disconnect packet does not have response code 50".into(),
        ));
    }
    Ok(StandardDepthDisconnectHeader {
        message_length: u16::from_le_bytes([data[1], data[2]]),
        exchange_segment: ExchangeSegment::from_segment_code(data[3]),
        exchange_segment_raw: data[3],
        security_id: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
    })
}

fn parse_level(data: &[u8]) -> Result<DepthLevel> {
    if data.len() != DEPTH_LEVEL_LEN {
        return Err(DhanError::InvalidArgument(format!(
            "invalid depth row length {}; expected {DEPTH_LEVEL_LEN}",
            data.len()
        )));
    }
    Ok(DepthLevel {
        price: f64::from_le_bytes(
            data[0..8]
                .try_into()
                .map_err(|_| DhanError::InvalidArgument("invalid depth price field".into()))?,
        ),
        quantity: u32::from_le_bytes(
            data[8..12]
                .try_into()
                .map_err(|_| DhanError::InvalidArgument("invalid depth quantity field".into()))?,
        ),
        order_count: u32::from_le_bytes(
            data[12..16].try_into().map_err(|_| {
                DhanError::InvalidArgument("invalid depth order-count field".into())
            })?,
        ),
    })
}

fn twenty_header(raw: RawDepthHeader) -> TwentyDepthHeader {
    TwentyDepthHeader {
        message_length: raw.message_length,
        exchange_segment: ExchangeSegment::from_segment_code(raw.exchange_segment_raw),
        exchange_segment_raw: raw.exchange_segment_raw,
        security_id: raw.security_id,
        message_sequence: raw.tail,
    }
}

fn two_hundred_header(raw: RawDepthHeader) -> TwoHundredDepthHeader {
    TwoHundredDepthHeader {
        message_length: raw.message_length,
        exchange_segment: ExchangeSegment::from_segment_code(raw.exchange_segment_raw),
        exchange_segment_raw: raw.exchange_segment_raw,
        security_id: raw.security_id,
        row_count: raw.tail,
    }
}

/// Parse all stacked 20-level packets in one binary WebSocket message.
pub fn parse_twenty_depth_packets(data: &[u8]) -> Result<Vec<TwentyDepthEvent>> {
    if data.is_empty() {
        return Err(DhanError::InvalidArgument(
            "depth message contains no packets".into(),
        ));
    }
    let mut events = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        // Code 50 in byte zero is the unambiguous standard 8-byte-header
        // disconnect layout. A depth-header packet keeps code 50 in byte two.
        if data[offset] == DepthResponseCode::Disconnect as u8 {
            let packet_len = checked_standard_disconnect_packet_len(data, offset)?;
            let packet = &data[offset..offset + packet_len];
            let header = parse_standard_disconnect_header(packet)?;
            let reason_code = i16::from_le_bytes([packet[8], packet[9]]);
            events.push(TwentyDepthEvent::Disconnect {
                header: TwentyDepthDisconnectHeader::Standard(header),
                reason_code,
            });
            offset += packet_len;
            continue;
        }
        let packet_len = checked_packet_len(data, offset)?;
        let packet = &data[offset..offset + packet_len];
        let raw = parse_raw_header(packet)?;
        let header = twenty_header(raw);
        let event = match raw.response_code {
            DepthResponseCode::Bid | DepthResponseCode::Ask => {
                let expected = DEPTH_HEADER_LEN + TWENTY_DEPTH_LEVELS * DEPTH_LEVEL_LEN;
                if packet.len() != expected {
                    return Err(DhanError::InvalidArgument(format!(
                        "20-level depth packet has length {}, expected {expected}",
                        packet.len()
                    )));
                }
                let levels: [DepthLevel; TWENTY_DEPTH_LEVELS] = (0..TWENTY_DEPTH_LEVELS)
                    .map(|index| {
                        let start = DEPTH_HEADER_LEN + index * DEPTH_LEVEL_LEN;
                        parse_level(&packet[start..start + DEPTH_LEVEL_LEN])
                    })
                    .collect::<Result<Vec<_>>>()?
                    .try_into()
                    .map_err(|_| {
                        DhanError::InvalidArgument(
                            "20-level depth packet did not contain 20 rows".into(),
                        )
                    })?;
                if raw.response_code == DepthResponseCode::Bid {
                    TwentyDepthEvent::Bid { header, levels }
                } else {
                    TwentyDepthEvent::Ask { header, levels }
                }
            }
            DepthResponseCode::Disconnect => {
                let expected = DEPTH_HEADER_LEN + 2;
                if packet.len() != expected {
                    return Err(DhanError::InvalidArgument(format!(
                        "depth disconnect packet has length {}, expected {expected}",
                        packet.len()
                    )));
                }
                let reason_code = i16::from_le_bytes([packet[12], packet[13]]);
                TwentyDepthEvent::Disconnect {
                    header: TwentyDepthDisconnectHeader::Depth(header),
                    reason_code,
                }
            }
        };
        events.push(event);
        offset += packet_len;
    }
    Ok(events)
}

/// Parse all stacked 200-level packets in one binary WebSocket message.
pub fn parse_two_hundred_depth_packets(data: &[u8]) -> Result<Vec<TwoHundredDepthEvent>> {
    if data.is_empty() {
        return Err(DhanError::InvalidArgument(
            "depth message contains no packets".into(),
        ));
    }
    let mut events = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        // See the 20-level parser for why byte zero identifies the standard
        // code-50 disconnect layout without ambiguously accepting depth data.
        if data[offset] == DepthResponseCode::Disconnect as u8 {
            let packet_len = checked_standard_disconnect_packet_len(data, offset)?;
            let packet = &data[offset..offset + packet_len];
            let header = parse_standard_disconnect_header(packet)?;
            let reason_code = i16::from_le_bytes([packet[8], packet[9]]);
            events.push(TwoHundredDepthEvent::Disconnect {
                header: TwoHundredDepthDisconnectHeader::Standard(header),
                reason_code,
            });
            offset += packet_len;
            continue;
        }
        let packet_len = checked_packet_len(data, offset)?;
        let packet = &data[offset..offset + packet_len];
        let raw = parse_raw_header(packet)?;
        let header = two_hundred_header(raw);
        let event = match raw.response_code {
            DepthResponseCode::Bid | DepthResponseCode::Ask => {
                let row_count = header.row_count as usize;
                if row_count > TWO_HUNDRED_DEPTH_MAX_LEVELS {
                    return Err(DhanError::InvalidArgument(format!(
                        "200-level depth row count {row_count} exceeds {TWO_HUNDRED_DEPTH_MAX_LEVELS}"
                    )));
                }
                let expected = DEPTH_HEADER_LEN + row_count * DEPTH_LEVEL_LEN;
                if packet.len() != expected {
                    return Err(DhanError::InvalidArgument(format!(
                        "200-level depth packet has length {}, expected {expected} for {row_count} rows",
                        packet.len()
                    )));
                }
                let levels = (0..row_count)
                    .map(|index| {
                        let start = DEPTH_HEADER_LEN + index * DEPTH_LEVEL_LEN;
                        parse_level(&packet[start..start + DEPTH_LEVEL_LEN])
                    })
                    .collect::<Result<Vec<_>>>()?;
                if raw.response_code == DepthResponseCode::Bid {
                    TwoHundredDepthEvent::Bid { header, levels }
                } else {
                    TwoHundredDepthEvent::Ask { header, levels }
                }
            }
            DepthResponseCode::Disconnect => {
                let expected = DEPTH_HEADER_LEN + 2;
                if packet.len() != expected {
                    return Err(DhanError::InvalidArgument(format!(
                        "depth disconnect packet has length {}, expected {expected}",
                        packet.len()
                    )));
                }
                let reason_code = i16::from_le_bytes([packet[12], packet[13]]);
                TwoHundredDepthEvent::Disconnect {
                    header: TwoHundredDepthDisconnectHeader::Depth(header),
                    reason_code,
                }
            }
        };
        events.push(event);
        offset += packet_len;
    }
    Ok(events)
}

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn redacted_url(url: &Url) -> String {
    let mut safe = url.clone();
    safe.set_query(None);
    safe.to_string()
}

fn redact_connection_diagnostic(url: &Url, diagnostic: String) -> String {
    let safe_url = redacted_url(url);
    let mut diagnostic = diagnostic.replace(url.as_str(), &safe_url);
    if let Some((_, token)) = url
        .query_pairs()
        .find(|(name, token)| name == "token" && !token.is_empty())
    {
        let form_encoded_token = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("token", token.as_ref())
            .finish()
            .strip_prefix("token=")
            .unwrap_or_default()
            .to_owned();
        let percent_encoded_token: String =
            url::form_urlencoded::byte_serialize(token.as_bytes()).collect();
        diagnostic = diagnostic
            .replace(token.as_ref(), "[REDACTED]")
            .replace(&form_encoded_token, "[REDACTED]")
            .replace(&percent_encoded_token, "[REDACTED]");
    }
    diagnostic
}

fn redacted_connection_error(
    protocol: &str,
    url: &Url,
    error: tokio_tungstenite::tungstenite::Error,
) -> DhanError {
    let safe_url = redacted_url(url);
    let diagnostic = redact_connection_diagnostic(url, error.to_string());
    DhanError::InvalidArgument(format!(
        "{protocol} WebSocket connection failed at {safe_url}: {diagnostic}"
    ))
}

async fn connect_depth(url: Url, protocol: &str) -> Result<WsStream> {
    connect_async(url.as_str())
        .await
        .map(|(socket, _)| socket)
        .map_err(|error| redacted_connection_error(protocol, &url, error))
}

async fn await_peer_close(
    read: &mut SplitStream<WsStream>,
    write: &mut SplitSink<WsStream, Message>,
) -> Result<DepthClose> {
    tokio::time::timeout(DEPTH_CLOSE_TIMEOUT, async {
        loop {
            match read.next().await {
                Some(Ok(Message::Close(frame))) => {
                    return Ok(match frame {
                        Some(frame) => DepthClose {
                            code: frame.code.into(),
                            reason: frame.reason.to_string(),
                        },
                        // RFC 6455's reserved "no status received" code is
                        // useful to callers when the peer sends an empty Close.
                        None => DepthClose {
                            code: 1005,
                            reason: String::new(),
                        },
                    });
                }
                Some(Ok(Message::Ping(_))) => {
                    // Tungstenite queues an automatic Pong while reading. It
                    // is emitted only once the write half is flushed.
                    write.flush().await?;
                }
                Some(Ok(_)) => continue,
                Some(Err(error)) => return Err(error.into()),
                None => {
                    return Err(DhanError::InvalidArgument(
                        "depth WebSocket ended before the peer Close frame".into(),
                    ));
                }
            }
        }
    })
    .await
    .map_err(|_| {
        DhanError::InvalidArgument(format!(
            "timed out after {} seconds waiting for depth WebSocket peer Close",
            DEPTH_CLOSE_TIMEOUT.as_secs()
        ))
    })?
}

/// A connected 20-level Full Market Depth stream.
///
/// This is a low-level caller-polled stream. Keep polling it continuously so
/// incoming Ping frames can be read and their automatic Pong frames flushed;
/// otherwise Dhan may close the socket after its documented keepalive window.
pub struct TwentyDepthStream {
    read: SplitStream<WsStream>,
    write: SplitSink<WsStream, Message>,
    subscribed: HashSet<DepthInstrument>,
    pending: VecDeque<TwentyDepthEvent>,
}

impl TwentyDepthStream {
    /// Connect using the documented 20-level endpoint and query authentication.
    pub async fn connect(client_id: &str, access_token: &str) -> Result<Self> {
        let url = twenty_depth_url(client_id, access_token)?;
        Self::connect_url(url).await
    }

    async fn connect_url(url: Url) -> Result<Self> {
        let ws = connect_depth(url, "20-level depth").await?;
        let (write, read) = ws.split();
        Ok(Self {
            read,
            write,
            subscribed: HashSet::new(),
            pending: VecDeque::new(),
        })
    }

    /// Subscribe up to 50 total instruments using the 20-level list envelope.
    pub async fn subscribe(&mut self, instruments: Vec<DepthInstrument>) -> Result<()> {
        validate_twenty_request(&instruments)?;
        if instruments
            .iter()
            .any(|item| self.subscribed.contains(item))
        {
            return Err(DhanError::InvalidArgument(
                "20-level depth instrument is already subscribed".into(),
            ));
        }
        if self.subscribed.len() + instruments.len() > TWENTY_DEPTH_MAX_INSTRUMENTS {
            return Err(DhanError::InvalidArgument(format!(
                "20-level depth supports at most {TWENTY_DEPTH_MAX_INSTRUMENTS} instruments per connection"
            )));
        }
        send_json(
            &mut self.write,
            &TwentyDepthSubscriptionRequest::subscribe(instruments.clone())?,
        )
        .await?;
        self.subscribed.extend(instruments);
        Ok(())
    }

    /// Unsubscribe instruments using the documented 20-level list envelope.
    pub async fn unsubscribe(&mut self, instruments: Vec<DepthInstrument>) -> Result<()> {
        let request = TwentyDepthSubscriptionRequest::unsubscribe(instruments.clone())?;
        send_json(&mut self.write, &request).await?;
        for instrument in instruments {
            self.subscribed.remove(&instrument);
        }
        Ok(())
    }

    /// Send Dhan's depth disconnect request and await the peer Close frame.
    ///
    /// The wait is bounded to five seconds. A peer-close code and reason are
    /// returned, while transport errors and a timeout remain typed errors.
    pub async fn disconnect(mut self) -> Result<DepthClose> {
        self.write
            .send(Message::Text(disconnect_json()?.into()))
            .await?;
        self.write.send(Message::Close(None)).await?;
        await_peer_close(&mut self.read, &mut self.write).await
    }
}

impl Stream for TwentyDepthStream {
    type Item = Result<TwentyDepthEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.pending.pop_front() {
            return Poll::Ready(Some(Ok(event)));
        }
        loop {
            match self.read.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(Message::Binary(data)))) => {
                    match parse_twenty_depth_packets(&data) {
                        Ok(events) => {
                            self.pending.extend(events);
                            if let Some(event) = self.pending.pop_front() {
                                return Poll::Ready(Some(Ok(event)));
                            }
                        }
                        Err(error) => return Poll::Ready(Some(Err(error))),
                    }
                }
                Poll::Ready(Some(Ok(Message::Ping(_)))) => {
                    // See the type-level keepalive warning. Reading queues
                    // tungstenite's Pong; polling the sink flushes it.
                    match Pin::new(&mut self.write).poll_flush(cx) {
                        Poll::Ready(Ok(())) => continue,
                        Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(error.into()))),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Poll::Ready(Some(Ok(Message::Pong(_))))
                | Poll::Ready(Some(Ok(Message::Text(_)))) => continue,
                Poll::Ready(Some(Ok(Message::Close(_)))) | Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(Ok(_))) => continue,
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error.into()))),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// A connected 200-level Full Market Depth stream.
///
/// This is a low-level caller-polled stream. Keep polling it continuously so
/// incoming Ping frames can be read and their automatic Pong frames flushed;
/// otherwise Dhan may close the socket after its documented keepalive window.
pub struct TwoHundredDepthStream {
    read: SplitStream<WsStream>,
    write: SplitSink<WsStream, Message>,
    subscribed: Option<DepthInstrument>,
    pending: VecDeque<TwoHundredDepthEvent>,
}

impl TwoHundredDepthStream {
    /// Connect using the documented 200-level endpoint and query authentication.
    pub async fn connect(client_id: &str, access_token: &str) -> Result<Self> {
        let url = two_hundred_depth_url(client_id, access_token)?;
        Self::connect_url(url).await
    }

    async fn connect_url(url: Url) -> Result<Self> {
        let ws = connect_depth(url, "200-level depth").await?;
        let (write, read) = ws.split();
        Ok(Self {
            read,
            write,
            subscribed: None,
            pending: VecDeque::new(),
        })
    }

    /// Subscribe the only instrument permitted on a 200-level connection.
    pub async fn subscribe(&mut self, instrument: DepthInstrument) -> Result<()> {
        if self.subscribed.is_some() {
            return Err(DhanError::InvalidArgument(
                "200-level depth permits only one instrument per connection".into(),
            ));
        }
        send_json(
            &mut self.write,
            &TwoHundredDepthSubscriptionRequest::subscribe(instrument.clone())?,
        )
        .await?;
        self.subscribed = Some(instrument);
        Ok(())
    }

    /// Unsubscribe the current 200-level instrument with the flat envelope.
    pub async fn unsubscribe(&mut self) -> Result<()> {
        let instrument = self.subscribed.clone().ok_or_else(|| {
            DhanError::InvalidArgument("no 200-level depth instrument is subscribed".into())
        })?;
        send_json(
            &mut self.write,
            &TwoHundredDepthSubscriptionRequest::unsubscribe(instrument)?,
        )
        .await?;
        self.subscribed = None;
        Ok(())
    }

    /// Send Dhan's depth disconnect request and await the peer Close frame.
    ///
    /// The wait is bounded to five seconds. A peer-close code and reason are
    /// returned, while transport errors and a timeout remain typed errors.
    pub async fn disconnect(mut self) -> Result<DepthClose> {
        self.write
            .send(Message::Text(disconnect_json()?.into()))
            .await?;
        self.write.send(Message::Close(None)).await?;
        await_peer_close(&mut self.read, &mut self.write).await
    }
}

impl Stream for TwoHundredDepthStream {
    type Item = Result<TwoHundredDepthEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(event) = self.pending.pop_front() {
            return Poll::Ready(Some(Ok(event)));
        }
        loop {
            match self.read.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(Message::Binary(data)))) => {
                    match parse_two_hundred_depth_packets(&data) {
                        Ok(events) => {
                            self.pending.extend(events);
                            if let Some(event) = self.pending.pop_front() {
                                return Poll::Ready(Some(Ok(event)));
                            }
                        }
                        Err(error) => return Poll::Ready(Some(Err(error))),
                    }
                }
                Poll::Ready(Some(Ok(Message::Ping(_)))) => {
                    // See the type-level keepalive warning. Reading queues
                    // tungstenite's Pong; polling the sink flushes it.
                    match Pin::new(&mut self.write).poll_flush(cx) {
                        Poll::Ready(Ok(())) => continue,
                        Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(error.into()))),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Poll::Ready(Some(Ok(Message::Pong(_))))
                | Poll::Ready(Some(Ok(Message::Text(_)))) => continue,
                Poll::Ready(Some(Ok(Message::Close(_)))) | Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(Ok(_))) => continue,
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error.into()))),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

async fn send_json<T: Serialize>(
    write: &mut SplitSink<WsStream, Message>,
    request: &T,
) -> Result<()> {
    write
        .send(Message::Text(serde_json::to_string(request)?.into()))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::time::Duration;

    use super::*;
    use tokio::net::TcpListener;
    use tokio::time::timeout;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    fn instrument(id: u32) -> DepthInstrument {
        DepthInstrument::new(DepthExchangeSegment::NseEq, id.to_string())
    }

    /// Binary golden fixture builder: header followed by exact little-endian rows.
    fn depth_packet(code: u8, segment: u8, security_id: u32, tail: u32, rows: usize) -> Vec<u8> {
        let mut packet = Vec::with_capacity(DEPTH_HEADER_LEN + rows * DEPTH_LEVEL_LEN);
        let len = (DEPTH_HEADER_LEN + rows * DEPTH_LEVEL_LEN) as u16;
        packet.extend_from_slice(&len.to_le_bytes());
        packet.push(code);
        packet.push(segment);
        packet.extend_from_slice(&security_id.to_le_bytes());
        packet.extend_from_slice(&tail.to_le_bytes());
        for row in 0..rows {
            packet.extend_from_slice(&(100.25 + row as f64).to_le_bytes());
            packet.extend_from_slice(&(1_000 + row as u32).to_le_bytes());
            packet.extend_from_slice(&(10 + row as u32).to_le_bytes());
        }
        packet
    }

    fn depth_disconnect_packet(reason: i16) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&(14u16).to_le_bytes());
        packet.push(50);
        packet.push(1);
        packet.extend_from_slice(&1333u32.to_le_bytes());
        packet.extend_from_slice(&77u32.to_le_bytes());
        packet.extend_from_slice(&reason.to_le_bytes());
        packet
    }

    fn standard_disconnect_packet(reason: i16) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.push(50);
        packet.extend_from_slice(&(10u16).to_le_bytes());
        packet.push(1);
        packet.extend_from_slice(&1333u32.to_le_bytes());
        packet.extend_from_slice(&reason.to_le_bytes());
        packet
    }

    async fn loopback_listener() -> Option<TcpListener> {
        match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => Some(listener),
            // The Codex filesystem sandbox can prohibit even loopback binds.
            // Keep the test active in normal CI, while allowing its no-network
            // sandbox to exercise the parser suite instead.
            Err(error) if error.kind() == ErrorKind::PermissionDenied => None,
            Err(error) => panic!("failed to bind loopback listener: {error}"),
        }
    }

    #[test]
    fn exact_auth_urls_are_protocol_specific() {
        assert_eq!(
            twenty_depth_url("1000000001", "abc.def").unwrap().as_str(),
            "wss://depth-api-feed.dhan.co/twentydepth?token=abc.def&clientId=1000000001&authType=2"
        );
        assert_eq!(
            two_hundred_depth_url("1000000001", "abc.def")
                .unwrap()
                .as_str(),
            "wss://full-depth-api.dhan.co/twohundreddepth?token=abc.def&clientId=1000000001&authType=2"
        );
    }

    #[test]
    fn exact_subscription_and_disconnect_serialization() {
        let twenty = TwentyDepthSubscriptionRequest::subscribe(vec![instrument(1333)]).unwrap();
        assert_eq!(
            serde_json::to_string(&twenty).unwrap(),
            r#"{"RequestCode":23,"InstrumentCount":1,"InstrumentList":[{"ExchangeSegment":"NSE_EQ","SecurityId":"1333"}]}"#
        );
        let two_hundred = TwoHundredDepthSubscriptionRequest::subscribe(instrument(1333)).unwrap();
        assert_eq!(
            serde_json::to_string(&two_hundred).unwrap(),
            r#"{"RequestCode":23,"ExchangeSegment":"NSE_EQ","SecurityId":"1333"}"#
        );
        assert_eq!(
            serde_json::to_string(
                &TwentyDepthSubscriptionRequest::unsubscribe(vec![instrument(1333)]).unwrap()
            )
            .unwrap(),
            r#"{"RequestCode":24,"InstrumentCount":1,"InstrumentList":[{"ExchangeSegment":"NSE_EQ","SecurityId":"1333"}]}"#
        );
        assert_eq!(
            serde_json::to_string(
                &TwoHundredDepthSubscriptionRequest::unsubscribe(instrument(1333)).unwrap()
            )
            .unwrap(),
            r#"{"RequestCode":24,"ExchangeSegment":"NSE_EQ","SecurityId":"1333"}"#
        );
        assert_eq!(disconnect_json().unwrap(), r#"{"RequestCode":12}"#);
        assert!(
            TwentyDepthSubscriptionRequest::subscribe(vec![DepthInstrument::new(
                DepthExchangeSegment::NseEq,
                "not-a-number"
            )])
            .is_err()
        );
        assert!(
            TwoHundredDepthSubscriptionRequest::subscribe(DepthInstrument::new(
                DepthExchangeSegment::NseEq,
                ""
            ))
            .is_err()
        );
    }

    #[test]
    fn twenty_depth_golden_bid_and_ask_packets_decode_all_fields() {
        let bid = depth_packet(41, 1, 1333, 9, TWENTY_DEPTH_LEVELS);
        let ask = depth_packet(51, 2, 25, 10, TWENTY_DEPTH_LEVELS);
        let mut stacked = bid;
        stacked.extend_from_slice(&ask);
        let events = parse_twenty_depth_packets(&stacked).unwrap();
        assert_eq!(events.len(), 2);
        match &events[0] {
            TwentyDepthEvent::Bid { header, levels } => {
                assert_eq!(header.message_length, 332);
                assert_eq!(header.exchange_segment, Some(ExchangeSegment::NSE_EQ));
                assert_eq!(header.security_id, 1333);
                assert_eq!(header.message_sequence, 9);
                assert_eq!(
                    levels[0],
                    DepthLevel {
                        price: 100.25,
                        quantity: 1_000,
                        order_count: 10
                    }
                );
                assert_eq!(
                    levels[19],
                    DepthLevel {
                        price: 119.25,
                        quantity: 1_019,
                        order_count: 29
                    }
                );
            }
            other => panic!("expected bid, got {other:?}"),
        }
        assert!(matches!(events[1], TwentyDepthEvent::Ask { .. }));
    }

    #[test]
    fn two_hundred_depth_golden_bid_and_ask_packets_decode_row_count() {
        let bid = depth_packet(41, 1, 1333, 200, TWO_HUNDRED_DEPTH_MAX_LEVELS);
        let ask = depth_packet(51, 2, 25, 2, 2);
        let mut stacked = bid;
        stacked.extend_from_slice(&ask);
        let events = parse_two_hundred_depth_packets(&stacked).unwrap();
        assert_eq!(events.len(), 2);
        match &events[0] {
            TwoHundredDepthEvent::Bid { header, levels } => {
                assert_eq!(header.message_length, 3212);
                assert_eq!(header.row_count, 200);
                assert_eq!(levels.len(), 200);
                assert_eq!(
                    levels[199],
                    DepthLevel {
                        price: 299.25,
                        quantity: 1_199,
                        order_count: 209
                    }
                );
            }
            other => panic!("expected bid, got {other:?}"),
        }
        match &events[1] {
            TwoHundredDepthEvent::Ask { header, levels } => {
                assert_eq!(header.row_count, 2);
                assert_eq!(
                    levels[1],
                    DepthLevel {
                        price: 101.25,
                        quantity: 1_001,
                        order_count: 11
                    }
                );
            }
            other => panic!("expected ask, got {other:?}"),
        }
    }

    #[test]
    fn disconnect_packets_preserve_reason_and_header_form() {
        let depth = depth_disconnect_packet(805);
        let standard = standard_disconnect_packet(806);
        match parse_twenty_depth_packets(&depth).unwrap().as_slice() {
            [
                TwentyDepthEvent::Disconnect {
                    header: TwentyDepthDisconnectHeader::Depth(header),
                    reason_code,
                },
            ] => {
                assert_eq!(*reason_code, 805);
                assert_eq!(header.message_sequence, 77);
            }
            other => panic!("expected 12-byte 20-level disconnect, got {other:?}"),
        }
        match parse_twenty_depth_packets(&standard).unwrap().as_slice() {
            [
                TwentyDepthEvent::Disconnect {
                    header: TwentyDepthDisconnectHeader::Standard(header),
                    reason_code,
                },
            ] => {
                assert_eq!(*reason_code, 806);
                assert_eq!(header.message_length, 10);
                assert_eq!(header.security_id, 1333);
            }
            other => panic!("expected standard 20-level disconnect, got {other:?}"),
        }
        match parse_two_hundred_depth_packets(&depth).unwrap().as_slice() {
            [
                TwoHundredDepthEvent::Disconnect {
                    header: TwoHundredDepthDisconnectHeader::Depth(header),
                    reason_code,
                },
            ] => {
                assert_eq!(*reason_code, 805);
                assert_eq!(header.row_count, 77);
            }
            other => panic!("expected 12-byte 200-level disconnect, got {other:?}"),
        }
        match parse_two_hundred_depth_packets(&standard)
            .unwrap()
            .as_slice()
        {
            [
                TwoHundredDepthEvent::Disconnect {
                    header: TwoHundredDepthDisconnectHeader::Standard(header),
                    reason_code,
                },
            ] => {
                assert_eq!(*reason_code, 806);
                assert_eq!(header.message_length, 10);
                assert_eq!(header.security_id, 1333);
            }
            other => panic!("expected standard 200-level disconnect, got {other:?}"),
        }

        let mut twenty_stack = depth_packet(41, 1, 1333, 1, TWENTY_DEPTH_LEVELS);
        twenty_stack.extend_from_slice(&standard);
        assert!(matches!(
            parse_twenty_depth_packets(&twenty_stack)
                .unwrap()
                .as_slice(),
            [
                TwentyDepthEvent::Bid { .. },
                TwentyDepthEvent::Disconnect {
                    header: TwentyDepthDisconnectHeader::Standard(_),
                    reason_code: 806,
                }
            ]
        ));
        let mut two_hundred_stack = depth_packet(41, 1, 1333, 2, 2);
        two_hundred_stack.extend_from_slice(&standard);
        assert!(matches!(
            parse_two_hundred_depth_packets(&two_hundred_stack)
                .unwrap()
                .as_slice(),
            [
                TwoHundredDepthEvent::Bid { .. },
                TwoHundredDepthEvent::Disconnect {
                    header: TwoHundredDepthDisconnectHeader::Standard(_),
                    reason_code: 806,
                }
            ]
        ));
    }

    #[test]
    fn framing_rejects_all_truncations_and_bad_stacks() {
        let twenty = depth_packet(41, 1, 1333, 1, TWENTY_DEPTH_LEVELS);
        let two_hundred = depth_packet(41, 1, 1333, 2, 2);
        for boundary in 0..twenty.len() {
            assert!(
                parse_twenty_depth_packets(&twenty[..boundary]).is_err(),
                "20 boundary {boundary}"
            );
        }
        for boundary in 0..two_hundred.len() {
            assert!(
                parse_two_hundred_depth_packets(&two_hundred[..boundary]).is_err(),
                "200 boundary {boundary}"
            );
        }
        let mut incomplete_stack = twenty.clone();
        incomplete_stack.push(0);
        assert!(parse_twenty_depth_packets(&incomplete_stack).is_err());
        let mut incomplete_two_hundred_stack = two_hundred.clone();
        incomplete_two_hundred_stack.push(0);
        assert!(parse_two_hundred_depth_packets(&incomplete_two_hundred_stack).is_err());
        let mut bad_length = twenty;
        bad_length[0..2].copy_from_slice(&331u16.to_le_bytes());
        assert!(parse_twenty_depth_packets(&bad_length).is_err());
        let mut unknown_code = two_hundred;
        unknown_code[2] = 99;
        assert!(parse_two_hundred_depth_packets(&unknown_code).is_err());
        for boundary in 0..standard_disconnect_packet(805).len() {
            assert!(
                parse_twenty_depth_packets(&standard_disconnect_packet(805)[..boundary]).is_err()
            );
            assert!(
                parse_two_hundred_depth_packets(&standard_disconnect_packet(805)[..boundary])
                    .is_err()
            );
        }
        let mut bad_standard_length = standard_disconnect_packet(805);
        bad_standard_length[1..3].copy_from_slice(&11u16.to_le_bytes());
        assert!(parse_twenty_depth_packets(&bad_standard_length).is_err());
    }

    #[test]
    fn row_count_and_request_limits_are_enforced() {
        let too_many = (0..=TWENTY_DEPTH_MAX_INSTRUMENTS)
            .map(|id| instrument(id as u32))
            .collect();
        assert!(TwentyDepthSubscriptionRequest::subscribe(too_many).is_err());
        assert!(TwentyDepthSubscriptionRequest::subscribe(Vec::new()).is_err());
        assert!(
            TwentyDepthSubscriptionRequest::subscribe(vec![instrument(1), instrument(1)]).is_err()
        );

        let mut too_many_rows = depth_packet(41, 1, 1333, 201, 201);
        assert!(parse_two_hundred_depth_packets(&too_many_rows).is_err());
        too_many_rows[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert!(parse_two_hundred_depth_packets(&too_many_rows).is_err());
    }

    #[test]
    fn parsers_never_panic_for_deterministic_byte_corpus() {
        let corpus = [
            vec![],
            vec![0],
            vec![0; 11],
            vec![14, 0, 50, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![255; 64],
            standard_disconnect_packet(805),
            depth_packet(41, 1, 1, 0, 0),
            depth_packet(51, 1, 1, 200, 200),
        ];
        for bytes in corpus {
            assert!(catch_unwind(AssertUnwindSafe(|| parse_twenty_depth_packets(&bytes))).is_ok());
            assert!(
                catch_unwind(AssertUnwindSafe(|| parse_two_hundred_depth_packets(&bytes))).is_ok()
            );
        }
    }

    #[test]
    fn connection_diagnostics_redact_query_credentials() {
        let url = twenty_depth_url("1000000001", "secret token+/?").unwrap();
        let diagnostic = redact_connection_diagnostic(
            &url,
            format!(
                "failed to connect to {} with token secret token+/? and encoded token=secret+token%2B%2F%3F",
                url.as_str()
            ),
        );
        assert!(!diagnostic.contains("secret token+/?"));
        assert!(!diagnostic.contains("secret+token%2B%2F%3F"));
        assert!(!diagnostic.contains("?token="));
        assert!(diagnostic.contains("wss://depth-api-feed.dhan.co/twentydepth"));
    }

    #[tokio::test]
    async fn continuously_polled_stream_flushes_ping_pong_and_observes_close() {
        let Some(listener) = loopback_listener().await else {
            return;
        };
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut peer = accept_async(tcp).await.unwrap();
            peer.send(Message::Ping(vec![7, 8, 9].into()))
                .await
                .unwrap();
            match timeout(Duration::from_secs(1), peer.next()).await.unwrap() {
                Some(Ok(Message::Pong(payload))) => assert_eq!(payload.as_ref(), [7, 8, 9]),
                other => panic!("expected pong, got {other:?}"),
            }
            peer.send(Message::Close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "loopback complete".into(),
            })))
            .await
            .unwrap();
        });

        let url = Url::parse(&format!("ws://{address}/depth-test")).unwrap();
        let mut client = TwentyDepthStream::connect_url(url).await.unwrap();
        assert!(
            timeout(Duration::from_secs(1), client.next())
                .await
                .unwrap()
                .is_none()
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn graceful_disconnect_waits_for_and_returns_peer_close_diagnostics() {
        let Some(listener) = loopback_listener().await else {
            return;
        };
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut peer = accept_async(tcp).await.unwrap();
            match timeout(Duration::from_secs(1), peer.next()).await.unwrap() {
                Some(Ok(Message::Text(request))) => assert_eq!(request, r#"{"RequestCode":12}"#),
                other => panic!("expected disconnect request, got {other:?}"),
            }
            peer.send(Message::Close(Some(CloseFrame {
                code: CloseCode::Policy,
                reason: "server diagnostic".into(),
            })))
            .await
            .unwrap();
        });

        let url = Url::parse(&format!("ws://{address}/depth-test")).unwrap();
        let client = TwoHundredDepthStream::connect_url(url).await.unwrap();
        let close = client.disconnect().await.unwrap();
        assert_eq!(close.code, 1008);
        assert_eq!(close.reason, "server diagnostic");
        server.await.unwrap();
    }
}
