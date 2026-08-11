//! WebSocket modules for real-time data streaming.
//!
//! DhanHQ documents four distinct WebSocket protocols: the standard market
//! feed, order updates, 20-level depth, and 200-level depth.
//!
//! ## [`market_feed`] — Live Market Feed
//!
//! Connects to `wss://api-feed.dhan.co` and streams real-time market data as
//! **binary packets**. Supports three subscription modes:
//!
//! - **Ticker** — 16-byte packet including the 8-byte header
//! - **Quote** — 50-byte packet including the 8-byte header
//! - **Full** — 162-byte packet including the header and five depth levels
//!
//! [`manager`] adds lazy socket ownership, retry, desired-subscription
//! restoration, health, durable gap reporting, and bounded shutdown.
//!
//! ## [`order_update`] — Live Order Updates
//!
//! Connects to `wss://api-order-update.dhan.co` and streams real-time order
//! status changes as **JSON messages**. Supports both individual and partner
//! authentication modes.
//!
//! ## [`depth`] — Full Market Depth
//!
//! Implements the separate 20-level and 200-level endpoints, request
//! envelopes, 12-byte stacked packet framing, limits, and close diagnostics.
//! These are caller-polled low-level streams and do not share the standard
//! feed manager.
//!
//! ## Usage
//!
//! The low-level streams implement [`futures_util::Stream`] so you can use them with
//! `StreamExt::next()`, `StreamExt::filter_map()`, and other async combinators.
//!
//! ## Limits
//!
//! - Maximum 5 WebSocket connections per user
//! - Up to 5,000 instruments per connection
//! - Up to 100 instruments per subscribe/unsubscribe message

pub mod depth;
pub mod manager;
pub mod market_feed;
pub mod order_update;
