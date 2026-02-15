//! API endpoint implementations.
//!
//! Each sub-module adds high-level methods to [`crate::client::DhanClient`].

pub mod auth;
pub mod conditional;
pub mod edis;
pub mod forever_order;
pub mod funds;
pub mod historical;
pub mod ip;
pub mod market_quote;
pub mod option_chain;
pub mod orders;
pub mod portfolio;
pub mod profile;
pub mod statements;
pub mod super_order;
pub mod traders_control;
