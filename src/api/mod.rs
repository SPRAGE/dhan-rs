//! REST API endpoint implementations.
//!
//! Each sub-module adds high-level `async` methods to
//! [`DhanClient`](crate::client::DhanClient) via `impl` blocks. All methods
//! handle JSON serialization, HTTP transport, and error mapping automatically.
//!
//! ## Usage
//!
//! Simply import the relevant types and call methods on your `DhanClient`:
//!
//! ```no_run
//! use dhan_rs::DhanClient;
//!
//! # #[tokio::main]
//! # async fn main() -> dhan_rs::Result<()> {
//! let client = DhanClient::new("client-id", "token");
//! let orders = client.get_orders().await?;
//! let holdings = client.get_holdings().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Modules
//!
//! | Module | Async methods | Description |
//! |---|---|---|
//! | [`orders`] | 9 | Order CRUD, slicing, trade book |
//! | [`super_order`] | 5 | Bracket/cover orders; both documented cancel envelopes |
//! | [`forever_order`] | 5 | GTT/OCO orders; both documented list paths |
//! | [`conditional`] | 6 | Alert triggers and multi-order placement |
//! | [`portfolio`] | 4 | Holdings, positions, exit all |
//! | [`funds`] | 3 | Margin calculator, fund limits |
//! | [`market_quote`] | 3 | LTP, OHLC, REST depth snapshots |
//! | [`historical`] | 2 | Daily & intraday candles |
//! | [`option_chain`] | 2 | Option chain, expiry lists |
//! | [`auth`] | 6 | Token generation and consent calls, plus two URL builders |
//! | [`profile`] | 1 | User profile |
//! | [`ip`] | 3 | Static IP management |
//! | [`edis`] | 4 | T-PIN, single/bulk eDIS forms, inquiry |
//! | [`traders_control`] | 5 | Kill switch, P&L-based exit |
//! | [`statements`] | 3 | Documented/tolerant ledger forms, trade history |
//! | [`data`] | 4 | Rolling options, technicals, movers, company data |
//! | [`global_stocks`] | 12 | Global Stocks orders, trades, portfolio, estimates |
//! | [`instruments`] | 3 | Compact, detailed, and segment CSV downloads |

pub mod auth;
pub mod conditional;
pub mod data;
pub mod edis;
pub mod forever_order;
pub mod funds;
pub mod global_stocks;
pub mod historical;
pub mod instruments;
pub mod ip;
pub mod market_quote;
pub mod option_chain;
pub mod orders;
pub mod portfolio;
pub mod profile;
pub mod statements;
pub mod super_order;
pub mod traders_control;
