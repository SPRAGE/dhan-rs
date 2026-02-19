//! WebSocket modules for real-time data streaming.
//!
//! DhanHQ provides two WebSocket endpoints for live data:
//!
//! ## [`market_feed`] — Live Market Feed
//!
//! Connects to `wss://api-feed.dhan.co` and streams real-time market data as
//! **binary packets**. Supports three subscription modes:
//!
//! - **Ticker** — LTP + last trade time (8 bytes)
//! - **Quote** — LTP + OHLC + volume + buy/sell quantities (42 bytes)
//! - **Full** — Quote + OI + 5-level market depth (154 bytes)
//!
//! Packets are parsed with zero-copy native `from_le_bytes()` for maximum
//! throughput during market hours.
//!
//! ## [`order_update`] — Live Order Updates
//!
//! Connects to `wss://api-order-update.dhan.co` and streams real-time order
//! status changes as **JSON messages**. Supports both individual and partner
//! authentication modes.
//!
//! ## Usage
//!
//! Both streams implement [`futures_util::Stream`] so you can use them with
//! `StreamExt::next()`, `StreamExt::filter_map()`, and other async combinators.
//!
//! ## Limits
//!
//! - Maximum 5 WebSocket connections per user
//! - Up to 5,000 instruments per connection
//! - Up to 100 instruments per subscribe/unsubscribe message

pub mod manager;
pub mod market_feed;
pub mod order_update;
