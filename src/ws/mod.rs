//! WebSocket modules for real-time data streaming.
//!
//! - [`market_feed`] — Live Market Feed (binary packets: ticker, quote, full depth).
//! - [`order_update`] — Live Order Updates (JSON messages).

pub mod market_feed;
pub mod order_update;
