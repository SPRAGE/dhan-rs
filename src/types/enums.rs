//! Shared enum types that map directly to DhanHQ API string values.
//!
//! Variant names use `SCREAMING_SNAKE_CASE` to match the JSON wire format
//! expected by the DhanHQ API, so we suppress the Rust naming convention lint.
#![allow(non_camel_case_types)]

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Exchange Segment
// ---------------------------------------------------------------------------

/// Exchange and segment identifier used across all DhanHQ APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExchangeSegment {
    /// Index value (segment code 0).
    IDX_I,
    /// NSE Equity Cash (segment code 1).
    NSE_EQ,
    /// NSE Futures & Options (segment code 2).
    NSE_FNO,
    /// NSE Currency (segment code 3).
    NSE_CURRENCY,
    /// BSE Equity Cash (segment code 4).
    BSE_EQ,
    /// MCX Commodity (segment code 5).
    MCX_COMM,
    /// BSE Currency (segment code 7).
    BSE_CURRENCY,
    /// BSE Futures & Options (segment code 8).
    BSE_FNO,
}

impl ExchangeSegment {
    /// Returns the numeric segment code used in binary WebSocket packets.
    pub fn segment_code(self) -> u8 {
        match self {
            Self::IDX_I => 0,
            Self::NSE_EQ => 1,
            Self::NSE_FNO => 2,
            Self::NSE_CURRENCY => 3,
            Self::BSE_EQ => 4,
            Self::MCX_COMM => 5,
            Self::BSE_CURRENCY => 7,
            Self::BSE_FNO => 8,
        }
    }

    /// Construct from a numeric segment code (as found in binary feed packets).
    pub fn from_segment_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::IDX_I),
            1 => Some(Self::NSE_EQ),
            2 => Some(Self::NSE_FNO),
            3 => Some(Self::NSE_CURRENCY),
            4 => Some(Self::BSE_EQ),
            5 => Some(Self::MCX_COMM),
            7 => Some(Self::BSE_CURRENCY),
            8 => Some(Self::BSE_FNO),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Transaction Type
// ---------------------------------------------------------------------------

/// Buy or sell side of a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransactionType {
    BUY,
    SELL,
}

// ---------------------------------------------------------------------------
// Product Type
// ---------------------------------------------------------------------------

/// Product type for an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProductType {
    /// Cash & Carry for equity deliveries.
    CNC,
    /// Intraday for Equity, Futures & Options.
    INTRADAY,
    /// Carry Forward in Futures & Options.
    MARGIN,
    /// Margin Trading Facility.
    MTF,
    /// Cover Order (intraday only).
    CO,
    /// Bracket Order (intraday only).
    BO,
}

// ---------------------------------------------------------------------------
// Order Type
// ---------------------------------------------------------------------------

/// Type of order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    LIMIT,
    MARKET,
    STOP_LOSS,
    STOP_LOSS_MARKET,
}

// ---------------------------------------------------------------------------
// Order Status
// ---------------------------------------------------------------------------

/// Status of an order in the order book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Did not reach the exchange server.
    TRANSIT,
    /// Awaiting execution.
    PENDING,
    /// Used for Super Order: both entry and exit legs placed.
    CLOSED,
    /// Used for Super Order: target or stop-loss leg triggered.
    TRIGGERED,
    /// Rejected by broker or exchange.
    REJECTED,
    /// Cancelled by user.
    CANCELLED,
    /// Partial quantity traded successfully.
    PART_TRADED,
    /// Executed successfully.
    TRADED,
    /// Order expired.
    EXPIRED,
    /// Confirmed (used for Forever Orders).
    CONFIRM,
}

// ---------------------------------------------------------------------------
// Validity
// ---------------------------------------------------------------------------

/// Order validity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Validity {
    /// Valid for the trading day.
    DAY,
    /// Immediate or Cancel.
    IOC,
}

// ---------------------------------------------------------------------------
// Leg Name
// ---------------------------------------------------------------------------

/// Identifies a leg in Super Order / Bracket Order / Cover Order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LegName {
    ENTRY_LEG,
    TARGET_LEG,
    STOP_LOSS_LEG,
}

// ---------------------------------------------------------------------------
// Position Type
// ---------------------------------------------------------------------------

/// Position direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PositionType {
    LONG,
    SHORT,
    CLOSED,
}

// ---------------------------------------------------------------------------
// Option Type
// ---------------------------------------------------------------------------

/// Derivative option type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    CALL,
    PUT,
}

// ---------------------------------------------------------------------------
// After Market Order Time
// ---------------------------------------------------------------------------

/// Timing for after-market orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AmoTime {
    /// Pumped at pre-market session.
    PRE_OPEN,
    /// Pumped at market open.
    OPEN,
    /// Pumped 30 minutes after market open.
    OPEN_30,
    /// Pumped 60 minutes after market open.
    OPEN_60,
}

// ---------------------------------------------------------------------------
// Instrument
// ---------------------------------------------------------------------------

/// Instrument type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Instrument {
    INDEX,
    FUTIDX,
    OPTIDX,
    EQUITY,
    FUTSTK,
    OPTSTK,
    FUTCOM,
    OPTFUT,
    FUTCUR,
    OPTCUR,
}

// ---------------------------------------------------------------------------
// Expiry Code
// ---------------------------------------------------------------------------

/// Expiry proximity for derivative instruments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ExpiryCode {
    /// Current / near expiry.
    Near = 0,
    /// Next expiry.
    Next = 1,
    /// Far expiry.
    Far = 2,
}

// ---------------------------------------------------------------------------
// Order Flag (Forever Orders)
// ---------------------------------------------------------------------------

/// Forever order flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderFlag {
    /// Single forever order.
    SINGLE,
    /// One-Cancels-Other order.
    OCO,
}

// ---------------------------------------------------------------------------
// Kill Switch Status
// ---------------------------------------------------------------------------

/// Kill switch activation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KillSwitchStatus {
    ACTIVATE,
    DEACTIVATE,
}

// ---------------------------------------------------------------------------
// IP Flag
// ---------------------------------------------------------------------------

/// Static IP designation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IpFlag {
    PRIMARY,
    SECONDARY,
}

// ---------------------------------------------------------------------------
// Feed Request Code (WebSocket market feed)
// ---------------------------------------------------------------------------

/// Request codes sent over the market feed WebSocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FeedRequestCode {
    /// Connect to feed.
    Connect = 11,
    /// Disconnect from feed.
    Disconnect = 12,
    /// Subscribe to Ticker packets.
    SubscribeTicker = 15,
    /// Unsubscribe from Ticker packets.
    UnsubscribeTicker = 16,
    /// Subscribe to Quote packets.
    SubscribeQuote = 17,
    /// Unsubscribe from Quote packets.
    UnsubscribeQuote = 18,
    /// Subscribe to Full packets.
    SubscribeFull = 21,
    /// Unsubscribe from Full packets.
    UnsubscribeFull = 22,
    /// Subscribe to Full Market Depth.
    SubscribeFullMarketDepth = 23,
    /// Unsubscribe from Full Market Depth.
    UnsubscribeFullMarketDepth = 24,
}

impl Serialize for FeedRequestCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

// ---------------------------------------------------------------------------
// Feed Response Code (WebSocket market feed)
// ---------------------------------------------------------------------------

/// Response codes received in binary market feed packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FeedResponseCode {
    /// Index packet.
    Index = 1,
    /// Ticker packet (LTP + LTT).
    Ticker = 2,
    /// Quote packet (LTP, qty, ATP, volume, OHLC, etc.).
    Quote = 4,
    /// Open Interest packet.
    OI = 5,
    /// Previous close packet.
    PrevClose = 6,
    /// Market status packet.
    MarketStatus = 7,
    /// Full packet (quote + depth + OI).
    Full = 8,
    /// Feed disconnect packet.
    Disconnect = 50,
}

impl FeedResponseCode {
    /// Parse a response code from the first byte of a binary packet header.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Index),
            2 => Some(Self::Ticker),
            4 => Some(Self::Quote),
            5 => Some(Self::OI),
            6 => Some(Self::PrevClose),
            7 => Some(Self::MarketStatus),
            8 => Some(Self::Full),
            50 => Some(Self::Disconnect),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Conditional Trigger — Comparison Type
// ---------------------------------------------------------------------------

/// How the condition in a conditional trigger is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComparisonType {
    /// Compare technical indicator against a fixed numeric value.
    TECHNICAL_WITH_VALUE,
    /// Compare technical indicator against another indicator.
    TECHNICAL_WITH_INDICATOR,
    /// Compare a technical indicator with closing price.
    TECHNICAL_WITH_CLOSE,
    /// Continuously scans live market data.
    LIVE_SCAN_ALERT,
    /// Compare market price against a fixed value.
    PRICE_WITH_VALUE,
    /// Compare price change by percentage.
    PRICE_WITH_PERCENT_CHANGE,
}

// ---------------------------------------------------------------------------
// Conditional Trigger — Indicator Name
// ---------------------------------------------------------------------------

/// Technical indicator names supported by conditional triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IndicatorName {
    SMA_5,
    SMA_10,
    SMA_20,
    SMA_50,
    SMA_100,
    SMA_200,
    EMA_5,
    EMA_10,
    EMA_20,
    EMA_50,
    EMA_100,
    EMA_200,
    /// Upper Bollinger Band.
    BB_UPPER,
    /// Lower Bollinger Band.
    BB_LOWER,
    /// Relative Strength Index (14-period).
    RSI_14,
    /// Average True Range (14-period).
    ATR_14,
    /// Stochastic Oscillator.
    STOCHASTIC,
    /// Stochastic RSI (14-period).
    STOCHRSI_14,
    /// MACD long-term component (26-period).
    MACD_26,
    /// MACD short-term component (12-period).
    MACD_12,
    /// MACD histogram.
    MACD_HIST,
}

// ---------------------------------------------------------------------------
// Conditional Trigger — Operator
// ---------------------------------------------------------------------------

/// Comparison operator for conditional trigger conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operator {
    CROSSING_UP,
    CROSSING_DOWN,
    CROSSING_ANY_SIDE,
    GREATER_THAN,
    LESS_THAN,
    GREATER_THAN_EQUAL,
    LESS_THAN_EQUAL,
    EQUAL,
    NOT_EQUAL,
}

// ---------------------------------------------------------------------------
// Conditional Trigger — Alert Status
// ---------------------------------------------------------------------------

/// Status of a conditional trigger alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertStatus {
    /// Alert is currently active and monitoring.
    ACTIVE,
    /// Alert condition has been met.
    TRIGGERED,
    /// Alert has expired.
    EXPIRED,
    /// Alert was cancelled by the user.
    CANCELLED,
}
