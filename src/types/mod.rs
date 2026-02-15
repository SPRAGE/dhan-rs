//! Request and response types for the DhanHQ API v2.
//!
//! This module contains all the strongly-typed structs used for serializing
//! requests and deserializing responses across every DhanHQ API endpoint.
//!
//! ## Organization
//!
//! - [`enums`] — Shared enumerations (exchange segments, order types, etc.)
//! - [`orders`] — Order placement, modification, book, and trade types
//! - [`super_order`] — Super Order (bracket/cover) types
//! - [`forever_order`] — Forever/GTT order types
//! - [`conditional`] — Conditional trigger types
//! - [`portfolio`] — Holdings, positions, and conversion types
//! - [`funds`] — Margin calculator and fund limit types
//! - [`market_quote`] — LTP, OHLC, and market depth quote types
//! - [`historical`] — Daily and intraday candle data types
//! - [`option_chain`] — Option chain and expiry list types with Greeks
//! - [`auth`] — Authentication request/response types
//! - [`profile`] — User profile types
//! - [`edis`] — eDIS (electronic delivery) types
//! - [`traders_control`] — Kill switch and P&L exit types
//! - [`statements`] — Ledger and trade history types
//! - [`postback`] — Webhook payload deserialization type
//!
//! All enums are re-exported at the module root via `pub use enums::*`.

pub mod auth;
pub mod conditional;
pub mod edis;
pub mod enums;
pub mod forever_order;
pub mod funds;
pub mod historical;
pub mod market_quote;
pub mod option_chain;
pub mod orders;
pub mod portfolio;
pub mod postback;
pub mod profile;
pub mod statements;
pub mod super_order;
pub mod traders_control;

pub use enums::*;
